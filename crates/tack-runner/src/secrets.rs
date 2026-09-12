//! Runner-local secret storage.
//!
//! Two backends, tried in this order at construction: the platform
//! credential store (macOS Keychain, Windows Credential Manager, Linux
//! Secret Service), and — only when no platform store answers — a single
//! owner-only file. Which backend a [`SecretStore`] ended up using is fixed
//! for its whole lifetime and reported by `tack runner doctor`, so a file is
//! never mistaken for a keychain.
//!
//! This depends on `keyring-core` and a specific per-platform store crate
//! directly, not the `keyring` convenience crate: `keyring`'s own docs say
//! an application that wants to choose its own fallback order, or swap the
//! store for a test double, should link `keyring-core` and a store crate
//! instead of `keyring` — exactly this module's situation. A platform-store
//! attempt has to be caught and turned into a file fallback, and tests need
//! a store that never touches a real Secret Service.
//!
//! Constructing the platform store is a blocking call that can stall far
//! longer than a normal answer takes — a Secret Service that has to
//! activate over D-Bus is the concrete case seen on this project's own
//! Linux boxes. `open` bounds that call with [`PLATFORM_STORE_TIMEOUT`] so
//! a slow or hung store degrades to the file backend instead of hanging
//! the caller; see that constant's doc comment for the split-brain
//! consequence of the bound existing at all.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use keyring_core::api::CredentialStoreApi;
use thiserror::Error;

/// Service name every keychain entry is filed under, and what a live
/// `secret-tool`/`security` lookup on the dev machine names.
pub const SERVICE: &str = "tack-runner";

/// How long [`SecretStore::open`] waits for the platform credential store
/// to answer before treating it as absent and falling back to the file
/// backend. A working store answers a local IPC call in well under this;
/// the bound only ever matters on the boot where the store would otherwise
/// have hung.
///
/// Its cost is a split-brain risk, not a false negative: a store that
/// clears this window on one boot and misses it on the next (a Secret
/// Service that is briefly slow to activate, a machine under load) makes
/// `SecretStore::open` choose a different backend for the same secret
/// names across restarts, silently, since nothing else about the machine
/// changed. `tack runner doctor` reports this bound and the backend it
/// picked so that swap is visible rather than only inferred from a key
/// that stops resolving.
pub const PLATFORM_STORE_TIMEOUT: Duration = Duration::from_secs(3);

/// Which backend answered. Never a mode a caller chooses — [`SecretStore`]
/// picks this once, at construction, from what the machine actually has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackendKind {
    Keychain,
    File,
}

impl fmt::Display for SecretBackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SecretBackendKind::Keychain => "keychain",
            SecretBackendKind::File => "file",
        })
    }
}

/// A secret value read from the store. `Debug`/`Display` are hardcoded to
/// `[REDACTED]`, exactly like `client::RunnerCredential` — the same rule,
/// applied to a second kind of secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret only to the caller that must put it into a spawned
    /// harness's environment. Callers must never put this into a log, error,
    /// or command line.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Every variant names an entry, an environment variable, or a reference
/// string — never a value. Safe to log, fold into a
/// `HarnessError::Rejected` reason, or print from the CLI as-is.
#[derive(Debug, Error)]
pub enum SecretError {
    #[error("no secret named {0:?} in the store")]
    NotFound(String),
    #[error("environment variable {0} is not set")]
    EnvVarNotSet(String),
    #[error("secret_reference {0:?} is not a store:<name> or env:<VARIABLE> reference")]
    InvalidReference(String),
    #[error("secret store I/O failed")]
    Io,
    #[error("secret store backend failed: {0}")]
    Backend(String),
}

#[derive(Clone)]
enum Inner {
    Keychain(Arc<dyn CredentialStoreApi + Send + Sync>),
    File(PathBuf),
}

/// A runner-local secret store bound to one backend for its whole lifetime.
///
/// Cloning is cheap: the keychain backend clones an `Arc` handle, the file
/// backend clones a path. Every adapter that resolves a `secret_reference`
/// holds its own clone of the one store the runner opened at startup.
#[derive(Clone)]
pub struct SecretStore {
    inner: Inner,
}

impl SecretStore {
    /// Tries the platform credential store; falls back to a single
    /// owner-only file at `file_fallback_path` only when no platform store
    /// answers within [`PLATFORM_STORE_TIMEOUT`] — callers pass
    /// `RunnerConfig::secret_store_path()`, which keeps the fallback path a
    /// `config.rs` concern. The choice is made once, here, and never
    /// re-attempted for the life of the returned `SecretStore` — matching
    /// `tack runner doctor`'s report, which reads this same choice back.
    pub fn open(file_fallback_path: &Path) -> Self {
        match Self::platform_store() {
            Ok(store) => {
                tracing::info!(backend = "keychain", "secret store backend selected");
                Self {
                    inner: Inner::Keychain(store),
                }
            }
            Err(reason) => {
                tracing::warn!(
                    backend = "file",
                    reason = %reason,
                    "no platform credential store answered; falling back to an owner-only file"
                );
                Self::file(file_fallback_path.to_path_buf())
            }
        }
    }

    /// The file backend directly, bypassing platform-store detection.
    /// Production code never needs this (`open` decides); tests use it to
    /// get a hermetic store that never touches a real keychain.
    pub fn file(path: PathBuf) -> Self {
        Self {
            inner: Inner::File(path),
        }
    }

    /// Binds directly to an already-constructed backend — the real platform
    /// store `open`'s target-gated probe would have found, or
    /// `keyring_core::mock::Store` in tests. Lets a test exercise the
    /// keychain code path (not just the file fallback) without a real
    /// Secret Service.
    #[cfg(test)]
    fn with_store(store: Arc<dyn CredentialStoreApi + Send + Sync>) -> Self {
        Self {
            inner: Inner::Keychain(store),
        }
    }

    /// Which backend this instance is actually using.
    pub fn backend(&self) -> SecretBackendKind {
        match &self.inner {
            Inner::Keychain(_) => SecretBackendKind::Keychain,
            Inner::File(_) => SecretBackendKind::File,
        }
    }

    /// Runs the platform-specific probe on a helper thread and gives it
    /// [`PLATFORM_STORE_TIMEOUT`] to answer.
    fn platform_store() -> Result<Arc<dyn CredentialStoreApi + Send + Sync>, String> {
        Self::bounded(PLATFORM_STORE_TIMEOUT, Self::platform_store_unbounded)
    }

    /// Runs `work` on its own thread and returns what it produces, or a
    /// timeout error if `timeout` passes first. `work` is never cancelled —
    /// a genuinely hung probe keeps its thread alive past the timeout, with
    /// nothing left to receive its eventual answer — so this bounds how
    /// long the *caller* waits, not how long the probe itself runs.
    fn bounded<T: Send + 'static>(
        timeout: Duration,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        match receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                Err(format!("did not answer within {timeout:?}"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("probe thread ended without answering".to_string())
            }
        }
    }

    fn platform_store_unbounded() -> Result<Arc<dyn CredentialStoreApi + Send + Sync>, String> {
        #[cfg(target_os = "macos")]
        {
            let store: Arc<dyn CredentialStoreApi + Send + Sync> =
                apple_native_keyring_store::keychain::Store::new()
                    .map_err(|error| error.to_string())?;
            Ok(store)
        }
        #[cfg(target_os = "windows")]
        {
            let store: Arc<dyn CredentialStoreApi + Send + Sync> =
                windows_native_keyring_store::Store::new().map_err(|error| error.to_string())?;
            Ok(store)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let store: Arc<dyn CredentialStoreApi + Send + Sync> =
                zbus_secret_service_keyring_store::Store::new()
                    .map_err(|error| error.to_string())?;
            Ok(store)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
        {
            Err("no platform credential store is implemented for this target".to_string())
        }
    }

    /// Stores `value` under `name`, overwriting any existing entry.
    pub fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        match &self.inner {
            Inner::Keychain(store) => {
                let entry = store
                    .build(SERVICE, name, None)
                    .map_err(|error| SecretError::Backend(error.to_string()))?;
                entry
                    .set_secret(value.as_bytes())
                    .map_err(|error| SecretError::Backend(error.to_string()))
            }
            Inner::File(path) => {
                let mut entries = read_file(path)?;
                entries.insert(name.to_owned(), value.to_owned());
                write_file(path, &entries)
            }
        }
    }

    /// Reads the value stored under `name`.
    pub fn get(&self, name: &str) -> Result<SecretValue, SecretError> {
        match &self.inner {
            Inner::Keychain(store) => {
                let entry = store
                    .build(SERVICE, name, None)
                    .map_err(|error| SecretError::Backend(error.to_string()))?;
                match entry.get_secret() {
                    Ok(bytes) => String::from_utf8(bytes).map(SecretValue::new).map_err(|_| {
                        SecretError::Backend("stored secret is not valid UTF-8".to_string())
                    }),
                    Err(keyring_core::Error::NoEntry) => {
                        Err(SecretError::NotFound(name.to_owned()))
                    }
                    Err(error) => Err(SecretError::Backend(error.to_string())),
                }
            }
            Inner::File(path) => {
                let entries = read_file(path)?;
                entries
                    .get(name)
                    .cloned()
                    .map(SecretValue::new)
                    .ok_or_else(|| SecretError::NotFound(name.to_owned()))
            }
        }
    }

    /// Removes the entry named `name`. Not an error if it was already
    /// absent — matches `rm -f`, not `rm`.
    pub fn remove(&self, name: &str) -> Result<(), SecretError> {
        match &self.inner {
            Inner::Keychain(store) => {
                let entry = store
                    .build(SERVICE, name, None)
                    .map_err(|error| SecretError::Backend(error.to_string()))?;
                match entry.delete_credential() {
                    Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
                    Err(error) => Err(SecretError::Backend(error.to_string())),
                }
            }
            Inner::File(path) => {
                let mut entries = read_file(path)?;
                entries.remove(name);
                write_file(path, &entries)
            }
        }
    }

    /// Names held in the store. Never values.
    pub fn list(&self) -> Result<Vec<String>, SecretError> {
        match &self.inner {
            Inner::Keychain(store) => {
                let mut spec = HashMap::new();
                spec.insert("service", SERVICE);
                let found = store
                    .search(&spec)
                    .map_err(|error| SecretError::Backend(error.to_string()))?;
                let mut names: Vec<String> = found
                    .into_iter()
                    .filter_map(|entry| entry.get_specifiers().map(|(_service, user)| user))
                    .collect();
                names.sort();
                names.dedup();
                Ok(names)
            }
            Inner::File(path) => {
                let entries = read_file(path)?;
                Ok(entries.into_keys().collect())
            }
        }
    }

    /// Resolves a `secret_reference` string. `store:<name>` (the default
    /// scheme when none is given, so the frozen contract fixture's bare
    /// name stays valid) reads from this store; `env:<VARIABLE>` reads the
    /// runner process's own environment at spawn time — the path for a
    /// systemd-started runner with no keychain and no wish to keep a file.
    pub fn resolve(&self, reference: &str) -> Result<SecretValue, SecretError> {
        if let Some(variable) = reference.strip_prefix("env:") {
            return std::env::var(variable)
                .map(SecretValue::new)
                .map_err(|_| SecretError::EnvVarNotSet(variable.to_owned()));
        }
        let name = reference.strip_prefix("store:").unwrap_or(reference);
        if name.is_empty() {
            return Err(SecretError::InvalidReference(reference.to_owned()));
        }
        self.get(name)
    }
}

fn read_file(path: &Path) -> Result<BTreeMap<String, String>, SecretError> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|_| SecretError::Backend("secrets file is malformed".to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(_) => Err(SecretError::Io),
    }
}

fn write_file(path: &Path, entries: &BTreeMap<String, String>) -> Result<(), SecretError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| SecretError::Io)?;
    }
    let encoded = serde_json::to_string(entries).map_err(|_| SecretError::Io)?;
    let temporary = path.with_extension("json.tmp");
    write_owner_only(&temporary, encoded.as_bytes())?;
    fs::rename(&temporary, path).map_err(|_| SecretError::Io)
}

#[cfg(unix)]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), SecretError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| SecretError::Io)?;
    file.write_all(bytes).map_err(|_| SecretError::Io)?;
    file.sync_all().map_err(|_| SecretError::Io)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), SecretError> {
    fs::write(path, bytes).map_err(|_| SecretError::Io)
}

#[cfg(test)]
#[path = "secrets/tests.rs"]
mod tests;

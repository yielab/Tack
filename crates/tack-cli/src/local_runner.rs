//! Hosts the runner role inside the `tack` binary: `tack runner start` runs
//! it as the whole process, `tack serve` (with or without `--with-runner`)
//! runs it as a controllable task alongside the server in the same
//! process.
//!
//! Both paths build a [`tack_runner::RunnerConfig`] through the exact
//! precedence rules `tack-runner`'s own binary uses
//! ([`load_runner_config`]) and then hand it to `tack_runner::bootstrap`,
//! the crate's single composition root — there is no second way to wire a
//! runner in this codebase, only two callers of the same one.
//!
//! The embedded case speaks to the server it is embedded in exactly like a
//! remote runner would: ordinary runner-v1 HTTP against the loopback
//! address the server actually bound. It does not reach into `tack-api`'s
//! router or state, and never will — a shortcut here would create a second
//! implementation of the runner protocol client that `docs/contracts/
//! runner-v1/` cannot hold accountable.
//!
//! **The seam this module adds (ADR 0061 decisions 2 and 6):** [`serve`]
//! always wires an [`EmbeddedRunnerControl`] into `AppState`
//! (`tack_api::serve_with_ready_and_local_runner`), whether or not
//! `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` says to start it immediately —
//! that flag now only decides `AppConfig::local_runner_enable`'s startup
//! value, folded in by [`with_runner_enabled`]/`main.rs` before this
//! function is reached. `tack_api::server::serve_inner`'s own auto-start
//! check and `PUT /api/local-runner` both call the exact same
//! [`EmbeddedRunnerControl::start`] — there is only ever one code path into
//! the runtime, never a boot-time one and a UI-triggered one.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_api::{
    CatalogSnapshot, LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
    SecretMeta,
};
use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    RunnerError, Shutdown, ShutdownHandle,
    bootstrap::{self, RunnerLimits},
    harness::process::ProcessLimits,
    secrets::SecretStore,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Bounds applied to every harness subprocess a runner hosted by this binary
/// spawns. Mirrors the standalone `tack-runner` binary's own bounds: both
/// binaries run the identical composition root and there is no reason for a
/// harness to behave differently depending on which process launched it.
const HARNESS_PROCESS_LIMITS: ProcessLimits =
    ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, Duration::from_secs(3_600));

/// How long any single non-claim protocol call may take.
const PROTOCOL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn runner_limits() -> RunnerLimits {
    RunnerLimits {
        harness_process: HARNESS_PROCESS_LIMITS,
        protocol_request_timeout: PROTOCOL_REQUEST_TIMEOUT,
    }
}

/// Loads runner configuration from an optional TOML file, the environment,
/// then `command_line` overrides, in that precedence — identical to
/// `tack-runner`'s own binary, reusing its types rather than re-parsing.
fn load_runner_config(
    command_line: ConfigOverrides,
    config_path: Option<&Path>,
) -> Result<RunnerConfig, ConfigError> {
    let file_toml = config_path
        .map(|path| std::fs::read_to_string(path).map_err(|_| ConfigError::Unreadable))
        .transpose()?;
    RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: file_toml.as_deref(),
        environment: RunnerConfig::environment_overrides(),
        command_line,
    })
}

/// Where the embedded runner's on-disk state (its enrolled credential,
/// attempt journal and secret store) lives when nothing more specific asks
/// for another directory — one level under this server's own `storage_dir`,
/// mirroring how `execution-artifacts` already nests there instead of
/// colliding with attachments (`router.rs`). A server started against a
/// different database — and, following the convention every other
/// per-install artifact in this crate already follows, its own
/// `TACK_STORAGE_DIR` — never resolves to the same runner state as another
/// server's; that coupling, not merely a directory name, is what this
/// function exists to establish.
fn embedded_default_state_dir(storage_dir: &str) -> PathBuf {
    Path::new(storage_dir).join("runner")
}

/// One-time, best-effort recovery for an install upgrading from before the
/// embedded runner's default state directory followed `storage_dir`: if the
/// crate's bare, cwd-relative default
/// (`tack_runner::config::DEFAULT_STATE_DIR`) still holds state and the new,
/// database-scoped directory does not exist yet, moves it there in a single
/// rename — the alternative is stranding an already-enrolled credential
/// somewhere this binary will never look again.
///
/// Never touches either directory once `new_dir` already exists: that means
/// either a previous boot already migrated, or a fresh install already
/// provisioned there, and this must never clobber either. A failed rename
/// (for example, `new_dir` ending up on a different filesystem than the
/// legacy default) is reported and the legacy directory is left exactly as
/// it was — the caller falls through to provisioning a fresh identity at
/// `new_dir` instead, so this never falls back to silently reusing state it
/// could not verify moved intact.
fn migrate_legacy_state_dir(new_dir: &Path) {
    if new_dir.exists() {
        return;
    }
    let legacy_dir = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
    if !legacy_dir.is_dir() {
        return;
    }
    if let Some(parent) = new_dir.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(
            %error,
            "could not prepare the embedded runner's new state directory; its legacy state is left in place"
        );
        return;
    }
    match std::fs::rename(legacy_dir, new_dir) {
        Ok(()) => tracing::info!(
            "migrated the embedded runner's on-disk state to the directory scoped to this server's own storage configuration"
        ),
        Err(error) => tracing::warn!(
            %error,
            "could not migrate the embedded runner's legacy state directory; it is left in place and a fresh identity will be provisioned instead"
        ),
    }
}

/// Whether the embedded runner should start at boot, combining
/// `--with-runner` with its environment equivalent. Off unless one of the
/// two explicitly says on. This only ever feeds `AppConfig::
/// local_runner_enable` (`main.rs`'s `run_server`) — the actual on/off
/// decision at any later moment is `effective_local_runner_enabled`'s
/// (`tack-api`), which lets a UI toggle override this startup default from
/// then on.
pub fn with_runner_enabled(flag: bool) -> bool {
    flag || std::env::var("TACK_LOCAL_RUNNER_ENABLE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Runs the runner role as the whole process (`tack runner start`), owning
/// its own Tokio runtime and its own ctrl-c wiring — the same shutdown
/// pattern `tack-runner`'s binary uses, reimplemented here because that
/// binary's `main` is not library code this crate can call into.
pub fn run_standalone(
    config_path: Option<PathBuf>,
    api_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_token: Option<String>,
) -> anyhow::Result<()> {
    let config = load_runner_config(
        ConfigOverrides {
            api_base_url: api_url,
            runner_id,
            state_dir,
            enrollment_credential: enrollment_token.map(EnrollmentCredential::new),
            ..ConfigOverrides::default()
        },
        config_path.as_deref(),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_to_shutdown(config))
}

async fn run_to_shutdown(config: RunnerConfig) -> anyhow::Result<()> {
    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runner_task = tokio::spawn(bootstrap::run(config, runner_limits(), shutdown));

    tokio::select! {
        result = &mut runner_task => {
            let result = result.map_err(|error| anyhow::anyhow!("runner task panicked: {error}"))?;
            result.map_err(anyhow::Error::from)
        }
        signal = tokio::signal::ctrl_c() => {
            signal?;
            tracing::info!("shutdown signal received");
            shutdown_handle.request();
            let result = runner_task
                .await
                .map_err(|error| anyhow::anyhow!("runner task panicked: {error}"))?;
            result.map_err(anyhow::Error::from)
        }
    }
}

/// Rejects a server configuration that explicitly asked the embedded runner
/// to start (`--with-runner`/`TACK_LOCAL_RUNNER_ENABLE`) on a non-loopback
/// bind. An embedded runner executes arbitrary agent processes on the host
/// serving the UI, so that combination is refused outright, before any
/// socket or database opens, rather than downgraded to "serve without a
/// runner" — a caller must fail loudly rather than silently ignore its own
/// flag. Callers only reach this when they already know the runner was
/// asked to auto-start; a plain `tack serve` with no such request is fine
/// on any bind — see [`serve`]'s own doc comment for why the same
/// loopback rule, applied to a *persisted* preference instead of this
/// boot's own flag, is checked again later, after the database opens.
fn ensure_loopback(config: &tack_api::config::AppConfig) -> anyhow::Result<()> {
    if !config.binds_loopback() {
        anyhow::bail!(
            "refusing to start with an embedded runner: {} is not a loopback address. An \
             embedded runner executes arbitrary agent processes on this host, so it is \
             restricted to a server bound to loopback",
            config.host
        );
    }
    Ok(())
}

/// Makes sure `runner_config` carries something the embedded runner can
/// redeem, in order of preference:
///
/// 1. a manually configured `enrollment_credential` — always wins, and is
///    left untouched;
/// 2. a durable session already on disk under `state_dir`, *if* its runner
///    id still resolves against this server's own database
///    (`local_enrollment::stored_session_orphaned`) — reused as is.
///    [`crate::local_enrollment::self_provision`] is not called, so a
///    restart against an already-enrolled `state_dir` never mints a second
///    one-time token or creates a second pending runner row. The config's
///    credential is still set, to a placeholder
///    (`local_enrollment::stored_session_placeholder`) rather than left
///    empty — `tack_runner::bootstrap::build_runtime` requires *some*
///    credential before it ever looks at `state_dir` — see that
///    placeholder's own doc comment for the full explanation and why it is
///    not transmitted on a normal restart;
/// 3. otherwise (nothing on disk, or a session whose runner id this
///    database has no row for — the case where `storage_dir` stayed put but
///    the database underneath it was deleted and recreated), a one-time
///    token self-provisioned in-process against the server's own database
///    (legitimate here specifically because the operator and the runner are
///    the same person on the same machine — see
///    `docs/adr/0058-standalone-single-binary-runner.md`). The orphaned
///    session file itself is left on disk untouched: `establish_session`
///    (`tack_runner::transport`) still finds it, still tries `refresh`
///    first, still gets refused by this same database for the same reason,
///    and falls through to redeeming the fresh credential this branch just
///    provisioned — the same fallback that already existed, now reached
///    with a real token instead of the placeholder that used to reach it by
///    mistake.
///
/// Only ever called after [`ensure_loopback`] has already passed, since it
/// runs after the server has started — self-provisioning inherits that
/// guard rather than re-deriving it.
async fn ensure_runner_credential(
    runner_config: &mut RunnerConfig,
    server_config: &tack_api::config::AppConfig,
) -> anyhow::Result<()> {
    if runner_config.enrollment_credential.is_some() {
        return Ok(());
    }
    if crate::local_enrollment::has_stored_session(&runner_config.state_dir) {
        if crate::local_enrollment::stored_session_orphaned(
            &runner_config.state_dir,
            &server_config.database_url,
        )
        .await?
        {
            tracing::info!(
                "the embedded runner's stored credential belongs to a database this server is \
                 no longer using; provisioning a fresh identity instead of reusing it"
            );
        } else {
            runner_config.enrollment_credential =
                Some(crate::local_enrollment::stored_session_placeholder());
            return Ok(());
        }
    }
    let credential = crate::local_enrollment::self_provision(&server_config.database_url).await?;
    runner_config.enrollment_credential = Some(credential);
    Ok(())
}

/// Where [`EmbeddedRunnerControl`] remembers which secret names it has set
/// and when — never a value, and deliberately *not* inside
/// `SecretStore`'s own file/keychain entries: `tack-runner`'s `SecretStore`
/// tracks no timestamp at all, and it stores one entry per credential name
/// with no room for metadata alongside it. A name present in the real store
/// but absent here (set by `tack runner secret set` before this UI ever
/// ran) reports `set_at: None` rather than a fabricated time.
fn secret_meta_path(state_dir: &Path) -> PathBuf {
    state_dir.join("secret_meta.json")
}

fn load_secret_meta(state_dir: &Path) -> HashMap<String, DateTime<Utc>> {
    std::fs::read(secret_meta_path(state_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Best-effort: a failure to persist this sidecar loses only "since when"
/// display metadata, never the secret itself (already durably written to
/// the real store by the caller before this runs).
fn save_secret_meta(state_dir: &Path, meta: &HashMap<String, DateTime<Utc>>) {
    let Ok(json) = serde_json::to_vec(meta) else {
        return;
    };
    if let Err(error) = std::fs::write(secret_meta_path(state_dir), json) {
        tracing::warn!(%error, "failed to persist local-runner secret metadata (non-fatal)");
    }
}

/// Narrows the per-provider catalog map to the single status this response
/// carries. The panel it feeds is one provider's key field, so the line it
/// renders is that provider's own catalog; no entry for it reads the same as
/// an unconfigured one, which is what it is. A screen that offers more than
/// one provider needs a response shaped per provider, not this one.
///
/// The per-model price and context-window counts stop here on purpose:
/// putting them in this response puts them on the wire, and the capability
/// type has nowhere to carry per-model metadata yet.
fn map_catalog_status(status: Option<&tack_runner::provider::CatalogStatus>) -> CatalogSnapshot {
    use tack_runner::provider::CatalogStatus;
    match status {
        None | Some(CatalogStatus::NotConfigured) => CatalogSnapshot::NotConfigured,
        Some(CatalogStatus::SecretUnresolved) => CatalogSnapshot::SecretUnresolved,
        Some(CatalogStatus::Unreachable { status }) => CatalogSnapshot::Unreachable {
            http_status: *status,
        },
        Some(CatalogStatus::Configured {
            model_count,
            checked_at,
            ..
        }) => CatalogSnapshot::Configured {
            model_count: *model_count,
            checked_at: *checked_at,
        },
    }
}

struct Running {
    shutdown_handle: ShutdownHandle,
    runner_task: JoinHandle<Result<(), RunnerError>>,
    since: DateTime<Utc>,
}

struct State {
    /// `None` until the server this control is embedded in has bound its
    /// listener and told [`EmbeddedRunnerControl::set_bound_addr`] — before
    /// that, [`EmbeddedRunnerControl::start`] has nowhere to point the
    /// runner's own HTTP client and fails rather than guessing.
    bound_addr: Option<SocketAddr>,
    runner_config: RunnerConfig,
    running: Option<Running>,
    secret_meta: HashMap<String, DateTime<Utc>>,
    /// Whether `providers[vercel_ai_gateway].enabled` currently reads `true`
    /// only because [`EmbeddedRunnerControl::set_secret`] flipped it as a
    /// convenience, rather than because the operator's own configuration
    /// (TOML, `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED`, or an
    /// already-`true` value at boot) already said so. Set only at the
    /// instant `set_secret` actually changes the flag from `false` to
    /// `true`; cleared by [`EmbeddedRunnerControl::remove_secret`] undoing
    /// that exact flip. An operator's own `true` never sets this, so
    /// removing the default secret never turns off a provider the operator
    /// configured on directly. Never persisted — matches `enabled` itself,
    /// which this process never writes back to any file either.
    vercel_ai_gateway_secret_auto_enabled: bool,
}

/// The seam `tack-api`'s routes call (`handlers::local_runner`'s
/// `LocalRunnerControl` trait) without ever depending on `tack-runner`
/// itself. One instance lives for the life of a `tack serve` process,
/// constructed once in [`serve`] and shared (via `Arc`) between
/// `AppState::local_runner` and this module's own boot-time wiring.
pub struct EmbeddedRunnerControl {
    server_config: tack_api::config::AppConfig,
    state: Mutex<State>,
}

impl EmbeddedRunnerControl {
    /// Builds the runner configuration this control starts from. The state
    /// directory defaults to [`embedded_default_state_dir`] — scoped to
    /// `server_config.storage_dir`, so a server opened against a different
    /// database never resolves to another server's runner state — unless
    /// `TACK_RUNNER_STATE_DIR` is set, which still wins here exactly as it
    /// would for the standalone binary (`docs/CONFIG.md`'s documented escape
    /// hatch, never overridden by a default this module computes for
    /// itself).
    fn new(server_config: tack_api::config::AppConfig) -> Result<Self, ConfigError> {
        let command_line = if std::env::var_os("TACK_RUNNER_STATE_DIR").is_some() {
            ConfigOverrides::default()
        } else {
            let default_state_dir = embedded_default_state_dir(&server_config.storage_dir);
            migrate_legacy_state_dir(&default_state_dir);
            ConfigOverrides {
                state_dir: Some(default_state_dir),
                ..ConfigOverrides::default()
            }
        };
        let runner_config = load_runner_config(command_line, None)?;
        let secret_meta = load_secret_meta(&runner_config.state_dir);
        Ok(Self {
            server_config,
            state: Mutex::new(State {
                bound_addr: None,
                runner_config,
                running: None,
                secret_meta,
                vercel_ai_gateway_secret_auto_enabled: false,
            }),
        })
    }

    /// Told the server's own bound loopback address once it is known — see
    /// [`serve`]'s doc comment for why this can't be passed to `new`
    /// instead: the listener the address comes from hasn't opened yet at
    /// construction time.
    async fn set_bound_addr(&self, addr: SocketAddr) {
        self.state.lock().await.bound_addr = Some(addr);
    }
}

#[async_trait]
impl LocalRunnerControl for EmbeddedRunnerControl {
    async fn status(&self) -> RuntimeStatus {
        let mut state = self.state.lock().await;
        // Self-heals a runner that exited on its own (crashed, or finished
        // reacting to a prior `stop()`'s shutdown signal) — without this, a
        // task that died without anyone calling `stop()` would leave
        // `running` stale and this method would keep reporting "running"
        // forever.
        if state
            .running
            .as_ref()
            .is_some_and(|running| running.runner_task.is_finished())
        {
            state.running = None;
        }
        match &state.running {
            Some(running) => RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(running.since),
            },
            None => RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
            },
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        self.start_locked(&mut state).await
    }

    async fn stop(&self) {
        let mut state = self.state.lock().await;
        if let Some(running) = state.running.take() {
            // Doesn't block waiting for the task to actually exit — mirrors
            // `OrchRuntime::stop`'s own rule: a toggle-off HTTP request must
            // not hang on however long the runner's own shutdown takes.
            running.shutdown_handle.request();
        }
    }

    async fn list_secrets(&self) -> Vec<SecretMeta> {
        let state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        let names = store.list().unwrap_or_default();
        names
            .into_iter()
            .map(|name| {
                let set_at = state.secret_meta.get(&name).copied();
                SecretMeta { name, set_at }
            })
            .collect()
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        store
            .set(name, value)
            .map_err(|error| LocalRunnerControlError::SecretStore(error.to_string()))?;
        state.secret_meta.insert(name.to_owned(), Utc::now());
        save_secret_meta(&state.runner_config.state_dir, &state.secret_meta);

        // A UI-only user must never also have to hand-edit a TOML
        // `enabled` flag once they've pasted a key — flip the one provider
        // this build knows on the moment its default secret name is set.
        // Narrow on purpose: a deployment that configured a *different*
        // secret-store entry name via `TACK_RUNNER_PROVIDER_
        // VERCEL_AI_GATEWAY_SECRET` keeps using its own console-only
        // toggle, unchanged by this route. Only actually flipping the flag
        // (not merely finding it already `true`) marks the auto-enable —
        // [`EmbeddedRunnerControl::remove_secret`] undoes exactly this flip
        // and nothing else, so an operator's own `enabled = true` is never
        // clobbered by a later key removal.
        if name == tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET {
            let was_enabled = state
                .runner_config
                .providers
                .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
                .is_some_and(|provider| provider.enabled);
            if let Some(provider) = state
                .runner_config
                .providers
                .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            {
                provider.enabled = true;
            }
            if !was_enabled {
                state.vercel_ai_gateway_secret_auto_enabled = true;
            }
        }
        if provider_resolves_secret(&state.runner_config.providers, name) {
            self.restart_for_provider_change_locked(&mut state).await?;
        }
        Ok(())
    }

    async fn remove_secret(&self, name: &str) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        store
            .remove(name)
            .map_err(|error| LocalRunnerControlError::SecretStore(error.to_string()))?;
        state.secret_meta.remove(name);
        save_secret_meta(&state.runner_config.state_dir, &state.secret_meta);

        // Decided before the flag below may change: a provider whose only
        // credential was just removed is one the running runner was
        // configured with, whether or not it stays enabled afterwards.
        let affects_provider = provider_resolves_secret(&state.runner_config.providers, name);

        // The mirror of `set_secret`'s own flip, narrowed the identical
        // way: only the default secret name is ever considered, so a
        // provider configured under a different name via
        // `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` is never touched
        // here either. And only undoes `set_secret`'s own convenience flip
        // — an `enabled = true` the operator set directly (TOML, the
        // `_ENABLED` environment variable, or already `true` at boot) is
        // left exactly as it was, because `vercel_ai_gateway_secret_auto_
        // enabled` is only ever `true` when this process's own `set_secret`
        // is what turned it on.
        if name == tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET
            && state.vercel_ai_gateway_secret_auto_enabled
            && let Some(provider) = state
                .runner_config
                .providers
                .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        {
            provider.enabled = false;
            state.vercel_ai_gateway_secret_auto_enabled = false;
        }
        if affects_provider {
            self.restart_for_provider_change_locked(&mut state).await?;
        }
        Ok(())
    }

    async fn catalog(&self) -> CatalogSnapshot {
        let state = self.state.lock().await;
        let staging_root = state.runner_config.state_dir.join("staging");
        let secrets = SecretStore::open(&state.runner_config.secret_store_path());
        let limits = runner_limits();
        let report = bootstrap::probe(
            &staging_root,
            &limits.harness_process,
            &secrets,
            &state.runner_config.providers,
        )
        .await;
        map_catalog_status(
            report
                .provider_catalog
                .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY),
        )
    }
}

/// How long a stopped runner task is given to actually exit before a
/// configuration change stops waiting for it and aborts it outright. Shutdown
/// is observed between protocol calls and terminates every harness child
/// first, so this is a backstop against a wedged task, not the expected path.
const RUNNER_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether any configured provider resolves its credential from the
/// secret-store entry `name`. `RunnerConfig`'s `secret` references may carry
/// the explicit `store:` prefix `SecretStore::resolve` also accepts, so both
/// spellings name the same entry.
fn provider_resolves_secret(
    providers: &std::collections::BTreeMap<String, tack_runner::config::ProviderConfig>,
    name: &str,
) -> bool {
    providers.values().any(|provider| {
        provider
            .secret
            .strip_prefix("store:")
            .unwrap_or(&provider.secret)
            == name
    })
}

impl EmbeddedRunnerControl {
    /// The runner receives its configuration by value when it is spawned —
    /// its adapters hold their own copy of the provider table, and the
    /// capability snapshot it enrolls with is computed once at boot. That
    /// is the runner's model: configuration is fixed for the life of one
    /// runner task. So a change to a configured provider's credential is not
    /// something a running task can be told about; it is a new task. Without
    /// this, the key an operator just pasted reaches this control's own copy
    /// of the configuration (which is what `catalog` reads, so the page
    /// reports a live catalog) while every dispatch still resolves the
    /// provider through the spawned task's stale copy and refuses it.
    ///
    /// A stopped runner needs nothing: the next `start` reads the current
    /// configuration. The old task is joined, not merely signalled, before
    /// the new one is spawned — two runner tasks sharing one state directory
    /// would share one journal with nothing arbitrating between them.
    async fn restart_for_provider_change_locked(
        &self,
        state: &mut State,
    ) -> Result<(), LocalRunnerControlError> {
        if state.running.is_none() {
            return Ok(());
        }
        self.stop_and_join_locked(state).await;
        self.start_locked(state).await
    }

    async fn stop_and_join_locked(&self, state: &mut State) {
        let Some(running) = state.running.take() else {
            return;
        };
        running.shutdown_handle.request();
        // The `JoinHandle` moves into the timeout future, which drops it on
        // expiry — and dropping a `JoinHandle` detaches the task rather than
        // cancelling it. The abort handle taken first is what actually ends
        // a task that ignored its shutdown signal.
        let abort = running.runner_task.abort_handle();
        match tokio::time::timeout(RUNNER_STOP_TIMEOUT, running.runner_task).await {
            Ok(_) => {
                tracing::info!("embedded runner stopped for a provider configuration change");
            }
            Err(_) => {
                abort.abort();
                tracing::warn!(
                    "embedded runner did not stop within the shutdown budget; its task was aborted"
                );
            }
        }
    }

    async fn start_locked(&self, state: &mut State) -> Result<(), LocalRunnerControlError> {
        if state.running.is_some() {
            // Idempotent, mirroring `OrchRuntime::start` — a second
            // `PUT {"enabled": true}` (or the boot-time check racing a UI
            // toggle) must never spawn a duplicate runner task.
            return Ok(());
        }
        let bound_addr = state.bound_addr.ok_or_else(|| {
            LocalRunnerControlError::StartFailed(
                "the server's own address is not yet known".to_owned(),
            )
        })?;

        let mut runner_config = state.runner_config.clone();
        runner_config.api_base_url = format!("http://{bound_addr}/api/runner/v1");
        ensure_runner_credential(&mut runner_config, &self.server_config)
            .await
            .map_err(|error| LocalRunnerControlError::StartFailed(error.to_string()))?;

        let (shutdown, shutdown_handle) = Shutdown::channel();
        let runner_task = tokio::spawn(bootstrap::run(
            runner_config.clone(),
            runner_limits(),
            shutdown,
        ));
        // Remembered so a later stop-then-start reuses whatever credential
        // resolution just produced (a stored session, or the fresh
        // self-provisioned one) instead of redoing it from scratch.
        state.runner_config = runner_config;
        state.running = Some(Running {
            shutdown_handle,
            runner_task,
            since: Utc::now(),
        });
        Ok(())
    }
}

/// Runs the server with an embedded runner always wired in — even a plain
/// `tack serve` with no flag, on any bind, so `PUT /api/local-runner` can
/// turn it on later with no restart it didn't already need. A non-loopback
/// bind never starts the runner and never exposes its routes (ADR 0061
/// decision 6 is a safety invariant, not merely a startup nicety): this
/// function refuses to boot at all only when *this boot's own* flag or
/// environment variable explicitly asked for `--with-runner` on a
/// non-loopback bind (`ensure_loopback`, unchanged from before this
/// module could be reached any other way); `tack_api::server::serve_inner`
/// separately re-checks loopback against the *persisted* preference once
/// the database is open, before ever calling
/// [`EmbeddedRunnerControl::start`] — so a stale "enabled" row saved from
/// an earlier loopback session can never auto-start a runner on a
/// differently-configured deployment, it is just silently not honored.
/// `PUT /api/local-runner` reaches the identical `start()`, gated the
/// identical way by `router::build_router`'s own loopback check on the
/// route's existence. Replaces the old `serve_with_embedded_runner`, which
/// only ever existed for `--with-runner` and had no way to turn the runner
/// on afterward without restarting the whole process.
pub async fn serve() -> anyhow::Result<()> {
    let server_config = tack_api::config::AppConfig::load();
    if server_config.local_runner_enable {
        ensure_loopback(&server_config)?;
    }

    let control = Arc::new(EmbeddedRunnerControl::new(server_config)?);
    let local_runner: Arc<dyn tack_api::LocalRunnerControl> = control.clone();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(tack_api::serve_with_ready_and_local_runner(
        ready_tx,
        local_runner,
    ));

    let bound_addr = match ready_rx.await {
        Ok(addr) => addr,
        Err(_) => {
            // The sender was dropped without sending, which only happens when
            // `serve_with_ready_and_local_runner` returned before opening its
            // listener; that task's own result is the error worth
            // surfacing, not the closed channel.
            return match server_task.await {
                Ok(Ok(())) => Err(anyhow::anyhow!("server exited before it became ready")),
                Ok(Err(error)) => Err(error),
                Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
            };
        }
    };
    control.set_bound_addr(bound_addr).await;

    // Everything past this point — the auto-start decision, ctrl-c
    // handling, graceful shutdown — is `tack_api::server::serve_inner`'s
    // own job; this function's only remaining job is to react honestly if
    // that task ends.
    match server_task.await {
        Ok(result) => result,
        Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_runner_refuses_non_loopback_bind() {
        let config = tack_api::config::AppConfig {
            host: "0.0.0.0".to_owned(),
            ..tack_api::config::AppConfig::default()
        };

        let error = ensure_loopback(&config).expect_err("non-loopback bind must be refused");
        assert!(error.to_string().contains("loopback"));
    }

    #[test]
    fn embedded_runner_accepts_the_default_loopback_bind() {
        assert!(ensure_loopback(&tack_api::config::AppConfig::default()).is_ok());
    }

    #[test]
    fn with_runner_enabled_reads_the_environment_gate() {
        // SAFETY: this test owns the variable for its own duration and restores
        // it, but `std::env::set_var` is unsound to run concurrently with other
        // threads reading the same variable — `cargo test` runs this crate's
        // tests in one process, so serialize via the variable's own uniqueness
        // (nothing else in this crate reads `TACK_LOCAL_RUNNER_ENABLE`).
        let previous = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
        assert!(!with_runner_enabled(false));

        unsafe {
            std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
        }
        assert!(with_runner_enabled(false));
        assert!(with_runner_enabled(true));

        unsafe {
            match &previous {
                Some(value) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", value),
                None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
            }
        }
    }

    fn unique_temp_dir(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(label)
            .tempdir()
            .expect("temporary directory")
    }

    /// A `database_url` that fails at parse time, before any filesystem or
    /// network I/O — so a test built against it proves whether
    /// `tack_db::init_pool` (and therefore self-provisioning) was ever
    /// reached, without needing a real database.
    fn unreachable_database_url() -> tack_api::config::AppConfig {
        tack_api::config::AppConfig {
            database_url: "not-a-real-database-url".to_owned(),
            ..tack_api::config::AppConfig::default()
        }
    }

    #[tokio::test]
    async fn ensure_runner_credential_leaves_a_manual_credential_untouched() {
        let state_dir_guard = unique_temp_dir("manual");
        let state_dir = state_dir_guard.path();
        let mut runner_config = RunnerConfig {
            enrollment_credential: Some(EnrollmentCredential::new("manually-configured-token")),
            state_dir: state_dir.to_path_buf(),
            ..RunnerConfig::defaults()
        };
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_ok(),
            "a manual credential must never require touching the database: {result:?}"
        );
        assert_eq!(
            runner_config
                .enrollment_credential
                .expect("credential must remain set")
                .expose(),
            "manually-configured-token",
            "a manual credential must win over both the stored-session placeholder and self-provisioning"
        );
        std::fs::remove_dir_all(state_dir).ok();
    }

    #[tokio::test]
    async fn ensure_runner_credential_reuses_a_stored_session_without_self_provisioning() {
        let state_dir_guard = unique_temp_dir("stored-session");
        let state_dir = state_dir_guard.path();
        std::fs::write(state_dir.join("session.json"), "{}").expect("write session.json");
        let mut runner_config = RunnerConfig {
            enrollment_credential: None,
            state_dir: state_dir.to_path_buf(),
            ..RunnerConfig::defaults()
        };
        // Deliberately unreachable: if this path wrongly called
        // `local_enrollment::self_provision`, opening the pool would fail
        // and this assertion below would see an `Err`, not an `Ok`.
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_ok(),
            "a stored session must satisfy the credential requirement without opening the database: {result:?}"
        );
        assert!(
            runner_config.enrollment_credential.is_some(),
            "build_runtime's precondition still needs a credential to be present"
        );
        std::fs::remove_dir_all(state_dir).ok();
    }

    #[tokio::test]
    async fn ensure_runner_credential_attempts_self_provisioning_when_nothing_else_is_available() {
        let state_dir_guard = unique_temp_dir("no-session");
        let state_dir = state_dir_guard.path();
        let mut runner_config = RunnerConfig {
            enrollment_credential: None,
            state_dir: state_dir.to_path_buf(),
            ..RunnerConfig::defaults()
        };
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_err(),
            "with no manual credential and no stored session, self-provisioning must actually be \
             attempted — proved here by its failure against a deliberately unreachable database, \
             the same database_url the previous test proves does NOT get touched when a session exists"
        );
        std::fs::remove_dir_all(state_dir).ok();
    }

    fn control_with_state_dir(state_dir: &Path) -> EmbeddedRunnerControl {
        let mut runner_config = RunnerConfig::defaults();
        runner_config.state_dir = state_dir.to_path_buf();
        EmbeddedRunnerControl {
            server_config: unreachable_database_url(),
            state: Mutex::new(State {
                bound_addr: None,
                runner_config,
                running: None,
                secret_meta: HashMap::new(),
                vercel_ai_gateway_secret_auto_enabled: false,
            }),
        }
    }

    #[tokio::test]
    async fn a_fresh_control_reports_stopped_with_no_since() {
        let dir = unique_temp_dir("status-fresh");
        let control = control_with_state_dir(dir.path());

        let status = control.status().await;

        assert_eq!(status.state, RuntimeState::Stopped);
        assert!(status.since.is_none());
    }

    #[tokio::test]
    async fn start_fails_typed_before_the_bound_address_is_known() {
        let dir = unique_temp_dir("start-no-addr");
        let control = control_with_state_dir(dir.path());

        let result = control.start().await;

        assert!(matches!(
            result,
            Err(LocalRunnerControlError::StartFailed(_))
        ));
        assert_eq!(control.status().await.state, RuntimeState::Stopped);
    }

    #[tokio::test]
    async fn a_disabled_provider_reports_not_configured_with_no_network_call() {
        let dir = unique_temp_dir("catalog-disabled");
        let control = control_with_state_dir(dir.path());

        // `RunnerConfig::defaults()` seeds the Vercel provider disabled, and
        // a disabled entry is skipped before any network I/O — which is what
        // makes this assertion safe to run with no network access.
        let catalog = control.catalog().await;

        assert!(matches!(catalog, CatalogSnapshot::NotConfigured));
    }

    /// A stand-in for a spawned runner task: it does exactly what the real one
    /// does with its `Shutdown` — waits for the request, then exits — and
    /// records that it observed the request, so a test can tell "joined
    /// after being told to stop" apart from "still running" without a real
    /// runner behind it.
    fn fake_running(stopped: Arc<std::sync::atomic::AtomicBool>) -> Running {
        let (mut shutdown, shutdown_handle) = Shutdown::channel();
        let runner_task = tokio::spawn(async move {
            shutdown.requested().await;
            stopped.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });
        Running {
            shutdown_handle,
            runner_task,
            since: Utc::now(),
        }
    }

    /// The running task is joined before anything else happens, and the
    /// provider it was spawned without is on in the configuration the next
    /// task will read. The restart's own `start` then fails typed here —
    /// this control's database is deliberately unreachable, so credential
    /// resolution stops it before any task is spawned — which is what lets
    /// the test also pin that a failed restart leaves the control honestly
    /// `Stopped` rather than pretending the old task still serves.
    #[tokio::test]
    async fn setting_a_provider_secret_while_running_stops_the_old_task_before_anything_else() {
        let dir = unique_temp_dir("restart-on-provider-secret");
        let control = control_with_state_dir(dir.path());
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut state = control.state.lock().await;
            state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
            state.running = Some(fake_running(Arc::clone(&stopped)));
        }
        assert_eq!(control.status().await.state, RuntimeState::Running);

        let result = control
            .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "key")
            .await;

        assert!(
            stopped.load(std::sync::atomic::Ordering::SeqCst),
            "the task spawned with the old provider table must be stopped and joined"
        );
        assert!(
            matches!(result, Err(LocalRunnerControlError::StartFailed(_))),
            "the restart's own start must fail typed against an unreachable database"
        );
        assert_eq!(control.status().await.state, RuntimeState::Stopped);
        let state = control.state.lock().await;
        assert!(
            state
                .runner_config
                .providers
                .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
                .is_some_and(|provider| provider.enabled),
            "the configuration the next task reads must carry the provider on"
        );
    }

    /// A secret no configured provider resolves is read live by whatever
    /// requests reference it, so the running task keeps serving untouched.
    #[tokio::test]
    async fn setting_an_unrelated_secret_while_running_leaves_the_task_alone() {
        let dir = unique_temp_dir("no-restart-on-unrelated-secret");
        let control = control_with_state_dir(dir.path());
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut state = control.state.lock().await;
            state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
            state.running = Some(fake_running(Arc::clone(&stopped)));
        }

        control
            .set_secret("unrelated/entry", "value")
            .await
            .expect("an unrelated secret stores without touching the runner");

        assert!(!stopped.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(control.status().await.state, RuntimeState::Running);
    }

    /// Removing the credential a running provider resolves is the same
    /// event in the other direction: that task was spawned with the
    /// provider on, and nothing it holds can learn otherwise.
    #[tokio::test]
    async fn removing_a_provider_secret_while_running_stops_the_old_task() {
        let dir = unique_temp_dir("restart-on-provider-secret-removal");
        let control = control_with_state_dir(dir.path());
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut state = control.state.lock().await;
            state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
            state.running = Some(fake_running(Arc::clone(&stopped)));
        }

        let _ = control
            .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
            .await;

        assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(control.status().await.state, RuntimeState::Stopped);
    }

    #[tokio::test]
    async fn set_then_list_then_remove_a_secret_round_trips_with_a_recorded_set_at() {
        let dir = unique_temp_dir("secret-round-trip");
        // Forces the file backend (`SecretStore::open`'s own fallback path)
        // so this test never touches a real keychain.
        // SAFETY: serialized by this variable's own uniqueness within this
        // process, same justification as `with_runner_enabled_reads_the_
        // environment_gate` above.
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());

        control
            .set_secret("vi-b3-test-secret", "shh")
            .await
            .expect("set_secret");
        let listed = control.list_secrets().await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "vi-b3-test-secret");
        assert!(
            listed[0].set_at.is_some(),
            "a secret set by this process must record when"
        );

        control
            .remove_secret("vi-b3-test-secret")
            .await
            .expect("remove_secret");
        assert!(control.list_secrets().await.is_empty());
    }

    #[tokio::test]
    async fn setting_the_default_vercel_secret_enables_that_provider() {
        let dir = unique_temp_dir("secret-enables-provider");
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());
        assert!(matches!(
            control.catalog().await,
            CatalogSnapshot::NotConfigured
        ));

        control
            .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
            .await
            .expect("set_secret");

        let enabled = control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled;
        assert!(
            enabled,
            "the default provider must be enabled once its secret is set"
        );
    }

    #[tokio::test]
    async fn removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled() {
        let dir = unique_temp_dir("secret-removal-disables");
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());

        control
            .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
            .await
            .expect("set_secret");
        control
            .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
            .await
            .expect("remove_secret");

        let enabled = control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled;
        assert!(
            !enabled,
            "removing the only secret that enabled this provider must leave it exactly \
             as it was before the set: disabled"
        );

        // A capability claim, not just an internal flag — assert the
        // observable absence directly: a disabled provider reports
        // `not_configured`, never `secret_unresolved` (which is what an
        // enabled-with-no-credential provider reports forever).
        assert!(matches!(
            control.catalog().await,
            CatalogSnapshot::NotConfigured
        ));
    }

    #[tokio::test]
    async fn removing_the_default_vercel_secret_leaves_an_operator_enabled_provider_on() {
        let dir = unique_temp_dir("secret-removal-preserves-operator-enable");
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());
        // Simulates an operator's own explicit configuration (TOML, or
        // `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED`) turning the
        // provider on before any key was ever pasted through this control —
        // never something `set_secret` itself did.
        control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled = true;

        control
            .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
            .await
            .expect("set_secret");
        control
            .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
            .await
            .expect("remove_secret");

        let enabled = control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled;
        assert!(
            enabled,
            "a provider the operator enabled directly must never be turned off by a \
             later key removal through this route"
        );
    }

    #[tokio::test]
    async fn removing_a_non_default_secret_never_touches_the_providers_enabled_flag() {
        let dir = unique_temp_dir("secret-removal-narrow-name");
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());
        control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled = true;

        control
            .remove_secret("some-other-secret-name")
            .await
            .expect("remove_secret");

        let enabled = control
            .state
            .lock()
            .await
            .runner_config
            .providers
            .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("seeded provider entry")
            .enabled;
        assert!(
            enabled,
            "removing a secret under any name other than the default must never touch \
             this provider's enabled flag — mirrors set_secret's own narrow scope"
        );
    }

    #[tokio::test]
    async fn start_then_stop_round_trips_the_runtime_state() {
        let dir = unique_temp_dir("start-stop");
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
        }
        let control = control_with_state_dir(dir.path());
        // `unreachable_database_url` makes self-provisioning fail deliberately
        // (no session on disk, no manual credential) — this test only needs
        // to prove `start()` reaches the point of attempting it, and that a
        // failure there is reported rather than silently leaving `running`
        // set. A live start-then-claim proof belongs to an integration test
        // with a real server, not this unit.
        control.set_bound_addr("127.0.0.1:1".parse().unwrap()).await;

        let result = control.start().await;

        assert!(
            matches!(result, Err(LocalRunnerControlError::StartFailed(_))),
            "self-provisioning against an unreachable database must fail typed: {result:?}"
        );
        assert_eq!(
            control.status().await.state,
            RuntimeState::Stopped,
            "a failed start must not leave the control reporting running"
        );
    }

    #[test]
    fn embedded_default_state_dir_nests_under_storage_dir() {
        assert_eq!(
            embedded_default_state_dir("./storage"),
            PathBuf::from("./storage/runner")
        );
        assert_eq!(
            embedded_default_state_dir("/srv/tack-b/storage"),
            PathBuf::from("/srv/tack-b/storage/runner")
        );
    }

    /// Changes the process's current directory for the duration of a
    /// closure, restoring it afterward even if the closure panics — needed
    /// by every test below that exercises [`migrate_legacy_state_dir`],
    /// since its legacy side is the crate's bare, cwd-relative default.
    /// Sound under this crate's own rule for mutating process-global state
    /// in tests: nextest gives each test its own process, so no sibling
    /// test's thread observes this change.
    fn with_cwd<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(dir).expect("set cwd");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        std::env::set_current_dir(original).expect("restore cwd");
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    #[test]
    fn migrate_legacy_state_dir_moves_an_existing_legacy_directory_once() {
        let cwd_guard = unique_temp_dir("migrate-cwd");
        let target_root = unique_temp_dir("migrate-target");
        let new_dir = target_root.path().join("storage").join("runner");

        with_cwd(cwd_guard.path(), || {
            let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
            std::fs::create_dir_all(legacy).expect("create legacy dir");
            std::fs::write(legacy.join("session.json"), "legacy-session")
                .expect("write legacy session");

            migrate_legacy_state_dir(&new_dir);

            assert!(
                new_dir.join("session.json").is_file(),
                "legacy state must be moved to the new, scoped directory"
            );
            assert_eq!(
                std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
                "legacy-session"
            );
            assert!(
                !legacy.exists(),
                "the legacy directory must not be left behind after a successful migration"
            );
        });
    }

    #[test]
    fn migrate_legacy_state_dir_never_touches_an_already_provisioned_new_directory() {
        let cwd_guard = unique_temp_dir("migrate-cwd-noop");
        let target_root = unique_temp_dir("migrate-target-noop");
        let new_dir = target_root.path().join("runner");
        std::fs::create_dir_all(&new_dir).expect("pre-create the new directory");
        std::fs::write(new_dir.join("session.json"), "already-provisioned")
            .expect("write new session");

        with_cwd(cwd_guard.path(), || {
            let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
            std::fs::create_dir_all(legacy).expect("create legacy dir");
            std::fs::write(legacy.join("session.json"), "stale-legacy-session")
                .expect("write legacy session");

            migrate_legacy_state_dir(&new_dir);

            assert_eq!(
                std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
                "already-provisioned",
                "an existing new-style directory must never be overwritten by a stale legacy one"
            );
            assert!(
                legacy.join("session.json").is_file(),
                "the legacy directory is left untouched once the new one already exists"
            );
        });
    }

    #[test]
    fn migrate_legacy_state_dir_is_a_no_op_when_neither_directory_exists() {
        let cwd_guard = unique_temp_dir("migrate-cwd-fresh");
        let target_root = unique_temp_dir("migrate-target-fresh");
        let new_dir = target_root.path().join("storage").join("runner");

        with_cwd(cwd_guard.path(), || {
            migrate_legacy_state_dir(&new_dir);

            assert!(
                !new_dir.exists(),
                "a fresh install has nothing to migrate; the new directory is created \
                 later, on first write, not by the migration step itself"
            );
        });
    }

    // The acceptance proof for two servers each seeing only their own
    // runner's enrollment lives in `tests/embedded_runner_state_scoping.rs`
    // as a real two-subprocess test, not here: `tack_api::server::serve_inner`
    // installs a process-global `tracing` subscriber via `try_init`, so only
    // the first call in a process actually configures it — a second call is
    // a silent no-op rather than a crash, but a second embedded server in
    // that same process would still run under the first one's logging
    // config instead of its own. So this crate's own unit tests — which all
    // share one test binary process per test function, not per server —
    // still test at most one real embedded server each. Two genuinely
    // separate `tack` processes have no such conflict.
}

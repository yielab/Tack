//! Shared executable locator for both harness adapters.
//!
//! A plain `PATH` search is correct when the runner is started from an
//! interactive shell, and wrong for the two other ways it starts: the
//! desktop app launched from a `.desktop` entry, Finder or the Start menu,
//! and `tack service` under systemd's user manager. Both inherit a minimal
//! session `PATH` that does not contain where `claude` and `codex` actually
//! live on most machines, so a real install reads as "not installed".
//!
//! [`locate`] searches `PATH` first (so a shell-launched runner behaves as a
//! shell would) and then a fixed, documented list of per-user install
//! locations. It is pure over its arguments — no `std::env` read —
//! so tests exercise it without touching the process environment; the only
//! process-environment read in this crate for this purpose is
//! [`locate_installed`], the thin impure wrapper both adapters call.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `program` was not found on `PATH` or under any well-known install
/// location. Carries every directory that was actually searched, so the
/// caller's error text can name them instead of just saying "not found".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    program: String,
    searched: Vec<PathBuf>,
}

impl NotFound {
    /// Every directory [`locate`] checked, in search order, before giving
    /// up. Exposed mainly so tests can assert against it without parsing
    /// the `Display` text.
    pub fn searched(&self) -> &[PathBuf] {
        &self.searched
    }
}

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` was not found on PATH", self.program)?;
        if self.searched.is_empty() {
            return Ok(());
        }
        write!(f, " or in: ")?;
        for (index, dir) in self.searched.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", dir.display())?;
        }
        Ok(())
    }
}

impl std::error::Error for NotFound {}

/// Searches `path` (a `PATH`-style, `:`/`;`-separated list, already
/// snapshotted by the caller) for an executable named `program`, then falls
/// back to a fixed list of well-known per-user install directories derived
/// from `home`. First hit wins: a machine whose shell can already see the
/// install behaves exactly as before this locator existed.
///
/// On Unix, a candidate must also carry the executable bit — a `PATH` or
/// fallback entry can legitimately hold a non-executable file (a stale
/// download, a doc), and that must not shadow a real install found through
/// the next remaining entry. Symlinks (nvm installs are symlinks) are
/// canonicalized so the resolved path is stable across the nvm version they
/// currently point at.
pub fn locate(
    program: &str,
    path: Option<&OsStr>,
    home: Option<&Path>,
) -> Result<PathBuf, NotFound> {
    let mut searched = Vec::new();

    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            // An empty `PATH` entry (a leading, trailing or doubled `:`, or
            // the whole variable being empty) means "the current directory"
            // to a shell. It is skipped here on purpose: the runner's working
            // directory is not a place an operator installs a harness, and
            // resolving an executable from it would let whatever sits there
            // stand in for one.
            if dir.as_os_str().is_empty() {
                continue;
            }
            if let Some(found) = check_dir(&dir, program) {
                return Ok(found);
            }
            searched.push(dir);
        }
    }

    for dir in well_known_dirs(home) {
        if let Some(found) = check_dir(&dir, program) {
            return Ok(found);
        }
        searched.push(dir);
    }

    Err(NotFound {
        program: program.to_string(),
        searched,
    })
}

/// Impure convenience wrapper: snapshots the runner process's own `PATH`
/// and home directory once and calls [`locate`]. What `claude_code.rs`'s
/// `discover()` calls, since it resolves once and caches the result.
pub fn locate_installed(program: &str) -> Result<PathBuf, NotFound> {
    let (path, home) = snapshot();
    locate(program, path.as_deref(), home.as_deref())
}

/// Reads the runner process's own `PATH` and home directory once. What
/// `codex.rs` calls at adapter construction, since it re-resolves on every
/// spawn from the snapshot rather than re-reading the environment each
/// time. The only place in this crate that reads `PATH` or a home-directory
/// variable for this purpose.
pub fn snapshot() -> (Option<std::ffi::OsString>, Option<PathBuf>) {
    (std::env::var_os("PATH"), home_dir())
}

#[cfg(unix)]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(windows)]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

fn check_dir(dir: &Path, program: &str) -> Option<PathBuf> {
    let candidate = dir.join(program);
    if !candidate.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Ok(metadata) = std::fs::metadata(&candidate) else {
            return None;
        };
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(candidate.canonicalize().unwrap_or(candidate))
}

/// The fixed, documented fallback list. Not configurable — a `TACK_*` knob
/// for it would be a flag whose off-state nothing exercises. Order: generic
/// user-local bins first, then the version-managed installers, then the
/// platform package managers.
fn well_known_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = home {
        // Many installers' `--user`/default per-user location (pipx, uv
        // tool installs, and several npm-alternative tools).
        dirs.push(home.join(".local/bin"));
        // `cargo install`, including a Rust-distributed harness wrapper.
        dirs.push(home.join(".cargo/bin"));
        // `bun install -g` and Bun's own installer.
        dirs.push(home.join(".bun/bin"));
        // npm configured with a user-writable global prefix
        // (`npm config set prefix ~/.npm-global`), a common fix for the
        // "EACCES" error npm gives on the system-wide default.
        dirs.push(home.join(".npm-global/bin"));
        dirs.push(home.join(".npm/bin"));
        // nvm never symlinks a version-independent "current" path outside
        // a sourced shell rc, so every installed Node version's own bin
        // directory is a candidate.
        dirs.extend(nvm_bin_dirs(home));
    }

    #[cfg(unix)]
    {
        // Homebrew on Apple Silicon.
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        // Homebrew on Intel macOS, and Linuxbrew's default prefix.
        dirs.push(PathBuf::from("/usr/local/bin"));
    }

    #[cfg(windows)]
    {
        // npm's global install location on Windows.
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
        // nvm-windows's install root.
        if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local_appdata).join("nvm"));
        }
    }

    dirs
}

fn nvm_bin_dirs(home: &Path) -> Vec<PathBuf> {
    let versions_dir = home.join(".nvm/versions/node");
    let Ok(entries) = std::fs::read_dir(&versions_dir) else {
        return Vec::new();
    };
    let mut versions: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    versions.sort();
    versions
        .into_iter()
        .map(|version| version.join("bin"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn write_file(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    }

    fn temp_home(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(label)
            .tempdir()
            .expect("temporary directory")
    }

    #[test]
    fn found_on_path_wins_over_a_fallback_dir() {
        let home = temp_home("locate-path-wins");
        let path_dir = home.path().join("on-path");
        let path_binary = path_dir.join("claude");
        write_file(&path_binary, "#!/bin/sh\n");
        #[cfg(unix)]
        make_executable(&path_binary);

        let fallback_binary = home.path().join(".local/bin/claude");
        write_file(&fallback_binary, "#!/bin/sh\n");
        #[cfg(unix)]
        make_executable(&fallback_binary);

        let path = std::env::join_paths([&path_dir]).expect("join paths");
        let found =
            locate("claude", Some(path.as_os_str()), Some(home.path())).expect("found on PATH");
        assert_eq!(found, path_binary.canonicalize().expect("canonicalize"));
    }

    #[test]
    fn found_only_under_local_bin_with_an_empty_path() {
        let home = temp_home("locate-fallback");
        let fallback_binary = home.path().join(".local/bin/claude");
        write_file(&fallback_binary, "#!/bin/sh\n");
        #[cfg(unix)]
        make_executable(&fallback_binary);

        let empty_path = std::ffi::OsStr::new("");
        let found =
            locate("claude", Some(empty_path), Some(home.path())).expect("found in fallback dir");
        assert_eq!(found, fallback_binary.canonicalize().expect("canonicalize"));
    }

    #[test]
    fn not_found_names_every_directory_searched() {
        let home = temp_home("locate-not-found");
        let path_dir = home.path().join("on-path");
        std::fs::create_dir_all(&path_dir).expect("mkdir");
        let path = std::env::join_paths([&path_dir]).expect("join paths");

        let error = locate("claude", Some(path.as_os_str()), Some(home.path()))
            .expect_err("claude is nowhere in this fixture");

        let searched = error.searched();
        assert!(
            searched.contains(&path_dir),
            "expected the PATH entry {path_dir:?} in {searched:?}"
        );
        assert!(
            searched.contains(&home.path().join(".local/bin")),
            "expected the fallback dir in {searched:?}"
        );
        let text = error.to_string();
        assert!(text.contains("claude"));
        assert!(text.contains(&path_dir.display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_executable_file_in_a_fallback_dir_is_skipped() {
        use std::os::unix::fs::PermissionsExt;

        let home = temp_home("locate-non-executable");
        let non_executable = home.path().join(".local/bin/claude");
        write_file(&non_executable, "#!/bin/sh\n");
        std::fs::set_permissions(&non_executable, std::fs::Permissions::from_mode(0o644))
            .expect("chmod");

        let empty_path = std::ffi::OsStr::new("");
        let error = locate("claude", Some(empty_path), Some(home.path()))
            .expect_err("non-executable file must not count as found");
        assert!(error.searched().contains(&home.path().join(".local/bin")));
    }

    #[test]
    fn nothing_panics_when_home_is_none() {
        let empty_path = std::ffi::OsStr::new("");
        let error = locate("claude", Some(empty_path), None).expect_err("nothing to find");
        assert!(!error.to_string().is_empty());
    }
}

use super::*;
use std::fs;
use std::path::Path;

/// Working directory that removes itself when the returned guard drops,
/// so a failed assertion leaves nothing behind either.
fn workdir(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(tag)
        .tempdir()
        .expect("temporary directory")
}

#[test]
fn a_configured_log_path_creates_its_directory_and_splits_into_dir_and_name() {
    let dir_guard = workdir("log-target");
    let nested = dir_guard.path().join("logs").join("tack.log");
    assert!(!nested.parent().unwrap().exists());

    let (dir, name) = log_file_target(nested.to_str().unwrap()).expect("a usable target");

    assert_eq!(dir, nested.parent().unwrap());
    assert_eq!(name, "tack.log");
    assert!(
        dir.is_dir(),
        "the log directory must be created, not assumed"
    );
}

#[test]
fn a_bare_file_name_logs_beside_the_working_directory() {
    let (dir, name) = log_file_target("tack.log").expect("a usable target");
    assert_eq!(dir, Path::new("."));
    assert_eq!(name, "tack.log");
}

#[test]
fn a_path_naming_no_file_is_refused_rather_than_guessed() {
    assert!(log_file_target("/").is_none());
    assert!(log_file_target("..").is_none());
}

/// The whole point of the setting: after `init_tracing` has read a config
/// carrying a log path, a line logged through the global subscriber has to
/// land in that file. Asserting that a layer was constructed would have
/// passed for the entire time this setting silently did nothing. This test
/// installs the process-wide subscriber, so it must stay the only test in
/// this binary that does.
#[test]
fn a_configured_log_file_receives_the_lines_that_are_logged() {
    let dir_guard = workdir("log-write");
    let path = dir_guard.path().join("logs").join("tack.log");

    let config = AppConfig {
        log_file: Some(path.to_string_lossy().into_owned()),
        log_level: "info".into(),
        log_json: false,
        ..AppConfig::default()
    };
    init_tracing(&config);

    tracing::error!(marker = "written-to-the-file", "log file smoke line");

    let written = fs::read_to_string(&path).expect("the log file must exist");
    assert!(
        written.contains("written-to-the-file"),
        "the configured log file got no line; it holds: {written:?}"
    );
}

/// The DB swap succeeds but the storage swap fails — the whole
/// operation must roll back so the original DB is intact and bootable.
#[test]
fn restore_swap_rolls_back_when_storage_swap_fails() {
    let dir_guard = workdir("storage-fail");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();

    // A staging dir exists (so the storage step runs)…
    let storage_restore = dir.join("storage.restore");
    fs::create_dir_all(&storage_restore).unwrap();
    fs::write(storage_restore.join("a.bin"), b"attach").unwrap();

    // …but the live storage path sits under a MISSING parent, so promoting
    // the staging dir into place fails with ENOENT — after the DB swap.
    let storage_dir = dir.join("missing_parent").join("storage");

    let result = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS");
    assert!(result.is_err(), "storage promotion should fail");

    // Rolled back: the ORIGINAL DB is back in place and bootable.
    assert_eq!(fs::read(&db_path).unwrap(), b"ORIGINAL");
    // The staged restore file is put back for a future attempt.
    assert_eq!(fs::read(&restore_db).unwrap(), b"RESTORED");
    // No stray .bak left behind for the DB.
    assert!(!Path::new(&format!("{}.bak-TS", db_path.to_string_lossy())).exists());

    fs::remove_dir_all(dir).ok();
}

/// Happy path: DB + storage both swap, and a timestamped .bak is kept.
#[test]
fn restore_swap_promotes_db_and_storage() {
    let dir_guard = workdir("ok");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();

    let storage_dir = dir.join("storage");
    fs::create_dir_all(&storage_dir).unwrap();
    fs::write(storage_dir.join("old.bin"), b"old").unwrap();
    let storage_restore = dir.join("storage.restore");
    fs::create_dir_all(&storage_restore).unwrap();
    fs::write(storage_restore.join("new.bin"), b"new").unwrap();

    let baks = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS")
        .expect("swap should succeed");

    assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");
    assert!(storage_dir.join("new.bin").exists());
    assert!(!storage_dir.join("old.bin").exists());
    assert!(Path::new(&baks.db_bak).exists());
    assert!(baks.storage_bak.is_some());
    // The staging inputs are consumed.
    assert!(!restore_db.exists());
    assert!(!storage_restore.exists());

    fs::remove_dir_all(dir).ok();
}

/// Stale `-wal`/`-shm` sidecars of the replaced DB are deleted so SQLite
/// cannot replay them onto the freshly restored database.
#[test]
fn restore_swap_deletes_stale_wal_shm() {
    let dir_guard = workdir("wal");
    let dir = dir_guard.path();
    let db_path = dir.join("tack.db");
    let restore_db = dir.join("tack.db.restore");
    fs::write(&db_path, b"ORIGINAL").unwrap();
    fs::write(&restore_db, b"RESTORED").unwrap();
    fs::write(dir.join("tack.db-wal"), b"waljunk").unwrap();
    fs::write(dir.join("tack.db-shm"), b"shmjunk").unwrap();

    // No storage staging dir → storage step is skipped.
    let storage_dir = dir.join("storage");
    let storage_restore = dir.join("storage.restore");

    apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS").unwrap();

    assert!(
        !dir.join("tack.db-wal").exists(),
        "stale -wal must be deleted"
    );
    assert!(
        !dir.join("tack.db-shm").exists(),
        "stale -shm must be deleted"
    );
    assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");

    fs::remove_dir_all(dir).ok();
}

// Serializes the tests below that must mutate process env vars to steer
// `AppConfig::load()`, since env vars are process-global and `cargo
// test` runs this binary's tests concurrently. An async-aware mutex
// because the guard needs to stay held across this test's `.await`s.
static SERVE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Puts back whatever `TACK_PORT`/`TACK_DATABASE_URL` held before the
/// test ran, including on panic, so a failure here can't leak a stray
/// `TACK_PORT=0` into a test that runs after it in the same process.
struct EnvRestore {
    port: Option<String>,
    database_url: Option<String>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        // SAFETY: the caller holds `SERVE_ENV_LOCK` for the guard's
        // whole lifetime, so no other thread in this test binary reads
        // or writes these two vars concurrently with this restore.
        unsafe {
            match &self.port {
                Some(v) => std::env::set_var("TACK_PORT", v),
                None => std::env::remove_var("TACK_PORT"),
            }
            match &self.database_url {
                Some(v) => std::env::set_var("TACK_DATABASE_URL", v),
                None => std::env::remove_var("TACK_DATABASE_URL"),
            }
        }
    }
}

/// Acceptance case for the readiness seam: start the server through
/// `serve_with_ready`, wait on the oneshot (no sleep, no retry loop),
/// then make one real HTTP request against the address it reports.
/// Requesting port 0 forces an OS-assigned port, so a passing request
/// also proves the signaled address is the real one, not the guess a
/// caller would otherwise have to poll.
#[tokio::test]
async fn serve_with_ready_signals_the_real_bound_address() {
    let _env_guard = SERVE_ENV_LOCK.lock().await;
    let _restore = EnvRestore {
        port: std::env::var("TACK_PORT").ok(),
        database_url: std::env::var("TACK_DATABASE_URL").ok(),
    };
    // SAFETY: serialized by `SERVE_ENV_LOCK` above.
    unsafe {
        std::env::set_var("TACK_PORT", "0");
        std::env::set_var("TACK_DATABASE_URL", "sqlite::memory:");
    }

    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(serve_with_ready(ready_tx));

    let addr = ready_rx.await.expect("readiness signal never arrived");
    assert_ne!(
        addr.port(),
        0,
        "signaled address must be the real OS-assigned port, not the configured 0"
    );

    let response = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .expect("request against the signaled address must succeed");
    assert!(response.status().is_success());

    server.abort();
}

/// The unmodified `serve()` entry point must still exist and behave the
/// same as before this seam was added: no readiness channel, same boot
/// sequence. This doesn't run it (that's `cargo run -p tack-cli -- serve`,
/// verified manually), it just pins the call shape so `serve_inner`'s
/// `None` path can't silently drift from what `tack-cli` calls.
#[test]
fn serve_still_takes_no_arguments() {
    let _: fn() -> _ = serve;
}

/// A control whose `start()` records whether it was ever called — the
/// auto-start check under test in
/// `a_persisted_enable_preference_never_auto_starts_on_a_non_loopback_bind`
/// below is the only thing calling it in that test.
struct RecordingControl {
    started: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl LocalRunnerControl for RecordingControl {
    async fn status(&self) -> crate::handlers::local_runner::RuntimeStatus {
        crate::handlers::local_runner::RuntimeStatus {
            state: crate::handlers::local_runner::RuntimeState::Stopped,
            since: None,
        }
    }
    async fn start(&self) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) {}
    async fn list_secrets(&self) -> Vec<crate::handlers::local_runner::SecretMeta> {
        Vec::new()
    }
    async fn set_secret(
        &self,
        _name: &str,
        _value: &str,
    ) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
        Ok(())
    }
    async fn remove_secret(
        &self,
        _name: &str,
    ) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
        Ok(())
    }
    async fn catalog(&self) -> crate::handlers::local_runner::CatalogSnapshot {
        crate::handlers::local_runner::CatalogSnapshot::NotConfigured
    }
}

/// The regression this seam exists to prevent: a preference saved from
/// an earlier loopback session (here, simulated with
/// `TACK_LOCAL_RUNNER_ENABLE=1` — the env-default half of the same
/// precedence `app_meta` would otherwise win) must never auto-start an
/// embedded runner on a server now bound to `0.0.0.0`. Proved directly
/// against `RecordingControl::start`, not just against the route being
/// absent — a route being unreachable would not by itself prove the
/// runner never actually started.
#[tokio::test]
async fn a_persisted_enable_preference_never_auto_starts_on_a_non_loopback_bind() {
    let _env_guard = SERVE_ENV_LOCK.lock().await;
    let _restore = EnvRestore {
        port: std::env::var("TACK_PORT").ok(),
        database_url: std::env::var("TACK_DATABASE_URL").ok(),
    };
    let previous_host = std::env::var("TACK_HOST").ok();
    let previous_enable = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
    let previous_unauth = std::env::var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK").ok();
    // SAFETY: serialized by `SERVE_ENV_LOCK` above.
    unsafe {
        std::env::set_var("TACK_PORT", "0");
        std::env::set_var("TACK_DATABASE_URL", "sqlite::memory:");
        std::env::set_var("TACK_HOST", "0.0.0.0");
        std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
        std::env::set_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK", "1");
    }

    let control = Arc::new(RecordingControl {
        started: std::sync::atomic::AtomicBool::new(false),
    });
    let local_runner: Arc<dyn LocalRunnerControl> = control.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(serve_with_ready_and_local_runner(ready_tx, local_runner));

    let addr = ready_rx.await.expect("readiness signal never arrived");
    // The server itself must still come up normally — refusing to
    // *start the runner* is not refusing to *serve*.
    let response = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .expect("server must still boot and serve on a non-loopback bind");
    assert!(response.status().is_success());

    assert!(
        !control.started.load(std::sync::atomic::Ordering::SeqCst),
        "a non-loopback bind must never auto-start the embedded runner, \
         even with TACK_LOCAL_RUNNER_ENABLE=1"
    );

    server.abort();
    // SAFETY: still serialized by `SERVE_ENV_LOCK`.
    unsafe {
        match previous_host {
            Some(v) => std::env::set_var("TACK_HOST", v),
            None => std::env::remove_var("TACK_HOST"),
        }
        match previous_enable {
            Some(v) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", v),
            None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
        }
        match previous_unauth {
            Some(v) => std::env::set_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK", v),
            None => std::env::remove_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK"),
        }
    }
}

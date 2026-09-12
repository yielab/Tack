//! Acceptance proof that the embedded runner recovers when its persisted
//! credential (`storage_dir/runner/session.json`) names a runner id the
//! *current* database has no row for — deleting `tack.db` without also
//! deleting `storage_dir` beside it.
//!
//! One real `tack serve --with-runner` subprocess is started twice against
//! the same `storage_dir`, with the database file removed and recreated in
//! between. A single subprocess per boot, not two in-process servers in one
//! test: `tack_api::server::serve_inner` installs a process-global
//! `tracing` subscriber once per process.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

mod common;
use common::free_port;

struct ServerGuard {
    child: Child,
    base_url: String,
    log_lines: Arc<Mutex<Vec<String>>>,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Calls `attempt` every 50ms until it returns `Some`, or gives up once
/// `timeout` has elapsed since the first call, returning `None`.
fn poll_until<T>(timeout: Duration, mut attempt: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = attempt() {
            return Some(value);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Starts `tack serve --with-runner` against `database_url`/`storage_dir`
/// (both caller-owned, so a second boot in this test can point at the exact
/// same `storage_dir` a first boot already wrote into) and continuously
/// drains its stdout into `log_lines` on a background thread — draining as
/// it is produced, not after the process exits, so a verbose boot can never
/// fill the pipe buffer and deadlock the child on a blocked write.
fn start_server(database_url: &str, storage_dir: &Path) -> ServerGuard {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    let mut child = Command::new(env!("CARGO_BIN_EXE_tack"))
        .arg("serve")
        .arg("--with-runner")
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", port.to_string())
        .env("TACK_DATABASE_URL", database_url)
        .env("TACK_STORAGE_DIR", storage_dir)
        .env_remove("TACK_RUNNER_STATE_DIR")
        // This test's own claim is about the credential the embedded runner
        // redeems, never about harness discovery — narrowing `PATH` to
        // exclude `claude`/`codex` (wherever this machine's own shell finds
        // them) keeps discovery a cheap, immediate "not found" instead of a
        // real subprocess probe, which on a machine with both installed has
        // been measured (`tack runner doctor`, this same binary) at up to
        // ~24s on its own — irrelevant cost this test would otherwise pay
        // twice, once per boot.
        .env("PATH", "/usr/bin:/bin")
        // `SecretStore::open` (`tack-runner/src/secrets.rs`) tries a real
        // platform keychain (over D-Bus/secret-service on Linux) before
        // falling back to the owner-only file backend, and on a machine
        // with a live desktop session bus that first attempt can itself
        // take tens of seconds — again irrelevant to this test's own claim.
        // An invalid bus address makes the attempt fail immediately, the
        // same trick this crate's own unit tests already rely on (see
        // `local_runner.rs`'s `set_then_list_then_remove_a_secret_round_trips_
        // with_a_recorded_set_at`).
        .env("DBUS_SESSION_BUS_ADDRESS", "/dev/null")
        // Narrowed to the `tack` binary's own target (`local_runner.rs` and
        // `local_enrollment.rs` are modules of `main.rs`, not of the
        // `tack_cli` library crate, so their tracing target is `tack`, not
        // `tack_cli`): the default filter never enables it at all
        // (`tack_api::server::init_tracing` only turns on
        // `tack_api`/`tack_db`/`tack_core`/`tower_http`), and widening it
        // would make the exact-count assertion below fragile against noise
        // from targets this test does not care about.
        .env("RUST_LOG", "tack=info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tack serve --with-runner");

    let stdout = child.stdout.take().expect("piped stdout");
    let log_lines = Arc::new(Mutex::new(Vec::new()));
    let log_lines_writer = Arc::clone(&log_lines);
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            log_lines_writer.lock().expect("log lines lock").push(line);
        }
    });

    let mut guard = ServerGuard {
        child,
        base_url,
        log_lines,
    };
    wait_for_ready(&guard.base_url, &mut guard.child);
    guard
}

fn wait_for_ready(base_url: &str, child: &mut Child) {
    let client = reqwest::blocking::Client::new();
    let ready = poll_until(Duration::from_secs(15), || {
        if let Some(status) = child.try_wait().expect("poll child status") {
            panic!("tack serve --with-runner exited early during startup: {status}");
        }
        client
            .get(format!("{base_url}/api/health"))
            .timeout(Duration::from_millis(500))
            .send()
            .ok()
            .filter(|response| response.status().is_success())
            .map(|_| ())
    });
    if ready.is_none() {
        let _ = child.kill();
        panic!("tack serve --with-runner did not become ready within 15s");
    }
}

/// Polls `GET /api/runners` until exactly one row reports `state ==
/// "active"`, returning its `runner_id`. Bounded, not indefinite: a credential
/// that never resolves would stall here forever, which is the real failure
/// this test exists to catch. Generous rather than tight given this machine
/// runs several agents' builds and test suites concurrently.
fn wait_for_active_runner(base_url: &str) -> Option<String> {
    let client = reqwest::blocking::Client::new();
    poll_until(Duration::from_secs(30), || {
        let response = client
            .get(format!("{base_url}/api/runners"))
            .timeout(Duration::from_millis(500))
            .send()
            .ok()?;
        let body = response.json::<Value>().ok()?;
        let rows = body.get("data").and_then(Value::as_array)?;
        rows.iter()
            .find(|row| row.get("state").and_then(Value::as_str) == Some("active"))
            .and_then(|row| row.get("runner_id").and_then(Value::as_str))
            .map(str::to_owned)
    })
}

/// Reads `session.json` once it exists and holds `expected_runner_id` —
/// `store_session` (`tack_runner::transport`) writes it to disk *after* the
/// server's own enrollment response already flipped the row to `active`, so
/// a read taken the instant `wait_for_active_runner` returns can race a
/// write still in flight. Bounded the same way every other poll in this file
/// is: a session that never lands, or never updates, is the real failure.
fn wait_for_session_containing(path: &Path, expected_runner_id: &str) -> String {
    poll_until(Duration::from_secs(10), || {
        std::fs::read_to_string(path)
            .ok()
            .filter(|contents| contents.contains(expected_runner_id))
    })
    .unwrap_or_else(|| {
        panic!(
            "session.json at {} never came to hold runner id {expected_runner_id}",
            path.display()
        )
    })
}

fn remove_sqlite_file(database_path: &Path) {
    std::fs::remove_file(database_path).ok();
    for suffix in ["-wal", "-shm"] {
        let mut with_suffix = database_path.as_os_str().to_owned();
        with_suffix.push(suffix);
        std::fs::remove_file(PathBuf::from(with_suffix)).ok();
    }
}

/// Asserts the recovery was reported exactly once, at info, and that the
/// line carries none of the paths or runner ids involved.
fn assert_recovery_logged_once_with_no_identifiers(
    boot: &ServerGuard,
    database_path: &Path,
    storage_dir: &Path,
    runner_id_before: &str,
    runner_id_after: &str,
) {
    let lines = boot.log_lines.lock().expect("log lines lock").clone();
    let recovery_lines: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("no longer using"))
        .collect();
    assert_eq!(
        recovery_lines.len(),
        1,
        "the recovery must be reported exactly once, at info; saw: {lines:?}"
    );
    let line = recovery_lines[0];
    assert!(
        !line.contains(&database_path.display().to_string())
            && !line.contains(storage_dir.to_str().unwrap())
            && !line.contains(runner_id_before)
            && !line.contains(runner_id_after),
        "the recovery log line must carry no path or id: {line}"
    );
}

/// A `storage_dir` an earlier boot already enrolled a runner into, paired
/// with a database that was deleted and recreated underneath it rather than
/// reconfigured alongside it. The embedded runner must still reach `active`,
/// under a fresh identity, and must say so exactly once, at `info`, with no
/// path, id, or credential in the line.
#[test]
fn orphaned_credential_recovers_on_recreated_database() {
    let root = tempfile::Builder::new()
        .prefix("embedded-orphan-credential")
        .tempdir()
        .expect("root");
    let database_path = root.path().join("tack.db");
    let database_url = format!("sqlite:{}?mode=rwc", database_path.display());
    let storage_dir = root.path().join("storage");
    let state_dir = storage_dir.join("runner");

    let first_boot = start_server(&database_url, &storage_dir);
    let runner_id_before = wait_for_active_runner(&first_boot.base_url)
        .expect("the first boot must reach an active embedded runner");
    let session_before =
        wait_for_session_containing(&state_dir.join("session.json"), &runner_id_before);
    drop(first_boot);

    // The database is deleted and recreated; `storage_dir` — and the
    // session inside it — never moves.
    remove_sqlite_file(&database_path);

    let second_boot = start_server(&database_url, &storage_dir);
    let runner_id_after = wait_for_active_runner(&second_boot.base_url).expect(
        "a credential orphaned by a recreated database must not stall the embedded runner; \
         it must recover under a fresh identity and still reach `active`",
    );
    assert_ne!(
        runner_id_before, runner_id_after,
        "an orphaned credential must never be reused as-is — the recovered identity must be new"
    );
    let session_after =
        wait_for_session_containing(&state_dir.join("session.json"), &runner_id_after);
    assert_ne!(
        session_before, session_after,
        "the on-disk session must be overwritten with the fresh identity's own credential"
    );

    assert_recovery_logged_once_with_no_identifiers(
        &second_boot,
        &database_path,
        &storage_dir,
        &runner_id_before,
        &runner_id_after,
    );
    drop(second_boot);
}

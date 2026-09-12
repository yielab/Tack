//! Acceptance proof that the embedded runner's on-disk state directory
//! follows the same configuration the database does, not the process's own
//! working directory. Two real `tack serve --with-runner` subprocesses
//! (`env!("CARGO_BIN_EXE_tack")`), started from the *same* current
//! directory but pointed at two different databases/`storage_dir`s — the
//! shape an operator running the command twice from one shell would
//! produce. Two separate subprocesses, not two in-process servers in one
//! test: `tack_api::server::serve_inner` installs a process-global
//! `tracing` subscriber once per process, so a second in-process boot would
//! panic on that second `.init()` call regardless of this fix.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

mod common;
use common::free_port;

struct ServerGuard {
    child: Child,
    base_url: String,
    #[allow(dead_code)]
    root: tempfile::TempDir,
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

/// Starts `tack serve --with-runner` against a fresh database and
/// `storage_dir` under its own `root`, with its current directory set to
/// `shared_cwd` — shared with the other server this test starts, so the
/// crate's bare, cwd-relative legacy default would collide between them if
/// the state directory were not scoped to `storage_dir`.
fn start_server_with_runner(shared_cwd: &Path, root: tempfile::TempDir) -> ServerGuard {
    let database_url = format!("sqlite:{}/tack.db?mode=rwc", root.path().display());
    let storage_dir = root.path().join("storage");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    let child = Command::new(env!("CARGO_BIN_EXE_tack"))
        .arg("serve")
        .arg("--with-runner")
        .current_dir(shared_cwd)
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", port.to_string())
        .env("TACK_DATABASE_URL", &database_url)
        .env("TACK_STORAGE_DIR", &storage_dir)
        .env_remove("TACK_RUNNER_STATE_DIR")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tack serve --with-runner");

    let mut guard = ServerGuard {
        child,
        base_url,
        root,
    };
    wait_for_ready(&guard.base_url, &mut guard.child);
    guard
}

/// Polls `GET /api/health` until the process answers or exits. A liveness
/// backstop, not part of the claim: a stall past it means the process is
/// genuinely wedged, never that it was merely slow to spawn, migrate and
/// bind while the rest of this suite runs alongside it.
fn wait_for_ready(base_url: &str, child: &mut Child) {
    let client = reqwest::blocking::Client::new();
    let ready = poll_until(Duration::from_secs(30), || {
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
        panic!("tack serve --with-runner did not become ready within 30s");
    }
}

/// Polls `GET /api/runners` until exactly one row reports `state ==
/// "active"`, returning its `runner_id`. Self-provisioning is a handful of
/// local HTTP round trips plus one SQLite write, cheap enough that a genuine
/// success never approaches this bound — only a busy machine or an actual
/// regression does.
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

/// Polls for `path` to exist. `store_session` (`tack_runner::client::
/// transport`) writes the session file *after* the server's own enrollment
/// response already flipped the runner's row to `active`, so a bare
/// `is_file()` check taken the instant `wait_for_active_runner` returns can
/// race that write.
fn wait_for_session_file(path: &Path) -> bool {
    poll_until(Duration::from_secs(10), || path.is_file().then_some(())).is_some()
}

/// Waits for `server`'s own embedded runner to reach `active` and its
/// session to land under its own `storage_dir` (never the shared cwd both
/// servers in this test share), returning the runner id and state directory
/// for the caller's own cross-server comparison.
fn assert_server_enrolled(
    server: &ServerGuard,
    root_path: &Path,
    label: &str,
) -> (String, PathBuf) {
    let runner_id = wait_for_active_runner(&server.base_url).unwrap_or_else(|| {
        panic!("server {label}'s own embedded runner must reach `active` in its own database")
    });
    let state_dir = root_path.join("storage").join("runner");
    assert!(
        wait_for_session_file(&state_dir.join("session.json")),
        "server {label}'s enrolled session must live under its own storage_dir, not the shared cwd"
    );
    (runner_id, state_dir)
}

/// Two servers, each against its own database and `storage_dir`, started in
/// turn from the same working directory: each must enroll its own runner,
/// under its own state directory, and neither may fall back to the shared
/// cwd's bare legacy default. Server A is checked (and dropped) before
/// server B ever starts — a collision is a property of where the *first*
/// server writes, so proving it needs only one server on disk at a time.
#[test]
fn each_server_sees_only_its_own_runner_enrollment() {
    let shared_cwd = tempfile::Builder::new()
        .prefix("embedded-scope-shared-cwd")
        .tempdir()
        .expect("shared cwd");

    let root_a = tempfile::Builder::new()
        .prefix("embedded-scope-install-a")
        .tempdir()
        .expect("install a root");
    let root_a_path = root_a.path().to_path_buf();
    let server_a = start_server_with_runner(shared_cwd.path(), root_a);
    let (runner_id_a, state_dir_a) = assert_server_enrolled(&server_a, &root_a_path, "A");
    drop(server_a);

    let root_b = tempfile::Builder::new()
        .prefix("embedded-scope-install-b")
        .tempdir()
        .expect("install b root");
    let root_b_path = root_b.path().to_path_buf();
    let server_b = start_server_with_runner(shared_cwd.path(), root_b);
    let (runner_id_b, state_dir_b) = assert_server_enrolled(&server_b, &root_b_path, "B");
    drop(server_b);

    assert_ne!(
        runner_id_a, runner_id_b,
        "each server's own embedded runner must enroll as a distinct identity"
    );
    assert_ne!(
        state_dir_a, state_dir_b,
        "two servers with different storage_dir must never resolve to the same runner state \
         directory"
    );
    assert!(
        !shared_cwd
            .path()
            .join(tack_runner::config::DEFAULT_STATE_DIR)
            .exists(),
        "neither server may fall back to the shared working directory's bare default"
    );
}

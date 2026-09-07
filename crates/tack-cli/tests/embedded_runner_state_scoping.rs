//! Acceptance proof that the embedded runner's on-disk state directory
//! follows the same configuration the database does, not the process's own
//! working directory. Two real `tack serve --with-runner` subprocesses
//! (`env!("CARGO_BIN_EXE_tack")`) are started from the *same* current
//! directory but pointed at two different databases/`storage_dir`s, exactly
//! the shape an operator running the command twice from one shell would
//! produce. Each is driven only through `GET /api/runners` and the
//! filesystem — never a UI.
//!
//! Two separate subprocesses, not two in-process servers in one test
//! function: `tack_api::server::serve_inner` installs a process-global
//! `tracing` subscriber once per process, so a second in-process boot in the
//! same test would panic on that second `.init()` call regardless of this
//! card's fix.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

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

fn free_port() -> u16 {
    // Bind-then-drop to find a free ephemeral port — the same small,
    // accepted race window `e6_scheduler_e2e_test.rs` already documents for
    // this crate's other real-subprocess tests.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
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

/// Polls `GET /api/health` until the process answers or exits. The claim
/// this test makes is that a server started this way becomes ready and
/// reaches its own active runner — not that it does so within any
/// particular number of seconds — so this deadline is a liveness backstop,
/// not part of the claim: a stall past it means the process is genuinely
/// wedged, never that it was merely slow to spawn, run its migrations, and
/// bind its listener while the rest of this suite runs alongside it.
fn wait_for_ready(base_url: &str, child: &mut Child) {
    let client = reqwest::blocking::Client::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(response) = client
            .get(format!("{base_url}/api/health"))
            .timeout(Duration::from_millis(500))
            .send()
            && response.status().is_success()
        {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll child status") {
            panic!("tack serve --with-runner exited early during startup: {status}");
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("tack serve --with-runner did not become ready within 30s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Polls `GET /api/runners` until exactly one row reports `state ==
/// "active"`, returning its `runner_id`. The same liveness-backstop
/// reasoning as `wait_for_ready` applies here: self-provisioning is a
/// handful of local HTTP round trips plus one SQLite write, cheap enough
/// that a genuine success is never expected to approach this budget — only
/// a machine busy enough to starve those round trips for tens of seconds,
/// or an actual regression, reaches it. Matches
/// `embedded_runner_orphaned_credential.rs`'s sibling wait, which polls for
/// the identical transition.
fn wait_for_active_runner(base_url: &str) -> Option<String> {
    let client = reqwest::blocking::Client::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(response) = client
            .get(format!("{base_url}/api/runners"))
            .timeout(Duration::from_millis(500))
            .send()
            && let Ok(body) = response.json::<Value>()
            && let Some(rows) = body.get("data").and_then(Value::as_array)
        {
            for row in rows {
                if row.get("state").and_then(Value::as_str) == Some("active")
                    && let Some(id) = row.get("runner_id").and_then(Value::as_str)
                {
                    return Some(id.to_owned());
                }
            }
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Polls for `path` to exist. `store_session`
/// (`tack_runner::client::transport`) writes the session file to disk
/// *after* the server's own enrollment response already flipped the
/// runner's row to `active` — `embedded_runner_orphaned_credential.rs`
/// documents and waits out the identical ordering for the same reason. A
/// bare `is_file()` check taken the instant `wait_for_active_runner`
/// returns can race that write; this bounds the same way every other wait
/// in this file does, so a session that never lands is still reported
/// promptly rather than read one instant too early.
fn wait_for_session_file(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if path.is_file() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Two servers, each against its own database and `storage_dir`, started in
/// turn from the same working directory — each must reach its own active
/// runner enrollment, proven from `GET /api/runners` on its own address and
/// from the state directory actually written on disk, never through a UI.
///
/// Reverting `EmbeddedRunnerControl::new`'s derivation to its old,
/// unconditional `load_runner_config(ConfigOverrides::default(), None)` call
/// (dropping the storage-scoped `state_dir` override) turns both servers'
/// state directory back into the crate's bare, cwd-relative default —
/// identical for both since they share `shared_cwd`. The first assertion
/// below is what catches that: server A enrolls successfully, but its
/// session lands in the shared working directory instead of under its own
/// `storage_dir`, and the test fails there before server B is ever started.
/// That ordering is deliberate — the collision is a property of *where the
/// first* server writes, so proving it needs only one server on disk; the
/// two-server half of this test proves the separate claim that two distinct
/// identities result once the scoping holds.
#[test]
fn two_servers_on_two_databases_each_see_only_their_own_runner_enrollment() {
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
    let runner_id_a = wait_for_active_runner(&server_a.base_url)
        .expect("server A's own embedded runner must reach `active` in its own database");
    let state_dir_a = root_a_path.join("storage").join("runner");
    assert!(
        wait_for_session_file(&state_dir_a.join("session.json")),
        "server A's enrolled session must live under its own storage_dir, not the shared cwd"
    );
    drop(server_a);

    let root_b = tempfile::Builder::new()
        .prefix("embedded-scope-install-b")
        .tempdir()
        .expect("install b root");
    let root_b_path = root_b.path().to_path_buf();
    let server_b = start_server_with_runner(shared_cwd.path(), root_b);
    let runner_id_b = wait_for_active_runner(&server_b.base_url).expect(
        "server B's own embedded runner must reach `active` in its own database, \
         independently of the identity server A already enrolled",
    );
    let state_dir_b = root_b_path.join("storage").join("runner");
    assert!(
        wait_for_session_file(&state_dir_b.join("session.json")),
        "server B's enrolled session must live under its own storage_dir, not the shared cwd"
    );
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

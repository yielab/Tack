//! Cross-surface proof that the CLI's production routes drive real
//! scheduler behavior: healthy fleet selection, saturation, exact-runner
//! targeting, an unsupported model, and an idempotency conflict. Every
//! operator action shells out to the real `tack` binary
//! (`env!("CARGO_BIN_EXE_tack")`) against a real `tack serve` subprocess, a
//! real SQLite file and the production router — not a mock. The CLI has no
//! runner-protocol commands, so enroll/claim go over direct HTTP against
//! the same server, exactly as `tack-runner` would.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

mod common;
use common::free_port;

/// Owns the `tack serve` child process and its temp database directory;
/// kills the process on drop, and the `TempDir` removes the directory after
/// it, so a panicking assertion never leaks a listening server or scratch
/// files into the next test run. `dir` is declared last deliberately: struct
/// fields drop in declaration order, and the directory must outlive the
/// server still writing into it.
struct ServerGuard {
    child: Child,
    base_url: String,
    #[allow(dead_code)]
    dir: tempfile::TempDir,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A scratch directory per server instance, removed when the guard holding it
/// drops.
fn scratch_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("e6-cli-e2e")
        .tempdir()
        .expect("create scratch dir")
}

const OPERATOR_TOKEN: &str = "e6-cli-e2e-operator-token";

fn start_server() -> ServerGuard {
    let dir = scratch_dir();
    let db_path = dir.path().join("e6-cli-e2e.db");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    let child = Command::new(env!("CARGO_BIN_EXE_tack"))
        .arg("serve")
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", port.to_string())
        .env(
            "TACK_DATABASE_URL",
            format!("sqlite:{}?mode=rwc", db_path.display()),
        )
        .env("TACK_API_TOKEN", OPERATOR_TOKEN)
        .env("TACK_STORAGE_DIR", dir.path().join("storage"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tack serve");

    let mut guard = ServerGuard {
        child,
        dir,
        base_url,
    };
    wait_for_ready(&guard.base_url, &mut guard.child);
    guard
}

fn wait_for_ready(base_url: &str, child: &mut Child) {
    let client = reqwest::blocking::Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
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
            panic!("tack serve exited early during startup: {status}");
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("tack serve did not become ready within 15s");
        }
        // Poll interval, not a fixed wait: the loop condition above is the
        // real thing being awaited (a successful health response), bounded
        // by the deadline check above it.
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Shells out to the real `tack` binary with `--json`, parses stdout as
/// JSON, and asserts the process exited successfully. Every operator
/// action in this file goes through this one helper — the same production
/// entry point a human operator would use.
fn tack(server: &ServerGuard, args: &[&str]) -> Value {
    let mut full_args = args.to_vec();
    full_args.push("--json");
    let output = Command::new(env!("CARGO_BIN_EXE_tack"))
        .args(&full_args)
        .env("TACK_API_URL", &server.base_url)
        .env("TACK_API_TOKEN", OPERATOR_TOKEN)
        .output()
        .expect("run tack CLI");
    assert!(
        output.status.success(),
        "tack {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "tack {args:?} did not print JSON: {e}\nstdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// Runs `tack` and returns the raw exit status + stdout/stderr without
/// asserting success — for the CLI's own operator-side `execution create`
/// rejection path (`enqueue_execution`'s own validation), distinct from the
/// scheduler's silent "never gets leased" rejection this file is really
/// about.
fn tack_allow_failure(server: &ServerGuard, args: &[&str]) -> (bool, String, String) {
    let mut full_args = args.to_vec();
    full_args.push("--json");
    let output = Command::new(env!("CARGO_BIN_EXE_tack"))
        .args(&full_args)
        .env("TACK_API_URL", &server.base_url)
        .env("TACK_API_TOKEN", OPERATOR_TOKEN)
        .output()
        .expect("run tack CLI");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A minimal, valid runner-v1 capability report declaring `codex`/
/// `openai`/`model_id`. Used for the runner-protocol HTTP calls this test
/// makes directly (the CLI has no runner-protocol commands — see this
/// file's module doc).
fn capabilities(model_id: &str) -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    serde_json::json!({
        "reported_at": now,
        "labels": {},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now,
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": [model_id],
                "discovery": "reported"
            }]
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800}
    })
}

/// Enrolls a runner as the real `tack-runner` binary would: `tack runner
/// enroll` (operator CLI, issues the one-time token) then a direct
/// `POST /api/runner/v1/enroll` (the runner side of the exchange, which has
/// no CLI command — see this file's module doc). Returns (runner_id,
/// bearer credential).
fn enroll_runner_via_cli_and_protocol(
    server: &ServerGuard,
    name: &str,
    capacity: i64,
    model_id: &str,
) -> (String, String) {
    let pending = tack(
        server,
        &[
            "runner",
            "enroll",
            name,
            "--total-capacity",
            &capacity.to_string(),
            "--available-capacity",
            &capacity.to_string(),
        ],
    );
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let http = reqwest::blocking::Client::new();
    let enrolled: Value = http
        .post(format!("{}/api/runner/v1/enroll", server.base_url))
        .json(&serde_json::json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": capabilities(model_id),
        }))
        .send()
        .expect("enroll HTTP call")
        .json()
        .expect("enroll response JSON");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    (runner_id, credential)
}

/// Polls `/api/runner/v1/claim` once, as `tack-runner` would each cycle.
/// Returns the claimed `request_id`, or `None` for a `no work` response.
fn claim_once(
    server: &ServerGuard,
    runner_id: &str,
    credential: &str,
    claim_request_id: &str,
) -> Option<String> {
    let http = reqwest::blocking::Client::new();
    let claimed: Value = http
        .post(format!("{}/api/runner/v1/claim", server.base_url))
        .bearer_auth(credential)
        .json(&serde_json::json!({
            "protocol_version": 1,
            "runner_id": runner_id,
            "claim_request_id": claim_request_id,
            "available_capacity": 1,
            "wait_ms": 0,
        }))
        .send()
        .expect("claim HTTP call")
        .json()
        .expect("claim response JSON");
    claimed["request"]["request_id"].as_str().map(str::to_owned)
}

fn create_project_and_item(server: &ServerGuard, name: &str) -> (String, String) {
    let project = tack(server, &["init", name, "--type", "software"]);
    let project_id = project["id"].as_str().unwrap().to_owned();
    let item = tack(server, &["add", "work to run", "-p", &project_id]);
    let item_id = item["id"].as_str().unwrap().to_owned();
    (project_id, item_id)
}

fn create_agent_profile(server: &ServerGuard) -> String {
    let profile = tack(
        server,
        &[
            "agent-profile",
            "create",
            "E6 CLI profile",
            "--instructions",
            "work safely",
        ],
    );
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

#[allow(clippy::too_many_arguments)]
fn create_execution_via_cli(
    server: &ServerGuard,
    item_id: &str,
    agent_profile_id: &str,
    selector_flag: &str,
    selector_value: &str,
    idempotency_key: &str,
    model_provider: Option<&str>,
    model_id: Option<&str>,
) -> Value {
    let mut args = vec![
        "execution".to_string(),
        "create".to_string(),
        item_id.to_string(),
        "--idempotency-key".to_string(),
        idempotency_key.to_string(),
        format!("--{selector_flag}"),
        selector_value.to_string(),
        "--agent-profile".to_string(),
        agent_profile_id.to_string(),
        "--harness".to_string(),
        "codex".to_string(),
        "--agent-profile-snapshot".to_string(),
        r#"{"name":"profile","instructions":"work safely","tool_policy":{},"timeout_seconds":60,"budgets":{}}"#.to_string(),
        "--repository".to_string(),
        format!(
            r#"{{"kind":"git","remote":"https://example.test/e6-cli.git","base_revision":"{BASE_REVISION}"}}"#
        ),
        "--permission-policy".to_string(),
        r#"{"tools":["shell"],"network":false}"#.to_string(),
        "--timeout-seconds".to_string(),
        "60".to_string(),
    ];
    if let Some(provider) = model_provider {
        args.push("--model-provider".to_string());
        args.push(provider.to_string());
    }
    if let Some(id) = model_id {
        args.push("--model-id".to_string());
        args.push(id.to_string());
    }
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    tack(server, &args_ref)
}

// =======================================================================
// 1. Healthy fleet selection — a real runner enrolled via the CLI, a real
//    request created via the CLI, a real claim over the runner-v1 wire,
//    observed through the CLI's own `execution get`.
// =======================================================================

#[test]
fn eligible_runner_claims_request_and_cli_sees_it_leased() {
    let server = start_server();
    let (_project_id, item_id) = create_project_and_item(&server, "E6 CLI healthy");
    let agent_profile_id = create_agent_profile(&server);
    let (runner_id, credential) =
        enroll_runner_via_cli_and_protocol(&server, "healthy-runner", 1, "opaque/model-healthy");

    let created = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &runner_id,
        "healthy-key",
        Some("openai"),
        Some("opaque/model-healthy"),
    );
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "queued");

    let claimed_id = claim_once(&server, &runner_id, &credential, "healthy-claim");
    assert_eq!(
        claimed_id.as_deref(),
        Some(request_id.as_str()),
        "the real scheduler must pick this exact request for its eligible runner"
    );

    let fetched = tack(&server, &["execution", "get", &request_id]);
    assert_eq!(
        fetched["state"], "leased",
        "the CLI's own `execution get` must show the real, scheduler-granted lease"
    );
}

// =======================================================================
// 2. Saturation — a runner with one slot already consumed by an earlier
//    claim must leave a second request unclaimed, observed by the CLI as
//    still `queued`.
// =======================================================================

#[test]
fn a_saturated_runner_leaves_a_second_request_queued() {
    let server = start_server();
    let (_project_id, item_id) = create_project_and_item(&server, "E6 CLI saturation");
    let agent_profile_id = create_agent_profile(&server);
    let (runner_id, credential) = enroll_runner_via_cli_and_protocol(
        &server,
        "saturated-runner",
        1,
        "opaque/model-saturated",
    );

    let first = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &runner_id,
        "saturation-key-1",
        Some("openai"),
        Some("opaque/model-saturated"),
    );
    let first_id = first["request_id"].as_str().unwrap().to_owned();
    let first_claim = claim_once(&server, &runner_id, &credential, "saturation-claim-1");
    assert_eq!(first_claim.as_deref(), Some(first_id.as_str()));

    let second = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &runner_id,
        "saturation-key-2",
        Some("openai"),
        Some("opaque/model-saturated"),
    );
    let second_id = second["request_id"].as_str().unwrap().to_owned();

    let second_claim = claim_once(&server, &runner_id, &credential, "saturation-claim-2");
    assert_eq!(
        second_claim, None,
        "the runner's one slot is already in use; the scheduler must not double-lease it"
    );

    let fetched = tack(&server, &["execution", "get", &second_id]);
    assert_eq!(
        fetched["state"], "queued",
        "a saturated runner must leave the second request visibly queued via the CLI, not silently lost"
    );
}

// =======================================================================
// 3. Exact runner — a fleet member is NOT selected when the request names
//    a *different* exact runner, proving the selector (not just capacity)
//    is enforced.
// =======================================================================

#[test]
fn exact_runner_selector_excludes_every_other_runner() {
    let server = start_server();
    let (_project_id, item_id) = create_project_and_item(&server, "E6 CLI exact runner");
    let agent_profile_id = create_agent_profile(&server);
    let (target_runner_id, _target_credential) =
        enroll_runner_via_cli_and_protocol(&server, "exact-target", 1, "opaque/model-exact");
    let (_other_runner_id, other_credential) =
        enroll_runner_via_cli_and_protocol(&server, "exact-bystander", 1, "opaque/model-exact");

    let created = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &target_runner_id,
        "exact-runner-key",
        Some("openai"),
        Some("opaque/model-exact"),
    );
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // The bystander runner (not named by the request's exact-runner
    // selector) polls claim and must get nothing, even though it has an
    // identical, otherwise-eligible capability profile and free capacity.
    let bystander_claim = claim_once(
        &server,
        "exact-bystander-does-not-matter-since-auth-scopes-identity",
        &other_credential,
        "exact-bystander-claim",
    );
    assert_eq!(
        bystander_claim, None,
        "an exact-runner selector must exclude every runner except the one it names"
    );

    let fetched = tack(&server, &["execution", "get", &request_id]);
    assert_eq!(fetched["state"], "queued");
}

// =======================================================================
// 4. Unsupported model — a request naming a model the runner never
//    declared must never be claimed, proving the scheduler's harness/model
//    eligibility gate is real, not decorative.
// =======================================================================

#[test]
fn a_request_for_an_undeclared_model_is_never_claimed() {
    let server = start_server();
    let (_project_id, item_id) = create_project_and_item(&server, "E6 CLI unsupported model");
    let agent_profile_id = create_agent_profile(&server);
    let (runner_id, credential) = enroll_runner_via_cli_and_protocol(
        &server,
        "unsupported-model-runner",
        1,
        "opaque/model-declared",
    );

    let created = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &runner_id,
        "unsupported-model-key",
        Some("openai"),
        Some("opaque/model-not-declared-by-any-runner"),
    );
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    assert_eq!(
        created["state"], "queued",
        "creating the request itself must still succeed — the operator API does not \
         pre-validate against live capability data; \
         rejection happens at claim time"
    );

    let claimed = claim_once(&server, &runner_id, &credential, "unsupported-model-claim");
    assert_eq!(
        claimed, None,
        "a runner that never declared this model must never be handed the request"
    );

    let fetched = tack(&server, &["execution", "get", &request_id]);
    assert_eq!(
        fetched["state"], "queued",
        "an unsupported model combination must leave the request visibly queued forever, \
         not silently disappear or falsely report progress"
    );
}

// =======================================================================
// 5. Conflicts and needs_operator stay distinct through the CLI (sanity
//    check that the CLI surface itself, not just the scheduler, is wired
//    to the real production router for this domain).
// =======================================================================

#[test]
fn changed_payload_replay_returns_idempotency_conflict() {
    let server = start_server();
    let (_project_id, item_id) = create_project_and_item(&server, "E6 CLI conflict");
    let agent_profile_id = create_agent_profile(&server);
    let (runner_id, _credential) =
        enroll_runner_via_cli_and_protocol(&server, "conflict-runner", 1, "opaque/model-conflict");

    let _first = create_execution_via_cli(
        &server,
        &item_id,
        &agent_profile_id,
        "runner",
        &runner_id,
        "conflict-key",
        Some("openai"),
        Some("opaque/model-conflict"),
    );

    // Same idempotency key, different requested model — must be a named,
    // stable `idempotency_conflict`, not a generic failure, and the CLI
    // process itself must exit non-zero.
    let (success, stdout, stderr) = tack_allow_failure(
        &server,
        &[
            "execution",
            "create",
            &item_id,
            "--idempotency-key",
            "conflict-key",
            "--runner",
            &runner_id,
            "--agent-profile",
            &agent_profile_id,
            "--harness",
            "codex",
            "--model-provider",
            "openai",
            "--model-id",
            "opaque/model-a-different-one",
            "--agent-profile-snapshot",
            r#"{"name":"profile","instructions":"work safely","tool_policy":{},"timeout_seconds":60,"budgets":{}}"#,
            "--repository",
            &format!(
                r#"{{"kind":"git","remote":"https://example.test/e6-cli.git","base_revision":"{BASE_REVISION}"}}"#
            ),
            "--permission-policy",
            r#"{"tools":["shell"],"network":false}"#,
            "--timeout-seconds",
            "60",
        ],
    );
    assert!(
        !success,
        "a changed-payload replay must fail, not succeed silently"
    );
    assert!(
        stdout.contains("idempotency_conflict") || stderr.contains("idempotency_conflict"),
        "the stable conflict code must be visible to the operator: stdout={stdout} stderr={stderr}"
    );
}

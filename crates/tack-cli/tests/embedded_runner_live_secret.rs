//! A provider key stored while the embedded runner is already running must
//! reach the very next dispatch — with no "Re-check" and no operator restart.
//!
//! The runner receives its configuration by value when it is spawned, so a
//! provider that was disabled at that moment stays disabled inside that task
//! no matter what the control it was spawned from learns later. The
//! control's own catalog view, which the Agents page reads, is computed from
//! the control's copy and so reports the provider live the moment its key is
//! stored. Before the control restarted the runner on such a change, those
//! two views disagreed silently: the page said the key worked, and every
//! dispatch resolved the provider through the spawned task's stale copy,
//! refused it, and left the attempt sitting in `preparing`.
//!
//! One real `tack serve --with-runner` subprocess, a fake `claude` on its
//! `PATH` that records the environment it was spawned with, and a fake
//! gateway on loopback that only answers a catalog request carrying the
//! pasted key. Reaching `succeeded` with the fake harness's own version in
//! the attempt's `actual_execution` is what proves the harness actually ran;
//! the fake gateway's authorised hit count is what proves the new key, not
//! the old configuration, is what the runner booted with.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const FAKE_KEY: &str = "live-secret-test-key-never-a-real-credential";
const FAKE_MODEL: &str = "fake/model-1";
const PRINCIPAL_HEADER: (&str, &str) = ("x-tack-principal", "live-secret-test");

struct ServerGuard {
    child: Child,
    base_url: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// A gateway that answers `/v1/models` with one model — but only when the
/// bearer is the key this test stores through the API — and everything else
/// with an empty success. It counts authorised catalog hits so the test can
/// tell "the runner booted with the pasted key" apart from "the runner booted
/// at all".
fn start_fake_gateway(authorized_hits: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake gateway");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            handle_gateway_request(stream, &authorized_hits);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn handle_gateway_request(mut stream: TcpStream, authorized_hits: &AtomicUsize) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut authorized = false;
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header == "\r\n" || header.is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if lower.starts_with("authorization:") && header.trim().ends_with(FAKE_KEY) {
            authorized = true;
        }
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    if content_length > 0 {
        let mut body = vec![0; content_length];
        let _ = reader.read_exact(&mut body);
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (status, body) = if path.starts_with("/v1/models") {
        if authorized {
            authorized_hits.fetch_add(1, Ordering::SeqCst);
            (
                "200 OK",
                json!({ "data": [{ "id": FAKE_MODEL, "object": "model" }] }).to_string(),
            )
        } else {
            (
                "401 Unauthorized",
                json!({ "error": "bad key" }).to_string(),
            )
        }
    } else {
        ("200 OK", "{}".to_owned())
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// A `claude` the runner's probe and spawn both accept. `--version` answers
/// with a version no real release carries; a run records the base URL it was
/// pointed at (never the credential) beside itself and prints the two
/// stream-json lines the adapter reads, so the attempt ends `succeeded` with
/// this version in its `actual_execution`.
fn install_fake_claude(dir: &Path) -> PathBuf {
    let script = dir.join("claude");
    std::fs::write(
        &script,
        r#"#!/bin/sh
case "$1" in
  --version) echo "9.9.1 (Claude Code)"; exit 0 ;;
esac
printf '%s\n' "${ANTHROPIC_BASE_URL:-unset}" > "$(dirname "$0")/invoked"
echo '{"type":"system","subtype":"init","model":"fake/model-1","claude_code_version":"9.9.1"}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"DONE"}'
exit 0
"#,
    )
    .expect("write fake claude");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake claude");
    }
    script
}

fn git_fixture_repo(dir: &Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?} failed: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    run(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "fixture\n").expect("write fixture file");
    run(&[
        "-c",
        "user.email=test@invalid",
        "-c",
        "user.name=test",
        "add",
        "README.md",
    ]);
    run(&[
        "-c",
        "user.email=test@invalid",
        "-c",
        "user.name=test",
        "commit",
        "-qm",
        "fixture",
    ]);
    run(&["rev-parse", "HEAD"])
}

fn start_server(
    database_url: &str,
    storage_dir: &Path,
    shim_dir: &Path,
    gateway_base: &str,
) -> ServerGuard {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let path = format!("{}:/usr/bin:/bin", shim_dir.display());
    let mut child = Command::new(env!("CARGO_BIN_EXE_tack"))
        .arg("serve")
        .arg("--with-runner")
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", port.to_string())
        .env("TACK_DATABASE_URL", database_url)
        .env("TACK_STORAGE_DIR", storage_dir)
        .env_remove("TACK_RUNNER_STATE_DIR")
        .env("TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL", gateway_base)
        // The fake `claude` must be the only harness the probe finds, so the
        // machine's own installation never enters this test's claim.
        .env("PATH", path)
        // An invalid session bus address makes the platform keychain attempt
        // fail immediately, so the secret store falls back to its owner-only
        // file at once instead of after a D-Bus timeout.
        .env("DBUS_SESSION_BUS_ADDRESS", "/dev/null")
        .env("RUST_LOG", "tack=info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tack serve --with-runner");
    wait_for_ready(&base_url, &mut child);
    ServerGuard { child, base_url }
}

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

fn get_json(client: &reqwest::blocking::Client, url: &str) -> Option<Value> {
    client
        .get(url)
        .header(PRINCIPAL_HEADER.0, PRINCIPAL_HEADER.1)
        .timeout(Duration::from_secs(5))
        .send()
        .ok()?
        .json::<Value>()
        .ok()
}

fn post_json(client: &reqwest::blocking::Client, url: &str, body: Value) -> Value {
    let response = client
        .post(url)
        .header(PRINCIPAL_HEADER.0, PRINCIPAL_HEADER.1)
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .expect("POST request");
    let status = response.status();
    let value = response.json::<Value>().unwrap_or(Value::Null);
    assert!(status.is_success(), "POST {url} -> {status}: {value}");
    value
}

/// The active runner's id, once its enrollment snapshot advertises
/// `FAKE_MODEL` under the gateway provider — which it only can once it has
/// booted with the pasted key and fetched the fake catalog with it. Bounded
/// generously: a stall here is the real failure, and the machine running this
/// suite is often busy with the rest of it.
fn wait_for_runner_advertising_the_model(
    client: &reqwest::blocking::Client,
    base_url: &str,
) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(body) = get_json(client, &format!("{base_url}/api/runners"))
            && let Some(runners) = body.get("data").and_then(Value::as_array)
        {
            for runner in runners {
                if runner.get("state").and_then(Value::as_str) != Some("active") {
                    continue;
                }
                let advertised = runner
                    .pointer("/capability_snapshot/harnesses")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|harness| {
                        harness.get("harness_kind").and_then(Value::as_str) == Some("claude-code")
                    })
                    .flat_map(|harness| {
                        harness
                            .get("model_combinations")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default()
                    })
                    .filter(|combination| {
                        combination.get("model_provider").and_then(Value::as_str)
                            == Some("vercel-ai-gateway")
                    })
                    .flat_map(|combination| {
                        combination
                            .get("model_ids")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default()
                    })
                    .any(|model| model.as_str() == Some(FAKE_MODEL));
                if advertised {
                    return runner
                        .get("runner_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
            }
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn wait_for_active_runner(client: &reqwest::blocking::Client, base_url: &str) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(body) = get_json(client, &format!("{base_url}/api/runners"))
            && let Some(runners) = body.get("data").and_then(Value::as_array)
            && let Some(active) = runners
                .iter()
                .find(|runner| runner.get("state").and_then(Value::as_str) == Some("active"))
        {
            return active
                .get("runner_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn wait_for_terminal_attempt(
    client: &reqwest::blocking::Client,
    base_url: &str,
    request_id: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last = Value::Null;
    loop {
        if let Some(body) = get_json(
            client,
            &format!("{base_url}/api/executions/{request_id}/attempts"),
        ) && let Some(attempt) = body.pointer("/data/0")
        {
            last = attempt.clone();
            if matches!(
                attempt.get("state").and_then(Value::as_str),
                Some("succeeded" | "failed" | "needs_operator" | "lost" | "cancelled")
            ) {
                return last;
            }
        }
        if Instant::now() > deadline {
            panic!(
                "the attempt never reached a terminal state — this is the silent hang the \
                 restart-on-change exists to rule out; last seen: {last}"
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The exact order the Agents page lays out, with no "Re-check" between
/// pasting the key and running: execution already on, key stored, dispatch.
#[test]
fn a_key_stored_while_the_runner_is_running_is_used_by_the_next_dispatch() {
    let root = tempfile::Builder::new()
        .prefix("embedded-live-secret")
        .tempdir()
        .expect("temporary root");
    let storage_dir = root.path().join("storage");
    std::fs::create_dir_all(&storage_dir).expect("storage dir");
    let shim_dir = root.path().join("bin");
    std::fs::create_dir_all(&shim_dir).expect("shim dir");
    install_fake_claude(&shim_dir);
    let repo_dir = root.path().join("repo");
    std::fs::create_dir_all(&repo_dir).expect("repo dir");
    let base_revision = git_fixture_repo(&repo_dir);
    let database_url = format!("sqlite:{}?mode=rwc", root.path().join("tack.db").display());
    let authorized_hits = Arc::new(AtomicUsize::new(0));
    let gateway_base = start_fake_gateway(Arc::clone(&authorized_hits));

    let server = start_server(&database_url, &storage_dir, &shim_dir, &gateway_base);
    let client = reqwest::blocking::Client::new();
    let base_url = server.base_url.clone();

    // Execution is on and the runner is serving before the key exists — the
    // shape this test is about. Its boot fetched no catalog: the provider was
    // disabled and there was no key to fetch with.
    let runner_before = wait_for_active_runner(&client, &base_url)
        .expect("the embedded runner must reach `active` before any key is stored");
    assert_eq!(
        authorized_hits.load(Ordering::SeqCst),
        0,
        "no catalog fetch can carry the key before the key exists"
    );

    // The key, stored exactly as the Agents page stores it.
    let stored = client
        .put(format!(
            "{base_url}/api/local-runner/secrets/vercel-ai-gateway%2Fdefault"
        ))
        .json(&json!({ "value": FAKE_KEY }))
        .timeout(Duration::from_secs(60))
        .send()
        .expect("PUT secret");
    assert_eq!(stored.status(), reqwest::StatusCode::NO_CONTENT);

    // The runner that serves the next dispatch booted with the key: its
    // enrollment snapshot advertises the fake gateway's model, which it could
    // only have fetched with that key.
    let runner_after = wait_for_runner_advertising_the_model(&client, &base_url).expect(
        "after the key is stored, the serving runner must advertise the gateway's catalog — \
         a runner that never restarted keeps the empty snapshot it booted with",
    );
    assert_eq!(
        runner_before, runner_after,
        "the restarted runner resumes its own stored session rather than enrolling anew"
    );
    assert!(
        authorized_hits.load(Ordering::SeqCst) >= 1,
        "the fake gateway must have answered a catalog fetch that carried the pasted key"
    );

    let project = post_json(
        &client,
        &format!("{base_url}/api/projects"),
        json!({ "name": "live-secret", "project_type": "software" }),
    );
    let project_id = project["id"].as_str().expect("project id").to_owned();
    let item = post_json(
        &client,
        &format!("{base_url}/api/projects/{project_id}/items"),
        json!({ "title": "run through the gateway", "item_type": "task" }),
    );
    let item_id = item["id"].as_str().expect("item id").to_owned();
    let profile = post_json(
        &client,
        &format!("{base_url}/api/agent-profiles"),
        json!({ "name": "live-secret-profile", "instructions": "Print DONE and exit." }),
    );
    let profile_id = profile["agent_profile_id"]
        .as_str()
        .expect("agent profile id")
        .to_owned();

    let request = post_json(
        &client,
        &format!("{base_url}/api/executions"),
        json!({
            "item_id": item_id,
            "idempotency_key": "live-secret-dispatch",
            "selector_kind": "exact_runner",
            "selector_id": runner_after,
            "agent_profile_id": profile_id,
            "requested_harness_kind": "claude-code",
            "requested_model_provider": "vercel-ai-gateway",
            "requested_model_id": FAKE_MODEL,
            "agent_profile_snapshot": {
                "name": "live-secret-profile",
                "instructions": "Print DONE and exit.",
                "tool_policy": {},
                "timeout_seconds": 60,
                "budgets": {}
            },
            "repository_snapshot": {
                "kind": "git",
                "remote": repo_dir.display().to_string(),
                "base_revision": base_revision,
                "subdirectory": null
            },
            "permission_policy": { "tools": [], "network": false },
            "budgets": {},
            "environment": {},
            "metadata": {},
            "timeout_seconds": 60
        }),
    );
    let request_id = request["request_id"]
        .as_str()
        .expect("request id")
        .to_owned();

    let attempt = wait_for_terminal_attempt(&client, &base_url, &request_id);
    assert_eq!(
        attempt["state"].as_str(),
        Some("succeeded"),
        "the dispatch right after storing the key must run through the gateway provider; \
         got {attempt}"
    );
    assert_eq!(
        attempt
            .pointer("/actual_execution/harness_version")
            .and_then(Value::as_str),
        Some("9.9.1"),
        "the fake harness's own version in the completion is what proves it ran"
    );
    assert_eq!(
        attempt
            .pointer("/actual_execution/model_provider")
            .and_then(Value::as_str),
        Some("vercel-ai-gateway")
    );

    // The harness was pointed at the fake gateway's per-harness endpoint —
    // the spawn environment carried the provider the key enabled.
    let invoked = std::fs::read_to_string(shim_dir.join("invoked"))
        .expect("the fake claude records the base URL it was spawned with");
    assert_eq!(
        invoked.trim(),
        format!("{gateway_base}/claude-code"),
        "the spawn environment must point at the gateway the key belongs to"
    );

    drop(server);
}

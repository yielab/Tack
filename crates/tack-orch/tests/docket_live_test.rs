//! Opt-in, live capture for `DocketAdapter::dispatch` against a real,
//! isolated `docket serve` — never runs under a plain `cargo nextest run`,
//! never required in CI, and never fails just because no `docket` binary is
//! configured. Matches the shape of `tack-runner`'s own `#[ignore]`-gated
//! live harness tests: a prerequisite check up front, a graceful `eprintln!`
//! skip when it's absent, and a real component driven end to end rather
//! than a hand-built `curl`.
//!
//! # Prerequisites
//!
//! `TACK_LIVE_DOCKET_BIN` must point at a `docket` launcher script built
//! from `../rack-cli` (never `~/.local/bin/docket`, which lags the
//! repository's own tag). One way to build it:
//!
//! ```text
//! cd ../rack-cli && uv build --out-dir /some/scratch/dist
//! uv venv /some/scratch/venv --python 3.11
//! uv pip install --python /some/scratch/venv/bin/python \
//!     /some/scratch/dist/docket-*.whl
//! export TACK_LIVE_DOCKET_BIN=/some/scratch/venv/bin/docket
//! ```
//!
//! This test spawns that binary itself, pointed at a fresh `DOCKET_HOME`
//! this test creates and tears down via a `tempfile::TempDir` guard — never
//! `~/.docket`, which holds the operator's real approvals and audit log.
//! Neither `ANTHROPIC_API_KEY` nor `OPENAI_API_KEY` (nor any other provider
//! credential) is forwarded into the spawned process's environment: the
//! dispatched pod's Lead has no reachable model endpoint, so its one hop
//! fails locally with "no endpoint configured" before any network call
//! reaches a real provider — this is the safety property under test, not
//! an incidental side effect.
//!
//! Run with:
//! `TACK_LIVE_DOCKET_BIN=... cargo nextest run --workspace --run-ignored
//! ignored-only -E 'test(live_dispatch_against_a_real_docket_server)'`

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::{ControlPlane, NewRemoteTask, OrchError, ProvisionPodParams, RunState};

/// Kills the spawned `docket serve` on drop, including on a panicking
/// assertion — a bare `Child` left running would otherwise leak past a
/// failed `assert!` and keep listening on its port.
struct ServeGuard(Child);

impl Drop for ServeGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

#[tokio::test]
#[ignore = "opt-in: requires TACK_LIVE_DOCKET_BIN pointing at a `docket` \
            launcher built from ../rack-cli (see this file's module doc for \
            how); spawns an isolated docket serve with its own DOCKET_HOME \
            (never ~/.docket) and forwards no provider credential, so the \
            dispatched pipeline fails locally rather than reaching a paid \
            API; run with TACK_LIVE_DOCKET_BIN=... cargo nextest run \
            --workspace --run-ignored ignored-only \
            -E 'test(live_dispatch_against_a_real_docket_server)'"]
async fn live_dispatch_against_a_real_docket_server() {
    let Ok(docket_bin) = std::env::var("TACK_LIVE_DOCKET_BIN") else {
        eprintln!(
            "skipping live dispatch test: set TACK_LIVE_DOCKET_BIN to a `docket` \
             launcher built from ../rack-cli"
        );
        return;
    };

    let docket_home = tempfile::tempdir().expect("create a scratch DOCKET_HOME");
    let port = free_port();
    let token = "viii-c2-live-test-token";

    let child = Command::new(&docket_bin)
        .args(["serve", "--port", &port.to_string(), "--interval", "3600"])
        // Deliberately not inherited from this process's own environment —
        // start from a clean map so a credential exported into the test
        // runner's shell can never reach the spawned server. `env_clear`
        // must come before the `env` calls that follow: it clears whatever
        // has been set on the builder so far, not just the inherited
        // environment.
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("DOCKET_HOME", docket_home.path())
        .env("DOCKET_SERVE_TOKEN", token)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn docket serve");
    let _guard = ServeGuard(child);

    let base_url = format!("http://127.0.0.1:{port}");
    let adapter =
        DocketAdapter::new(&base_url, Some(token.to_string())).expect("build a DocketAdapter");

    // Wait for the server to answer /health rather than sleeping a fixed
    // amount — the process needs a moment to import and bind.
    let mut ready = false;
    for _ in 0..50 {
        if adapter.health().await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "docket serve never answered /health");

    let project = "viii-c2-live-proof";
    let pod = adapter
        .provision_pod(ProvisionPodParams {
            project: project.to_string(),
            path: docket_home.path().to_string_lossy().to_string(),
            blueprint: "software".to_string(),
            pod: None,
            budget: None,
            verify_cmd: String::new(),
        })
        .await
        .expect("provision a fresh pod against the live server");
    assert_eq!(pod.project, project);
    assert!(
        pod.members.iter().any(|m| m.role == "lead"),
        "a software-blueprint pod must have a Lead"
    );

    let task_id = adapter
        .enqueue_task(
            project,
            NewRemoteTask {
                description: "say hello".to_string(),
                priority: None,
                trusted: true,
            },
        )
        .await
        .expect("enqueue a task against the live server");
    assert!(task_id.starts_with("task-"));

    // The observable under test: `dispatch` hands back a run id before the
    // pipeline it names has done anything at all.
    let run_id = adapter
        .dispatch(project, serde_json::json!({}))
        .await
        .expect("dispatch must succeed synchronously, before the pipeline runs");
    assert!(run_id.starts_with("run-"));

    // Poll `get_run` — the only place the outcome ever becomes visible —
    // until it reaches a terminal state.
    let mut final_run = None;
    for _ in 0..100 {
        let run = adapter
            .get_run(&run_id)
            .await
            .expect("get_run must succeed against the live server");
        if !matches!(run.state, RunState::Queued | RunState::Running) {
            final_run = Some(run);
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let final_run = final_run.expect("the dispatched run must reach a terminal state");

    assert_eq!(
        final_run.state,
        RunState::Failed,
        "with no provider credential configured, the Lead's one hop must fail locally"
    );
    assert!(
        final_run.error.contains("no endpoint configured"),
        "the failure must be the local endpoint-resolution error, never a network/auth \
         error from a real provider — got: {}",
        final_run.error
    );

    // A second dispatch against a project docket has never heard of
    // succeeds synchronously the same way, and fails asynchronously for a
    // different, still entirely local reason — no pod exists to run
    // against. This is the same run-id-before-outcome shape `dispatch`
    // documents, reached without provisioning anything at all.
    let unknown_run_id = adapter
        .dispatch("viii-c2-unprovisioned", serde_json::json!({}))
        .await
        .expect("dispatch against an unprovisioned project still succeeds synchronously");
    let mut unknown_final = None;
    for _ in 0..50 {
        let run = adapter
            .get_run(&unknown_run_id)
            .await
            .expect("get_run must succeed against the live server");
        if !matches!(run.state, RunState::Queued | RunState::Running) {
            unknown_final = Some(run);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let unknown_final = unknown_final.expect("the second run must also reach a terminal state");
    assert_eq!(unknown_final.state, RunState::Failed);
    assert!(
        unknown_final.error.contains("no pod"),
        "dispatching an unprovisioned project must fail for lack of a pod, not a network \
         error — got: {}",
        unknown_final.error
    );

    // Unauthenticated and malformed requests never reach the pipeline at
    // all — confirmed against the compiled adapter's own error mapping,
    // not just read from source.
    let unauthed = DocketAdapter::new(&base_url, None).expect("build an unauthenticated adapter");
    let auth_err = unauthed
        .dispatch(project, serde_json::json!({}))
        .await
        .expect_err("a dispatch call with no bearer token must be rejected");
    assert!(matches!(auth_err, OrchError::Auth));
}

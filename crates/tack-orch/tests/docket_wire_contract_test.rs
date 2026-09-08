//! Per-method wire oracle for `DocketAdapter` — the secondary regression
//! oracle named in
//! `docs/plans/agnostic-control-plane.md` §6. `docket_tick_contract_test.rs`
//! is the primary
//! oracle: it drives a full reconciler tick and would catch a refactor that
//! changes *which* requests get issued in steady state. This file is
//! narrower and complementary — for **every one of the current thirteen
//! `ControlPlane` methods**, drive a real `DocketAdapter` against `wiremock`
//! and snapshot both what left the process and what the adapter decoded, so
//! a change to any single method's wire behavior is visible even in
//! isolation from the reconciler that happens to call it today.
//!
//! Before this file, only 4 of the 37 tests in `docket_adapter_test.rs`
//! asserted anything about the outgoing request
//! (`enqueue_task_sends_the_trusted_flag_on_the_wire`,
//! `decide_approval_grant_sends_channel_tack_and_returns_the_resulting_state`,
//! `provision_pod_sends_the_full_request_shape_on_the_wire`,
//! `unauthenticated_routes_never_send_authorization_header`); the other 33
//! only assert decoding. Those fixtures are reused here verbatim rather than
//! re-derived — see `tests/fixtures/*.json`'s own provenance comments for
//! which are live captures vs. constructed.
//!
//! # What each golden file records
//!
//! One file per method, `tests/golden/wire/<method>.json`, holding:
//!
//! - `requests` — the **ordered** list of HTTP requests the call issued
//!   (method, path, query pairs sorted by key, the *names* of headers
//!   present, and the canonicalised JSON body). Zero entries for `kind`
//!   (pure, synchronous, no I/O).
//! - `result` — the decoded outcome: `{"outcome":"ok","value":...}` with the
//!   DTO serialized through `serde_json::to_value`, or
//!   `{"outcome":"err","error":"<Display text>"}` for a method that errors.
//!
//! **Header names only, never values** — `grep -rn "Bearer"
//! tests/golden/wire/` must stay clean. The bearer token used by these tests is a
//! throwaway fixture constant, never a real credential, but the discipline
//! is enforced structurally here regardless: [`RequestTranscript`] has no
//! field a header *value* could land in.
//!
//! **Body canonicalisation is free, not bespoke.** `tack-orch` never enables
//! `serde_json`'s `preserve_order` feature (see `Cargo.toml`), so
//! `serde_json::Value::Object` is backed by a `BTreeMap` and always
//! serializes its keys in sorted order — the exact "canonicalised form,
//! stable field order" property this oracle needs falls out of the existing
//! workspace configuration.
//!
//! # `dispatch`
//!
//! `POST /dispatch/{project}`, the same "one request, snapshot both sides"
//! treatment as every other write method — see `adapters::docket`'s module
//! doc, "Write methods", for the shape of its response and why a `pre_input`
//! block is never observable synchronously on this route the way it is on
//! `enqueue_task`'s.
//!
//! # Auth split, preserved in the golden
//!
//! Per `adapters::docket`'s own module doc: `/health`, `/status.json`, and
//! `/metrics` never carry a Bearer token, even with one configured; every
//! other route does. `health_wire_contract`, `status_wire_contract`, and
//! `metrics_wire_contract` all use an adapter configured *with* a token
//! (same as `unauthenticated_routes_never_send_authorization_header` in
//! `docket_adapter_test.rs`) precisely so their golden's `header_names`
//! genuinely proves the split, rather than trivially lacking the header
//! because none was ever configured.
//!
//! `UPDATE_GOLDEN=1` regenerates every golden in this file, mirroring
//! `crates/tack-api/tests/openapi_contract.rs`'s `UPDATE_OPENAPI=1` pattern.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::{ControlPlane, NewRemoteTask, OrchError, ProvisionPodParams};
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Throwaway fixture credential — never a real token, and never written to
/// a golden file (only the header *name* `authorization` is captured; see
/// the module doc).
const TOKEN: &str = "wire-oracle-fixture-token";

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_text_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

/// JSON fixtures carry a leading block of `//`-prefixed provenance-comment
/// lines — not valid JSON syntax — stripped before use as a mock response.
/// Same convention as `docket_adapter_test.rs`; duplicated rather than
/// shared because integration test binaries in this crate don't share a
/// `tests/common` module (there isn't one — see `ingestion/traces.rs`
/// and `docket_adapter_test.rs`, each self-contained) and this file's
/// ownership is deliberately scoped to not touch either.
fn load_json_fixture(name: &str) -> String {
    load_text_fixture(name)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn adapter_for(server: &MockServer) -> DocketAdapter {
    DocketAdapter::new(server.uri(), Some(TOKEN.to_string())).expect("adapter must construct")
}

// ---------------------------------------------------------------------------
// The golden shape
// ---------------------------------------------------------------------------

/// One HTTP request as observed by `wiremock`, reduced to exactly what the
/// oracle needs to be sensitive to — see the module doc for what's
/// deliberately excluded (header *values*, in particular).
#[derive(Debug, Serialize)]
struct RequestTranscript {
    method: String,
    path: String,
    /// `(key, value)` pairs, sorted by key — query values are never secret
    /// on any docket route this adapter calls, unlike headers.
    query: Vec<(String, String)>,
    /// Lowercased, sorted, deduplicated header *names*. Never values — see
    /// the module doc.
    header_names: Vec<String>,
    /// `Null` when no body was sent (every `GET`); otherwise the parsed,
    /// canonically-key-sorted JSON body.
    body: serde_json::Value,
}

/// The decoded result of a `ControlPlane` call. `OrchError` has no
/// `Serialize` impl (by design — see `lib.rs`; it's a `thiserror` enum meant
/// for `Display`, not wire transport), so the error case is captured as its
/// `Display` text — deterministic, human-legible in a diff, and incapable of
/// leaking anything a header value could (there are no headers in an
/// `OrchError`).
#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum Outcome {
    Ok { value: serde_json::Value },
    Err { error: String },
}

fn ok<T: Serialize>(v: T) -> Outcome {
    Outcome::Ok {
        value: serde_json::to_value(v).expect("decoded DTO must serialize"),
    }
}

fn err(e: OrchError) -> Outcome {
    Outcome::Err {
        error: e.to_string(),
    }
}

/// `ProvisionedPod` derives only `Deserialize` in `lib.rs` (it's a response
/// DTO this crate decodes, never re-encodes) — so unlike every other DTO
/// this file snapshots via [`ok`]'s generic `Serialize` bound, its golden
/// value is built by hand instead: widening the derive list would mean
/// editing `lib.rs`, which is out of scope for this test file.
fn provisioned_pod_to_value(p: &tack_orch::ProvisionedPod) -> serde_json::Value {
    serde_json::json!({
        "project": p.project,
        "blueprint": p.blueprint,
        "members": p.members.iter().map(|m| serde_json::json!({
            "id": m.id,
            "role": m.role,
            "model": m.model,
        })).collect::<Vec<_>>(),
    })
}

#[derive(Debug, Serialize)]
struct MethodGolden {
    method: &'static str,
    requests: Vec<RequestTranscript>,
    result: Outcome,
}

/// Reduces every request `wiremock` recorded, in receipt order, to a
/// [`RequestTranscript`]. Called *after* the adapter call under test, so the
/// list is exactly what that one call produced (each test starts a fresh
/// `MockServer`).
async fn transcripts(server: &MockServer) -> Vec<RequestTranscript> {
    let received = server
        .received_requests()
        .await
        .expect("request recording is enabled by default");

    received
        .iter()
        .map(|r| {
            let mut query: Vec<(String, String)> = r
                .url
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            query.sort();

            let mut header_names: Vec<String> = r
                .headers
                .keys()
                .map(|name| name.as_str().to_ascii_lowercase())
                .collect();
            header_names.sort();
            header_names.dedup();

            let body = if r.body.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&r.body).unwrap_or_else(|_| {
                    serde_json::Value::String(String::from_utf8_lossy(&r.body).into_owned())
                })
            };

            RequestTranscript {
                method: r.method.to_string(),
                path: r.url.path().to_string(),
                query,
                header_names,
                body,
            }
        })
        .collect()
}

fn golden_path(method: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/wire")
        .join(format!("{method}.json"))
}

/// Compares (or, under `UPDATE_GOLDEN=1`, writes) `golden` against
/// `tests/golden/wire/<method>.json`. Mirrors
/// `crates/tack-api/tests/openapi_contract.rs`'s `UPDATE_OPENAPI` pattern
/// exactly.
fn assert_matches_golden(method: &str, golden: &MethodGolden) {
    let path = golden_path(method);
    let rendered = serde_json::to_string_pretty(golden).expect("serialize golden") + "\n";

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create tests/golden/wire");
        }
        fs::write(&path, &rendered).expect("write golden file");
        eprintln!("regenerated {}", path.display());
        return;
    }

    let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {} ({e}).\nGenerate it with: UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_wire_contract_test)'",
            path.display()
        )
    });
    assert_eq!(
        committed,
        rendered,
        "\n\n{} drifted from what DocketAdapter actually sent/decoded.\n\
         Regenerate with:\n    UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_wire_contract_test)'\n",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// One golden per method, in trait declaration order (`lib.rs`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kind_wire_contract() {
    // Synchronous, no I/O — snapshotted anyway so all thirteen methods have
    // a golden, not twelve-plus-a-remembered-exception.
    let server = MockServer::start().await;
    let adapter = adapter_for(&server);

    let golden = MethodGolden {
        method: "kind",
        requests: transcripts(&server).await,
        result: ok(adapter.kind()),
    };
    assert_matches_golden("kind", &golden);
}

#[tokio::test]
async fn health_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(load_json_fixture("health.json")))
        .mount(&server)
        .await;

    // Configured *with* a token, so the golden's absent `authorization`
    // header genuinely proves the auth split rather than trivially lacking
    // it because none was ever set — see the module doc.
    let adapter = adapter_for(&server);
    let result = match adapter.health().await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "health",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("health", &golden);
}

#[tokio::test]
async fn status_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("status_with_agent.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.status().await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "status",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("status", &golden);
}

#[tokio::test]
async fn metrics_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_text_fixture("metrics_with_agent.txt")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.metrics().await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "metrics",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("metrics", &golden);
}

#[tokio::test]
async fn list_runs_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo-lead"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("runs_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.list_runs(Some("demo-lead")).await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "list_runs",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("list_runs", &golden);
}

#[tokio::test]
async fn get_run_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/runs/run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("run_single.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter
        .get_run("run-25d46fd9-04d4-4257-8b82-1d2cf5167cbb")
        .await
    {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "get_run",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("get_run", &golden);
}

#[tokio::test]
async fn list_approvals_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("approvals_pending.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.list_approvals().await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "list_approvals",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("list_approvals", &golden);
}

#[tokio::test]
async fn list_tasks_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("tasks_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.list_tasks("demo").await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "list_tasks",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("list_tasks", &golden);
}

#[tokio::test]
async fn traces_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .and(query_param("since", "2026-08-04T00:00:00Z"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(load_json_fixture("traces_list.json")),
        )
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.traces("demo", Some("2026-08-04T00:00:00Z")).await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "traces",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("traces", &golden);
}

#[tokio::test]
async fn enqueue_task_wire_contract() {
    // The `trusted: false` case, reusing
    // `enqueue_task_sends_the_trusted_flag_on_the_wire`'s scenario from
    // `docket_adapter_test.rs` — the wire-level half of the prompt-injection
    // trust boundary that `ItemSource::is_trusted` enforces at the data
    // layer, and the one place this crate already had a wire assertion
    // to reuse rather than invent.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_partial_json(serde_json::json!({"trusted": false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "task": "task-untrusted", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter
        .enqueue_task(
            "demo",
            NewRemoteTask {
                description: "GitHub-imported title".into(),
                priority: None,
                trusted: false,
            },
        )
        .await
    {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "enqueue_task",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("enqueue_task", &golden);
}

#[tokio::test]
async fn dispatch_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/dispatch/demo"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_partial_json(serde_json::json!({"branch": "main"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "run": "run-1", "project": "demo", "status": "dispatched"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter
        .dispatch("demo", serde_json::json!({"branch": "main"}))
        .await
    {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "dispatch",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("dispatch", &golden);
}

#[tokio::test]
async fn decide_approval_wire_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/approvals/apr-1"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_partial_json(serde_json::json!({
            "action": "grant",
            "channel": "tack"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "token": "apr-1", "state": "granted"
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter.decide_approval("apr-1", true).await {
        Ok(v) => ok(v),
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "decide_approval",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("decide_approval", &golden);
}

#[tokio::test]
async fn provision_pod_wire_contract() {
    // Every optional field populated — same scenario as
    // `provision_pod_sends_the_full_request_shape_on_the_wire` in
    // `docket_adapter_test.rs` — plus a non-empty `members` roster in the
    // response, so `result` actually exercises `ProvisionedPod`'s full
    // decode, not just the empty-roster shape that test used.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pods"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_partial_json(serde_json::json!({
            "project": "blog-api",
            "path": "/home/ox/code/blog-api",
            "blueprint": "software",
            "pod": "full",
            "budget": 25.0,
            "verifyCmd": "cargo test --workspace"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "ok": true,
            "project": "blog-api",
            "blueprint": "software",
            "members": [
                {"id": "blog-api-lead", "role": "lead", "model": "anthropic/claude-opus-4-5"},
                {"id": "blog-api-impl-1", "role": "implementer", "model": "anthropic/claude-sonnet-4-5"}
            ]
        })))
        .mount(&server)
        .await;

    let adapter = adapter_for(&server);
    let result = match adapter
        .provision_pod(ProvisionPodParams {
            project: "blog-api".into(),
            path: "/home/ox/code/blog-api".into(),
            blueprint: "software".into(),
            pod: Some("full".into()),
            budget: Some(25.0),
            verify_cmd: "cargo test --workspace".into(),
        })
        .await
    {
        Ok(v) => Outcome::Ok {
            value: provisioned_pod_to_value(&v),
        },
        Err(e) => err(e),
    };

    let golden = MethodGolden {
        method: "provision_pod",
        requests: transcripts(&server).await,
        result,
    };
    assert_matches_golden("provision_pod", &golden);
}

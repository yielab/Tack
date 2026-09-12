use super::*;
use crate::config::Config;

fn test_client() -> TackClient {
    // Never connects until a request is made; safe for validation-only tests.
    TackClient::new(&Config {
        base_url: "http://127.0.0.1:0".into(),
        token: None,
    })
    .unwrap()
}

#[test]
fn initialize_echoes_protocol_version() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(v["result"]["serverInfo"]["name"], "tack");
    assert_eq!(v["id"], 1);
}

#[test]
fn initialize_defaults_protocol_version() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["protocolVersion"], DEFAULT_PROTOCOL_VERSION);
}

#[test]
fn initialized_notification_has_no_response() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    assert!(resp.is_none());
}

#[test]
fn tools_list_advertises_all_fifteen() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    // The original 8 item/project tools plus 7 execution/fleet/
    // profile tools (list_fleets, list_agent_profiles,
    // list_model_profiles, list_executions, get_execution,
    // cancel_execution, create_execution).
    assert_eq!(tools.len(), 15);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"list_projects"));
    assert!(names.contains(&"move_item"));
    assert!(names.contains(&"add_comment"));
    assert!(names.contains(&"list_fleets"));
    assert!(names.contains(&"list_agent_profiles"));
    assert!(names.contains(&"list_model_profiles"));
    assert!(names.contains(&"list_executions"));
    assert!(names.contains(&"get_execution"));
    assert!(names.contains(&"cancel_execution"));
    assert!(names.contains(&"create_execution"));
    // This module deliberately keeps admin/secret-bearing actions off the MCP
    // surface (see the dispatch-time doc comment) — pin their absence so
    // a future edit can't add them without a second look.
    for excluded in [
        "enroll_runner",
        "revoke_runner",
        "create_fleet",
        "create_agent_profile",
        "create_model_profile",
        "reconcile_execution",
    ] {
        assert!(
            !names.contains(&excluded),
            "{excluded} should not be an MCP tool"
        );
    }
}

#[test]
fn unknown_method_returns_method_not_found() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["error"]["code"], -32601);
}

#[test]
fn parse_error_is_reported() {
    let resp = handle_line(&test_client(), "{not json").unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["error"]["code"], -32700);
}

#[test]
fn tool_call_missing_required_arg_is_tool_error() {
    // Missing `project_id` must fail before any network call.
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_items","arguments":{}}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("project_id"), "unexpected: {text}");
}

#[test]
fn tool_call_unknown_tool_is_tool_error() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("unknown tool"), "unexpected: {text}");
}

#[test]
fn update_item_requires_a_field() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"update_item","arguments":{"id":"x"}}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
}

// ── MCP write path: If-Match ──────────────────────────────────
//
// `update_item`/`move_item` now read the item before writing it so they
// can send back its `ETag` as `If-Match` — closing the race named in
// `docs/plans/agnostic-control-plane.md` trap T2: an agent write via MCP
// was the one path in the whole system with no way to attach a header
// at all, so it was unconditionally last-write-wins even after every
// other writer already had a precondition to send.

// Run a blocking closure on a thread allowed to block. `TackClient` is
// synchronous `reqwest`; calling it directly from a `#[tokio::test]`
// body would block that test's own runtime thread against itself,
// since `wiremock`'s server answers requests on that same runtime.
async fn run_blocking<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("blocking task panicked")
}

fn mock_client(base_url: &str) -> TackClient {
    TackClient::new(&Config {
        base_url: base_url.to_string(),
        token: None,
    })
    .unwrap()
}

#[tokio::test]
async fn mcp_update_item_sends_if_match() {
    let server = wiremock::MockServer::start().await;
    let item_id = "11111111-0000-0000-0000-000000000000";

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("ETag", "\"4\"")
                .set_body_json(json!({ "id": item_id, "title": "before" })),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PATCH"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .and(wiremock::matchers::header("If-Match", "\"4\""))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "id": item_id, "title": "after"
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"after"}}}}}}"#
    );
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(
        v["result"]["isError"], false,
        "unexpected error: {v}\n(a wiremock 404 here means the PATCH did \
         not carry the expected If-Match header)"
    );
}

#[tokio::test]
async fn mcp_move_item_sends_if_match() {
    let server = wiremock::MockServer::start().await;
    let item_id = "22222222-0000-0000-0000-000000000000";

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("ETag", "\"9\"")
                .set_body_json(json!({ "id": item_id, "status": "todo" })),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PATCH"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .and(wiremock::matchers::header("If-Match", "\"9\""))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "id": item_id, "status": "in_progress"
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"move_item","arguments":{{"id":"{item_id}","status":"in_progress"}}}}}}"#
    );
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
}

/// An absent `ETag` on the read must produce an unconditional write
/// (no `If-Match` at all), never a failure — the precondition is
/// opt-in from the server's side, and a route/server that doesn't send
/// an `ETag` yet must keep working exactly as it did before.
#[tokio::test]
async fn mcp_update_item_omits_if_match_when_server_sends_no_etag() {
    struct NoIfMatch;
    impl wiremock::Match for NoIfMatch {
        fn matches(&self, request: &wiremock::Request) -> bool {
            !request.headers.contains_key("if-match")
        }
    }

    let server = wiremock::MockServer::start().await;
    let item_id = "33333333-0000-0000-0000-000000000000";

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(json!({ "id": item_id, "title": "before" })),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PATCH"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .and(NoIfMatch)
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "id": item_id, "title": "after"
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"after"}}}}}}"#
    );
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
}

/// The failure mode this whole path is designed against: a 412 must
/// read as "you raced, re-read and retry," not as an opaque failure an
/// agent might retry blindly and clobber the change that won.
#[tokio::test]
async fn mcp_update_item_412_tells_the_agent_to_reread() {
    let server = wiremock::MockServer::start().await;
    let item_id = "44444444-0000-0000-0000-000000000000";

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("ETag", "\"1\"")
                .set_body_json(json!({ "id": item_id, "title": "stale" })),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("PATCH"))
        .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
        .respond_with(wiremock::ResponseTemplate::new(412).set_body_json(json!({
            "error": { "status": 412, "message": "version mismatch" }
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"new"}}}}}}"#
    );
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("412"), "unexpected: {text}");
    assert!(
        text.to_lowercase().contains("re-read"),
        "must tell the agent to re-read, not just fail: {text}"
    );
}

#[test]
fn compact_item_projects_expected_fields() {
    let full = json!({
        "id": "a", "title": "t", "item_type": "task", "status": "todo",
        "priority": "high", "assignee": "me", "project_id": "p",
        "description": "should be dropped", "tags": ["x"]
    });
    let c = compact_item(&full);
    assert_eq!(c["title"], "t");
    assert!(c.get("description").is_none());
}

// ── Execution/fleet/profile tools ──────────────────────────────

fn create_execution_min_args() -> Value {
    json!({
        "item_id": "item-1",
        "fleet_id": "fleet_1",
        "agent_profile_id": "ap_1",
        "harness": "claude_code",
        "agent_profile_snapshot": {
            "name": "a", "instructions": "be careful", "tool_policy": {},
            "timeout_seconds": 60, "budgets": {}
        },
        "repository": {
            "kind": "git", "remote": "https://example.test/repo.git", "base_revision": "main"
        },
        "permission_policy": { "network": false },
        "timeout_seconds": 3600
    })
}

#[test]
fn create_execution_requires_agent_profile_snapshot_before_any_network_call() {
    let mut args = create_execution_min_args();
    args.as_object_mut()
        .unwrap()
        .remove("agent_profile_snapshot");
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"create_execution","arguments":{}}}}}"#,
        args
    );
    let resp = handle_line(&test_client(), &line).unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("agent_profile_snapshot"),
        "unexpected: {text}"
    );
}

#[test]
fn create_execution_requires_exactly_one_selector() {
    // Neither runner_id nor fleet_id.
    let mut args = create_execution_min_args();
    args.as_object_mut().unwrap().remove("fleet_id");
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"create_execution","arguments":{}}}}}"#,
        args
    );
    let resp = handle_line(&test_client(), &line).unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);

    // Both runner_id and fleet_id.
    let mut both = create_execution_min_args();
    both["runner_id"] = json!("runr_1");
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"create_execution","arguments":{}}}}}"#,
        both
    );
    let resp = handle_line(&test_client(), &line).unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
}

/// End-to-end proof that `create_execution` sends the object-typed JSON
/// blobs an LLM caller would naturally provide (not a stringified
/// second encoding of them) straight through to `POST /api/executions`,
/// and that the resolved `fleet_id` becomes `selector_kind: "fleet"` /
/// `selector_id`.
#[tokio::test]
async fn mcp_create_execution_posts_the_expected_body() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/executions"))
        .and(wiremock::matchers::body_partial_json(json!({
            "item_id": "item-1",
            "selector_kind": "fleet",
            "selector_id": "fleet_1",
            "agent_profile_id": "ap_1",
            "requested_harness_kind": "claude_code",
            "permission_policy": { "network": false },
            "timeout_seconds": 3600
        })))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "protocol_version": 1, "request_id": "exec_1", "state": "queued", "replayed": false
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let args = create_execution_min_args();
    let line = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"create_execution","arguments":{}}}}}"#,
        args
    );
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(
        v["result"]["isError"], false,
        "unexpected error: {v}\n(a wiremock 404 here means the POST body \
         did not match — e.g. a JSON blob was double-encoded as a string)"
    );
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("exec_1"), "unexpected: {text}");
}

#[tokio::test]
async fn mcp_list_executions_unwraps_the_data_envelope() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/executions"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "protocol_version": 1,
            "data": [
                { "request_id": "exec_1", "item_id": "item-1", "state": "queued", "created_at": "2026-01-01T00:00:00Z" }
            ]
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_executions","arguments":{}}}"#.to_string();
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["count"], 1);
    assert_eq!(parsed["executions"][0]["request_id"], "exec_1");
}

#[tokio::test]
async fn mcp_cancel_execution_posts_to_the_cancel_route() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/executions/exec_1/cancel"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "protocol_version": 1, "request_id": "exec_1", "state": "cancellation_requested"
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"cancel_execution","arguments":{"request_id":"exec_1"}}}"#.to_string();
    let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
}

#[test]
fn get_execution_requires_request_id() {
    let resp = handle_line(
        &test_client(),
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_execution","arguments":{}}}"#,
    )
    .unwrap();
    let v: Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], true);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("request_id"), "unexpected: {text}");
}

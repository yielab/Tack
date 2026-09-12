//! Tests for the template `orchestration` block: `POST /api/templates`'s
//! save-time validation.
//!
//! Covers: `orchestration.status_map` rejected with a 400 naming the bad
//! key when it references a status the template's own workflow doesn't
//! have; `orchestration.pipeline_yaml` rejected when it isn't parseable
//! YAML; and backward compatibility — an absent/null `orchestration` key
//! behaves exactly as before, with no `TACK_ORCH_ENABLE` dependency.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn req(app: &Router, method: Method, uri: &str, body: Value) -> axum::response::Response {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");
    app.clone()
        .oneshot(
            builder
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// A template with no `orchestration` key at all — the shape every template
/// had before `orchestration` was added — and one with an explicit
/// `"orchestration": null` are the same absent-means-nothing case: both must
/// keep working unchanged. This is TACK_ORCH_ENABLE-independent: the default
/// test app has orchestration disabled entirely, and both must still
/// succeed.
#[tokio::test]
async fn create_template_without_or_with_null_orchestration_works() {
    let cases = [None, Some(Value::Null)];
    for orchestration in cases {
        let (app, _workspace_id) = common::test_app().await;
        let mut body = json!({"name": "Plain Template", "project_type": "software"});
        if let Some(v) = orchestration {
            body["orchestration"] = v;
        }

        let res = req(&app, Method::POST, "/api/templates", body).await;
        assert_eq!(res.status(), StatusCode::OK);
        let created = body_json(res).await;
        assert!(created.get("orchestration").is_none_or(|v| v.is_null()));
    }
}

/// The main requirement: an unknown status name in
/// `orchestration.status_map` is rejected with a 400 naming the bad key —
/// not silently stored, not caught only later when a dispatch misbehaves.
#[tokio::test]
async fn create_template_rejects_unknown_status_map_name() {
    let (app, _workspace_id) = common::test_app().await;

    let res = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Bad Status Map Template",
            "project_type": "software",
            // No explicit workflow -> falls back to simple_workflow()
            // ("To Do" / "Doing" / "Done").
            "orchestration": {
                "status_map": {
                    "dispatch_from": ["Ready"], // not a real status
                }
            },
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Ready"),
        "error should name the bad key, got: {message}"
    );
}

/// `status_map` is validated against the workflow *this template will
/// actually create* — not `simple_workflow()`'s default names. Supplying a
/// custom `workflow` whose only status is "Backlog" and pointing
/// `dispatch_from` at "Backlog" must succeed; pointing it at "To Do" (a
/// `simple_workflow()` name that isn't in *this* template's workflow) must
/// fail.
#[tokio::test]
async fn create_template_validates_status_map_against_own_workflow() {
    let (app, _workspace_id) = common::test_app().await;

    let custom_workflow = json!({
        "workflow_type": "kanban",
        "statuses": [
            { "name": "Backlog", "category": "todo", "wip_limit": null, "order": 0 },
            { "name": "Building", "category": "in_progress", "wip_limit": null, "order": 1 },
            { "name": "Shipped", "category": "done", "wip_limit": null, "order": 2 },
        ],
        "transitions": null,
    });

    // References a status that exists only in `simple_workflow()`, not in
    // this template's own custom workflow -> rejected.
    let bad = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Custom Workflow, Wrong Status Map",
            "project_type": "software",
            "workflow": custom_workflow,
            "orchestration": { "status_map": { "dispatch_from": ["To Do"] } },
        }),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // References a status that IS in this template's own workflow -> ok.
    let good = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Custom Workflow, Right Status Map",
            "project_type": "software",
            "workflow": custom_workflow,
            "orchestration": {
                "status_map": {
                    "dispatch_from": ["Backlog"],
                    "on_succeeded": "Shipped"
                }
            },
        }),
    )
    .await;
    assert_eq!(good.status(), StatusCode::OK);
}

/// `pipeline_yaml`, when supplied inline, must at least parse as YAML.
/// (Deliberately not a check against docket's real pipeline schema — see
/// `handlers::templates::validate_template_orchestration`'s doc comment for
/// why.)
#[tokio::test]
async fn create_template_rejects_unparseable_pipeline_yaml() {
    let (app, _workspace_id) = common::test_app().await;

    let res = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Broken Pipeline Template",
            "project_type": "software",
            "orchestration": {
                // Unbalanced flow-mapping brace -> not valid YAML at all.
                "pipeline_yaml": "name: demo\nsteps: [ { id: lead\n",
            },
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

/// A fully populated, valid orchestration block round-trips through create
/// + get + list, including the blueprint enum's exact wire name.
#[tokio::test]
async fn create_template_with_valid_orchestration_round_trips() {
    let (app, _workspace_id) = common::test_app().await;

    let res = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Agentic Product Template",
            "project_type": "software",
            "orchestration": {
                "blueprint": "agentic-product",
                "pipeline_yaml": "name: demo\nsteps:\n  - id: lead\n",
                "verify_cmd": "cargo test --workspace",
                "budget_usd": 25.0,
                "status_map": {
                    "dispatch_from": ["To Do"],
                    "on_running": "Doing",
                    "on_succeeded": "Done"
                },
                "auto_dispatch": true,
                "pod_shape": "full"
            },
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let created = body_json(res).await;
    let orch = &created["orchestration"];
    assert_eq!(orch["blueprint"], "agentic-product");
    assert_eq!(orch["budget_usd"], 25.0);
    assert_eq!(orch["auto_dispatch"], true);
    assert_eq!(orch["status_map"]["dispatch_from"][0], "To Do");

    let template_id = created["id"].as_str().unwrap();
    let get_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/templates/{template_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
    let fetched = body_json(get_res).await;
    assert_eq!(fetched["orchestration"]["blueprint"], "agentic-product");
}

/// An invalid `orchestration.status_map` must not create the template at
/// all — the whole request is rejected, not stored-then-flagged.
#[tokio::test]
async fn rejected_orchestration_template_is_not_persisted() {
    let (app, _workspace_id) = common::test_app().await;

    let res = req(
        &app,
        Method::POST,
        "/api/templates",
        json!({
            "name": "Should Not Exist",
            "project_type": "software",
            "orchestration": { "status_map": { "dispatch_from": ["Nonexistent"] } },
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let list_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/templates")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list = body_json(list_res).await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"Should Not Exist"));
}

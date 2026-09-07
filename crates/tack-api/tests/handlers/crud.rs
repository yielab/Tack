//! CRUD and lifecycle coverage for the operator-facing HTTP handlers: health,
//! the API token gate, request body limits, input validation, vocabulary and
//! workflow configuration, backup/restore, the embedded SPA, custom fields,
//! board filtering, item update/delete, sprints, roles, comments,
//! dependencies, search, export, item provenance, and the GitHub push sync.

use crate::common;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tack_api::config::AppConfig;
use tower::ServiceExt;

// ─── Health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_response_contains_version_and_migration_count() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string(), "version must be a string");
    assert!(
        json["migrations_applied"].as_i64().unwrap_or(0) > 0,
        "migrations_applied must be positive"
    );
}

// ─── API token ───────────────────────────────────────────────────────

#[tokio::test]
async fn no_token_configured_allows_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Without a token configured, the API is open
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn correct_token_allows_request() {
    let config = AppConfig {
        api_token: Some("secret-test-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .header("Authorization", "Bearer secret-test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_rejected() {
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_token_rejected() {
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_bypasses_token_check() {
    // Even with a token configured, /api/health must remain public
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// ─── Body limit ──────────────────────────────────────────────────────

#[tokio::test]
async fn oversized_body_rejected() {
    let config = AppConfig {
        max_body_size_bytes: 512, // very small for this test
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let big_body = "x".repeat(1024); // 1 KB > 512 B limit
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(big_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

// ─── Input validation ────────────────────────────────────────────────

#[tokio::test]
async fn create_project_empty_name_rejected() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_project_valid_accepted() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"My Project","project_type":"software"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_item_empty_title_rejected() {
    let (app, workspace_id) = common::test_app().await;

    // First create a project
    let create_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"P","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::OK);

    // Extract project ID from body
    let body_bytes = axum::body::to_bytes(create_res.into_body(), 65536)
        .await
        .unwrap();
    let project: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let project_id = project["id"].as_str().expect("id field");
    let _ = workspace_id; // used via app state

    // Now try to create an item with empty title
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ─── Vocabulary + workflow ───────────────────────────────────────────

async fn create_test_project(app: &axum::Router) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"Vocab Project","project_type":"software"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn update_project_vocabulary_persists() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let project_id = create_test_project(&app).await;

    let patch = serde_json::json!({
        "vocabulary": { "task": "Work Order", "sprint": "Phase" }
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/projects/{project_id}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&patch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["vocabulary"]["task"], "Work Order");
    assert_eq!(v["vocabulary"]["sprint"], "Phase");
}

#[tokio::test]
async fn update_project_workflow_statuses_valid() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let project_id = create_test_project(&app).await;

    let patch = serde_json::json!({
        "workflow": {
            "workflow_type": "custom",
            "statuses": [
                { "name": "Queue",  "category": "todo",        "wip_limit": null, "order": 0 },
                { "name": "Active", "category": "in_progress", "wip_limit": 3,    "order": 1 },
                { "name": "Done",   "category": "done",        "wip_limit": null, "order": 2 }
            ],
            "transitions": null
        }
    });
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/projects/{project_id}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&patch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let statuses = v["workflow"]["statuses"].as_array().unwrap();
    assert_eq!(statuses.len(), 3);
    assert_eq!(statuses[1]["name"], "Active");
    assert_eq!(statuses[1]["wip_limit"], 3);
}

// ─── Backup / restore ────────────────────────────────────────────────

#[tokio::test]
async fn backup_in_memory_db_returns_bad_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/backup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restore_invalid_bytes_returns_bad_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/restore")
                .header("content-type", "application/octet-stream")
                .body(Body::from("not a sqlite file"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backup_roundtrip_with_file_db() {
    use axum::body::to_bytes;
    use std::path::PathBuf;

    let tmp_dir = tempfile::tempdir().expect("temporary directory");
    let db_path = tmp_dir.path().join("test.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let (app, _) = common::test_app_with_file_db(&db_url).await;

    // Backup should succeed and return a SQLite file.
    let backup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/backup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup_res.status(), StatusCode::OK);

    let backup_bytes = to_bytes(backup_res.into_body(), usize::MAX).await.unwrap();
    assert!(
        backup_bytes.starts_with(b"SQLite format 3\x00"),
        "backup must be a valid SQLite file"
    );

    // Staging the backup should succeed and write a .restore file.
    let restore_res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/restore")
                .header("content-type", "application/octet-stream")
                .body(Body::from(backup_bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore_res.status(), StatusCode::OK);

    let restore_path = PathBuf::from(format!("{}.restore", db_path.display()));
    assert!(restore_path.exists(), ".restore file should be staged");

    // Clean up
}

// ─── Embedded SPA — only compiled with --features embed-spa ──────────

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn spa_root_serves_html() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"), "expected text/html, got {ct}");
}

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn spa_unknown_route_returns_index_html() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/projects/some-client-side-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(
        ct.contains("text/html"),
        "SPA fallback must return index.html"
    );
}

// ─── Custom field value validation (handler integration) ─────────────────────

/// Helper: create a project and return its id string.
async fn make_project(app: &axum::Router) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"P","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let p: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    p["id"].as_str().unwrap().to_owned()
}

/// Helper: create a custom field and return its id string.
async fn make_custom_field(app: &axum::Router, project_id: &str, body: &str) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/custom-fields"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let f: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    f["id"].as_str().unwrap().to_owned()
}

/// Helper: create a default item and return its id string.
async fn make_item(app: &axum::Router, project_id: &str) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":"Item","item_type":"task"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let i: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    i["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn restore_rejects_non_sqlite_body_with_structured_error_envelope() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/restore")
                .body(Body::from("this is not a database"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Unified envelope: { "error": { "status", "message" } } — not a flat string.
    assert_eq!(body["error"]["status"], 400);
    assert!(
        body["error"]["message"].is_string(),
        "message must be a human-readable string, got: {body}"
    );
}

#[tokio::test]
async fn backup_settings_validation_returns_structured_422_envelope() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/settings/backup")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"retention":0}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["status"], 422);
    assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn list_items_returns_pagination_envelope_and_slices_pages() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    // Three items so a per_page=2 page 1 has a remainder on page 2.
    for _ in 0..3 {
        make_item(&app, &pid).await;
    }

    // Page 1: envelope shape + total count + first slice.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/items?per_page=2&page=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let page1: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page1["total"], 3, "total must count all matching items");
    assert_eq!(page1["page"], 1);
    assert_eq!(page1["per_page"], 2);
    assert_eq!(
        page1["data"].as_array().unwrap().len(),
        2,
        "page 1 holds per_page items"
    );

    // Page 2: the remaining slice.
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/items?per_page=2&page=2"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let page2: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page2["total"], 3);
    assert_eq!(page2["page"], 2);
    assert_eq!(
        page2["data"].as_array().unwrap().len(),
        1,
        "page 2 holds the remaining item"
    );
}

#[tokio::test]
async fn set_custom_field_value_correct_type_returns_ok() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(&app, &pid, r#"{"name":"Score","field_type":"number"}"#).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from("42"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_custom_field_value_wrong_type_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(&app, &pid, r#"{"name":"Score","field_type":"number"}"#).await;
    let iid = make_item(&app, &pid).await;

    // Send a string value to a Number field — must be rejected
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""not a number""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_select_invalid_option_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Priority","field_type":"select","options":["Low","High"]}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""Critical""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_value_passes_pattern_validation() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Code","field_type":"text","validation":{"pattern":"^[A-Z]{3}$"}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""ABC""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_custom_field_value_fails_pattern_validation() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Code","field_type":"text","validation":{"pattern":"^[A-Z]{3}$"}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""lowercase""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_number_out_of_range_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Score","field_type":"number","validation":{"min":0,"max":100}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from("150"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ─── Board filter integration ─────────────────────────────────────────────────

#[tokio::test]
async fn board_view_filter_by_item_type_returns_only_matching_items() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    // Create a task and a bug
    for (title, item_type) in [("Task A", "task"), ("Bug B", "bug")] {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/projects/{pid}/items"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"title":"{title}","item_type":"{item_type}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // Create a board that filters to only "task" items
    let board_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/boards"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"Tasks Only","filters":{"item_type":"task"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(board_res.into_body(), 65536).await.unwrap();
    let board: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let board_id = board["id"].as_str().unwrap();

    // Fetch the board view
    let view_res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/boards/{board_id}/view"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view_res.status(), StatusCode::OK);
    let bytes = to_bytes(view_res.into_body(), 65536).await.unwrap();
    let view: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // All items across all columns must be of type "task"
    let all_items: Vec<&serde_json::Value> = view["columns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|col| col["items"].as_array().unwrap())
        .collect();
    assert!(!all_items.is_empty(), "board should have at least one item");
    assert!(
        all_items.iter().all(|i| i["item_type"] == "task"),
        "all items must be of type 'task', got: {:?}",
        all_items
            .iter()
            .map(|i| &i["item_type"])
            .collect::<Vec<_>>()
    );
}

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn api_routes_take_priority_over_spa_fallback() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

// ─── Item update and delete ───────────────────────────────────────────────────

#[tokio::test]
async fn update_item_title_persists() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/items/{iid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":"Updated Title"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["item"]["title"], "Updated Title");
}

#[tokio::test]
async fn update_item_status_moves_to_in_progress() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/items/{iid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"In Progress"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["item"]["status"], "In Progress");
}

#[tokio::test]
async fn delete_item_returns_404_on_subsequent_get() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

// ─── Sprint lifecycle ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_sprint_appears_in_list() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/sprints"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Sprint 1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/sprints"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let sprints: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sprints.as_array().unwrap().len(), 1);
    assert_eq!(sprints[0]["name"], "Sprint 1");
    assert_eq!(sprints[0]["status"], "planning");
}

#[tokio::test]
async fn sprint_status_transitions_to_active() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/sprints"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Sprint A"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let sprint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sid = sprint["id"].as_str().unwrap().to_owned();

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/sprints/{sid}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"active"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/sprints/{sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let s: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(s["status"], "active");
}

#[tokio::test]
async fn editing_a_sprint_rewrites_its_fields_and_clears_the_ones_left_out() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/sprints"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"Sprint A","goal":"Ship the MVP","start_date":"2026-01-01T00:00:00Z"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let sprint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sid = sprint["id"].as_str().unwrap().to_owned();

    // The edit form sends every editable field it holds, so a goal and a start
    // date the user emptied arrive omitted and must end up NULL — not left at
    // their old values.
    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/sprints/{sid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Sprint A, renamed"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/sprints/{sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let s: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(s["name"], "Sprint A, renamed");
    assert!(
        s["goal"].is_null(),
        "an omitted goal must clear, got {}",
        s["goal"]
    );
    assert!(
        s["start_date"].is_null(),
        "an omitted start_date must clear, got {}",
        s["start_date"]
    );
    // The edit route leaves the lifecycle alone; only the status route moves it.
    assert_eq!(s["status"], "planning");
}

#[tokio::test]
async fn editing_a_sprint_that_does_not_exist_is_a_404() {
    let (app, _) = common::test_app().await;
    let missing = uuid::Uuid::new_v4();

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/sprints/{missing}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Nowhere"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ─── Role CRUD ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_roles() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/roles"))
                .header("Content-Type", "application/json")
                .body(Body::from(r##"{"name":"Backend Dev","color":"#3B82F6"}"##))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/roles"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let roles: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(roles.as_array().unwrap().len(), 1);
    assert_eq!(roles[0]["name"], "Backend Dev");
}

// ─── Comment CRUD ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_comments() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{iid}/comments"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"content":"Great progress!","author":"alice"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}/comments"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let comments: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["content"], "Great progress!");
    assert_eq!(comments[0]["author"], "alice");
}

// ─── Dependencies ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_dependency_blocks_relationship() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let item_a = make_item(&app, &pid).await;
    let item_b = make_item(&app, &pid).await;

    let body = format!(r#"{{"target_item_id":"{item_b}","dependency_type":"blocks"}}"#);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{item_a}/dependencies"))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{item_a}/dependencies"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let deps: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(deps.as_array().unwrap().len(), 1);
    assert_eq!(deps[0]["dependency_type"], "blocks");
}

#[tokio::test]
async fn self_dependency_rejected() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let body = format!(r#"{{"target_item_id":"{iid}","dependency_type":"blocks"}}"#);
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{iid}/dependencies"))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ─── Search ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn project_search_finds_matching_item() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    // Create an item with a distinctive title
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"title":"xyzzy unique search token","item_type":"task"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/search?q=xyzzy"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let results: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = results.as_array().unwrap();
    assert!(!arr.is_empty(), "search should return at least one result");
    assert!(
        arr[0]["title"].as_str().unwrap().contains("xyzzy"),
        "first result should contain the search token"
    );
}

#[tokio::test]
async fn global_search_finds_item_across_projects() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"title":"qwerty global search token","item_type":"task"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/search?q=qwerty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let results: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = results.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "global search should return at least one result"
    );
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn export_json_contains_project_and_items() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let export: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        export["project"].is_object(),
        "export must contain a project object"
    );
    assert!(
        export["items"].is_array(),
        "export must contain an items array"
    );
    assert_eq!(export["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn export_csv_starts_with_header_row() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=csv"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let csv = std::str::from_utf8(&bytes).unwrap();
    let first_line = csv.lines().next().unwrap_or("");
    assert!(
        first_line.contains("id") && first_line.contains("title"),
        "CSV header must contain id and title columns, got: {first_line}"
    );
    assert!(
        csv.lines().count() >= 2,
        "CSV must have header + at least one data row"
    );
}

#[tokio::test]
async fn export_yaml_round_trips_through_import() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    // Export as YAML.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=yaml"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let yaml = String::from_utf8(bytes.to_vec()).unwrap();
    // It must be YAML (block mappings), not JSON braces.
    assert!(
        yaml.contains("project:") && yaml.contains("items:"),
        "got: {yaml}"
    );
    let parsed: serde_json::Value = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        parsed["items"][0]["source"], "manual",
        "an ordinarily-created item's provenance marker must round-trip through export"
    );

    // Import the same YAML back: a new project is created with the item.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects/import")
                .header("Content-Type", "application/x-yaml")
                .body(Body::from(yaml))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(out["success"], true, "import response: {out}");
    let new_pid = out["project"]["id"].as_str().unwrap();
    assert_ne!(new_pid, pid, "import must create a new project");

    // The imported project has the round-tripped item.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{new_pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let items: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        items["data"].as_array().unwrap().len(),
        1,
        "imported items: {items}"
    );
    assert_eq!(
        items["data"][0]["source"], "manual",
        "the imported item must still be recorded as manual/trusted, not reset to unknown"
    );
}

// ─── Item provenance / trust boundary ────────────────────────────

/// An item imported
/// from GitHub is marked untrusted at creation time, and that marker
/// survives an export → import round trip rather than resetting to
/// trusted. (The wire-level "docket sees trusted:false" assertion lives in
/// `crates/tack-api/tests/orchestration/auto_dispatch/hook.rs` and
/// `orchestration/dispatch/item.rs`
/// — this test covers the provenance marker itself, end to end through the
/// real HTTP import/export/import path.)
#[tokio::test]
async fn github_imported_item_source_is_untrusted_and_survives_export_import_round_trip() {
    use axum::body::to_bytes;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "number": 7, "title": "Untrusted issue", "body": "", "state": "open",
                "labels": [], "assignee": null,
                "html_url": "https://github.com/acme/widgets/issues/7"
            }
        ])))
        .mount(&gh)
        .await;

    let config = AppConfig {
        github_api_base: gh.uri(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/import-github"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"repo":"acme/widgets"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let items: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        items["data"][0]["source"], "github",
        "an item imported from GitHub must be recorded with source: github"
    );

    // Export the linked project, then re-import that snapshot into a fresh
    // project — the item's `source` must survive, not reset to `manual`.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let export_bytes = to_bytes(res.into_body(), 131072).await.unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects/import")
                .header("Content-Type", "application/json")
                .body(Body::from(export_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let new_pid = out["project"]["id"].as_str().unwrap();
    assert_ne!(new_pid, pid);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{new_pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let reimported: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        reimported["data"][0]["source"], "github",
        "the trust marker must survive an export -> import round trip, never reset to trusted"
    );
}

#[tokio::test]
async fn github_import_redirect_does_not_leak_user_token_to_private_destination() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let origin = MockServer::start().await;
    let private_destination = MockServer::start().await;
    let redirect_target = format!("{}/instance-metadata", private_destination.uri());

    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(header("authorization", "Bearer user-pat"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", redirect_target))
        .mount(&origin)
        .await;

    let config = AppConfig {
        github_api_base: origin.uri(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = make_project(&app).await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/import-github"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"repo":"acme/widgets","token":"user-pat"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        private_destination
            .received_requests()
            .await
            .expect("inspect private destination")
            .is_empty(),
        "a redirect must never forward a user-supplied GitHub token"
    );
}

#[tokio::test]
async fn csv_import_marks_items_with_csv_import_source() {
    use axum::body::to_bytes;

    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let csv = "title,description\nFrom a spreadsheet,could be anyone's data\n";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/import-csv"))
                .header("Content-Type", "text/csv")
                .body(Body::from(csv))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let items: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        items["data"][0]["source"], "csv_import",
        "a CSV-imported row must be recorded with source: csv_import (untrusted for dispatch)"
    );
}

// ─── GitHub push sync ───────────────────────────────────────────────

#[tokio::test]
async fn github_import_links_items_then_completion_pushes_close() {
    use axum::body::to_bytes;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;

    // GitHub issues list returns one open issue (#42).
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "number": 42, "title": "Fix the thing", "body": "", "state": "open",
                "labels": [], "assignee": null,
                "html_url": "https://github.com/acme/widgets/issues/42"
            }
        ])))
        .mount(&gh)
        .await;

    // The close we expect once the item is completed.
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/widgets/issues/42"))
        .and(body_json(serde_json::json!({ "state": "closed" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "number": 42 })))
        .mount(&gh)
        .await;

    let config = AppConfig {
        github_token: Some("tok".into()),
        github_api_base: gh.uri(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = make_project(&app).await;

    // Import → creates one item linked to issue #42.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/import-github"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"repo":"acme/widgets"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Grab the imported item.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let items: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let item_id = items["data"].as_array().unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Move it to Done → fires a best-effort close to GitHub.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/items/{item_id}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"Done"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // The push is fire-and-forget; poll the mock until the PATCH to #42 arrives.
    let mut closed = false;
    for _ in 0..40 {
        let reqs = gh.received_requests().await.unwrap_or_default();
        if reqs
            .iter()
            .any(|r| r.url.path() == "/repos/acme/widgets/issues/42")
        {
            closed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        closed,
        "expected a PATCH closing GitHub issue #42 after completion"
    );
}

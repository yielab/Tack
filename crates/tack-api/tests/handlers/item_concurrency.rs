//! Optimistic concurrency for items —
//! `GET /api/items/{id}` returns an `ETag` derived from the item's id +
//! `version` (migration 034); `PATCH /api/items/{id}` honours `If-Match`,
//! rejecting a stale value with `412 Precondition Failed`. A `PATCH` with no
//! `If-Match` header still succeeds unchanged, for callers that predate it.
//!
//! **The gate is the sequential tests, not the concurrent ones** — each
//! sequential test's own doc comment explains what it proves and why; see
//! `docs/plans/agnostic-control-plane.md` for the full reasoning.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers (mirrors security/board_drag_wip_race.rs) ─────────────────────

async fn app_with_state() -> (Router, AppState) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..AppConfig::default()
    };
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };

    (build_router(state.clone()), state)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    req_with_if_match(app, method, uri, body, None).await
}

async fn req_with_if_match(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    if_match: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(etag) = if_match {
        builder = builder.header("If-Match", etag);
    }
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn create_item(app: &Router, project_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

/// `GET`s the item and returns the `ETag` response header, panicking if the
/// header is missing — every `GET /api/items/{id}` response must carry one.
async fn get_etag(app: &Router, item_id: Uuid) -> String {
    let res = req(app, Method::GET, &format!("/api/items/{item_id}"), None).await;
    assert_eq!(res.status(), StatusCode::OK);
    res.headers()
        .get("etag")
        .expect("GET /api/items/{id} must return an ETag header")
        .to_str()
        .unwrap()
        .to_string()
}

// ─── GET returns an ETag ────────────────────────────────────────────────

#[tokio::test]
async fn get_item_returns_an_etag_derived_from_id_and_version() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Has an ETag").await;

    let etag = get_etag(&app, item_id).await;
    // The ETag format: a quoted "<id>-<version>" string. Not a public
    // contract clients should parse — just confirming it embeds what the
    // doc comment says it embeds, for a freshly created (version 1) item.
    assert_eq!(etag, format!("\"{item_id}-1\""));
}

// ─── Absent If-Match: unchanged behavior ───────────────────────────────

#[tokio::test]
async fn patch_with_no_if_match_header_succeeds_unchanged() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "No If-Match sent").await;

    let res = req(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Renamed with no If-Match"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    assert_eq!(v["title"], "Renamed with no If-Match");

    // A second such PATCH must also succeed — an absent If-Match is not a
    // one-shot allowance, it means "no concurrency check requested" every
    // time, for every caller that never adopts the header (MCP tools, any
    // client that predates the If-Match header).
    let res2 = req(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Renamed again, still no If-Match"})),
    )
    .await;
    assert_eq!(res2.status(), StatusCode::OK, "{:?}", body_json(res2).await);
}

// ─── Present If-Match: matching proceeds, stale is rejected ───────────────

#[tokio::test]
async fn patch_with_a_matching_if_match_succeeds() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Matching If-Match").await;

    let etag = get_etag(&app, item_id).await;
    let res = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Renamed with a matching ETag"})),
        Some(&etag),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

#[tokio::test]
async fn stale_if_match_is_rejected_with_412_and_standard_envelope() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Stale If-Match").await;

    let etag = get_etag(&app, item_id).await;

    // First PATCH with the fresh ETag succeeds and moves the version.
    let res = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "First edit"})),
        Some(&etag),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    // Reusing the now-stale ETag must be rejected, not silently accepted.
    let res2 = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Second edit, stale precondition"})),
        Some(&etag),
    )
    .await;
    assert_eq!(
        res2.status(),
        StatusCode::PRECONDITION_FAILED,
        "reusing a stale If-Match must be rejected with 412, not silently applied"
    );
    let body = body_json(res2).await;
    assert_eq!(body["error"]["status"], 412);
    assert!(
        body["error"]["message"].as_str().is_some(),
        "the standard {{error:{{status,message}}}} envelope must be present: {body:?}"
    );

    // And the item's title was not touched by the rejected PATCH.
    let res3 = req(&app, Method::GET, &format!("/api/items/{item_id}"), None).await;
    let v3 = body_json(res3).await;
    assert_eq!(v3["item"]["title"], "First edit");
}

#[tokio::test]
async fn patch_with_an_if_match_for_a_different_item_is_rejected() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_a = create_item(&app, project_id, "Item A").await;
    let item_b = create_item(&app, project_id, "Item B").await;

    let etag_a = get_etag(&app, item_a).await;

    // item_b starts at the same version (1) as item_a, but the ETag embeds
    // the id — a same-version, wrong-id ETag must not accidentally match.
    let res = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_b}"),
        Some(json!({"title": "Should be rejected"})),
        Some(&etag_a),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::PRECONDITION_FAILED,
        "an If-Match minted for a different item must not match this one's version by \
         coincidence"
    );
}

/// The deterministic proof that it is specifically the `If-Match` *value*
/// that gates a write — not merely "some atomic claim happened underneath".
/// Unlike the two concurrent tests below, this issues its `PATCH`es one at a
/// time, in strict sequence: capture an `ETag`, let the first `PATCH` land
/// and fully complete, then replay that now-stale `ETag` on a second,
/// independent `PATCH`. There is no race here for a scheduler to decide —
/// if `check_if_match` only performed the atomic version-claim and never
/// actually compared `provided` against the header's expected value, the
/// second `PATCH` would simply read the item's current version fresh,
/// claim it uncontested, and return `200`. That makes this test fail 100%
/// of the time against that mutation, with no scheduler dependence —
/// verified deterministic over 20+ local runs (see
/// `docs/plans/agnostic-control-plane.md` for the full reasoning).
#[tokio::test]
async fn stale_if_match_is_rejected_with_412_with_no_racer() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "No racer, just a stale header").await;

    let etag = get_etag(&app, item_id).await;

    // The first PATCH lands and fully completes before the second is even
    // constructed — sequential, not concurrent.
    let first = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "First write, using the fresh ETag"})),
        Some(&etag),
    )
    .await;
    assert_eq!(
        first.status(),
        StatusCode::OK,
        "the first PATCH carries the fresh ETag and must succeed: {:?}",
        body_json(first).await
    );

    // Reusing the pre-write ETag now must be rejected on its value alone —
    // nothing is racing this request, so there is no CAS-layer coincidence
    // available to explain a 412 away.
    let second = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Second write, stale ETag, no racer"})),
        Some(&etag),
    )
    .await;
    assert_eq!(
        second.status(),
        StatusCode::PRECONDITION_FAILED,
        "a stale If-Match must be rejected by its value alone, with no concurrent racer to \
         explain the rejection away as a coincidental CAS loss"
    );
}

// ─── The headline test: real concurrency, one winner, one loser ───────────

/// **What this proves:** two genuinely concurrent `PATCH`es, same item, same
/// `If-Match` value (both fetched from the same prior `GET`), race
/// `claim_item_version`'s atomic `UPDATE ... WHERE version = ?`. However the
/// two requests interleave, SQLite's writer serialization means only one of
/// the two compare-and-swap calls can succeed — the loser must see `412`,
/// never a second `200` and never a `500`. That is a real, worthwhile
/// property of the CAS layer underneath `check_if_match`.
///
/// **What this does NOT prove:** that the `If-Match` header's *value* is
/// what gates the write. Both racers here start from the same freshly-read
/// version regardless of what string either one actually sent, so an
/// implementation that silently drops the header-value comparison but still
/// funnels every `PATCH` through `claim_item_version` reproduces the
/// identical one-`200`/one-`412` shape this test watches for. A mutation
/// that drops the header-value comparison passed this test most of the
/// time in practice (caught only 5/15 runs). For a deterministic proof that
/// the header's value — not merely the presence of a race — decides the
/// outcome, see the sequential tests instead:
/// `stale_if_match_is_rejected_with_412_and_standard_envelope`,
/// `patch_with_an_if_match_for_a_different_item_is_rejected`, and
/// `stale_if_match_is_rejected_with_412_with_no_racer`.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_concurrent_patches_on_one_version_yield_one_cas_winner() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Race target").await;
    let etag = get_etag(&app, item_id).await;

    let mut handles = Vec::with_capacity(2);
    for i in 0..2 {
        let app = app.clone();
        let etag = etag.clone();
        handles.push(tokio::spawn(async move {
            req_with_if_match(
                &app,
                Method::PATCH,
                &format!("/api/items/{item_id}"),
                Some(json!({"title": format!("Racer {i}")})),
                Some(&etag),
            )
            .await
            .status()
        }));
    }

    let mut statuses = Vec::with_capacity(2);
    for h in handles {
        statuses.push(h.await.expect("PATCH task panicked"));
    }
    statuses.sort_by_key(|s| s.as_u16());

    assert_eq!(
        statuses,
        vec![StatusCode::OK, StatusCode::PRECONDITION_FAILED],
        "exactly one of two concurrent PATCHes racing the same starting version must win \
         (200) and the other must lose (412) — an implementation with no real concurrency \
         control would return 200 twice, and a torn/non-atomic check could return 412 twice \
         or panic into a 500"
    );
}

/// A larger-fanout variant of the CAS-layer race above: `N` `PATCH`es racing
/// one still-valid version must still yield exactly one `200`, since
/// `claim_item_version`'s atomic `UPDATE ... WHERE version = ?` only lets one
/// writer through per version, however many are racing it.
///
/// Like the pair-sized race above, this is a property of the CAS layer, not
/// proof that `If-Match`'s *value* is enforced: an implementation that
/// accepts the header without ever comparing it, but still funnels every
/// `PATCH` through `claim_item_version`, can still produce more than one
/// winner here — a racer that reads the version *after* an earlier racer in
/// the same batch has already committed sees a new "current" version and
/// wins again too. That mutation was caught more often with this test
/// (7/10 runs) than with the pair test above, but still not every time.
/// Treat this as a more sensitive smoke
/// test for the same failure mode, not a deterministic gate — see
/// `stale_if_match_is_rejected_with_412_and_standard_envelope`,
/// `patch_with_an_if_match_for_a_different_item_is_rejected`, and
/// `stale_if_match_is_rejected_with_412_with_no_racer` for that.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_patches_at_higher_fanout_yield_one_winner() {
    const N: usize = 6;
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Race target, N racers").await;
    let etag = get_etag(&app, item_id).await;

    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let app = app.clone();
        let etag = etag.clone();
        handles.push(tokio::spawn(async move {
            req_with_if_match(
                &app,
                Method::PATCH,
                &format!("/api/items/{item_id}"),
                Some(json!({"title": format!("Racer {i}")})),
                Some(&etag),
            )
            .await
            .status()
        }));
    }

    let mut ok_count = 0;
    let mut precondition_failed_count = 0;
    for h in handles {
        match h.await.expect("PATCH task panicked") {
            StatusCode::OK => ok_count += 1,
            StatusCode::PRECONDITION_FAILED => precondition_failed_count += 1,
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(ok_count, 1, "exactly one racer must win the claim");
    assert_eq!(precondition_failed_count, N - 1);
}

// ─── Atomic PATCH invariants ────────────────────────────────────────────────

#[tokio::test]
async fn multi_field_wip_rejection_writes_nothing_no_version_bump() {
    let (app, state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    for i in 0..5 {
        let item_id = create_item(&app, project_id, &format!("Capacity {i}")).await;
        let moved = req(
            &app,
            Method::PATCH,
            &format!("/api/items/{item_id}"),
            Some(json!({"status": "In Progress"})),
        )
        .await;
        assert_eq!(moved.status(), StatusCode::OK);
    }
    let target = create_item(&app, project_id, "Must remain unchanged").await;
    let before = state.repo.get_item_version(target).await.unwrap().unwrap();

    let rejected = req(
        &app,
        Method::PATCH,
        &format!("/api/items/{target}"),
        Some(json!({"title": "Must not land", "status": "In Progress"})),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let after = req(&app, Method::GET, &format!("/api/items/{target}"), None).await;
    let item = body_json(after).await["item"].clone();
    assert_eq!(item["title"], "Must remain unchanged");
    assert_eq!(item["status"], "Backlog");
    assert_eq!(
        state.repo.get_item_version(target).await.unwrap().unwrap(),
        before
    );
}

#[tokio::test]
async fn nullable_fields_clear_and_body_etag_match_one_snapshot() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Nullable fields").await;
    let etag = get_etag(&app, item_id).await;

    let seeded = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"description": "note", "assignee": "Ada", "estimate": 3.5})),
        Some(&etag),
    )
    .await;
    assert_eq!(seeded.status(), StatusCode::OK);
    let seed_etag = seeded
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let cleared = req_with_if_match(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"description": null, "assignee": null, "estimate": null})),
        Some(&seed_etag),
    )
    .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    assert_eq!(
        cleared.headers().get("etag").unwrap().to_str().unwrap(),
        format!("\"{item_id}-3\""),
        "one multi-field PATCH increments version once"
    );
    let body = body_json(cleared).await;
    assert!(body["description"].is_null());
    assert!(body["assignee"].is_null());
    assert!(body["estimate"].is_null());

    let fresh = req(&app, Method::GET, &format!("/api/items/{item_id}"), None).await;
    assert_eq!(
        fresh.headers().get("etag").unwrap().to_str().unwrap(),
        format!("\"{item_id}-3\""),
        "GET observes the same version as the PATCH body/ETag snapshot"
    );
}

#[tokio::test]
async fn before_update_failure_cannot_partially_apply_patch() {
    let (app, state) = app_with_state().await;
    let project_id = common::create_project(&app, "Item Concurrency Test", "software").await;
    let item_id = create_item(&app, project_id, "Original").await;
    let before = state.repo.get_item_version(item_id).await.unwrap().unwrap();

    // The trigger runs after the transaction has performed every validation
    // but before SQLite applies the one generated UPDATE. It is a deterministic
    // failure injection: the old field-by-field implementation would already
    // have committed earlier fields at this point.
    sqlx::query(
        "CREATE TRIGGER fail_atomic_item_patch BEFORE UPDATE ON items WHEN NEW.title = 'Blocked' BEGIN SELECT RAISE(ABORT, 'injected before-update failure'); END",
    )
    .execute(state.repo.pool())
    .await
    .unwrap();
    let failed = req(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Blocked", "description": "must not persist", "assignee": "Ada"})),
    )
    .await;
    assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let after = req(&app, Method::GET, &format!("/api/items/{item_id}"), None).await;
    let item = body_json(after).await["item"].clone();
    assert_eq!(item["title"], "Original");
    assert!(item["description"].is_null());
    assert!(item["assignee"].is_null());
    assert_eq!(
        state.repo.get_item_version(item_id).await.unwrap().unwrap(),
        before
    );
}

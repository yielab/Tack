//! HTTP tests for `handlers/executions.rs` and `handlers/runner_admin.rs`,
//! loaded via `#[path]` — global-router registration is proven separately
//! (`production_router.rs`).

#[path = "../../src/handlers/executions.rs"]
mod executions;
#[path = "../../src/handlers/runner_admin.rs"]
mod runner_admin;

use crate::common;
// Local alias: every route in this file sends as the default "operator-1"
// principal, and `common::send_as_operator` is long enough to force several
// argument lists onto their own lines where the shorter original name did not.
use crate::common::send_as_operator as snd;
use axum::http::StatusCode;
use chrono::Utc;
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{NewAgentProfile, NewRunner, RedeemEnrollmentResult, SystemExecutionClock},
};
use uuid::Uuid;

async fn setup() -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'C1', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "C1".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                title: "I".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item");
    let clock = SystemExecutionClock;
    repo.register_runner(
        NewRunner {
            id: "runner-active",
            name: "Active",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-c1",
            name: "C1",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &clock,
    )
    .await
    .expect("profile");
    let state = executions::OperatorExecutionState::with_clock(
        repo.clone(),
        std::sync::Arc::new(SystemExecutionClock),
        std::sync::Arc::new(|_pool| Box::pin(async { false })),
    );
    let app = executions::routes(state.clone()).merge(runner_admin::routes(state));
    (app, repo, item.id.to_string())
}

fn create_body(item_id: &str) -> String {
    serde_json::json!({
        "item_id":item_id,
        "idempotency_key":"same-key",
        "selector_kind":"exact_runner",
        "selector_id":"runner-active",
        "agent_profile_id":"profile-c1",
        "requested_harness_kind":"codex",
        "agent_profile_snapshot":{"name":"C1","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "repository_snapshot":{"kind":"git","remote":"https://example.test/c1.git","base_revision":"abc123","subdirectory":null},
        "permission_policy":{"tools":["shell"],"network":false},
        "timeout_seconds":60,
        "budgets":{"tokens":1000},
        "environment":{"MODE":{"value":"test","secret_reference":null}},
        "metadata":{"source":"c1-test"}
    }).to_string()
}
#[tokio::test]
async fn duplicate_create_replays_same_request_and_revoked_runner_is_rejected() {
    let (app, repo, item_id) = setup().await;
    let (first_status, first) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    let (second_status, second) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(first_status, StatusCode::OK, "{first}");
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first["request_id"], second["request_id"]);
    assert_eq!(second["replayed"], true);

    let (other_status, other) = common::send_as(
        &app,
        "POST",
        "/executions",
        create_body(&item_id),
        "operator-2",
    )
    .await;
    assert_eq!(other_status, StatusCode::OK);
    assert_ne!(first["request_id"], other["request_id"]);
    assert_eq!(other["replayed"], false);

    let changed = create_body(&item_id).replace("\"timeout_seconds\":60", "\"timeout_seconds\":61");
    let (conflict, conflict_body) = snd(&app, "POST", "/executions", changed).await;
    assert_eq!(conflict, StatusCode::CONFLICT);
    assert_eq!(conflict_body["error"]["code"], "idempotency_conflict");
    assert_eq!(conflict_body["error"]["request_id"], "req_operator");

    sqlx::query("UPDATE agent_runners SET state='revoked', revoked_at=? WHERE id='runner-active'")
        .bind(Utc::now().to_rfc3339())
        .execute(repo.pool())
        .await
        .unwrap();
    let (replay_after_revoke, replay_body) =
        snd(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(replay_after_revoke, StatusCode::OK);
    assert_eq!(replay_body["request_id"], first["request_id"]);
    assert_eq!(replay_body["replayed"], true);
    let unavailable = create_body(&item_id).replace("same-key", "new-key");
    let (status, body) = snd(&app, "POST", "/executions", unavailable).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "runner_revoked");
}

#[tokio::test]
async fn cancellation_is_requested_not_terminal() {
    let (app, repo, item_id) = setup().await;
    let (_, created) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    let request_id = created["request_id"].as_str().unwrap();
    let (status, body) = snd(
        &app,
        "POST",
        &format!("/executions/{request_id}/cancel"),
        "{}".into(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "cancellation_requested");
    let state: String = sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
        .bind(request_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state, "queued");
}

#[tokio::test]
async fn only_needs_operator_can_be_requeued_and_recovery_is_audited() {
    let (app, repo, item_id) = setup().await;
    let (_, created) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO execution_attempts (id,request_id,attempt_number,runner_id,fencing_token,state,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES ('attempt-1',?,1,'runner-active',1,'lost',?,?,?,?)")
        .bind(&request_id).bind(&now).bind(&now).bind(&now).bind(&now).execute(repo.pool()).await.unwrap();
    let (denied, _) = snd(
        &app,
        "POST",
        &format!("/executions/{request_id}/requeue"),
        r#"{"recovery_key":"operator-recovery-1","reason":"operator reviewed"}"#.into(),
    )
    .await;
    assert_eq!(denied, StatusCode::CONFLICT);
    sqlx::query("UPDATE execution_attempts SET state='needs_operator' WHERE id='attempt-1'")
        .execute(repo.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET state='needs_operator' WHERE id=?")
        .bind(&request_id)
        .execute(repo.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES ('attempt-1','runner-recovery-1','needs_operator','{}','fingerprint','{}',?)")
        .bind(&now)
        .execute(repo.pool())
        .await
        .unwrap();
    let (allowed, body) = snd(
        &app,
        "POST",
        &format!("/executions/{request_id}/requeue"),
        r#"{"recovery_key":"operator-recovery-1","reason":"operator reviewed"}"#.into(),
    )
    .await;
    assert_eq!(allowed, StatusCode::OK);
    assert_eq!(body["state"], "queued");
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-1' AND classification='operator_requeue'").fetch_one(repo.pool()).await.unwrap();
    assert_eq!(audits, 1);
}

#[tokio::test]
async fn enrollment_token_is_returned_once_hash_only_and_revoke_or_redeem_blocks_reuse() {
    let (app, repo, _) = setup().await;
    let enrollment = serde_json::json!({
        "name":"Pending runner",
        "labels":{"region":"test"},
        "total_capacity":1,
        "available_capacity":1,
        "capability_snapshot":{"runner_version":"test"}
    });
    let overflowing_enrollment = serde_json::json!({
        "name":"Overflowing pending runner",
        "total_capacity":1,
        "available_capacity":1,
        "enrollment_lifetime_seconds": i64::MAX
    });
    let (overflow_status, overflow_body) = snd(
        &app,
        "POST",
        "/runners/enrollment",
        overflowing_enrollment.to_string(),
    )
    .await;
    assert_eq!(overflow_status, StatusCode::BAD_REQUEST);
    assert_eq!(overflow_body["error"]["code"], "invalid_request");
    assert_eq!(overflow_body["error"]["request_id"], "req_operator");

    let (status, created) = snd(&app, "POST", "/runners/enrollment", enrollment.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    let runner_id = created["runner_id"].as_str().unwrap().to_owned();
    let token_id = created["token_id"].as_str().unwrap().to_owned();
    let raw_token = created["enrollment_token"].as_str().unwrap().to_owned();
    let stored_hash: String =
        sqlx::query_scalar("SELECT token_hash FROM agent_enrollment_tokens WHERE id=?")
            .bind(&token_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_ne!(stored_hash, raw_token);
    assert!(!created.to_string().contains(&stored_hash));

    let clock = SystemExecutionClock;
    let redeemed = repo
        .redeem_enrollment_token(
            &stored_hash,
            "runner-credential-hash",
            Utc::now() + chrono::Duration::hours(1),
            "test-runner",
            "Pending runner",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(
        redeemed,
        RedeemEnrollmentResult::Redeemed(runner_id.clone())
    );
    assert_eq!(
        repo.redeem_enrollment_token(
            &stored_hash,
            "another",
            Utc::now() + chrono::Duration::hours(1),
            "test-runner",
            "Pending runner",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock,
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );

    let second_enrollment = enrollment
        .to_string()
        .replace("Pending runner", "Second pending runner");
    let (status, second) = snd(&app, "POST", "/runners/enrollment", second_enrollment).await;
    assert_eq!(status, StatusCode::OK);
    let second_runner = second["runner_id"].as_str().unwrap();
    let second_token_id = second["token_id"].as_str().unwrap();
    let (revoked, revoke_body) = snd(
        &app,
        "POST",
        &format!("/runners/{second_runner}/enrollment-tokens/{second_token_id}/revoke"),
        "{}".into(),
    )
    .await;
    assert_eq!(revoked, StatusCode::OK);
    assert!(revoke_body.get("enrollment_token").is_none());
    let second_hash: String =
        sqlx::query_scalar("SELECT token_hash FROM agent_enrollment_tokens WHERE id=?")
            .bind(second_token_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        repo.redeem_enrollment_token(
            &second_hash,
            "runner-credential-hash",
            Utc::now() + chrono::Duration::hours(1),
            "test-runner",
            "Pending runner",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock,
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );
}

/// `tack_orch::execution::ProtocolErrorEnvelope::new` sets `retryable` from
/// `StableErrorCode::retryable`, which the frozen fixtures classify `true`
/// for `conflict`. This drives a real duplicate-name request through
/// `create_fleet` and inspects the actual response body, not the envelope
/// constructor in isolation, so it fails if a handler ever goes back to
/// hand-rolling `retryable` or drops `conflict`'s `{}` details shape.
#[tokio::test]
async fn duplicate_fleet_name_conflict_is_retryable_with_empty_details() {
    let (app, _repo, _item_id) = setup().await;
    let body = serde_json::json!({"name": "shared-fleet-name"}).to_string();
    let (first_status, _) = snd(&app, "POST", "/runner-fleets", body.clone()).await;
    assert_eq!(first_status, StatusCode::OK);

    let (second_status, second_body) = snd(&app, "POST", "/runner-fleets", body).await;
    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(second_body["error"]["code"], "conflict");
    assert_eq!(second_body["error"]["retryable"], true);
    assert_eq!(second_body["error"]["details"], serde_json::json!({}));
    assert_eq!(second_body["error"]["request_id"], "req_operator");
}

/// Companion to the conflict test above for a non-retryable code that also
/// carries contract-shaped structured `details` (`not_found` ->
/// `{"resource": ...}`), rather than a hand-rolled
/// `"retryable":false,"details":{}` for every code. Drives `get_execution`
/// for a request id that was never created.
#[tokio::test]
async fn missing_execution_not_found_is_not_retryable_with_resource_detail() {
    let (app, _repo, _item_id) = setup().await;
    let (status, body) = snd(
        &app,
        "GET",
        "/executions/exec_does_not_exist",
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["retryable"], false);
    assert_eq!(
        body["error"]["details"],
        serde_json::json!({"resource": "execution_request"})
    );
    assert_eq!(body["error"]["request_id"], "req_operator");
}

/// `idempotency_conflict` is non-retryable but, unlike `conflict`, carries a
/// structured `{"idempotency_key": ...}` detail per
/// `docs/contracts/runner-v1/errors/idempotency-conflict.json`. Drives
/// `create_execution` with the same idempotency key and a changed payload,
/// the same scenario `duplicate_create_replays_same_request_and_revoked_runner_is_rejected`
/// already exercises for `code`, and additionally asserts `retryable` and
/// `details` on the real response body.
#[tokio::test]
async fn changed_payload_idempotency_conflict_is_not_retryable_with_key_detail() {
    let (app, _repo, item_id) = setup().await;
    let (created_status, _) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(created_status, StatusCode::OK);

    let changed = create_body(&item_id).replace("\"timeout_seconds\":60", "\"timeout_seconds\":61");
    let (status, body) = snd(&app, "POST", "/executions", changed).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idempotency_conflict");
    assert_eq!(body["error"]["retryable"], false);
    assert_eq!(
        body["error"]["details"],
        serde_json::json!({"idempotency_key": "same-key"})
    );
}

/// `provision_local_runner` opens its own connection pool against a
/// `database_url` rather than reusing an existing `Repository`, since that
/// is exactly what an in-process caller in a different crate (`tack-cli`,
/// which cannot see this router's `AppState`) has to do. Uses a genuine
/// file-backed database, migrated first exactly like a real server's boot
/// sequence, so the row this function writes through its own pool has to be
/// independently visible through a second, already-open pool against the
/// same file to pass.
#[tokio::test]
async fn provision_local_runner_writes_through_its_own_pool_to_the_same_database() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("local-provision.db");
    let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = init_pool(&database_url).await.expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);

    let response = runner_admin::provision_local_runner(&database_url)
        .await
        .expect("local provisioning should succeed against a migrated, file-backed database");

    assert!(response.runner_id.starts_with("runr_"));
    assert!(!response.enrollment_token.is_empty());

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runners WHERE id = ?")
        .bind(&response.runner_id)
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(
        count, 1,
        "the runner provisioned through provision_local_runner's own pool must be visible \
         through a separate, already-open pool against the same database file"
    );
}

/// Negative-space companion to the test above: an unparseable `database_url`
/// must fail before ever reaching `provision_pending_runner`, proving
/// `provision_local_runner` really does open its own pool rather than
/// silently falling back to some other connection.
#[tokio::test]
async fn provision_local_runner_fails_on_an_unparseable_database_url() {
    let error = runner_admin::provision_local_runner("not-a-real-database-url")
        .await
        .expect_err("an unparseable database_url must not silently succeed");

    assert!(matches!(
        error,
        runner_admin::ProvisionLocalRunnerError::Pool(_)
    ));
}

/// `local_runner_id_exists` is the authoritative check for distinguishing a
/// credential this database issued from one a replaced database has no row
/// for: a runner id it just provisioned must read back as present, and a
/// runner id it never wrote at all — even one shaped like a real one — must
/// read back as absent, against the same live database rather than an
/// inference from a query failure.
#[tokio::test]
async fn local_runner_id_exists_reports_presence_and_absence_against_a_real_database() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("local-runner-id-exists.db");
    let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = init_pool(&database_url).await.expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    drop(pool);

    let response = runner_admin::provision_local_runner(&database_url)
        .await
        .expect("local provisioning should succeed against a migrated, file-backed database");

    assert!(
        runner_admin::local_runner_id_exists(&database_url, &response.runner_id)
            .await
            .expect("checking an id this same call just provisioned must not fail"),
        "a runner id this database actually holds a row for must read back as present"
    );
    assert!(
        !runner_admin::local_runner_id_exists(&database_url, "runr_never-provisioned")
            .await
            .expect("checking a well-formed but absent id must not fail"),
        "a runner id no row exists for — the shape of a credential orphaned by a recreated \
         database — must read back as absent, not merely fail to answer"
    );
}

/// Seeds two items, each with executions of its own, and proves
/// `?item_id=` scopes `GET /executions` to exactly one of them by row
/// count — never by what a component would go on to render. The unscoped
/// call is checked in the same test to prove it still sees every row
/// across both items.
#[tokio::test]
async fn list_executions_scoped_to_item_id_excludes_other_items_rows() {
    let (app, repo, item_id) = setup().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&item_id)
        .fetch_one(repo.pool())
        .await
        .expect("project id for the seeded item");
    let other_item = repo
        .create_item(
            Uuid::parse_str(&project_id).expect("project id parses"),
            "To Do",
            CreateItem {
                title: "Other".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("second item");
    let other_item_id = other_item.id.to_string();

    let (status, _) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = snd(
        &app,
        "POST",
        "/executions",
        create_body(&item_id).replace("same-key", "same-key-2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = snd(
        &app,
        "POST",
        "/executions",
        create_body(&other_item_id).replace("same-key", "other-item-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, scoped) = snd(
        &app,
        "GET",
        &format!("/executions?item_id={item_id}"),
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = scoped["data"].as_array().expect("data array");
    assert_eq!(
        rows.len(),
        2,
        "expected only the seeded item's two rows, got {scoped}"
    );
    assert!(
        rows.iter().all(|row| row["item_id"] == item_id),
        "a row for another item leaked into the scoped response: {scoped}"
    );

    let (status, unscoped) = snd(&app, "GET", "/executions", String::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        unscoped["data"].as_array().expect("data array").len(),
        3,
        "an omitted item_id must still return every item's rows"
    );
}

/// `limit` clamps the row count even for the unscoped call — the bound this
/// card's endpoint change added alongside the `item_id` filter.
#[tokio::test]
async fn list_executions_limit_bounds_the_unscoped_response() {
    let (app, _repo, item_id) = setup().await;
    for key in ["k1", "k2", "k3"] {
        let (status, _) = snd(
            &app,
            "POST",
            "/executions",
            create_body(&item_id).replace("same-key", key),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, body) = snd(&app, "GET", "/executions?limit=2", String::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().expect("data array").len(), 2);
}

/// Inserts `count` execution-request rows directly (never through
/// `create_execution` — this is a pure listing fixture, not an idempotency
/// or model-policy proof), all against `item_id`, with `created_at`
/// strictly increasing from `base + 1s`. Test-only: values are interpolated
/// into the SQL text rather than bound, which would be unacceptable in
/// production code but is safe here since every value is generated by this
/// function itself, never caller input.
async fn seed_noise_rows(
    repo: &Repository,
    prefix: &str,
    item_id: &str,
    count: usize,
    base: chrono::DateTime<Utc>,
) {
    let rows: Vec<String> = (0..count)
        .map(|i| {
            let id = format!("{prefix}-{i}");
            let ts = (base + chrono::Duration::seconds(1 + i as i64)).to_rfc3339();
            format!(
                "('{id}','{item_id}','{prefix}','{id}','{id}','queued','exact_runner','runner-active','{{}}','{{}}','{{}}','{ts}','{ts}')"
            )
        })
        .collect();
    let sql = format!(
        "INSERT INTO execution_requests \
         (id, item_id, idempotency_scope, idempotency_key, request_fingerprint, state, \
          selector_kind, selector_id, agent_profile_snapshot, repository_snapshot, \
          permission_policy, created_at, updated_at) VALUES {}",
        rows.join(",")
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(repo.pool())
        .await
        .expect("seed noise rows");
}

/// An item whose only execution predates every other execution the
/// install has recorded (the table pushed past `MAX_LIMIT` rows, all
/// newer, all against a different item) still surfaces its real, correct
/// state when asked for **by id** — the question `?item_ids=` answers, as
/// opposed to `?limit=` install-wide,
/// which this exact row would fall out of.
#[tokio::test]
async fn list_executions_item_ids_finds_an_item_whose_only_execution_predates_every_other_row() {
    let (app, repo, item_id) = setup().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&item_id)
        .fetch_one(repo.pool())
        .await
        .expect("project id for the seeded item");
    let noise_item = repo
        .create_item(
            Uuid::parse_str(&project_id).expect("project id parses"),
            "To Do",
            CreateItem {
                title: "Noise".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("noise item");
    let noise_item_id = noise_item.id.to_string();

    let (status, created) = snd(&app, "POST", "/executions", create_body(&item_id)).await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE execution_requests SET state='succeeded' WHERE id=?")
        .bind(&request_id)
        .execute(repo.pool())
        .await
        .unwrap();
    let created_at: String =
        sqlx::query_scalar("SELECT created_at FROM execution_requests WHERE id=?")
            .bind(&request_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let base = chrono::DateTime::parse_from_rfc3339(&created_at)
        .unwrap()
        .with_timezone(&Utc);

    let noise_count = (executions::ListExecutionsQuery::MAX_LIMIT as usize) + 5;
    seed_noise_rows(&repo, "noise", &noise_item_id, noise_count, base).await;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert!(
        total as usize > executions::ListExecutionsQuery::MAX_LIMIT as usize,
        "the table must exceed MAX_LIMIT for this proof to mean anything, got {total}"
    );

    let (status, body) = snd(
        &app,
        "GET",
        &format!("/executions?item_ids={item_id}"),
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"].as_array().expect("data array");
    assert_eq!(
        rows.len(),
        1,
        "expected exactly the target item's own row: {body}"
    );
    assert_eq!(rows[0]["request_id"], request_id);
    assert_eq!(rows[0]["state"], "succeeded");
}

/// Two items, two executions each — `?item_ids=` must return exactly one
/// row per id (never every row for that id) and it must be each item's
/// most recent one, not an arbitrary member of the set.
#[tokio::test]
async fn list_executions_item_ids_returns_exactly_the_latest_row_per_item() {
    let (app, repo, item_a) = setup().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&item_a)
        .fetch_one(repo.pool())
        .await
        .expect("project id");
    let item_b = repo
        .create_item(
            Uuid::parse_str(&project_id).expect("project id parses"),
            "To Do",
            CreateItem {
                title: "B".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item b")
        .id
        .to_string();

    // item_a: an older 'queued' row, then a newer 'succeeded' one — the
    // scoped answer must be the newer, not the older, and never both.
    let (_, a_old) = snd(&app, "POST", "/executions", create_body(&item_a)).await;
    let a_old_id = a_old["request_id"].as_str().unwrap().to_owned();
    let (_, a_new) = snd(
        &app,
        "POST",
        "/executions",
        create_body(&item_a).replace("same-key", "a-newer"),
    )
    .await;
    let a_new_id = a_new["request_id"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE execution_requests SET created_at='2020-01-01T00:00:00Z', updated_at='2020-01-01T00:00:00Z' WHERE id=?")
        .bind(&a_old_id).execute(repo.pool()).await.unwrap();
    sqlx::query("UPDATE execution_requests SET state='succeeded', created_at='2021-01-01T00:00:00Z', updated_at='2021-01-01T00:00:00Z' WHERE id=?")
        .bind(&a_new_id).execute(repo.pool()).await.unwrap();

    let (_, b_created) = snd(
        &app,
        "POST",
        "/executions",
        create_body(&item_b).replace("same-key", "b-only"),
    )
    .await;
    let b_id = b_created["request_id"].as_str().unwrap().to_owned();

    let (status, body) = snd(
        &app,
        "GET",
        &format!("/executions?item_ids={item_a},{item_b}"),
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut rows: Vec<serde_json::Value> = body["data"].as_array().expect("data array").clone();
    assert_eq!(rows.len(), 2, "expected one row per id: {body}");
    rows.sort_by(|a, b| a["item_id"].as_str().cmp(&b["item_id"].as_str()));
    let by_item: std::collections::HashMap<String, serde_json::Value> = rows
        .into_iter()
        .map(|row| (row["item_id"].as_str().unwrap().to_owned(), row))
        .collect();
    assert_eq!(
        by_item[&item_a]["request_id"], a_new_id,
        "must be item_a's newer row, not {a_old_id}"
    );
    assert_eq!(by_item[&item_a]["state"], "succeeded");
    assert_eq!(by_item[&item_b]["request_id"], b_id);
}

/// `item_ids` takes precedence over `item_id`/`limit` when both are given —
/// the shape a caller that only half-migrated a call site might send.
#[tokio::test]
async fn list_executions_item_ids_takes_precedence_over_item_id_and_limit() {
    let (app, repo, item_a) = setup().await;
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
        .bind(&item_a)
        .fetch_one(repo.pool())
        .await
        .expect("project id");
    let item_b = repo
        .create_item(
            Uuid::parse_str(&project_id).expect("project id parses"),
            "To Do",
            CreateItem {
                title: "B".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("item b")
        .id
        .to_string();
    let (status, _) = snd(&app, "POST", "/executions", create_body(&item_b)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = snd(
        &app,
        "GET",
        &format!("/executions?item_id={item_a}&limit=0&item_ids={item_b}"),
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"].as_array().expect("data array");
    assert_eq!(
        rows.len(),
        1,
        "item_ids must win over item_id/limit, got {body}"
    );
    assert_eq!(rows[0]["item_id"], item_b);
}

/// A malformed `item_ids` entry is rejected with this route's own
/// contract-shaped `invalid_request`, naming the bad fragment — never a
/// silently-dropped id, and never axum's generic query-rejection body.
#[tokio::test]
async fn list_executions_item_ids_rejects_a_malformed_id() {
    let (app, _repo, _item_id) = setup().await;
    let (status, body) = snd(
        &app,
        "GET",
        "/executions?item_ids=not-a-uuid",
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
    assert_eq!(body["error"]["details"]["field"], "item_ids");
    assert_eq!(body["error"]["details"]["value"], "not-a-uuid");
}

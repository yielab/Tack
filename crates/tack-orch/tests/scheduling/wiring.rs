//! Proves `tack_orch::scheduler::wiring::choose_request_for_runner` against a
//! real, file-backed-shape (in-memory) `tack_db::Repository` — not the pure
//! unit tests in `scheduler/wiring.rs` itself, which only cover the small
//! pure helper functions. This file is the actual "wire the scheduler to
//! live data" proof: real
//! `agent_runners`/`agent_fleet_members`/`agent_fleets`/`execution_requests`
//! rows, inserted through the same repository methods the real API handlers
//! use, then handed to `choose_request_for_runner` exactly as
//! `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler does.

use chrono::Utc;
use tack_db::Repository;
use tack_db::repo::execution::{NewExecutionRequest, NewRunner};
use tack_orch::scheduler::wiring::choose_request_for_runner;
use tack_orch::scheduler::{SchedulingPolicy, choose_request_for_runner as choose};

use crate::support::{FixedClock, codex_capability_snapshot, setup_repo};

const ALPHA_MODEL: &str = "opaque/model-alpha";

/// Registers a runner and sets its heartbeat `heartbeat_offset_secs` away
/// from `now` (negative for stale), or leaves it `NULL` (`None`) to mirror a
/// runner that enrolled/refreshed but has never yet been granted a lease.
async fn setup_runner(
    repo: &Repository,
    id: &str,
    total_capacity: i64,
    available_capacity: i64,
    capability_snapshot: &str,
    heartbeat_offset_secs: Option<i64>,
    now: chrono::DateTime<Utc>,
) {
    repo.register_runner(
        NewRunner {
            id,
            name: id,
            credential_hash: "test-hash",
            labels: "{}",
            total_capacity,
            available_capacity,
            capability_snapshot,
            protocol_version: 1,
        },
        &FixedClock(now),
    )
    .await
    .expect("register runner");
    if let Some(offset) = heartbeat_offset_secs {
        sqlx::query("UPDATE agent_runners SET last_heartbeat_at = ? WHERE id = ?")
            .bind((now + chrono::Duration::seconds(offset)).to_rfc3339())
            .bind(id)
            .execute(repo.pool())
            .await
            .expect("set heartbeat");
    }
}

/// `setup_runner` with the standard `codex` + `openai/opaque/model-alpha` snapshot.
async fn setup_codex_runner(
    repo: &Repository,
    id: &str,
    total_capacity: i64,
    available_capacity: i64,
    heartbeat_offset_secs: Option<i64>,
    now: chrono::DateTime<Utc>,
) {
    setup_runner(
        repo,
        id,
        total_capacity,
        available_capacity,
        &codex_capability_snapshot(now),
        heartbeat_offset_secs,
        now,
    )
    .await;
}

async fn setup_fleet(
    repo: &Repository,
    id: &str,
    concurrency_limit: i64,
    now: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, ?, ?, '{}', ?, ?)",
    )
    .bind(id)
    .bind(id)
    .bind(concurrency_limit)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert fleet");
}

async fn join_fleet(
    repo: &Repository,
    fleet_id: &str,
    runner_id: &str,
    now: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(fleet_id)
    .bind(runner_id)
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("fleet membership");
}

#[allow(clippy::too_many_arguments)]
async fn enqueue(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    idempotency_key: &str,
    selector_kind: &str,
    selector_id: &str,
    harness: &str,
    model_provider: Option<&str>,
    model_id: Option<&str>,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) {
    let selector = match selector_kind {
        "exact_runner" => serde_json::json!({"kind":"exact_runner","runner_id":selector_id}),
        "fleet" => serde_json::json!({"kind":"fleet","fleet_id":selector_id}),
        other => panic!("unsupported selector kind in test fixture: {other}"),
    };
    let snapshot = serde_json::json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": idempotency_key,
        "created_by": {"source": "test", "subject_id": "wiring-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": "profile-1",
        "resolved_agent_profile": {
            "name": "profile", "instructions": "work safely",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "requested_harness_kind": harness,
        "requested_model_provider": model_provider,
        "requested_model_id": model_id,
        "repository": {
            "kind": "git", "remote": "https://example.test/wiring.git",
            "base_revision": "abc123def456abc123def456abc123def456abc", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": serde_json::from_str::<serde_json::Value>(metadata).unwrap()
    })
    .to_string();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: request_id,
            item_id,
            idempotency_scope: "wiring-test",
            idempotency_key,
            request_fingerprint: request_id,
            selector_kind,
            selector_id,
            agent_profile_id: Some("profile-1"),
            agent_profile_snapshot: r#"{"name":"profile","instructions":"work safely","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
            requested_harness_kind: Some(harness),
            requested_model_provider: model_provider,
            requested_model_id: model_id,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/wiring.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata,
            request_snapshot: &snapshot,
        },
        &FixedClock(created_at),
    )
    .await
    .expect("enqueue");
}

/// `enqueue` with the standard `codex`/`openai` harness, targeting one runner directly.
async fn enqueue_for_runner(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    runner_id: &str,
    model_id: &str,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) {
    let key = format!("{request_id}-key");
    enqueue(
        repo,
        item_id,
        request_id,
        &key,
        "exact_runner",
        runner_id,
        "codex",
        Some("openai"),
        Some(model_id),
        metadata,
        created_at,
    )
    .await;
}

/// `enqueue` with the standard `codex`/`openai` harness, targeting a fleet selector.
async fn enqueue_for_fleet(
    repo: &Repository,
    item_id: &str,
    request_id: &str,
    fleet_id: &str,
    model_id: &str,
    metadata: &str,
    created_at: chrono::DateTime<Utc>,
) {
    let key = format!("{request_id}-key");
    enqueue(
        repo,
        item_id,
        request_id,
        &key,
        "fleet",
        fleet_id,
        "codex",
        Some("openai"),
        Some(model_id),
        metadata,
        created_at,
    )
    .await;
}

/// One row of the eligibility table below: registers a runner with the given
/// capacity/capability/heartbeat, enqueues one request for `model_id`, and
/// asserts whether it gets chosen.
#[allow(clippy::too_many_arguments)]
async fn assert_eligibility(
    case: &str,
    total: i64,
    available: i64,
    capability: &str,
    heartbeat_offset_secs: i64,
    model_id: &str,
    expect_chosen: bool,
) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_runner(
        &repo,
        "runner",
        total,
        available,
        capability,
        Some(heartbeat_offset_secs),
        now,
    )
    .await;
    enqueue_for_runner(&repo, &item_id, "req", "runner", model_id, "{}", now).await;

    let chosen = choose_request_for_runner(&repo, "runner", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.is_some(), expect_chosen, "case: {case}");
}

#[tokio::test]
async fn eligibility_needs_capability_capacity_and_fresh_heartbeat() {
    let codex = codex_capability_snapshot(Utc::now());
    let bad_model = "opaque/model-that-does-not-exist";

    assert_eligibility("match", 1, 1, &codex, 0, ALPHA_MODEL, true).await;
    assert_eligibility("model_mismatch", 1, 1, &codex, 0, bad_model, false).await;
    assert_eligibility("no_harness", 1, 1, "{}", 0, ALPHA_MODEL, false).await;
    assert_eligibility("zero_capacity", 1, 0, &codex, 0, ALPHA_MODEL, false).await;
    assert_eligibility("stale_heartbeat", 1, 1, &codex, -600, ALPHA_MODEL, false).await;
}

#[tokio::test]
async fn reexported_choose_matches_the_primary_function() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    enqueue_for_runner(&repo, &item_id, "req-a", "runner-a", ALPHA_MODEL, "{}", now).await;

    let via_module =
        choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    let via_reexport = choose(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(
        via_module, via_reexport,
        "tack_orch::scheduler::choose_request_for_runner must not drift from the wiring module's own copy"
    );
}

/// One row of the priority table below: enqueues an older normal-priority
/// request and a newer one carrying `second_metadata`, then checks which id
/// gets chosen.
async fn assert_priority(case: &str, second_metadata: &str, expect_chosen: &str) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let earlier = now - chrono::Duration::seconds(60);
    setup_codex_runner(&repo, "runner-a", 1, 1, Some(0), now).await;
    enqueue_for_runner(
        &repo,
        &item_id,
        "req-first",
        "runner-a",
        ALPHA_MODEL,
        "{}",
        earlier,
    )
    .await;
    enqueue_for_runner(
        &repo,
        &item_id,
        "req-second",
        "runner-a",
        ALPHA_MODEL,
        second_metadata,
        now,
    )
    .await;

    let chosen = choose_request_for_runner(&repo, "runner-a", now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen.as_deref(), Some(expect_chosen), "case: {case}");
}

#[tokio::test]
async fn priority_and_recency_break_ties_in_the_expected_order() {
    assert_priority("high_priority_wins", r#"{"priority":"high"}"#, "req-second").await;
    assert_priority("fifo_within_same_priority", "{}", "req-first").await;
}

/// One row of the fleet table below: an already-joined `runner-member` polls
/// while the fleet is at `concurrency_limit`, optionally saturated by a
/// second member, and checks whether it can still claim.
async fn assert_fleet_concurrency(
    case: &str,
    concurrency_limit: i64,
    saturate: bool,
    expect_chosen: bool,
) {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let fleet_id = "fleet-under-test";
    setup_fleet(&repo, fleet_id, concurrency_limit, now).await;
    setup_codex_runner(&repo, "runner-member", 1, 1, Some(0), now).await;
    join_fleet(&repo, fleet_id, "runner-member", now).await;
    if saturate {
        setup_codex_runner(&repo, "runner-other-member", 1, 0, Some(0), now).await;
        join_fleet(&repo, fleet_id, "runner-other-member", now).await;
    }
    enqueue_for_fleet(
        &repo,
        &item_id,
        "req-fleet",
        fleet_id,
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-member", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(chosen.is_some(), expect_chosen, "case: {case}");
}

#[tokio::test]
async fn fleet_concurrency_limit_gates_every_member() {
    assert_fleet_concurrency("at_limit_blocks", 1, true, false).await;
    assert_fleet_concurrency("under_limit_allows", 5, false, true).await;
}

/// One row of the absence table below: with or without an idle runner
/// registered, no queued work ever produces an error — only a clean `None`.
async fn assert_absence(case: &str, register_idle_runner: bool) {
    let (repo, _item_id) = setup_repo().await;
    let now = Utc::now();
    let runner_id = "runner-idle";
    if register_idle_runner {
        setup_codex_runner(&repo, runner_id, 1, 1, Some(0), now).await;
    }
    let chosen = choose_request_for_runner(&repo, runner_id, now, &SchedulingPolicy::default())
        .await
        .expect("no db error");
    assert_eq!(chosen, None, "case: {case}");
}

#[tokio::test]
async fn absence_of_eligible_work_resolves_to_none_not_an_error() {
    assert_absence("no_queued_work", true).await;
    assert_absence("unknown_runner_id", false).await;
}

/// A freshly enrolled runner polling for its very first claim has never
/// called `/heartbeat` (that endpoint only reports *active attempt lease*
/// renewals — `agent_runners.last_heartbeat_at` stays `NULL` until a runner
/// has already been granted at least one lease). Without the capability
/// snapshot's own `reported_at` fallback in `wiring.rs`, this would be a
/// deadlock: no runner could ever get its first piece of work.
#[tokio::test]
async fn fresh_runner_with_no_heartbeat_can_claim_its_first_request() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    setup_codex_runner(&repo, "runner-fresh", 1, 1, None, now).await;
    let heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-fresh'")
            .fetch_one(repo.pool())
            .await
            .expect("read heartbeat column");
    assert_eq!(
        heartbeat, None,
        "fixture must start with no heartbeat, matching real enrollment"
    );

    enqueue_for_runner(
        &repo,
        &item_id,
        "req-first-claim",
        "runner-fresh",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen =
        choose_request_for_runner(&repo, "runner-fresh", now, &SchedulingPolicy::default())
            .await
            .expect("no db error");
    assert_eq!(
        chosen.as_deref(),
        Some("req-first-claim"),
        "a runner's own attested capability report time must stand in for a heartbeat it has \
         never had the chance to send yet"
    );
}

/// The fallback proven above is not "no heartbeat ever means eligible" — a
/// runner that enrolled/refreshed long ago and never heartbeated since still
/// goes stale once its *capability report* itself ages past the policy window.
#[tokio::test]
async fn stale_capability_report_rejects_despite_no_heartbeat() {
    let (repo, item_id) = setup_repo().await;
    let now = Utc::now();
    let stale_report_time = now - chrono::Duration::seconds(600);
    setup_runner(
        &repo,
        "runner-stale-report",
        1,
        1,
        &codex_capability_snapshot(stale_report_time),
        None,
        now,
    )
    .await;

    enqueue_for_runner(
        &repo,
        &item_id,
        "req-stale-report",
        "runner-stale-report",
        ALPHA_MODEL,
        "{}",
        now,
    )
    .await;

    let chosen = choose_request_for_runner(
        &repo,
        "runner-stale-report",
        now,
        &SchedulingPolicy::default(),
    )
    .await
    .expect("no db error");
    assert_eq!(chosen, None);
}

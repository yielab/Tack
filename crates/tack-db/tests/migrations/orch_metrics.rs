//! Tests for migrations 025-027 (`orch_metrics`, `orch_events_daily`,
//! `orch_metrics_daily`) and the repository functions built on them:
//! `upsert_orch_metrics`, `list_latest_orch_metrics`,
//! `rollup_and_purge_orch_events`/`_metrics`, and their `list_*_daily` readers.
//! Covers fresh install, upgrade-in-place, FK enforcement, append-only
//! insert, and retention: a 91-day-old row is purged but its day's
//! aggregate keeps the same totals, a re-run after everything is already
//! purged is a documented no-op, and a backlog larger than one batch is
//! swept across multiple bounded transactions.

use crate::common::setup_test_db;
use chrono::{Duration, Utc};
use sqlx::Row;
use tack_db::repo::orch::{CreateControlPlane, NewOrchEvent, NewOrchMetric};
use tack_db::{init_pool, migrations};
use uuid::Uuid;

const NEW_TABLES: [&str; 3] = ["orch_metrics", "orch_events_daily", "orch_metrics_daily"];

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
    sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_optional(pool)
        .await
        .expect("query sqlite_master")
        .is_some()
}

async fn make_control_plane(repo: &tack_db::Repository) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: "Plane".into(),
        kind: None,
        base_url: "http://localhost:9999".into(),
        token: None,
    })
    .await
    .expect("create control plane")
    .id
}

/// Inserts a raw `orch_events` row with an explicit `occurred_at`, bypassing
/// `upsert_orch_events` so the test can backdate rows past the retention cutoff
/// (the repo API always stamps `created_at` at call time, but `occurred_at` here
/// needs to be arbitrarily old).
async fn insert_raw_event(
    pool: &sqlx::SqlitePool,
    control_plane_id: Uuid,
    event_type: &str,
    occurred_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO orch_events (id, control_plane_id, event_type, occurred_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(control_plane_id.to_string())
    .bind(event_type)
    .bind(occurred_at.to_rfc3339())
    .execute(pool)
    .await
    .expect("insert raw orch_event");
}

/// Same idea as [`insert_raw_event`], for `orch_metrics` (backdating `scraped_at`).
async fn insert_raw_metric(
    pool: &sqlx::SqlitePool,
    control_plane_id: Uuid,
    name: &str,
    value: f64,
    scraped_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO orch_metrics (id, control_plane_id, name, labels, value, scraped_at) \
         VALUES (?, ?, ?, '{}', ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(control_plane_id.to_string())
    .bind(name)
    .bind(value)
    .bind(scraped_at.to_rfc3339())
    .execute(pool)
    .await
    .expect("insert raw orch_metric");
}

// ─── Fresh install / upgrade-in-place ──────────────────────────────────────

#[tokio::test]
async fn fresh_db_migrates_all_metrics_tables() {
    let repo = setup_test_db().await;

    for table in NEW_TABLES {
        assert!(
            table_exists(repo.pool(), table).await,
            "expected table {table} to exist after a fresh migration run"
        );
    }

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    // Migrations 025, 026 and 027 must all be present and applied in their
    // declared order. Deliberately *not* asserting anything about what comes
    // after 027 — that is a fact about the whole project's migration
    // history, not about metrics, and would break as soon as a later
    // migration lands. Presence-and-order here means a future migration
    // never breaks this test.
    let this_files_migrations = [
        "025_orch_metrics",
        "026_orch_events_daily",
        "027_orch_metrics_daily",
    ];
    let positions: Vec<Option<usize>> = this_files_migrations
        .iter()
        .map(|name| applied.iter().position(|a| a == name))
        .collect();
    assert!(
        positions.iter().all(Option::is_some),
        "expected all of {this_files_migrations:?} to be applied; got {applied:?}"
    );
    let positions: Vec<usize> = positions.into_iter().flatten().collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "025/026/027 must apply in their declared order: {applied:?}"
    );
}

#[tokio::test]
async fn upgrade_from_024_applies_metrics_migrations_in_place() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Simulate an installed tack.db that only ever saw migrations 001-024
    // (i.e. an install predating the metrics/retention schema).
    migrations::run_up_to(&pool, "024_orch_approvals")
        .await
        .expect("apply migrations up to 024");

    assert!(
        table_exists(&pool, "orch_approvals").await,
        "024_orch_approvals should have applied"
    );
    for table in NEW_TABLES {
        assert!(
            !table_exists(&pool, table).await,
            "table {table} must not exist before the upgrade runs"
        );
    }

    migrations::run_all(&pool).await.expect("upgrade in place");

    for table in NEW_TABLES {
        assert!(
            table_exists(&pool, table).await,
            "table {table} must exist after upgrading an existing db in place"
        );
    }

    // Migrations 025, 026 and 027 must be recorded as applied on top of the
    // pre-existing 001-024 set — not a specific total count, which is a fact
    // about the whole project's migration history rather than about this
    // upgrade path, and breaks every time a later migration adds one more.
    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations")
        .fetch_all(&pool)
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    for name in [
        "025_orch_metrics",
        "026_orch_events_daily",
        "027_orch_metrics_daily",
    ] {
        assert!(
            applied.iter().any(|a| a == name),
            "expected {name} to be recorded as applied after upgrading in place; got {applied:?}"
        );
    }
}

// ─── FK enforcement ─────────────────────────────────────────────────────────

#[tokio::test]
async fn orphan_fk_insert_is_rejected() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();

    let cases = [
        (
            "orch_metrics.control_plane_id",
            format!(
                "INSERT INTO orch_metrics (id, control_plane_id, name, value) \
                 VALUES ('{}', '{bogus_plane}', 'x', 1.0)",
                Uuid::new_v4()
            ),
        ),
        (
            "orch_events_daily.control_plane_id",
            format!(
                "INSERT INTO orch_events_daily (id, day, control_plane_id, event_type) \
                 VALUES ('{}', '2026-01-01', '{bogus_plane}', 'tool_call')",
                Uuid::new_v4()
            ),
        ),
        (
            "orch_metrics_daily.control_plane_id",
            format!(
                "INSERT INTO orch_metrics_daily (id, day, control_plane_id, metric_name) \
                 VALUES ('{}', '2026-01-01', '{bogus_plane}', 'docket_agents_total')",
                Uuid::new_v4()
            ),
        ),
    ];

    for (label, sql) in cases {
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(repo.pool())
            .await;
        assert!(result.is_err(), "{label}: dangling FK must be rejected");
    }
}

// ─── upsert_orch_metrics ─────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_orch_metrics_is_append_only_not_deduplicated() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    let mut labels = std::collections::BTreeMap::new();
    labels.insert("agent".to_string(), "demo-lead".to_string());

    let sample = NewOrchMetric {
        name: "docket_agent_cost_usd".into(),
        labels: labels.clone(),
        value: 1.5,
    };

    // Same logical sample "upserted" twice (two scrapes returning the same
    // reading) must land as two distinct rows — metrics are a time series, not
    // a corrected record (see the doc comment on upsert_orch_metrics).
    repo.upsert_orch_metrics(plane_id, std::slice::from_ref(&sample))
        .await
        .expect("first insert");
    repo.upsert_orch_metrics(plane_id, std::slice::from_ref(&sample))
        .await
        .expect("second insert");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(
        count, 2,
        "two scrapes of the same metric+labels must persist as two rows, not one"
    );
}

#[tokio::test]
async fn upsert_orch_metrics_empty_slice_is_noop() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    repo.upsert_orch_metrics(plane_id, &[])
        .await
        .expect("empty slice must not error");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn list_latest_orch_metrics_returns_most_recent_sample() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    // Three scrapes of the same series with different values, at strictly
    // increasing (backdated, to make ordering unambiguous) scraped_at.
    let base = Utc::now() - Duration::hours(1);
    for (value, offset) in [(1.0, 0), (2.0, 1), (3.0, 2)] {
        insert_raw_metric(
            repo.pool(),
            plane_id,
            "docket_agents_total",
            value,
            base + Duration::minutes(offset),
        )
        .await;
    }

    let latest = repo.list_latest_orch_metrics().await.expect("list latest");
    assert_eq!(latest.len(), 1, "one series should yield one latest row");
    assert_eq!(
        latest[0].value, 3.0,
        "must be the most recently scraped value"
    );
    assert_eq!(latest[0].control_plane_name, "Plane");
}

// ─── Retention: rollup-before-purge, and the "same totals survive" bar ────────

#[tokio::test]
async fn rollup_and_purge_orch_events_preserves_totals_deletes_raw() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    // A 91-day-old event (must be purged, but counted into the day's aggregate)
    // and a fresh one (must survive untouched — inside the retention window).
    let old = Utc::now() - Duration::days(91);
    let fresh = Utc::now();
    insert_raw_event(repo.pool(), plane_id, "tool_call", old).await;
    insert_raw_event(
        repo.pool(),
        plane_id,
        "tool_call",
        old + Duration::minutes(5),
    )
    .await;
    insert_raw_event(repo.pool(), plane_id, "approval_granted", old).await;
    insert_raw_event(repo.pool(), plane_id, "tool_call", fresh).await;

    let cutoff = Utc::now() - Duration::days(90);
    let stats = repo
        .rollup_and_purge_orch_events(cutoff, 500)
        .await
        .expect("rollup");
    assert_eq!(
        stats.rows_purged, 3,
        "only the three 91-day-old rows are stale"
    );

    // The 91-day-old raw rows are gone...
    let raw_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count raw events");
    assert_eq!(raw_count, 1, "only the fresh event should remain raw");

    // ...but the day's aggregate survives with the same totals.
    let daily = repo
        .list_orch_events_daily(plane_id)
        .await
        .expect("list daily");
    let tool_call_count: i64 = daily
        .iter()
        .filter(|d| d.event_type == "tool_call")
        .map(|d| d.event_count)
        .sum();
    let approval_count: i64 = daily
        .iter()
        .filter(|d| d.event_type == "approval_granted")
        .map(|d| d.event_count)
        .sum();
    assert_eq!(tool_call_count, 2, "both purged tool_call events counted");
    assert_eq!(approval_count, 1);
}

#[tokio::test]
async fn rollup_and_purge_orch_events_rerun_is_noop() {
    // The externally observable half of the atomicity guarantee: once a batch's
    // aggregate write and delete have committed together, nothing is left for a
    // re-run to reprocess — so re-running immediately must never double-count,
    // which is exactly what a crash-then-retry would also produce (see the doc
    // comment on rollup_and_purge_orch_events for the full argument).
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);
    insert_raw_event(repo.pool(), plane_id, "tool_call", old).await;

    let cutoff = Utc::now() - Duration::days(90);
    let first = repo
        .rollup_and_purge_orch_events(cutoff, 500)
        .await
        .expect("first rollup");
    assert_eq!(first.rows_purged, 1);

    let second = repo
        .rollup_and_purge_orch_events(cutoff, 500)
        .await
        .expect("second rollup");
    assert_eq!(second.rows_purged, 0, "nothing stale left to reprocess");

    let daily = repo
        .list_orch_events_daily(plane_id)
        .await
        .expect("list daily");
    assert_eq!(daily.len(), 1);
    assert_eq!(
        daily[0].event_count, 1,
        "count must not have doubled on the second (no-op) run"
    );
}

#[tokio::test]
async fn rollup_and_purge_orch_events_sweeps_backlog_over_one_batch() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);

    for i in 0..12 {
        insert_raw_event(
            repo.pool(),
            plane_id,
            "tool_call",
            old + Duration::seconds(i),
        )
        .await;
    }

    // Small batch size forces multiple transactions to fully sweep the backlog.
    let stats = repo
        .rollup_and_purge_orch_events(Utc::now() - Duration::days(90), 5)
        .await
        .expect("rollup");
    assert_eq!(
        stats.rows_purged, 12,
        "every stale row must be swept, not just the first batch"
    );
    assert_eq!(
        stats.batches_run, 3,
        "12 rows at batch_size=5 takes 3 batches (5+5+2)"
    );

    let raw_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(raw_count, 0);

    let daily = repo
        .list_orch_events_daily(plane_id)
        .await
        .expect("list daily");
    assert_eq!(
        daily.len(),
        1,
        "all 12 rows share one day/plane/type bucket"
    );
    assert_eq!(daily[0].event_count, 12);
}

#[tokio::test]
async fn rollup_and_purge_orch_events_leaves_fresh_rows_untouched() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    insert_raw_event(repo.pool(), plane_id, "tool_call", Utc::now()).await;

    let stats = repo
        .rollup_and_purge_orch_events(Utc::now() - Duration::days(90), 500)
        .await
        .expect("rollup");
    assert_eq!(stats.rows_purged, 0);

    let raw_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(
        raw_count, 1,
        "a fresh event must survive the sweep untouched"
    );
}

#[tokio::test]
async fn rollup_and_purge_orch_metrics_preserves_sum_min_max() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);

    for (value, offset) in [(1.0, 0), (5.0, 1), (3.0, 2)] {
        insert_raw_metric(
            repo.pool(),
            plane_id,
            "docket_turn_duration_seconds",
            value,
            old + Duration::minutes(offset),
        )
        .await;
    }
    // A fresh sample of the same series must survive, untouched by the sweep.
    insert_raw_metric(
        repo.pool(),
        plane_id,
        "docket_turn_duration_seconds",
        99.0,
        Utc::now(),
    )
    .await;

    let stats = repo
        .rollup_and_purge_orch_metrics(Utc::now() - Duration::days(90), 500)
        .await
        .expect("rollup");
    assert_eq!(stats.rows_purged, 3);

    let raw_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(raw_count, 1, "only the fresh sample remains raw");

    let daily = repo
        .list_orch_metrics_daily(plane_id)
        .await
        .expect("list daily");
    assert_eq!(daily.len(), 1);
    let agg = &daily[0];
    assert_eq!(agg.sample_count, 3);
    assert_eq!(agg.value_sum, 9.0, "1.0 + 5.0 + 3.0");
    assert_eq!(agg.value_min, Some(1.0));
    assert_eq!(agg.value_max, Some(5.0));
}

#[tokio::test]
async fn rollup_and_purge_orch_metrics_excludes_non_finite_values() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);

    insert_raw_metric(repo.pool(), plane_id, "docket_weird_gauge", 2.0, old).await;
    insert_raw_metric(repo.pool(), plane_id, "docket_weird_gauge", f64::NAN, old).await;
    insert_raw_metric(
        repo.pool(),
        plane_id,
        "docket_weird_gauge",
        f64::INFINITY,
        old,
    )
    .await;

    let stats = repo
        .rollup_and_purge_orch_metrics(Utc::now() - Duration::days(90), 500)
        .await
        .expect("rollup");
    assert_eq!(stats.rows_purged, 3, "all three rows are still purged");

    let daily = repo
        .list_orch_metrics_daily(plane_id)
        .await
        .expect("list daily");
    assert_eq!(daily.len(), 1);
    let agg = &daily[0];
    assert_eq!(
        agg.sample_count, 3,
        "sample_count counts every sample, finite or not"
    );
    assert_eq!(
        agg.value_sum, 2.0,
        "NaN/Inf must not poison the sum — only the one finite sample contributes"
    );
    assert_eq!(agg.value_min, Some(2.0));
    assert_eq!(agg.value_max, Some(2.0));
}

#[tokio::test]
async fn rollup_and_purge_orch_events_and_metrics_are_independent() {
    // A regression guard: purging orch_events must never touch orch_metrics and
    // vice versa — they're separate tables with separate cutoff comparisons.
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);

    insert_raw_event(repo.pool(), plane_id, "tool_call", old).await;
    insert_raw_metric(repo.pool(), plane_id, "docket_agents_total", 1.0, old).await;

    let cutoff = Utc::now() - Duration::days(90);
    repo.rollup_and_purge_orch_events(cutoff, 500)
        .await
        .expect("rollup events");

    let metrics_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(
        metrics_count, 1,
        "purging orch_events must not touch orch_metrics"
    );

    repo.rollup_and_purge_orch_metrics(cutoff, 500)
        .await
        .expect("rollup metrics");
    let metrics_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_metrics")
        .fetch_one(repo.pool())
        .await
        .expect("count");
    assert_eq!(metrics_count_after, 0);
}

// ─── Sanity: NewOrchEvent import isn't dead weight ─────────────────────────
// (kept as a living cross-check that upsert_orch_events' normal path also
// respects the retention sweep, not just directly-inserted raw rows).

#[tokio::test]
async fn events_inserted_via_upsert_are_also_swept() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    let old = Utc::now() - Duration::days(91);

    repo.upsert_orch_events(
        plane_id,
        &[NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: None,
            run_id: None,
            event_type: "tool_call".into(),
            payload: serde_json::json!({}),
            occurred_at: old,
        }],
    )
    .await
    .expect("upsert event");

    let stats = repo
        .rollup_and_purge_orch_events(Utc::now() - Duration::days(90), 500)
        .await
        .expect("rollup");
    assert_eq!(stats.rows_purged, 1);
}

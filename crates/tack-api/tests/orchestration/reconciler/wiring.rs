//! Tests for the glue that lets the orchestration reconciler
//! (`tack-orch::reconciler`) poll a real docket instance:
//! `RepoControlPlaneStore` (`crates/tack-api/src/orch_store.rs`) plus the
//! `server.rs` wiring that only spawns it when `TACK_ORCH_ENABLE` is set.
//!
//! Covers: the store round-trips health through the real repo; a plane with
//! an unregistered `kind` doesn't fail the whole poll cycle; a spawned
//! reconciler polls a real (wiremock) docket end to end; and an unset
//! `TACK_ORCH_ENABLE` spawns no tasks even with a plane registered.

use chrono::Utc;
use tack_api::config::AppConfig;
use tack_api::orch_store::RepoControlPlaneStore;
use tack_db::repo::orch::CreateControlPlane;
use tack_db::{Repository, init_pool, migrations};
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, HealthState, ReconcilerConfig, spawn_reconcilers,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh in-memory-DB-backed `Repository`, migrations applied.
async fn test_repo() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

// ─── 1. Store round-trips health through the real repo ────────────────────

#[tokio::test]
async fn record_health_round_trips_through_the_real_repo() {
    let repo = test_repo().await;
    let plane = repo
        .create_control_plane(CreateControlPlane {
            name: "docket-prod".into(),
            kind: Some("docket".into()),
            base_url: "http://127.0.0.1:7331".into(),
            token: Some("secret-token".into()),
        })
        .await
        .expect("create control plane");

    // Freshly created rows start at the DB default, untouched by the store.
    assert_eq!(plane.health, "unknown");
    assert_eq!(plane.consecutive_failures, 0);
    assert!(plane.last_seen_at.is_none());

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = RepoControlPlaneStore::new(repo.clone(), broadcast_tx);
    let now = Utc::now();
    store
        .record_health(
            plane.id,
            &HealthRecord {
                health: HealthState::Healthy,
                consecutive_failures: 0,
                last_seen_at: Some(now),
                api_version: Some("2".into()),
            },
        )
        .await
        .expect("record_health must succeed");

    let reloaded = repo.get_control_plane(plane.id).await.expect("reload");
    assert_eq!(reloaded.health, "healthy");
    assert_eq!(reloaded.consecutive_failures, 0);
    assert_eq!(reloaded.api_version.as_deref(), Some("2"));
    // Round-trips through SQLite's TEXT storage, so compare at second
    // resolution rather than requiring bit-for-bit equality.
    let seen = reloaded.last_seen_at.expect("last_seen_at must be set");
    assert_eq!(seen.timestamp(), now.timestamp());

    // A failed poll (`last_seen_at: None`) must leave the stored timestamp
    // untouched — this is the exact "None means don't touch, not clear"
    // contract reconciler.rs relies on and the repo layer must honor.
    store
        .record_health(
            plane.id,
            &HealthRecord {
                health: HealthState::Degraded,
                consecutive_failures: 3,
                last_seen_at: None,
                api_version: None,
            },
        )
        .await
        .expect("record_health must succeed");

    let reloaded_again = repo.get_control_plane(plane.id).await.expect("reload");
    assert_eq!(reloaded_again.health, "degraded");
    assert_eq!(reloaded_again.consecutive_failures, 3);
    assert_eq!(
        reloaded_again.last_seen_at.expect("still set").timestamp(),
        now.timestamp(),
        "a failed poll (last_seen_at: None) must leave the previous value untouched"
    );
    // api_version: None must not clobber the previously stored "2" either
    // (COALESCE semantics in update_control_plane_health).
    assert_eq!(reloaded_again.api_version.as_deref(), Some("2"));
}

// ─── 2. list_registered builds a live adapter, dispatched on `kind` ───────

#[tokio::test]
async fn list_registered_builds_a_live_adapter_for_a_docket_plane() {
    let repo = test_repo().await;
    let plane = repo
        .create_control_plane(CreateControlPlane {
            name: "docket-prod".into(),
            kind: Some("docket".into()),
            base_url: "http://127.0.0.1:7331".into(),
            token: Some("secret-token".into()),
        })
        .await
        .expect("create control plane");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = RepoControlPlaneStore::new(repo, broadcast_tx);
    let registered = store
        .list_registered()
        .await
        .expect("list_registered must succeed");

    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].id, plane.id);
    assert_eq!(registered[0].control_plane.kind(), "docket");
}

#[tokio::test]
async fn list_registered_skips_an_unknown_kind_without_failing() {
    let repo = test_repo().await;
    repo.create_control_plane(CreateControlPlane {
        name: "docket-prod".into(),
        kind: Some("docket".into()),
        base_url: "http://127.0.0.1:7331".into(),
        token: None,
    })
    .await
    .expect("create docket plane");
    let mystery = repo
        .create_control_plane(CreateControlPlane {
            name: "mystery-orchestrator".into(),
            kind: Some("some-future-thing".into()),
            base_url: "http://127.0.0.1:9999".into(),
            token: None,
        })
        .await
        .expect("create unknown-kind plane");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = RepoControlPlaneStore::new(repo, broadcast_tx);
    let registered = store
        .list_registered()
        .await
        .expect("an unknown kind must be skipped, not surfaced as an Err");

    assert_eq!(
        registered.len(),
        1,
        "only the recognized docket plane should be registered"
    );
    assert_ne!(
        registered[0].id, mystery.id,
        "the unknown-kind row must not be the one that got through"
    );
}

// ─── A plane `list_registered` could not build an adapter for is marked ───
// "unconfigured", not left at the pre-poll "unknown" default forever.

#[tokio::test]
async fn unconfigured_plane_reports_unconfigured_not_unknown() {
    let repo = test_repo().await;
    let mystery = repo
        .create_control_plane(CreateControlPlane {
            name: "mystery-orchestrator".into(),
            kind: Some("some-future-thing".into()),
            base_url: "http://127.0.0.1:9999".into(),
            token: None,
        })
        .await
        .expect("create unknown-kind plane");
    assert_eq!(
        mystery.health, "unknown",
        "sanity: a freshly created row starts at the column default"
    );

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = RepoControlPlaneStore::new(repo.clone(), broadcast_tx);
    store
        .list_registered()
        .await
        .expect("an unknown kind must be skipped, not surfaced as an Err");

    let after = repo
        .get_control_plane(mystery.id)
        .await
        .expect("plane must still exist — list_registered only skips it, never deletes it");
    assert_eq!(
        after.health, "unconfigured",
        "a no-op implementation that leaves health untouched must fail this — the whole \
         point is that the plane no longer reads as the pre-poll \"unknown\" default"
    );
}

// ─── 3. End-to-end: spawn_reconcilers against a real store + wiremock ─────

#[tokio::test]
async fn spawn_reconcilers_polls_docket_persists_health_via_store() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"status":"ok","gateway":0}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"apiVersion":"2","timestamp":"2026-08-04T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#,
        ))
        .mount(&server)
        .await;

    let repo = test_repo().await;
    let plane = repo
        .create_control_plane(CreateControlPlane {
            name: "docket-wiremock".into(),
            kind: Some("docket".into()),
            base_url: server.uri(),
            token: None,
        })
        .await
        .expect("create control plane");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = std::sync::Arc::new(RepoControlPlaneStore::new(repo.clone(), broadcast_tx));
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 60,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // The reconciler polls immediately on start, doing real (loopback) HTTP
    // I/O against `server` on a background task. `Interval::tick`, not a
    // bare `yield_now` loop, forces this current-thread runtime to actually
    // park and let its I/O driver service that task's socket I/O — see
    // `auto_dispatch/hook.rs`'s `wait_for_hits` for why a `yield_now` loop
    // was tried first there and found to starve exactly this kind of wait.
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(25));
    let mut reloaded = repo.get_control_plane(plane.id).await.expect("reload");
    for _ in 0..80 {
        if reloaded.health == "healthy" {
            break;
        }
        ticker.tick().await;
        reloaded = repo.get_control_plane(plane.id).await.expect("reload");
    }
    for h in handles {
        h.abort();
    }

    assert_eq!(reloaded.health, "healthy");
    assert!(reloaded.last_seen_at.is_some());
    assert_eq!(reloaded.api_version.as_deref(), Some("2"));
}

// ─── 4. Off-by-default: unset TACK_ORCH_ENABLE spawns nothing ─────────────

#[tokio::test]
async fn disabled_orch_enable_spawns_no_tasks_with_registered_plane() {
    // AppConfig::default() ⇒ orch_enable: false, matching TACK_ORCH_ENABLE
    // unset — the exact off-by-default condition this test asserts against,
    // wired through the real store rather than reconciler.rs's own fake
    // (that module already covers the generic case; this covers that the
    // real RepoControlPlaneStore's data doesn't leak through the gate).
    let config = AppConfig::default();
    assert!(!config.orch_enable);

    let repo = test_repo().await;
    repo.create_control_plane(CreateControlPlane {
        name: "docket-prod".into(),
        kind: Some("docket".into()),
        base_url: "http://127.0.0.1:7331".into(),
        token: None,
    })
    .await
    .expect("create control plane");

    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);
    let store = std::sync::Arc::new(RepoControlPlaneStore::new(repo, broadcast_tx));
    let handles = spawn_reconcilers(
        config.orch_enable,
        store,
        ReconcilerConfig {
            poll_secs: config.orch_poll_secs,
            ..Default::default()
        },
    )
    .await;

    assert!(
        handles.is_empty(),
        "no reconciler task should spawn while orch_enable is false, even with a plane registered"
    );
}

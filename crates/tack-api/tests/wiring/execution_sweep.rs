//! Proves the artifact/event retention sweep and the overdue-decision
//! expiry sweep are wired into the real production `ExecutionRuntime`
//! (`src/execution_runtime.rs`): both ride its own `start`/`stop` lifecycle,
//! never a hand-rolled loop calling the sweep functions directly — a unit
//! test can prove the sweep functions work in isolation, only this file
//! proves the running server actually calls them.

use std::future::Future;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tack_api::execution_runtime::{ExecutionRuntime, ExecutionRuntimeConfig};
use tack_api::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        NewAgentProfile, NewArtifact, NewDecision, NewExecutionRequest, NewRunner,
        RequestSelection, SystemExecutionClock,
    },
};
use uuid::Uuid;

const RUNNER_ID: &str = "runner-wiring";
const PROFILE_ID: &str = "profile-wiring";

async fn setup() -> (Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'W', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "W".into(),
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
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "Wiring Runner",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &SystemExecutionClock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: PROFILE_ID,
            name: "Wiring Profile",
            instructions: "work",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("profile");
    (repo, item.id.to_string())
}

/// A single fixed instant, used both as the snapshot's own declared
/// `created_at` field below and as the clock `enqueue_execution` is called
/// with in `running_attempt` — `enqueue_execution` rejects a snapshot whose
/// `created_at` does not match the clock's `now()` *exactly*, so this must
/// not be two separate `Utc::now()` reads.
struct FixedExecutionClock(chrono::DateTime<Utc>);

impl tack_db::repo::execution::ExecutionClock for FixedExecutionClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0
    }
}

fn new_request<'a>(
    id: &'a str,
    item_id: &'a str,
    key: &'a str,
    now: chrono::DateTime<Utc>,
) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"test","subject_id":"wiring-test"}},"created_at":"{now}","selector":{{"kind":"exact_runner","runner_id":"{RUNNER_ID}"}},"agent_profile_id":"{PROFILE_ID}","resolved_agent_profile":{{"name":"P","instructions":"work","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{}}}},"requested_harness_kind":"codex","requested_model_provider":null,"requested_model_id":null,"repository":{{"kind":"git","remote":"https://example.test/wiring.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":[],"network":false}},"timeout_seconds":60,"budgets":{{}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#,
            now = now.to_rfc3339(),
        )
        .into_boxed_str(),
    );
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: key,
        selector_kind: "exact_runner",
        selector_id: RUNNER_ID,
        agent_profile_id: Some(PROFILE_ID),
        agent_profile_snapshot: r#"{"name":"P","instructions":"work","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: None,
        requested_model_id: None,
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/wiring.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":[],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

/// Claims a fresh attempt against a real, long-lived lease (10 minutes —
/// comfortably longer than any sweep-wait loop below) and bumps it straight
/// to `running`; the resolve/expiry paths under test here don't gate on how
/// an attempt got to `running`, only on its current state/lease.
async fn running_attempt(repo: &Repository, item_id: &str, tag: &str) -> (String, i64) {
    let request_id = format!("req-{tag}");
    let now = Utc::now();
    repo.enqueue_execution(
        new_request(&request_id, item_id, &request_id, now),
        &FixedExecutionClock(now),
    )
    .await
    .expect("enqueue");
    let attempt_id = format!("att-{tag}");
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            RUNNER_ID,
            &attempt_id,
            &attempt_id,
            Duration::minutes(10),
            &SystemExecutionClock,
            RequestSelection::Naive,
        )
        .await
        .expect("claim")
        .expect("work available");
    sqlx::query("UPDATE execution_attempts SET state='running' WHERE id=?")
        .bind(&attempt_id)
        .execute(repo.pool())
        .await
        .expect("bump to running");
    (claim.lease.attempt_id, claim.lease.fencing_token)
}

/// Artifact storage root for one sweep-wiring test. The `TempDir` removes
/// the directory and everything under it when it drops, so a failing
/// assertion leaves nothing behind either.
fn temp_storage_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Writes a real blob via the same `ArtifactStorage::store_streaming` path
/// production uses, and records a matching `execution_artifacts` manifest
/// row pointing at it — so "the blob is gone from disk" is a meaningful,
/// non-vacuous assertion later.
#[allow(clippy::too_many_arguments)]
async fn seed_real_artifact(
    repo: &Repository,
    storage: &ArtifactStorage,
    artifact_root: &std::path::Path,
    attempt_id: &str,
    fence: i64,
    row_id: &str,
    artifact_id: &str,
    content: &[u8],
    backdate_days: Option<i64>,
) -> std::path::PathBuf {
    let digest = sha256_hex(content);
    let stored = storage
        .store_streaming(
            attempt_id,
            artifact_id,
            content.len() as u64,
            &digest,
            futures::stream::iter(std::iter::once(Ok::<_, std::io::Error>(
                axum::body::Bytes::copy_from_slice(content),
            ))),
        )
        .await
        .expect("store real blob");

    repo.record_execution_artifact(
        RUNNER_ID,
        attempt_id,
        fence,
        NewArtifact {
            id: row_id,
            artifact_id,
            kind: "patch",
            name: "content.patch",
            media_type: Some("text/plain"),
            size_bytes: content.len() as i64,
            sha256: &digest,
            content_disposition: Some("inline_upload"),
            content_reference: Some(&stored.content_reference),
            metadata: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("record artifact");

    if let Some(days) = backdate_days {
        sqlx::query("UPDATE execution_artifacts SET created_at = ? WHERE id = ?")
            .bind((Utc::now() - Duration::days(days)).to_rfc3339())
            .bind(row_id)
            .execute(repo.pool())
            .await
            .expect("backdate artifact");
    }

    artifact_root.join(&stored.content_reference)
}

async fn artifact_row_exists(repo: &Repository, row_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE id=?")
        .bind(row_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    count > 0
}

async fn decision_state(repo: &Repository, decision_id: &str) -> String {
    sqlx::query_scalar("SELECT state FROM execution_decisions WHERE decision_id=?")
        .bind(decision_id)
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

/// Bounded, deterministic poll for an async condition to become true — never
/// a blind sleep standing in for the actual assertion. Uses an interval's
/// pause between checks rather than a bare `sleep` in a loop, since a
/// background `tokio::spawn` doing real DB/filesystem I/O shares this same
/// single-threaded test runtime.
async fn wait_for<F, Fut>(mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let mut ticker = tokio::time::interval(StdDuration::from_millis(50));
    ticker.tick().await; // the first tick fires immediately; discard it
    for _ in 0..100 {
        if condition().await {
            return true;
        }
        ticker.tick().await;
    }
    false
}

/// The inverse of [`wait_for`]: polls a condition for a bounded number of
/// ticks and fails as soon as it stops holding, proving *absence* of change
/// over the same bounded window a fixed sleep would have blindly waited out.
async fn stays_true_for<F, Fut>(mut condition: F, ticks: u32) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let mut ticker = tokio::time::interval(StdDuration::from_millis(50));
    for _ in 0..ticks {
        ticker.tick().await;
        if !condition().await {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------
// sweep_artifacts/sweep_events via the real ExecutionRuntime.
// ---------------------------------------------------------------------

/// Runs the artifact-retention scenario once for a given `retention_enable`
/// setting: an old (backdated) artifact and a fresh one are seeded, the
/// real runtime starts with that setting, and the old one is purged only
/// when the flag is on — the fresh one must never be swept either way.
async fn assert_artifact_retention_gate(retention_enable: bool) {
    let (repo, item_id) = setup().await;
    let tag = format!("artifacts-{retention_enable}");
    let (attempt_id, fence) = running_attempt(&repo, &item_id, &tag).await;
    let storage_root_dir = temp_storage_root(&tag);
    let storage_dir = storage_root_dir.path().to_string_lossy().into_owned();
    let artifact_root = std::path::PathBuf::from(format!("{storage_dir}/execution-artifacts"));
    let artifact_storage = ArtifactStorage::new(artifact_root.clone());

    let old_blob_path = seed_real_artifact(
        &repo,
        &artifact_storage,
        &artifact_root,
        &attempt_id,
        fence,
        "art-row-old",
        "art-old",
        b"old artifact content",
        Some(5),
    )
    .await;
    let fresh_blob_path = seed_real_artifact(
        &repo,
        &artifact_storage,
        &artifact_root,
        &attempt_id,
        fence,
        "art-row-fresh",
        "art-fresh",
        b"fresh artifact content",
        None,
    )
    .await;
    assert!(tokio::fs::try_exists(&old_blob_path).await.unwrap());
    assert!(tokio::fs::try_exists(&fresh_blob_path).await.unwrap());

    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable,
                retention_days: 1,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_dir.clone(),
            },
        )
        .await;

    if retention_enable {
        let purged = wait_for(|| async { !artifact_row_exists(&repo, "art-row-old").await }).await;
        assert!(
            purged,
            "an expired artifact row must be purged when retention is enabled"
        );
        assert!(!tokio::fs::try_exists(&old_blob_path).await.unwrap());
    } else {
        let stayed = stays_true_for(
            || async { artifact_row_exists(&repo, "art-row-old").await },
            10,
        )
        .await;
        assert!(
            stayed,
            "the row must survive when retention is disabled (the default)"
        );
        assert!(tokio::fs::try_exists(&old_blob_path).await.unwrap());
    }

    assert!(
        artifact_row_exists(&repo, "art-row-fresh").await,
        "a fresh, under-age artifact's row must never be swept"
    );
    assert!(tokio::fs::try_exists(&fresh_blob_path).await.unwrap());

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(storage_root_dir.path()).await;
}

#[tokio::test]
async fn retention_enable_gates_the_artifact_sweep() {
    assert_artifact_retention_gate(true).await;
    assert_artifact_retention_gate(false).await;
}

/// The immutability guard (`set_execution_artifact_content_reference`'s own
/// `WHERE content_reference IS NULL`) means a manifested-but-never-uploaded
/// artifact is exactly the shape the race guard cares about. This does not
/// attempt to win that race against the real background sweep (inherently
/// timing-dependent — the deterministic proof lives in
/// `crates/tack-db/tests/repository/event_artifact_retention.rs`); it only
/// confirms the production path purges a manifest-only row end to end.
#[tokio::test]
async fn retention_enabled_purges_a_manifest_only_artifact_row() {
    let (repo, item_id) = setup().await;
    let (attempt_id, fence) = running_attempt(&repo, &item_id, "manifest-only").await;
    let storage_root_dir = temp_storage_root("manifest-only");

    repo.record_execution_artifact(
        RUNNER_ID,
        &attempt_id,
        fence,
        NewArtifact {
            id: "art-row-manifest-only",
            artifact_id: "art-manifest-only",
            kind: "patch",
            name: "never-uploaded.patch",
            media_type: Some("text/plain"),
            size_bytes: 4,
            sha256: &"7".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: None,
            metadata: "{}",
        },
        &SystemExecutionClock,
    )
    .await
    .expect("record manifest-only artifact");
    sqlx::query("UPDATE execution_artifacts SET created_at = ? WHERE id = 'art-row-manifest-only'")
        .bind((Utc::now() - Duration::days(5)).to_rfc3339())
        .execute(repo.pool())
        .await
        .unwrap();

    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable: true,
                retention_days: 1,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_root_dir.path().to_string_lossy().into_owned(),
            },
        )
        .await;

    let purged =
        wait_for(|| async { !artifact_row_exists(&repo, "art-row-manifest-only").await }).await;
    assert!(
        purged,
        "a manifest-only row (content_reference: None) must still be purged when genuinely stale"
    );

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(storage_root_dir.path()).await;
}

// ---------------------------------------------------------------------
// expire_overdue_decisions via the real ExecutionRuntime.
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn create_decision(
    repo: &Repository,
    attempt_id: &str,
    fence: i64,
    row_id: &str,
    decision_id: &str,
    expires_at: chrono::DateTime<Utc>,
) {
    let written = repo
        .create_execution_decision(
            RUNNER_ID,
            attempt_id,
            fence,
            NewDecision {
                id: row_id,
                decision_id,
                kind: "tool_permission",
                prompt: "Allow the harness to run a command?",
                options: "[]",
                metadata: "{}",
                expires_at: Some(expires_at),
            },
            &SystemExecutionClock,
        )
        .await
        .expect("create decision");
    assert!(written);
}

/// Runs the decision-expiry scenario once for a given `retention_enable`
/// setting (the periodic expiry sweep currently rides the same gate as
/// artifact retention): an overdue decision expires only when the flag is
/// on, while a not-yet-due one always stays pending either way.
async fn assert_decision_expiry_gate(retention_enable: bool) {
    let (repo, item_id) = setup().await;
    let tag = format!("decisions-{retention_enable}");
    let (attempt_id, fence) = running_attempt(&repo, &item_id, &tag).await;

    let overdue_row = format!("row-overdue-{}", Uuid::new_v4());
    create_decision(
        &repo,
        &attempt_id,
        fence,
        &overdue_row,
        "dec-overdue",
        Utc::now() - Duration::seconds(5),
    )
    .await;
    let future_row = format!("row-future-{}", Uuid::new_v4());
    create_decision(
        &repo,
        &attempt_id,
        fence,
        &future_row,
        "dec-future",
        Utc::now() + Duration::minutes(30),
    )
    .await;

    let storage_root_dir = temp_storage_root(&tag);
    let runtime = ExecutionRuntime::new();
    runtime
        .start(
            repo.clone(),
            ExecutionRuntimeConfig {
                retention_enable,
                retention_days: 90,
                retention_interval_secs: 1,
                health_enable: false,
                health_interval_secs: 3600,
                storage_dir: storage_root_dir.path().to_string_lossy().into_owned(),
            },
        )
        .await;

    if retention_enable {
        let expired =
            wait_for(|| async { decision_state(&repo, "dec-overdue").await == "expired" }).await;
        assert!(
            expired,
            "an overdue, unresolved decision must expire via the periodic sweep"
        );
    } else {
        let stayed = stays_true_for(
            || async { decision_state(&repo, "dec-overdue").await == "pending" },
            10,
        )
        .await;
        assert!(
            stayed,
            "with the sweep disabled, an overdue decision is left pending"
        );
    }
    assert_eq!(
        decision_state(&repo, "dec-future").await,
        "pending",
        "a not-yet-overdue decision must remain pending regardless of the sweep setting"
    );

    runtime.stop().await;
    let _ = tokio::fs::remove_dir_all(storage_root_dir.path()).await;
}

#[tokio::test]
async fn retention_enable_gates_overdue_decision_expiry() {
    assert_decision_expiry_gate(true).await;
    assert_decision_expiry_gate(false).await;
}

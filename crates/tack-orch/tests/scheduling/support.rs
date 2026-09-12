//! Shared fixtures for `wiring.rs` and `policy.rs`: both stand up a
//! real, migrated `tack_db::Repository` with one workspace/project/item and
//! one agent profile already in it, and both need a runner whose declared
//! capability snapshot is `codex` + `openai/opaque/model-alpha`. `scheduler.rs`
//! doesn't use any of this — it drives `tack_orch::scheduler`'s pure
//! functions directly, with no repository at all.

use chrono::Utc;
use tack_core::models::{CreateItem, ItemType, Priority as ItemPriority, ProjectType};
use tack_core::vocabulary;
use tack_db::Repository;
use tack_db::repo::execution::NewAgentProfile;
use uuid::Uuid;

use crate::common::setup_test_db;

pub(crate) struct FixedClock(pub(crate) chrono::DateTime<Utc>);
impl tack_db::repo::execution::ExecutionClock for FixedClock {
    fn now(&self) -> chrono::DateTime<Utc> {
        self.0
    }
}

pub(crate) async fn setup_repo() -> (Repository, String) {
    let repo = setup_test_db().await;
    let workspace_id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Scheduling', ?)",
    )
    .bind(workspace_id.to_string())
    .bind(vocab)
    .execute(repo.pool())
    .await
    .expect("workspace");
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "Scheduling".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let status = project.workflow.initial_status().unwrap().to_string();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Scheduling fixture item".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: Some(ItemPriority::Medium),
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
    let clock = FixedClock(Utc::now());
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-1",
            name: "profile-1",
            instructions: "work safely",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("agent profile");
    (repo, item.id.to_string())
}

/// A runner that has declared exactly one harness/model combination:
/// `codex` + `openai/opaque/model-alpha`. Anything else is unavailable to
/// it.
pub(crate) fn codex_capability_snapshot(now: chrono::DateTime<Utc>) -> String {
    serde_json::json!({
        "reported_at": now.to_rfc3339(),
        "labels": {},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now.to_rfc3339(),
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-alpha"],
                "discovery": "reported"
            }]
        }],
        "features": {
            "cancel": {"support": "supported", "reason": null},
            "resume": {"support": "unsupported", "reason": "n/a"},
            "decisions": {"support": "supported", "reason": null},
            "artifacts": {"support": "supported", "reason": null},
            "usage": {"support": "advisory", "reason": "n/a"}
        },
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 1048576}
    })
    .to_string()
}

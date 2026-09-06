//! Binds `crates/tack-orch/src/model_policy`'s precedence walk to the shared
//! fixture `docs/contracts/model-policy/precedence-table.json`, the single
//! file `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts` reads
//! against its own copy of the same walk. Same shape as
//! `crates/tack-api/tests/openapi_contract.rs`: this test regenerates the
//! committed fixture from the real `resolve_model_policy` /
//! `parse_model_default_convention` behavior, and otherwise asserts the
//! committed file still matches that behavior byte-for-byte.
//!
//! Regenerate with:
//!
//! ```sh
//! UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'
//! ```
//!
//! The fixture only covers the three tiers an Auto request can resolve
//! through (agent profile, project, fleet) — a request override is never
//! present when resolving what Auto means, by construction on both sides,
//! so it is left out rather than encoded as an input nothing exercises.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use tack_core::models::ProjectModelDefault;
use tack_orch::execution::{RequestedModelId, RequestedModelProvider};
use tack_orch::model_policy::wiring::parse_model_default_convention;
use tack_orch::model_policy::{ModelPolicySources, ModelPolicyTier, ResolvedModelPolicy, resolve_model_policy};
use tack_orch::scheduler::types::ModelSelector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TierState {
    Absent,
    Auto,
    Explicit,
}

impl TierState {
    const ALL: [TierState; 3] = [TierState::Absent, TierState::Auto, TierState::Explicit];

    fn label(self) -> &'static str {
        match self {
            TierState::Absent => "absent",
            TierState::Auto => "auto",
            TierState::Explicit => "explicit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct FixtureRow {
    id: String,
    description: String,
    /// Raw `agent_profiles.limits` JSON, read by
    /// `parse_model_default_convention` — `null` means the column carries no
    /// `default_model` opinion, exactly as an absent/malformed column does.
    agent_profile_limits: Option<Value>,
    /// `projects.default_model`'s own typed JSON shape
    /// (`ProjectModelDefault`), decoded directly rather than through the
    /// convention parser — this tier is never an untyped blob on the wire.
    project_default_model: Option<Value>,
    /// Raw `agent_fleets.default_policy` JSON, read the same way as
    /// `agent_profile_limits`.
    fleet_default_policy: Option<Value>,
    expected: ExpectedOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum ExpectedOutcome {
    Unresolved,
    PinnedAuto { source: String },
    Explicit {
        source: String,
        provider: String,
        model_id: String,
    },
}

fn raw_blob(tier_label: &str, state: TierState) -> Option<Value> {
    match state {
        TierState::Absent => None,
        TierState::Auto => Some(json!({ "default_model": "auto" })),
        TierState::Explicit => Some(json!({
            "default_model": {
                "provider": format!("{tier_label}-provider"),
                "model_id": format!("{tier_label}-model"),
            }
        })),
    }
}

fn project_value(state: TierState) -> Option<Value> {
    match state {
        TierState::Absent => None,
        TierState::Auto => Some(json!({ "kind": "auto" })),
        TierState::Explicit => Some(json!({
            "kind": "explicit",
            "provider": "project-provider",
            "model_id": "project-model",
        })),
    }
}

fn selector_from_raw_blob(raw: &Option<Value>) -> Option<ModelSelector> {
    let value = raw.as_ref()?;
    let json_text = serde_json::to_string(value).expect("serialize fixture blob");
    parse_model_default_convention(&json_text)
}

fn selector_from_project_value(raw: &Option<Value>) -> Option<ModelSelector> {
    let value = raw.as_ref()?;
    let parsed: ProjectModelDefault =
        serde_json::from_value(value.clone()).expect("fixture project value must decode");
    Some(match parsed {
        ProjectModelDefault::Auto => ModelSelector::AutoSelect,
        ProjectModelDefault::Explicit { provider, model_id } => ModelSelector::Explicit {
            provider: RequestedModelProvider::new(provider),
            model_id: RequestedModelId::new(model_id),
        },
    })
}

fn tier_label(tier: ModelPolicyTier) -> &'static str {
    match tier {
        ModelPolicyTier::RequestOverride => "request_override",
        ModelPolicyTier::AgentProfile => "agent_profile",
        ModelPolicyTier::Project => "project",
        ModelPolicyTier::Fleet => "fleet",
    }
}

fn expected_outcome(resolved: &ResolvedModelPolicy) -> ExpectedOutcome {
    match (resolved.source, &resolved.selector) {
        (None, _) => ExpectedOutcome::Unresolved,
        (Some(tier), ModelSelector::AutoSelect) => ExpectedOutcome::PinnedAuto {
            source: tier_label(tier).to_string(),
        },
        (Some(tier), ModelSelector::Explicit { provider, model_id }) => ExpectedOutcome::Explicit {
            source: tier_label(tier).to_string(),
            provider: provider.as_str().to_string(),
            model_id: model_id.as_str().to_string(),
        },
    }
}

fn build_row(agent_profile: TierState, project: TierState, fleet: TierState) -> FixtureRow {
    let agent_profile_limits = raw_blob("agent-profile", agent_profile);
    let project_default_model = project_value(project);
    let fleet_default_policy = raw_blob("fleet", fleet);

    let sources = ModelPolicySources {
        request_override: None,
        agent_profile_default: selector_from_raw_blob(&agent_profile_limits),
        project_default: selector_from_project_value(&project_default_model),
        fleet_default: selector_from_raw_blob(&fleet_default_policy),
    };
    let resolved = resolve_model_policy(&sources);

    FixtureRow {
        id: format!(
            "ap_{}__pj_{}__fl_{}",
            agent_profile.label(),
            project.label(),
            fleet.label()
        ),
        description: format!(
            "agent profile {}, project {}, fleet {}",
            agent_profile.label(),
            project.label(),
            fleet.label()
        ),
        agent_profile_limits,
        project_default_model,
        fleet_default_policy,
        expected: expected_outcome(&resolved),
    }
}

/// The exhaustive 3-state x 3-tier table (absent / pinned-auto / explicit for
/// agent profile, project, fleet) — 27 rows, generated by walking
/// `ModelPolicyTier::ORDER` via the real `resolve_model_policy`, never
/// hand-typed. Order is fixed (agent profile outer, fleet inner) so
/// regenerating twice in a row produces an identical file.
fn build_fixture_rows() -> Vec<FixtureRow> {
    let mut rows = Vec::with_capacity(27);
    for agent_profile in TierState::ALL {
        for project in TierState::ALL {
            for fleet in TierState::ALL {
                rows.push(build_row(agent_profile, project, fleet));
            }
        }
    }
    rows
}

/// Path to the committed fixture, relative to this crate (`crates/tack-orch`).
fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/contracts/model-policy/precedence-table.json")
}

#[test]
fn precedence_table_matches_committed_fixture() {
    let rows = build_fixture_rows();
    assert_eq!(
        rows.len(),
        27,
        "fixture row count changed; update this guard deliberately"
    );

    let mut generated = serde_json::to_string_pretty(&rows).expect("serialize fixture rows");
    generated.push('\n');
    let path = fixture_path();

    if std::env::var_os("UPDATE_MODEL_POLICY_FIXTURE").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create docs/contracts/model-policy dir");
        }
        std::fs::write(&path, &generated).expect("write precedence-table.json");
        eprintln!("Regenerated {}", path.display());
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {} ({error}).\nGenerate it with: UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'",
            path.display()
        )
    });

    assert_eq!(
        committed, generated,
        "\n\ndocs/contracts/model-policy/precedence-table.json is out of date with \
         crates/tack-orch/src/model_policy's actual behavior.\nRegenerate it with:\n    \
         UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'\n"
    );
}

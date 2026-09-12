//! Live wiring between the pure [`super::resolve_model_policy`] and real
//! `agent_profiles.limits` / `agent_fleets.default_policy` rows.
//!
//! # Where each tier's data actually lives today
//!
//! - **Request override**: `execution_requests.requested_model_provider`/
//!
//! Design notes: docs/dev-notes/tack-orch/model_policy/wiring.md

use tack_core::models::ProjectModelDefault;
use tack_db::Repository;

use super::{ModelPolicySources, ResolvedModelPolicy, resolve_model_policy};
use crate::execution::{RequestedModelId, RequestedModelProvider};
use crate::scheduler::types::ModelSelector;

/// The JSON key this module reads a tier's default model from. See this
/// module's doc comment for the exact shapes accepted.
pub const DEFAULT_MODEL_KEY: &str = "default_model";

/// Parses the `{"default_model": ...}` convention out of a raw JSON blob
/// (`agent_profiles.limits` or `agent_fleets.default_policy`). Never errors:
/// a missing key, malformed JSON, wrong type, or a partial provider/model_id
/// pair all read as "this tier expressed no opinion" (`None`) — matching
/// `crate::scheduler::wiring::priority_from_metadata`'s own established
/// posture for an unenforced convention over a real column.
pub fn parse_model_default_convention(raw_json: &str) -> Option<ModelSelector> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let default_model = value.get(DEFAULT_MODEL_KEY)?;
    if let Some(literal) = default_model.as_str() {
        return (literal == "auto").then_some(ModelSelector::AutoSelect);
    }
    let provider = default_model.get("provider")?.as_str()?;
    let model_id = default_model.get("model_id")?.as_str()?;
    Some(ModelSelector::Explicit {
        provider: RequestedModelProvider::new(provider),
        model_id: RequestedModelId::new(model_id),
    })
}

/// Decodes `projects.default_model`'s JSON into the [`ModelSelector`] this
/// module resolves against. `None` when the column itself is `None` (the
/// project expressed no opinion) — a genuine decode failure is returned as
/// an error, not folded into that same `None`, per this module's doc
/// comment.
fn parse_project_default_model(raw_json: &str) -> Result<ModelSelector, sqlx::Error> {
    let parsed: ProjectModelDefault =
        serde_json::from_str(raw_json).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    Ok(match parsed {
        ProjectModelDefault::Auto => ModelSelector::AutoSelect,
        ProjectModelDefault::Explicit { provider, model_id } => ModelSelector::Explicit {
            provider: RequestedModelProvider::new(provider),
            model_id: RequestedModelId::new(model_id),
        },
    })
}

/// Fetches each tier's configured default (agent profile, project, fleet)
/// and resolves the final [`ResolvedModelPolicy`] via [`resolve_model_policy`].
///
/// `agent_profile_id`/`project_id`/`fleet_id` are each `None` whenever the
/// request has nothing to read that tier from — no agent profile, no
/// project on the underlying item, or a selector that isn't `fleet` (there
/// is no fleet to read a default from in that case). An absent tier is
/// simply skipped, exactly as if no default had been configured.
pub async fn resolve_request_model_policy(
    repo: &Repository,
    agent_profile_id: Option<&str>,
    project_id: Option<&str>,
    fleet_id: Option<&str>,
    request_override: Option<ModelSelector>,
) -> Result<ResolvedModelPolicy, sqlx::Error> {
    let agent_profile_default = match agent_profile_id {
        Some(id) => repo
            .fetch_agent_profile_limits(id)
            .await?
            .as_deref()
            .and_then(parse_model_default_convention),
        None => None,
    };
    let project_default = match project_id {
        Some(id) => repo
            .fetch_project_default_model(id)
            .await?
            .as_deref()
            .map(parse_project_default_model)
            .transpose()?,
        None => None,
    };
    let fleet_default = match fleet_id {
        Some(id) => repo
            .fetch_fleet_default_policy(id)
            .await?
            .as_deref()
            .and_then(parse_model_default_convention),
        None => None,
    };
    let sources = ModelPolicySources {
        request_override,
        agent_profile_default,
        project_default,
        fleet_default,
    };
    Ok(resolve_model_policy(&sources))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_explicit_default_model() {
        let raw = r#"{"default_model":{"provider":"openai","model_id":"opaque/model-alpha"}}"#;
        let parsed = parse_model_default_convention(raw);
        assert_eq!(
            parsed,
            Some(ModelSelector::Explicit {
                provider: RequestedModelProvider::new("openai"),
                model_id: RequestedModelId::new("opaque/model-alpha"),
            })
        );
    }

    #[test]
    fn parses_an_explicit_auto_default() {
        let raw = r#"{"default_model":"auto"}"#;
        assert_eq!(
            parse_model_default_convention(raw),
            Some(ModelSelector::AutoSelect)
        );
    }

    #[test]
    fn an_unrecognised_literal_is_no_opinion_not_a_crash() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":"sometimes"}"#),
            None
        );
    }

    #[test]
    fn a_missing_key_is_no_opinion() {
        assert_eq!(parse_model_default_convention(r#"{}"#), None);
        assert_eq!(
            parse_model_default_convention(r#"{"other_field":true}"#),
            None
        );
    }

    #[test]
    fn malformed_json_is_no_opinion_not_a_panic() {
        assert_eq!(parse_model_default_convention("not json"), None);
        assert_eq!(parse_model_default_convention(""), None);
    }

    #[test]
    fn a_partial_provider_model_pair_is_no_opinion() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":{"provider":"openai"}}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(
                r#"{"default_model":{"model_id":"opaque/model-alpha"}}"#
            ),
            None
        );
    }

    #[test]
    fn wrong_value_types_are_no_opinion() {
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":5}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":null}"#),
            None
        );
        assert_eq!(
            parse_model_default_convention(r#"{"default_model":{"provider":5,"model_id":"m"}}"#),
            None
        );
    }

    /// Nonsense/opaque values inside the convention JSON round-trip exactly
    /// as typed — the parser must not normalize or reject an id it does not
    /// recognise.
    #[test]
    fn nonsense_ids_inside_the_convention_round_trip_unmodified() {
        let raw = r#"{"default_model":{"provider":"totally-made-up-provider-9000","model_id":"totally-made-up-model-9000"}}"#;
        assert_eq!(
            parse_model_default_convention(raw),
            Some(ModelSelector::Explicit {
                provider: RequestedModelProvider::new("totally-made-up-provider-9000"),
                model_id: RequestedModelId::new("totally-made-up-model-9000"),
            })
        );
    }
}

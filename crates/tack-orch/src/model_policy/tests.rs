use super::*;
use crate::execution::{RequestedModelId, RequestedModelProvider};

fn sentinel(tag: &str) -> ModelSelector {
    ModelSelector::Explicit {
        provider: RequestedModelProvider::new(format!("{tag}-provider")),
        model_id: RequestedModelId::new(format!("{tag}-model")),
    }
}

/// Exhaustive table test over the 2^4 presence combinations of request
/// override / agent profile / project / fleet — including all-absent —
/// with the precedence order pinned. Each tier gets a distinct sentinel
/// value so a wrong winner is caught, not just "some value came back."
#[test]
fn every_presence_combination_resolves_to_the_pinned_precedence_order() {
    for mask in 0u8..16 {
        let request_present = mask & 0b0001 != 0;
        let profile_present = mask & 0b0010 != 0;
        let project_present = mask & 0b0100 != 0;
        let fleet_present = mask & 0b1000 != 0;

        let sources = ModelPolicySources {
            request_override: request_present.then(|| sentinel("request")),
            agent_profile_default: profile_present.then(|| sentinel("profile")),
            project_default: project_present.then(|| sentinel("project")),
            fleet_default: fleet_present.then(|| sentinel("fleet")),
        };

        let (expected_tier, expected_selector) = if request_present {
            (Some(ModelPolicyTier::RequestOverride), sentinel("request"))
        } else if profile_present {
            (Some(ModelPolicyTier::AgentProfile), sentinel("profile"))
        } else if project_present {
            (Some(ModelPolicyTier::Project), sentinel("project"))
        } else if fleet_present {
            (Some(ModelPolicyTier::Fleet), sentinel("fleet"))
        } else {
            (None, ModelSelector::AutoSelect)
        };

        let resolved = resolve_model_policy(&sources);
        assert_eq!(
            resolved.source, expected_tier,
            "mask {mask:#06b}: wrong winning tier"
        );
        assert_eq!(
            resolved.selector, expected_selector,
            "mask {mask:#06b}: wrong resolved selector"
        );
    }
}

#[test]
fn all_tiers_absent_resolves_to_auto_select_with_no_source() {
    let resolved = resolve_model_policy(&ModelPolicySources::default());
    assert_eq!(resolved.selector, ModelSelector::AutoSelect);
    assert_eq!(resolved.source, None);
}

#[test]
fn a_tier_explicitly_configured_as_auto_select_stops_the_walk_there() {
    // Distinct from "absent": the agent profile tier here has a real
    // opinion (auto-select), so a fleet default underneath it must
    // never be consulted, even though AutoSelect and "nothing
    // configured" both ultimately produce ModelSelector::AutoSelect.
    let sources = ModelPolicySources {
        request_override: None,
        agent_profile_default: Some(ModelSelector::AutoSelect),
        project_default: None,
        fleet_default: Some(sentinel("fleet")),
    };
    let resolved = resolve_model_policy(&sources);
    assert_eq!(resolved.selector, ModelSelector::AutoSelect);
    assert_eq!(resolved.source, Some(ModelPolicyTier::AgentProfile));
}

#[test]
fn resolution_is_deterministic_across_repeated_calls() {
    let sources = ModelPolicySources {
        request_override: None,
        agent_profile_default: Some(sentinel("profile")),
        project_default: Some(sentinel("project")),
        fleet_default: Some(sentinel("fleet")),
    };
    let first = resolve_model_policy(&sources);
    for _ in 0..25 {
        assert_eq!(resolve_model_policy(&sources), first);
    }
}

/// "Nonsense id round-trips": an unrecognised model id/provider must
/// flow through resolution byte-for-byte — never normalized, rejected,
/// or coerced into a known value. Covers ASCII punctuation, unicode
/// (including combining/emoji code points), and a very long string.
#[test]
fn nonsense_opaque_ids_round_trip_through_resolution_unmodified() {
    let long_model = "x".repeat(10_000);
    let cases: [(&str, &str); 3] = [
        (
            "totally-made-up-provider-9000",
            "totally-made-up-model-9000",
        ),
        (
            "プロバイダー::🚀",
            "weird/model id with spaces::and:colons 🧭",
        ),
        ("provider-x", long_model.as_str()),
    ];
    for (provider_raw, model_raw) in cases {
        let sources = ModelPolicySources {
            request_override: Some(ModelSelector::Explicit {
                provider: RequestedModelProvider::new(provider_raw),
                model_id: RequestedModelId::new(model_raw),
            }),
            agent_profile_default: None,
            project_default: None,
            fleet_default: None,
        };
        let resolved = resolve_model_policy(&sources);
        match resolved.selector {
            ModelSelector::Explicit { provider, model_id } => {
                assert_eq!(provider.as_str(), provider_raw);
                assert_eq!(model_id.as_str(), model_raw);
            }
            ModelSelector::AutoSelect => panic!("expected the explicit override to win"),
        }
    }
}

/// The same opaque values must also survive a JSON round trip
/// unmodified (the `opaque_id!` macro's `#[serde(transparent)]`
/// contract in `crate::execution::types`) — proven directly here rather
/// than only assumed from that macro's own doc comment.
#[test]
fn nonsense_opaque_ids_round_trip_through_json_unmodified() {
    let very_long = "y".repeat(10_000);
    for raw in [
        "totally-made-up-model-9000",
        "プロバイダー::🚀 combining-\u{0301}",
        very_long.as_str(),
    ] {
        let id = RequestedModelId::new(raw);
        let json = serde_json::to_string(&id).expect("serialize opaque id");
        let round_tripped: RequestedModelId =
            serde_json::from_str(&json).expect("deserialize opaque id");
        assert_eq!(round_tripped.as_str(), raw);
        assert_eq!(id, round_tripped);
    }
}

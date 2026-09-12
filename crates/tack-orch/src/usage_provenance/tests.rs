use super::*;
use std::collections::BTreeMap;

fn actual_model(provider: &str, model_id: &str) -> (ActualModelProvider, ActualModelId) {
    (
        ActualModelProvider::new(provider),
        ActualModelId::new(model_id),
    )
}

#[test]
fn matched_when_requested_equals_actual() {
    let requested = (
        RequestedModelProvider::new("anthropic"),
        RequestedModelId::new("opaque/sonnet"),
    );
    let (actual_provider, actual_model_id) = actual_model("anthropic", "opaque/sonnet");
    let provenance = compare_model_provenance(
        Some((&requested.0, &requested.1)),
        &actual_provider,
        &actual_model_id,
    );
    assert_eq!(
        provenance,
        ModelProvenance::Matched {
            provider: "anthropic".to_string(),
            model_id: "opaque/sonnet".to_string(),
        }
    );
}

/// A requested/actual mismatch must stay visible: both sides must be
/// present in the result, not silently reconciled to whichever the
/// caller might expect.
#[test]
fn mismatch_carries_both_requested_and_actual_values() {
    let requested = (
        RequestedModelProvider::new("anthropic"),
        RequestedModelId::new("opaque/sonnet"),
    );
    let (actual_provider, actual_model_id) = actual_model("anthropic", "opaque/haiku");
    let provenance = compare_model_provenance(
        Some((&requested.0, &requested.1)),
        &actual_provider,
        &actual_model_id,
    );
    assert_eq!(
        provenance,
        ModelProvenance::Mismatched {
            requested_provider: "anthropic".to_string(),
            requested_model_id: "opaque/sonnet".to_string(),
            actual_provider: "anthropic".to_string(),
            actual_model_id: "opaque/haiku".to_string(),
        }
    );

    // And visible on the wire too — both sides present simultaneously,
    // not coalesced into one field.
    let json = serde_json::to_value(&provenance).expect("serialize");
    assert_eq!(json["requested_model_id"], "opaque/sonnet");
    assert_eq!(json["actual_model_id"], "opaque/haiku");
    assert_ne!(json["requested_model_id"], json["actual_model_id"]);
}

#[test]
fn auto_select_observed_is_distinct_from_matched_and_mismatched() {
    let (actual_provider, actual_model_id) = actual_model("openai", "opaque/model-alpha");
    let provenance = compare_model_provenance(None, &actual_provider, &actual_model_id);
    assert_eq!(
        provenance,
        ModelProvenance::AutoSelectObserved {
            actual_provider: "openai".to_string(),
            actual_model_id: "opaque/model-alpha".to_string(),
        }
    );
}

/// Nonsense ids must appear verbatim on both sides of a mismatch —
/// never normalized away.
#[test]
fn nonsense_ids_round_trip_through_a_mismatch_comparison() {
    let requested = (
        RequestedModelProvider::new("totally-made-up-provider-9000"),
        RequestedModelId::new("totally-made-up-model-9000"),
    );
    let (actual_provider, actual_model_id) = actual_model("openai", "opaque/model-alpha");
    let provenance = compare_model_provenance(
        Some((&requested.0, &requested.1)),
        &actual_provider,
        &actual_model_id,
    );
    assert_eq!(
        provenance,
        ModelProvenance::Mismatched {
            requested_provider: "totally-made-up-provider-9000".to_string(),
            requested_model_id: "totally-made-up-model-9000".to_string(),
            actual_provider: "openai".to_string(),
            actual_model_id: "opaque/model-alpha".to_string(),
        }
    );
}

/// Absent usage must never serialize as zero. Asserts the literal JSON
/// shape, not just the Rust value, per CLAUDE.md's "assert the absence
/// directly."
#[test]
fn absent_usage_never_serializes_as_zero() {
    let economics = build_usage_economics(None, None, None, None);
    let json = serde_json::to_value(&economics).expect("serialize");
    assert_eq!(
        json,
        serde_json::json!({
            "model_token_cost_usd_estimated": {"value": null, "source": "not_measured"},
            "runner_time_cost": {
                "wall_clock_ms": null,
                "cost_usd_estimated": {"value": null, "source": "not_measured"}
            }
        })
    );
    // Literal-value sanity check on top of structural equality: no
    // numeric zero anywhere in the serialized economics.
    let raw = json.to_string();
    assert!(
        !raw.contains(":0"),
        "must never encode absent usage as 0: {raw}"
    );
    assert!(
        !raw.contains(":0.0"),
        "must never encode absent usage as 0.0: {raw}"
    );
}

/// The positive control for the test above: real inputs must actually
/// produce real, non-null values — proving the null-everywhere case
/// above is not simply a vacuous "always null" implementation.
#[test]
fn present_usage_and_timestamps_produce_real_values() {
    let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ended = DateTime::parse_from_rfc3339("2026-08-06T12:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let usage = Usage {
        tokens_in: Measurement {
            value: Some(1234),
            source: MeasurementSource::Measured,
            additional: BTreeMap::new(),
        },
        tokens_out: Measurement {
            value: Some(456),
            source: MeasurementSource::Measured,
            additional: BTreeMap::new(),
        },
        duration_ms: Measurement {
            value: Some(1_800_000),
            source: MeasurementSource::Measured,
            additional: BTreeMap::new(),
        },
        cost_usd: Measurement {
            value: Some(0.42),
            source: MeasurementSource::Measured,
            additional: BTreeMap::new(),
        },
        additional: BTreeMap::new(),
    };
    let economics = build_usage_economics(Some(&usage), Some(started), Some(ended), Some(3.0));
    assert_eq!(economics.model_token_cost_usd_estimated.value, Some(0.42));
    assert_eq!(
        economics.model_token_cost_usd_estimated.source,
        MeasurementSource::Measured
    );
    assert_eq!(economics.runner_time_cost.wall_clock_ms, Some(1_800_000));
    // 30 minutes at $3.00/hour = $1.50, and this figure is an
    // independent estimate — it must never equal (or be silently
    // summed with) the harness's own $0.42 cost_usd.
    assert_eq!(
        economics.runner_time_cost.cost_usd_estimated.value,
        Some(1.5)
    );
    assert_eq!(
        economics.runner_time_cost.cost_usd_estimated.source,
        MeasurementSource::Estimated
    );
    assert_ne!(
        economics.runner_time_cost.cost_usd_estimated.value,
        economics.model_token_cost_usd_estimated.value
    );
}

/// A wall clock is derivable even with no rate configured — it must not
/// collapse to `not_measured` just because the dollar estimate does.
#[test]
fn wall_clock_is_known_even_without_a_configured_rate() {
    let started = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let ended = DateTime::parse_from_rfc3339("2026-08-06T12:00:05Z")
        .unwrap()
        .with_timezone(&Utc);
    let cost = compute_runner_time_cost(Some(started), Some(ended), None);
    assert_eq!(cost.wall_clock_ms, Some(5_000));
    assert_eq!(cost.cost_usd_estimated.value, None);
    assert_eq!(
        cost.cost_usd_estimated.source,
        MeasurementSource::NotMeasured
    );
}

#[test]
fn derive_attempt_facts_treats_malformed_json_as_not_yet_reported() {
    let facts = derive_attempt_facts(
        Some("openai"),
        Some("opaque/model-alpha"),
        Some("not json"),
        Some("also not json"),
        None,
        None,
        None,
    );
    assert_eq!(facts.model_provenance, None);
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.value,
        None
    );
}

#[test]
fn derive_attempt_facts_end_to_end_with_a_real_completion_fixture() {
    let actual_execution_json =
        include_str!("../../../../docs/contracts/runner-v1/completion.request.json");
    let completion: serde_json::Value =
        serde_json::from_str(actual_execution_json).expect("fixture JSON");
    let actual = completion["actual_execution"].to_string();
    let usage = completion["usage"].to_string();
    let facts = derive_attempt_facts(
        Some("openai"),
        Some("opaque/model-alpha"),
        Some(&actual),
        Some(&usage),
        Some(
            DateTime::parse_from_rfc3339("2026-08-06T12:20:05Z")
                .unwrap()
                .with_timezone(&Utc),
        ),
        Some(
            DateTime::parse_from_rfc3339("2026-08-06T12:25:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ),
        None,
    );
    assert_eq!(
        facts.model_provenance,
        Some(ModelProvenance::Matched {
            provider: "openai".to_string(),
            model_id: "opaque/model-alpha".to_string(),
        })
    );
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.value,
        None
    );
    assert_eq!(
        facts.usage_economics.model_token_cost_usd_estimated.source,
        MeasurementSource::NotMeasured
    );
    assert_eq!(
        facts.usage_economics.runner_time_cost.wall_clock_ms,
        Some(295_000)
    );
}

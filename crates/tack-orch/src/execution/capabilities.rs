//! Runner capability snapshots and support declarations.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{HarnessKind, ModelId, ModelProvider, ProtocolVersion};

/// The three support levels fixed by runner protocol v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Advisory,
}

/// A capability value coupled to the reason supplied by the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityValue {
    pub support: CapabilitySupport,
    /// `null` is meaningful fixture data: preserve it instead of silently
    /// omitting the key during a round trip.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Per-feature support statements reported by a runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCapabilities {
    pub cancel: CapabilityValue,
    pub resume: CapabilityValue,
    pub decisions: CapabilityValue,
    pub artifacts: CapabilityValue,
    pub usage: CapabilityValue,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Current and total execution capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concurrency {
    pub total: u32,
    pub available: u32,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A provider's own catalog quote for one model — never a Tack judgment and
/// never invented for a vendor that publishes nothing (ADR 0063 decision
/// 7): a field the catalog does not publish stays `None`, not a default or
/// a zero. `price` and `modality` are recorded exactly as the provider's
/// own catalog shapes them (ADR 0063 decision 5) rather than normalized
/// into a typed struct — vendor catalogs use dozens of mutually
/// incompatible shapes for both, and normalizing would silently falsify
/// most of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modality: Option<serde_json::Value>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Models observed for a harness/provider pair. Model IDs are deliberately
/// opaque: their punctuation and prefixes are not a compatibility contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCombination {
    pub model_provider: ModelProvider,
    pub model_ids: Vec<ModelId>,
    pub discovery: String,
    /// Per-model price, context window and modality (ADR 0063 decision 5),
    /// keyed by the model id it describes. A model absent from this map is
    /// one the provider's catalog said nothing about — not a claim of
    /// zero. Absent as a whole field (a runner built before this metadata
    /// existed) defaults to an empty map on parse and is omitted again on
    /// re-serialization, so an older runner's `capabilities.json` round-trips
    /// unchanged against a newer board.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_metadata: BTreeMap<ModelId, ModelMetadata>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// One installed harness and the models it can report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapability {
    pub harness_kind: HarnessKind,
    pub installed_version: String,
    pub probe_error: Option<String>,
    pub probed_at: DateTime<Utc>,
    #[serde(default)]
    pub model_combinations: Vec<ModelCombination>,
    /// Whether this harness accepts an **operator-specified opaque model**
    /// forwarded verbatim by its adapter.
    ///
    /// `Supported` is a claim about the *adapter's own invocation contract*
    /// — "whatever `model_id` the request carries is handed to the harness
    /// unmodified; validity is established by the harness at run time, and a
    /// bad model fails the attempt with the harness's own error, never a
    /// fabricated one." It says nothing about which models exist, so it can
    /// be attested honestly by adapters (claude-code, codex) whose harness
    /// offers no model enumeration — exactly the harnesses whose
    /// `model_combinations` are deliberately empty.
    ///
    /// The scheduler treats only `Supported` as schedulable
    /// (`crates/tack-orch/src/scheduler/select.rs`): `Advisory` is an
    /// unverified claim and capability claims are load-bearing, so it is
    /// rejected identically to `Unsupported`. `None` means the runner did
    /// not attest either way (an older runner, or the shared fake probe)
    /// and behaves the same as if this field did not exist: only declared
    /// `model_combinations` are eligible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_passthrough: Option<CapabilityValue>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Maximum payload values the runner says it can handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    pub event_payload_bytes_max: u64,
    pub artifact_content_bytes_max: u64,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A complete point-in-time runner capability report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerCapabilities {
    /// Standalone capability reports carry the protocol version; embedded
    /// enrollment/refresh capability snapshots inherit it from the enclosing
    /// protocol message and therefore omit this member. Keep that distinction
    /// on the wire rather than materializing a field on re-serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<ProtocolVersion>,
    pub runner_version: String,
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    pub features: FeatureCapabilities,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A capability snapshot embedded inside `enrollment.request.json` or
/// `refresh.request.json`, distinct from a standalone [`RunnerCapabilities`]
/// report (`capabilities.json`).
///
/// This is a different wire shape, not a loosened copy of the standalone
/// one — each field's strictness follows directly from what the two
/// embedding fixtures actually contain:
///
/// - `runner_version` and `protocol_version` have **no field here at all**.
///   Both are present only as *siblings* of `capabilities` in the enclosing
///   enrollment/refresh envelope, never nested inside it, so there is
///   nothing on the wire to default or make optional — a field for either
///   would just always be absent. This is why [`RunnerCapabilities`] (whose
///   `runner_version` is required and has no `serde(default)`, by design —
///   see its own doc comment) cannot parse this shape, and why widening
///   that field there was rejected in favor of this additive type.
/// - `concurrency` and `labels` stay structurally required/typed, matching
///   what `validate_capability_payload` in
///   `crates/tack-api/src/handlers/runner_protocol.rs` already enforces by
///   hand: it errors on a missing or malformed `concurrency`, and rejects a
///   non-object `labels` or non-string label value. Reusing [`Concurrency`]
///   here gives that same shape a real type instead of a second hand-rolled
///   check.
/// - `harnesses` and `features` stay permissive. `refresh.request.json`'s
///   example reports `"harnesses": []` and `"features": {}`, while
///   `enrollment.request.json`'s reports a populated harness list and full
///   per-feature support statements — so `features` is opaque
///   `serde_json::Value` rather than [`FeatureCapabilities`] (whose five
///   support fields are all required, correctly, for the terminal
///   `capability_snapshot` use at completion) and `harnesses` defaults to
///   empty rather than requiring the full [`HarnessCapability`] list.
/// - `reported_at` and `limits` appear, identically shaped, in both
///   fixtures, so they stay required and reuse [`CapabilityLimits`] rather
///   than being loosened without evidence.
/// - Unrecognised keys are preserved via `serde(flatten)`, matching every
///   other additive type in this module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedCapabilitySnapshot {
    pub reported_at: DateTime<Utc>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub concurrency: Concurrency,
    #[serde(default)]
    pub harnesses: Vec<HarnessCapability>,
    #[serde(default)]
    pub features: serde_json::Value,
    pub limits: CapabilityLimits,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
#[path = "capabilities/tests.rs"]
mod tests;

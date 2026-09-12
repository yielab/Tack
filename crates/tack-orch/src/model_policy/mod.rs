//! Deterministic model-selection precedence: request override →
//! agent-profile default → project default → fleet default → (nothing
//! configured) auto-select.
//!
//! [`resolve_model_policy`] is pure: no I/O, no clock, no database handle —
//! see [`wiring`] for the live `tack-db`-backed caller that fetches each
//! tier's configured default and hands the result here, mirroring
//! `crate::scheduler`'s own pure-core/live-wiring split
//! (`select`/`batch` vs `wiring`).
//!
//! # Vocabulary discipline
//!
//! Every tier's value is a [`crate::scheduler::types::ModelSelector`], which
//! itself carries [`crate::execution::RequestedModelProvider`]/
//! [`crate::execution::RequestedModelId`] — the *requested* namespace, never
//! conflated with the *actual* namespace
//! ([`crate::execution::ActualModelProvider`]/[`crate::execution::ActualModelId`])
//! that `crate::usage_provenance` compares against. A resolved value from
//! this module is always still a *request*, whichever tier supplied it —
//! intersecting it against a runner's declared capability is
//! [`crate::scheduler::select::select_runner`]'s job — unmodified by this
//! module; see [`wiring`]'s module doc comment for the integration proof.

pub mod wiring;

use crate::scheduler::types::ModelSelector;

/// The four precedence tiers, most specific first. See `mod.rs`'s test
/// module for the exhaustive 2^4 table proving every presence combination
/// resolves to exactly this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelPolicyTier {
    RequestOverride,
    AgentProfile,
    Project,
    Fleet,
}

impl ModelPolicyTier {
    /// The precedence walk order. `resolve_model_policy` iterates this
    /// exact slice; nothing else in this module may reorder it.
    pub const ORDER: [ModelPolicyTier; 4] = [
        ModelPolicyTier::RequestOverride,
        ModelPolicyTier::AgentProfile,
        ModelPolicyTier::Project,
        ModelPolicyTier::Fleet,
    ];
}

/// One `Option<ModelSelector>` per precedence tier.
///
/// `None` means "this tier expressed no opinion" — not "this tier
/// explicitly requests auto-select." That distinction is load-bearing:
/// `Some(ModelSelector::AutoSelect)` at a tier is a real, if unusual,
/// configuration (an operator explicitly pinning "always auto-select at
/// this level") that *stops* the walk at that tier rather than falling
/// through to a less-specific tier that might name a concrete model. Only
/// an *absent* tier (`None`) is skipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelPolicySources {
    pub request_override: Option<ModelSelector>,
    pub agent_profile_default: Option<ModelSelector>,
    /// Read from `projects.default_model` by
    /// [`wiring::resolve_request_model_policy`].
    pub project_default: Option<ModelSelector>,
    pub fleet_default: Option<ModelSelector>,
}

impl ModelPolicySources {
    fn get(&self, tier: ModelPolicyTier) -> &Option<ModelSelector> {
        match tier {
            ModelPolicyTier::RequestOverride => &self.request_override,
            ModelPolicyTier::AgentProfile => &self.agent_profile_default,
            ModelPolicyTier::Project => &self.project_default,
            ModelPolicyTier::Fleet => &self.fleet_default,
        }
    }
}

/// The outcome of walking [`ModelPolicyTier::ORDER`] against a
/// [`ModelPolicySources`] value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelPolicy {
    pub selector: ModelSelector,
    /// Which tier supplied `selector`. `None` only when every tier was
    /// absent — in that case `selector` is `ModelSelector::AutoSelect` by
    /// construction (the request shape already allows both
    /// `requested_model_provider`/`requested_model_id` to be nullable), not
    /// any tier's own opinion.
    pub source: Option<ModelPolicyTier>,
}

/// Walks [`ModelPolicyTier::ORDER`] and returns the first present tier's
/// value, or `AutoSelect` with `source: None` if every tier is absent. Pure,
/// deterministic, and total — every one of the 2^4 = 16 presence
/// combinations of `sources`' four fields produces exactly one outcome; see
/// this module's test suite for the exhaustive table.
pub fn resolve_model_policy(sources: &ModelPolicySources) -> ResolvedModelPolicy {
    for tier in ModelPolicyTier::ORDER {
        if let Some(selector) = sources.get(tier) {
            return ResolvedModelPolicy {
                selector: selector.clone(),
                source: Some(tier),
            };
        }
    }
    ResolvedModelPolicy {
        selector: ModelSelector::AutoSelect,
        source: None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

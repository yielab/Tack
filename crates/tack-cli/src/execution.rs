//! Execution/fleet/runner/profile request bodies and display helpers shared
//! by `main.rs` (the `tack execution|fleet|runner|agent-profile|model-profile`
//! commands) and `mcp.rs` (the matching MCP tools), so the two entry points
//! can never build a differently-shaped request for the same operation.
//!
//! There is no `docs/contracts/runner-v1/` fixture for this surface — that
//! directory is the frozen authority for the *runner* wire protocol
//! (`/api/runner/v1/*`), a different, deliberately distinct domain from the
//! *operator* execution/fleet/runner/profile API this module targets. The shape
//! authority for these routes is instead the request structs the handlers
//! themselves deserialize (`tack_api::handlers::executions::CreateExecution`
//! and `tack_api::handlers::runner_admin::{CreateFleet, CreateProfile,
//! CreateModelProfile, CreatePendingRunner}`), which every request-body
//! builder here is tested against directly (see the `shape` tests below) —
//! deserializing the exact JSON this module builds into the exact struct the
//! server deserializes, so a renamed or dropped field fails a test in this
//! crate instead of only surfacing as a live 400 later.

use serde_json::{Value, json};

/// Parse a CLI/MCP-supplied JSON blob argument. Unlike `tack field set`
/// (which falls back to treating unparsable input as a literal string,
/// appropriate for a single scalar field value), these arguments always map
/// onto a structured `Value` field on the wire — silently downgrading bad
/// JSON to a string here would send a request shaped nothing like what the
/// caller asked for. Fail with a clear, field-named message instead —
/// unsupported/invalid input is reported, never quietly reinterpreted.
pub fn parse_json_field(raw: &str, field: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|e| format!("--{field} must be valid JSON: {e}"))
}

/// Same as [`parse_json_field`], but `None` (the flag was omitted) becomes
/// `{}` — every optional JSON-blob argument in this module defaults to an
/// empty object, matching the server's own defaults for the same fields
/// (`CreateFleet::default_policy`, `CreateProfile::tool_policy`/`limits`,
/// `CreatePendingRunner::labels`/`capability_snapshot`).
pub fn parse_json_field_or_empty(raw: Option<&str>, field: &str) -> Result<Value, String> {
    match raw {
        Some(s) => parse_json_field(s, field),
        None => Ok(json!({})),
    }
}

/// Generates a fresh per-invocation idempotency/recovery key when the caller
/// doesn't supply one. Deliberately *not* `uuid::Uuid` — the server treats
/// `idempotency_key`/`recovery_key` as an opaque `String` it hashes into a
/// scope, never as a parsed UUID (unlike `item_id`), so a random-enough
/// opaque string needs no extra dependency. A caller that wants a stable,
/// retry-safe key passes `--idempotency-key`/`--recovery-key` explicitly;
/// this default only has to avoid same-process collisions, not be
/// cryptographically unpredictable.
pub fn new_opaque_key(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{}-{nanos:x}", std::process::id())
}

// ── Execution request bodies ───────────────────────────────────────────────

/// One selector value from `--runner <id>` / `--fleet <id>` (mutually
/// exclusive; `main.rs`'s `clap::ArgGroup` enforces exactly one is given).
pub enum Selector {
    ExactRunner(String),
    Fleet(String),
}

impl Selector {
    fn kind(&self) -> &'static str {
        match self {
            Selector::ExactRunner(_) => "exact_runner",
            Selector::Fleet(_) => "fleet",
        }
    }
    fn id(&self) -> &str {
        match self {
            Selector::ExactRunner(id) | Selector::Fleet(id) => id,
        }
    }
}

#[derive(Default)]
pub struct CreateExecutionArgs<'a> {
    pub item_id: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub agent_profile_id: &'a str,
    pub requested_harness_kind: &'a str,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    /// JSON object matching `tack_orch::execution::AgentProfileSnapshot`:
    /// `{name, instructions, tool_policy, timeout_seconds, budgets}` are all
    /// required (`subdirectory`-style `Option` fields aside, this struct has
    /// none). Required, not defaulted to `{}` — an empty object always fails
    /// this shape server-side, so defaulting to it would just move a
    /// guaranteed error one step later instead of catching it at the CLI.
    pub agent_profile_snapshot: &'a str,
    pub repository_snapshot: &'a str,
    /// JSON object matching `tack_orch::execution::PermissionPolicy`:
    /// `{network: bool, tools: [string] (optional, default [])}`. Required
    /// for the same reason as `agent_profile_snapshot` — `network` has no
    /// default server-side, so `{}` cannot succeed.
    pub permission_policy: &'a str,
    pub budgets: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub metadata: Option<&'a str>,
    pub timeout_seconds: u64,
    pub status_map_policy_id: Option<&'a str>,
}

/// Already-parsed values for the same fields `CreateExecutionArgs` carries as
/// argv strings. `mcp.rs`'s `create_execution` tool receives these blobs as
/// native JSON-RPC values (an LLM caller writes a nested object, not a
/// stringified one) and builds this directly; `build_create_execution_body`
/// below parses the CLI's `--flag '<json text>'` strings into exactly this
/// shape and then defers to it. Either entry point ends here, so neither can
/// diverge from the other.
pub struct CreateExecutionValues<'a> {
    pub item_id: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub agent_profile_id: &'a str,
    pub requested_harness_kind: &'a str,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    pub agent_profile_snapshot: Value,
    pub repository_snapshot: Value,
    pub permission_policy: Value,
    pub budgets: Value,
    pub environment: Value,
    pub metadata: Value,
    pub timeout_seconds: u64,
    pub status_map_policy_id: Option<&'a str>,
}

/// The single place that assembles a `POST /api/executions` body
/// (`CreateExecution`'s exact field set) from already-parsed values. See
/// [`CreateExecutionValues`] for why both the CLI and MCP entry points call
/// this instead of building the `json!` object independently.
pub fn create_execution_body(selector: &Selector, values: CreateExecutionValues<'_>) -> Value {
    let idempotency_key = values
        .idempotency_key
        .map(str::to_owned)
        .unwrap_or_else(|| new_opaque_key("cli-exec"));
    json!({
        "item_id": values.item_id,
        "idempotency_key": idempotency_key,
        "selector_kind": selector.kind(),
        "selector_id": selector.id(),
        "agent_profile_id": values.agent_profile_id,
        "requested_harness_kind": values.requested_harness_kind,
        "requested_model_provider": values.requested_model_provider,
        "requested_model_id": values.requested_model_id,
        "agent_profile_snapshot": values.agent_profile_snapshot,
        "repository_snapshot": values.repository_snapshot,
        "permission_policy": values.permission_policy,
        "budgets": values.budgets,
        "environment": values.environment,
        "metadata": values.metadata,
        "timeout_seconds": values.timeout_seconds,
        "status_map_policy_id": values.status_map_policy_id,
    })
}

/// Builds the `POST /api/executions` body from the CLI's argv-string flags,
/// parsing each JSON-blob flag and then delegating to
/// [`create_execution_body`] — see that function's doc for why.
pub fn build_create_execution_body(
    selector: &Selector,
    args: &CreateExecutionArgs<'_>,
) -> Result<Value, String> {
    let values = CreateExecutionValues {
        item_id: args.item_id,
        idempotency_key: args.idempotency_key,
        agent_profile_id: args.agent_profile_id,
        requested_harness_kind: args.requested_harness_kind,
        requested_model_provider: args.requested_model_provider,
        requested_model_id: args.requested_model_id,
        agent_profile_snapshot: parse_json_field(
            args.agent_profile_snapshot,
            "agent-profile-snapshot",
        )?,
        repository_snapshot: parse_json_field(args.repository_snapshot, "repository")?,
        permission_policy: parse_json_field(args.permission_policy, "permission-policy")?,
        budgets: parse_json_field_or_empty(args.budgets, "budgets")?,
        environment: parse_json_field_or_empty(args.environment, "environment")?,
        metadata: parse_json_field_or_empty(args.metadata, "metadata")?,
        timeout_seconds: args.timeout_seconds,
        status_map_policy_id: args.status_map_policy_id,
    };
    Ok(create_execution_body(selector, values))
}

pub fn selector_from_flags(runner: Option<&str>, fleet: Option<&str>) -> Result<Selector, String> {
    match (runner, fleet) {
        (Some(r), None) => Ok(Selector::ExactRunner(r.to_string())),
        (None, Some(f)) => Ok(Selector::Fleet(f.to_string())),
        (None, None) => Err("exactly one of --runner or --fleet is required".to_string()),
        (Some(_), Some(_)) => Err("--runner and --fleet are mutually exclusive".to_string()),
    }
}

/// Builds the `POST /api/executions/{id}/requeue` body (`RecoveryConfirmation`).
pub fn build_requeue_body(recovery_key: &str, reason: &str) -> Value {
    json!({ "recovery_key": recovery_key, "reason": reason })
}

// ── Fleet / profile / runner bodies ────────────────────────────────────────

/// Builds the `POST /api/runner-fleets` body (`CreateFleet`).
pub fn build_create_fleet_body(
    name: &str,
    concurrency_limit: Option<i64>,
    default_policy: Option<&str>,
) -> Result<Value, String> {
    Ok(json!({
        "name": name,
        "concurrency_limit": concurrency_limit,
        "default_policy": parse_json_field_or_empty(default_policy, "policy")?,
    }))
}

/// Builds the `POST /api/agent-profiles` body (`CreateProfile`).
pub fn build_create_agent_profile_body(
    name: &str,
    instructions: &str,
    tool_policy: Option<&str>,
    limits: Option<&str>,
) -> Result<Value, String> {
    Ok(json!({
        "name": name,
        "instructions": instructions,
        "tool_policy": parse_json_field_or_empty(tool_policy, "tool-policy")?,
        "limits": parse_json_field_or_empty(limits, "limits")?,
    }))
}

/// Builds the `POST /api/model-profiles` body (`CreateModelProfile`). No JSON
/// blob arguments, so this cannot fail on parse.
pub fn build_create_model_profile_body(
    name: &str,
    model_provider: &str,
    model_id: &str,
    config_reference: Option<&str>,
) -> Value {
    json!({
        "name": name,
        "model_provider": model_provider,
        "model_id": model_id,
        "config_reference": config_reference,
    })
}

#[derive(Default)]
pub struct EnrollRunnerArgs<'a> {
    pub name: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub labels: Option<&'a str>,
    pub capability_snapshot: Option<&'a str>,
    pub protocol_version: Option<i64>,
    pub enrollment_lifetime_seconds: Option<i64>,
}

/// Builds the `POST /api/runners/enrollment` body (`CreatePendingRunner`).
/// Takes no secret — the raw enrollment token is generated server-side and
/// returned in the response, never supplied by the caller, so it never has
/// to flow through a CLI argument (and therefore never through `argv`/`ps`)
/// on the way in.
pub fn build_enroll_runner_body(args: &EnrollRunnerArgs<'_>) -> Result<Value, String> {
    let mut body = json!({
        "name": args.name,
        "labels": parse_json_field_or_empty(args.labels, "labels")?,
        "total_capacity": args.total_capacity,
        "available_capacity": args.available_capacity,
        "capability_snapshot": parse_json_field_or_empty(args.capability_snapshot, "capability-snapshot")?,
    });
    if let Some(v) = args.protocol_version {
        body["protocol_version"] = json!(v);
    }
    if let Some(v) = args.enrollment_lifetime_seconds {
        body["enrollment_lifetime_seconds"] = json!(v);
    }
    Ok(body)
}

// ── Display helpers ─────────────────────────────────────────────────────────

/// Short, fixed-width-friendly annotation for an execution request's
/// `state`, used in both `execution list`'s table and `execution get`'s
/// detail view so `needs_operator` and `lost` never render as if they were
/// just another quiet in-progress value — distinct, visible outcomes, not
/// collapsed into one generic line. States come from
/// the frozen `tack_orch::execution::ExecutionState` lifecycle; an unrecognized value is flagged rather
/// than silently printed bare, in case a newer server adds one this CLI
/// doesn't know about yet. `execution get` prints additional guidance below
/// this marker for `needs_operator`/`lost` — see `main.rs`'s
/// `cmd_execution_get` — since a single-record detail view has room for
/// prose that a table column does not.
pub fn describe_state(state: &str) -> &'static str {
    match state {
        "queued" | "leased" | "preparing" | "running" | "waiting_decision" => "",
        "succeeded" => " (done)",
        "failed" => " (failed)",
        "cancelled" => " (cancelled)",
        "lost" => " (LOST)",
        "needs_operator" => " (NEEDS OPERATOR)",
        _ => " (unrecognized state)",
    }
}

#[cfg(test)]
#[path = "execution/tests.rs"]
mod tests;

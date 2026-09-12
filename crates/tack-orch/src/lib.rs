//! `tack-orch` — the control-plane orchestration client for Tack.
//!
//! Defines [`ControlPlane`], the trait every agent-fleet backend (docket today,
//! something else tomorrow) implements, plus the DTOs that cross the
//! Tack ⇄ control-plane boundary. Concrete adapters (`adapters::docket`)
//! and the reconciler poll loop (`reconciler`) build on top of this.
//!
//! # Dependency direction
//!
//! This crate depends inward on `tack-core` and `tack-db` only. **It must never
//! depend on `tack-api`.** `tack-api` depends on `tack-orch` — to spawn the
//! reconciler and to expose the `/api/control-planes`, `/api/fleet`, and
//! dispatch routes — not the other way around. If you're an agent reaching for
//! `tack-api` types from in here (e.g. to reuse a handler DTO), stop: define the
//! type here instead and let `tack-api` depend on it, or duplicate the small
//! shape rather than inverting the graph. See
//! `docs/book/src/developer/orchestration.md`.
//!
//! # Money is always an estimate
//!
//! Every dollar-valued field in this crate is named `*_usd_estimated` (never
//! `*_usd` alone) — token counts are the primary, trustworthy measure; docket's
//! own driver does not report real spend (see `docket/core/dispatch.py`'s
//! `pod_gating_cost`), so any dollar figure downstream of it is derived, not
//! recorded.
//!
//! # Unknown enum values never fail a poll
//!
//! [`RunState`], [`RunSource`], [`TaskStatus`], and [`ApprovalState`] each carry
//! an `Unknown(String)` fallback variant with a hand-written `Deserialize` (a
//! plain `#[serde(other)]` only works on unit variants, and we need the
//! original string preserved so it can round-trip back out). A docket upgrade
//! that adds a new state must degrade to "shown as-is", never to a
//! deserialization error that kills the reconciler's poll loop.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod adapters;
pub mod execution;
// Runtime retention and observability for the
// execution domain. Siblings of `execution` (not submodules of it) because
// both are I/O-bearing background tasks — see `execution_retention`'s
// module doc for why that boundary matters.
pub mod execution_observability;
pub mod execution_retention;
pub mod model_policy;
pub mod reconciler;
pub mod scheduler;
pub mod usage_provenance;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong talking to a control plane.
#[derive(Debug, thiserror::Error)]
pub enum OrchError {
    /// Transport-level failure: connection refused, timeout, DNS, TLS, a non-2xx
    /// status the other variants don't more specifically describe, etc.
    #[error("control plane http error: {0}")]
    Http(String),

    /// The control plane rejected our credentials (401/403).
    #[error("control plane authentication failed")]
    Auth,

    /// The response body didn't parse into the DTO we expected — malformed
    /// JSON, a field of the wrong shape, a Prometheus line we couldn't tokenize.
    #[error("failed to decode control plane response: {0}")]
    Decode(String),

    /// The requested resource (run, task, approval, control plane) doesn't
    /// exist on the remote side.
    #[error("not found: {0}")]
    NotFound(String),

    /// The control plane is configured but not currently reachable (down,
    /// health check failing, apiVersion mismatch). Distinct from `Http` so
    /// callers can degrade a plane's displayed state without treating every
    /// single request failure as one.
    #[error("control plane unavailable: {0}")]
    Unavailable(String),

    /// The requested operation is gated behind a feature flag or missing
    /// configuration (e.g. `TACK_ORCH_ENABLE` unset, no approval token
    /// configured, a write method called against an adapter that only
    /// implements the read side).
    #[error("control plane feature disabled")]
    Disabled,

    /// docket's `pre_input` policy gate deliberately refused a dispatch —
    /// a transport *success* carrying a considered "no", not a transport
    /// failure. Verified live that a `block` verdict comes back as
    /// HTTP 400 naming the policy id that fired
    /// (`"task rejected by guardrail policy '<id>' at enqueue: <message>"`).
    /// `policy_id` is that id, parsed out once here so every caller gets a
    /// typed field instead of pattern-matching a prefix on a string.
    /// `message` is
    /// docket's own text, kept verbatim for display.
    #[error("blocked by guardrail policy {policy_id:?}: {message}")]
    PolicyBlocked { policy_id: String, message: String },

    /// docket already resolved this approval before our decision reached it —
    /// `POST /approvals/{token}` returning HTTP 409, `approval.ApprovalNoop`
    /// server-side (e.g. granted moments earlier from the CLI, or expired
    /// past `APPROVAL_TIMEOUT`). Distinct from [`OrchError::NotFound`] (404 —
    /// no such token at all, or docket's `approval.ApprovalError` for an
    /// illegal state transition, which `serve.py` happens to report with the
    /// same status) so a caller building an approvals inbox can
    /// render "someone already decided this" — a normal, expected race, not
    /// a hard error — and simply drop the stale row rather than surface a
    /// scary failure. `message` is docket's own text (e.g. `"Already
    /// granted: apr-..."`), kept verbatim for display.
    #[error("approval already decided: {0}")]
    AlreadyDecided(String),

    /// The remote resource we tried to create already exists — docket's
    /// `PodAlreadyExistsError` (`POST /pods` → HTTP 409). Its own
    /// doc comment (`core/pod_provisioning.py`) calls this "skip, don't
    /// clobber," matching the declarative `--from` path's long-standing
    /// idempotence contract. Distinct from [`OrchError::AlreadyDecided`] (an
    /// approval-specific conflict shape) so a provisioning caller isn't
    /// forced to pattern-match a message string to tell "this name is
    /// taken" from any other conflict. `message` is docket's own text
    /// (e.g. `"'my-project' already exists"`), kept verbatim for display.
    #[error("already exists: {0}")]
    AlreadyExists(String),
}

// ---------------------------------------------------------------------------
// Remote state enums — the exact strings docket emits, each
// with an `Unknown(String)` fallback that round-trips.
// ---------------------------------------------------------------------------

/// Generates a fieldless enum over a fixed set of wire strings, plus an
/// `Unknown(String)` fallback, with hand-written `Serialize`/`Deserialize`
/// (via `String`) so an unrecognised value degrades instead of erroring — and
/// re-serializes back to the exact string it was read from.
macro_rules! remote_string_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$vmeta:meta])* $variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$vmeta])* $variant,)+
            /// A value docket sent that this version of Tack doesn't recognise.
            /// Carries the original wire string verbatim so it can be shown
            /// as-is and re-serialized without loss.
            Unknown(String),
        }

        impl $name {
            /// The exact wire string this variant round-trips to/from.
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $wire,)+
                    $name::Unknown(s) => s.as_str(),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                match s.as_str() {
                    $($wire => $name::$variant,)+
                    _ => $name::Unknown(s),
                }
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                match s {
                    $($wire => $name::$variant,)+
                    other => $name::Unknown(other.to_string()),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                // `#[serde(other)]` only works on a unit fallback variant with
                // no payload; capturing the original string requires going
                // through `String` ourselves.
                let s = String::deserialize(deserializer)?;
                Ok(Self::from(s))
            }
        }
    };
}

remote_string_enum! {
    /// A dispatch run's lifecycle state. Verified against
    /// `docket/core/runs.py`'s `RunState` literal.
    pub enum RunState {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

remote_string_enum! {
    /// What triggered a dispatch run. Verified against `docket/core/runs.py`'s
    /// `RunSource` literal.
    pub enum RunSource {
        Cli => "cli",
        Webhook => "webhook",
        Schedule => "schedule",
        Sweep => "sweep",
        Mcp => "mcp",
    }
}

remote_string_enum! {
    /// A queued task's status within a pod's pipeline. Verified against the
    /// task state machine in `docket/core/dispatch.py` (`enqueue_task`,
    /// `_claim_next_task`, `TaskResult.status`).
    pub enum TaskStatus {
        Pending => "pending",
        Running => "running",
        Done => "done",
        Failed => "failed",
        Blocked => "blocked",
        WaitingApproval => "waiting_approval",
    }
}

remote_string_enum! {
    /// A pending-approval record's state. Verified against
    /// `docket/core/approval.py`'s `approval_grant`/`approval_deny`/
    /// `list_pending`.
    pub enum ApprovalState {
        Pending => "pending",
        Granted => "granted",
        Denied => "denied",
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------
//
// Field-name convention: every struct below mirrors the JSON shape of the
// docket endpoint it comes from. docket's `serve.py`-rendered endpoints
// (`/status.json`, `/runs`, `/runs/{id}`, `/approvals`) use camelCase; its
// trace/event records (`core/trace.py`) use snake_case — that split is real,
// not an inconsistency introduced here, so each struct's `#[serde(...)]`
// attributes follow the endpoint it actually came from rather than a single
// blanket convention.

/// `GET /health` — liveness only. Format: `{"status":"ok","gateway":N}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Health {
    /// Always `"ok"` when the server answers at all — docket's `/health`
    /// has no unhealthy body, only "doesn't respond".
    pub status: String,
    /// The gateway service's own liveness bit: `1` = active, `0` = inactive.
    /// Kept as the wire integer rather than coerced to `bool` so this struct
    /// never silently disagrees with docket about what the byte means.
    pub gateway: u8,
}

/// One entry in [`FleetStatus::agents`] — a docket project agent or
/// specialist. Mirrors `serve.py`'s `_agent_record()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetAgent {
    pub id: String,
    pub name: String,
    /// `"project"` or `"specialist"`.
    pub kind: String,
    pub scope: String,
    pub model: String,
    pub registered: bool,
    pub bindings: Vec<FleetBinding>,
    /// RFC3339 timestamp, or the literal string `"never"` — docket's own
    /// sentinel (`serve.py`'s `_last_activity_or_never`), not empty/null.
    pub last_activity: String,
    /// Cumulative estimated spend for this agent. See the module-level note
    /// on money fields — docket's own driver reports no real cost, so this
    /// number is always an estimate even though docket's wire field is named
    /// `costUsd` without qualification.
    #[serde(rename = "costUsd")]
    pub cost_usd_estimated: f64,
    /// `None` when unset or `0`/empty on the docket side (see
    /// `_agent_record`'s `budget_raw` handling) — "no budget cap configured",
    /// not "budget is zero".
    pub budget_usd: Option<f64>,
}

/// One channel binding for a [`FleetAgent`]. Mirrors
/// `core/fleet.py`'s `agent_bindings()`: `[{channel, peerId}, ...]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetBinding {
    pub channel: String,
    pub peer_id: String,
}

/// `GET /status.json` — the fleet-wide snapshot. Mirrors `serve.py`'s
/// `build_status()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetStatus {
    /// docket's `SERVE_API_VERSION` — bumped on any breaking contract change.
    /// Check this on every poll; a mismatch should degrade the plane rather
    /// than risk misparsing a shape that has silently changed underneath us.
    pub api_version: String,
    pub timestamp: String,
    /// `"active"` or `"inactive"`.
    pub gateway: String,
    pub channels: Vec<String>,
    pub agents: Vec<FleetAgent>,
    /// Sum of every agent's `cost_usd_estimated`. See the module-level note
    /// on money fields.
    #[serde(rename = "totalCostUsd")]
    pub total_cost_usd_estimated: f64,
}

/// One parsed line out of `GET /metrics` (Prometheus text exposition format).
/// `name` is the bare metric name (e.g. `docket_agent_cost_usd`); `labels` is
/// the `{k="v", ...}` label set, if any; `value` is the trailing number.
/// Comment (`#`) lines never produce a sample. The parser that produces these
/// lives in `adapters::prometheus` (`adapters::docket`'s `metrics()` calls
/// it) — do not write a second one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

/// `GET /runs` (one element of the `runs` array) / `GET /runs/{id}` (the body
/// directly). Mirrors `core/runs.py`'s run record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRun {
    /// docket's own id, formatted `run-<uuid>` — kept as an opaque string
    /// rather than parsed as a `Uuid`, since the `run-` prefix is part of the
    /// identifier docket's own APIs expect back (`GET /runs/{id}`).
    pub id: String,
    pub source: RunSource,
    pub project: String,
    pub state: RunState,
    /// Task ids this run actually touched — populated only once the run
    /// reaches a terminal state (`core/runs.py`'s `finish_run`).
    #[serde(default)]
    pub task_ids: Vec<String>,
    /// Exception text for a `failed` run; empty for every other state.
    #[serde(default)]
    pub error: String,
    /// ISO 8601 timestamp (`datetime.now(UTC).isoformat()` — offset form, not
    /// docket's other `...Z` convention). Kept as a raw string; parse at the
    /// call site if a typed timestamp is needed, rather than risk a poll-loop
    /// failure on a format this crate doesn't defensively handle.
    pub created: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// pids of any hop subprocess currently in flight for this run. Only ever
    /// meaningful on the machine actually running docket; mirrored here for
    /// display/audit, never acted on remotely.
    #[serde(default)]
    pub pids: Vec<i64>,
    /// The resolved pipeline variable namespace this run was dispatched with
    /// (only ever populated for a `webhook`-sourced run today).
    #[serde(default)]
    pub variables: serde_json::Value,
}

/// `GET /approvals` (one element of the `pending` array). Mirrors
/// `core/approval.py`'s approval record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteApproval {
    /// Formatted `apr-<uuid>` — see [`RemoteRun::id`]'s note on why this
    /// stays an opaque string.
    pub token: String,
    pub project: String,
    pub role: String,
    /// Human-readable description of the gated action, already redacted by
    /// docket before it reaches this struct.
    pub action: String,
    pub state: ApprovalState,
    pub created: String,
    /// Caller-supplied, stored verbatim by docket (`approval_create`'s
    /// `context` parameter). The one documented shape today is
    /// `{"taskId": "...", "pipelineIndex": 0}` from a dispatch-pipeline gate,
    /// but the field is an open dict on the docket side — kept as
    /// `serde_json::Value` rather than a typed struct so a caller with a
    /// different context shape (or none) still deserializes cleanly. Callers
    /// that want the dispatch correlation should look up `taskId` explicitly
    /// and treat its absence as "an uncorrelated approval", not as a parse
    /// error.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// `GET /tasks/{project}` — real wire shape, live-verified. See
/// `adapters::docket`'s module doc, "Verified live", for the capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTask {
    pub id: String,
    pub description: String,
    /// `"high"` | `"normal"` | `"low"`.
    pub priority: String,
    pub status: TaskStatus,
    pub created: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    /// `"operator"` in every task docket enqueues today; kept as a plain
    /// string (not an enum) since this is speculative pending the real
    /// endpoint, and a closed set here would be one more thing to get wrong.
    pub source: String,
    #[serde(default)]
    pub reason: String,
    #[serde(rename = "costUsd", default)]
    pub cost_usd_estimated: f64,
    pub claim_id: Option<String>,
    pub claimed_at: Option<String>,
    pub approval_token: Option<String>,
    pub pending_approval_index: Option<i64>,
}

/// Body for `POST /tasks/{project}` — real, live-verified endpoint. See
/// `adapters::docket`'s module doc, "Verified live", for the capture.
/// `description` and
/// `priority` mirror `core/dispatch.py`'s `enqueue_task(project, description,
/// priority)`. `trusted` is not a parameter of
/// `enqueue_task` today (which derives trust from a fixed `source ==
/// "operator"` check); an imported item must enqueue with `trusted: false`
/// so docket's `pre_input` guardrail policy
/// evaluates it as untrusted, attacker-authored text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRemoteTask {
    pub description: String,
    /// `"high"` | `"normal"` | `"low"`; docket defaults to `"normal"` when
    /// omitted or unrecognised (`enqueue_task`'s own fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// `false` for any item whose text originated outside Tack's own UI/CLI
    /// (e.g. GitHub/Linear import) — see this struct's doc comment.
    #[serde(default)]
    pub trusted: bool,
}

/// `POST /pods` request body — verified
/// directly against `serve.py::_handle_post_pods` and
/// `core/pod_provisioning.py::provision_pod` (docket commit `0d84f47`,
/// P22-5), not inferred: `{project, path, blueprint, pod, budget,
/// verifyCmd}`, every field but `project` optional. Field-by-field:
///
/// - `project` — the docket-side pod identifier. **Not** derived from
///   Tack's own project name anywhere in this crate — the HTTP handler
///   that builds this (`tack-api::handlers::provisioning`) requires the
///   caller to name it explicitly, so a retry after a partial failure can
///   be typed back in verbatim instead of risking a second, differently
///   -named pod for the same intent.
/// - `path` — interpreted per the blueprint's `workspace_kind`: a
///   `codebase` blueprint (`software`) treats it as the pod's codebase
///   path; a `workdir` blueprint (`research`/`content`/`ops`/
///   `agentic-product`) treats it as the shared working directory,
///   auto-provisioned by docket when empty. Empty string, not `None` —
///   docket's own `body.get("path", "")` default.
/// - `pod` — mirrors `docket add --pod full`. docket only accepts the
///   literal string `"full"` (any other value is a `400`); `None` omits
///   the key entirely, which is what every blueprint other than `software`
///   should send (docket silently ignores it there per its own
///   `_handle_post_pods` comment).
/// - `budget` — a cap override (`None` = fall back to the blueprint's own
///   default). Unsuffixed (not `*_usd_estimated`) because this is an
///   operator-set ceiling, not a derived spend figure — same reasoning as
///   `orch_links.budget_usd` (estimates are governed separately from
///   caps).
/// - `verify_cmd` — applied to Implementer member(s) at creation time.
///   docket validates it server-side (`validate_verify_cmd`: no NUL byte,
///   no newline, ≤2000 chars) and returns `400` on failure; this crate does
///   not duplicate that check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProvisionPodParams {
    pub project: String,
    #[serde(default)]
    pub path: String,
    pub blueprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<f64>,
    #[serde(
        rename = "verifyCmd",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub verify_cmd: String,
}

/// One pod member docket actually created — `POST /pods`'s `members[]`,
/// `{"id": ..., "role": ..., "model": ...}` (verified against
/// `_handle_post_pods`'s response body).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvisionedPodMember {
    pub id: String,
    pub role: String,
    pub model: String,
}

/// `POST /pods`'s `201` success body: `{"ok": true, "project": ...,
/// "blueprint": ..., "members": [...]}` — `ok` is not modeled (only ever
/// `true` on a 2xx), same "unmodeled key costs nothing" discipline as
/// [`EnqueueTaskResponse`] in `adapters::docket`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProvisionedPod {
    pub project: String,
    pub blueprint: String,
    pub members: Vec<ProvisionedPodMember>,
}

/// One page of `GET /traces/{project}?since=` — the events themselves, plus
/// the remote's own resume cursor to send back as `since` on the next call.
///
/// **`next` is opaque.** Tack must never parse it, decode it, or reconstruct
/// it client-side — it is whatever the control plane minted, persisted
/// verbatim, and handed back unexamined. This DTO exists specifically so
/// that discipline is structural rather than a convention someone has to
/// remember: before this type existed, [`ControlPlane::traces`] had nowhere
/// to carry a remote-minted cursor back out, and `tack-orch`'s reconciler
/// reimplemented docket's own compound `"<ts>Z:<n>"` cursor algorithm
/// client-side to work around that gap — correct, but one silent algorithm
/// change away from quietly skipping or duplicating events. That
/// reconstruction is gone; this field is the fix.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TracesPage {
    pub events: Vec<RemoteEvent>,
    /// `None` only if the remote genuinely didn't send one — defensive, not
    /// expected: docket always mints `next` on `GET /traces/{project}`
    /// A caller with `next: None` should treat the
    /// cursor as unchanged rather than erroring the poll.
    #[serde(default)]
    pub next: Option<String>,
}

/// One trace/event record. Mirrors `core/trace.py`'s JSONL record shape
/// exactly, **including its snake_case field names** — trace events are the
/// one docket surface that is not camelCase (contrast every other DTO in this
/// file, which mirrors a `serve.py` JSON endpoint). Produced by `GET
/// /traces/{project}` — real, live-verified endpoint; see
/// `adapters::docket`'s module doc.
///
/// `event_type` is deliberately a plain `String`, not an enum: docket's
/// `EVENT_TYPES` set (`core/trace.py`) is large, open to growth, and an
/// unknown type must be **stored verbatim** — exactly the property a plain
/// string gives for free, without needing this crate's `Unknown(String)`
/// machinery at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteEvent {
    /// `"YYYY-MM-DDTHH:MM:SSZ"` (`core/trace.py`'s `_now_iso()`).
    pub ts: String,
    pub project: String,
    pub session_id: String,
    pub agent_role: String,
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Only ever set on a handful of event types (e.g. `cost_charged`) — see
    /// the module-level note on money fields for why this is named
    /// `_estimated` even though docket's own field is the bare `cost_usd`.
    #[serde(rename = "cost_usd", default)]
    pub cost_usd_estimated: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Capabilities — what an adapter can actually do
// ---------------------------------------------------------------------------
//
// Why this exists: with one adapter, every UI control could safely assume
// "docket can do this" (or hard-code the one case it can't, e.g. the budget-
// pause note `frontend/src/features/settings/orchestration/format.ts` used
// to carry as a prose string). A second adapter with a genuinely different
// shape (no pods, no roles, no approval store) breaks that assumption
// silently unless "what can this plane do" becomes a value the caller reads,
// not a fact baked into a component. A capability is
// a value, never a provider check — `rg -n "kind === 'docket'"` in
// `frontend/src` must stay empty.

/// Three-state support level for a capability that isn't a plain yes/no —
/// `pause`/`resume`/`model_selection` all have a middle ground a bare `bool`
/// can't express (docket ignoring a model override is not the same failure
/// mode as GitHub Actions having no pause endpoint at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// No mechanism exists on this provider, in either direction.
    Unsupported,
    /// A mechanism exists but the provider may not honour it (e.g. a
    /// caller-supplied model the provider's own routing can still override).
    Advisory,
    /// The provider does exactly what was asked.
    Supported,
}

/// How narrowly an adapter's event stream can be scoped. Not a ranking —
/// `Project` and `Run` are incomparable, not one "better" than the other —
/// each adapter serves whatever its own provider's event API actually
/// offers. See [`ControlPlane::capabilities`]'s doc comment for why this
/// can't be widened or narrowed by a caller: docket's `RemoteEvent` carries
/// no run id (`reconciler.rs`'s `persist_events` says so directly), so a
/// `Run`-scoped read is unimplementable for it, not just unimplemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventScope {
    /// No event/trace stream exists on this provider at all.
    None,
    Run,
    Project,
    Plane,
}

/// How a caller learns about a decision (approval, deployment gate, …)
/// waiting on a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSupport {
    /// The provider has no concept of a decision blocking progress.
    None,
    /// The reconciler's regular poll cadence is the only way to discover a
    /// pending decision (docket's `GET /approvals` today).
    Poll,
    /// The provider can notify Tack the moment a decision opens, without
    /// waiting for the next poll tick.
    Push,
}

/// Where a usage/cost figure downstream of this adapter actually comes from
/// — see the crate doc's "Money is always an estimate" note. This is the
/// field that turns that crate-wide caveat into something the UI can name
/// per plane instead of applying blanket distrust everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSupport {
    /// No token/cost figure exists for this provider (e.g. GitHub Actions
    /// reports runner minutes, not model usage — two
    /// meters are never one number).
    NotMeasured,
    /// The provider's own driver estimates its cost/token usage and reports
    /// it directly (docket today — its own driver, not a metering gateway).
    FromProvider,
    /// A separate LLM gateway in front of the provider meters usage.
    FromGateway,
}

/// Whether a caller-supplied model identifier actually reaches the work.
/// Model identifiers are opaque strings Tack never
/// parses or classifies — this only records what the *provider* does with
/// one once Tack hands it over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSelection {
    /// The provider owns its own routing and may silently ignore an
    /// externally supplied model.
    Unsupported,
    /// The provider accepts a model hint but isn't guaranteed to use it.
    Advisory,
    /// The provider passes the identifier straight through, unexamined.
    Honoured,
}

/// Pairs a non-boolean capability's level with a human-readable reason —
/// the whole point of this module. A bare `Support::Unsupported` tells a UI
/// *that* a control is off; `reason` is what lets it say *why* without a
/// provider-specific string hand-written into a component. The reason is
/// always adapter-authored data, produced by the
/// same code that decided the level — never a caller-side literal invented
/// after the fact.
///
/// **`Serialize` only, deliberately no `Deserialize`.** `reason: &'static
/// str` can only ever borrow from a `&'static` string literal an adapter's
/// own source wrote — there is no way to produce one from parsed input
/// without leaking memory, and nothing in this crate ever needs to decode a
/// `Capabilities` back in from JSON: it is always constructed by an
/// adapter and only ever crosses the wire outbound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rated<T> {
    pub level: T,
    pub reason: &'static str,
}

impl<T> Rated<T> {
    pub const fn new(level: T, reason: &'static str) -> Self {
        Self { level, reason }
    }
}

/// What one control plane can actually do — derived from the adapter's own
/// static configuration, never guessed from `kind` by a caller. Two ad-hoc
/// capability bits this struct retires:
/// `PendingApprovalListResponse.grant_available` and
/// `useAgentActivityMap`'s `orchAvailable()` used as a dispatch gate — both
/// really meant "orchestration is on," not "this provider can do this,"
/// which stops being a safe conflation the moment a second provider exists.
///
/// Every non-boolean field is a [`Rated`] pairing its level with a reason —
/// see that type's doc comment. The boolean fields don't carry one: they
/// answer a plain yes/no question a UI can act on directly (show/hide a
/// control), where the six [`Rated`] fields answer "yes, but…" questions a
/// UI needs to explain.
///
/// `Serialize` only — see [`Rated`]'s doc comment for why this type never
/// needs (and cannot cleanly support) `Deserialize`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Capabilities {
    /// Can this plane accept new work at all?
    pub dispatch: bool,
    /// Can an in-flight run be stopped?
    pub cancel: bool,
    pub pause: Rated<Support>,
    pub resume: Rated<Support>,
    pub event_scope: Rated<EventScope>,
    /// Can build/run artifacts be retrieved after the fact?
    pub artifacts: bool,
    pub decisions: Rated<DecisionSupport>,
    pub usage: Rated<UsageSupport>,
    pub model_selection: Rated<ModelSelection>,
    /// Does this plane expose a roster of available agent runtimes/models?
    pub runtimes: bool,
    /// Does this plane expose a plane-wide metrics scrape (docket's
    /// `/metrics`, read by [`ControlPlane::metrics`])? Plane-wide, not
    /// per-run or per-project — `GET /api/projects/{id}/orch-policy` is
    /// built entirely from it, including a server-computed denial rate, so
    /// a caller needs to know up front whether that figure can exist at all
    /// for this plane.
    pub plane_metrics: bool,
    /// Can this plane provision a fresh execution environment (docket's
    /// `POST /pods`) rather than only running against one that already
    /// exists?
    pub provisioning: bool,
}

// ---------------------------------------------------------------------------
// The ControlPlane trait
// ---------------------------------------------------------------------------

/// A control plane Tack can read fleet/run/approval/task state from and,
/// gated behind `TACK_ORCH_ENABLE`, dispatch work to. `docket` is
/// the only implementor today (`adapters::docket::DocketAdapter`); the
/// trait exists so a second backend never has to touch the reconciler,
/// handlers, or frontend that consume it.
///
/// **Not frozen.** These signatures changed once already when the original
/// freeze started forcing designs worse than the churn it was meant to
/// prevent (see [`traces`](Self::traces)'s return
/// type and [`OrchError::PolicyBlocked`] for the two concrete cases). Treat
/// the shape below as current, not eternal — change it again if the next
/// design genuinely needs to, and update every implementor/caller in the
/// same change.
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str; // "docket"

    /// What this adapter can actually do. **Synchronous and does no I/O** —
    /// derived from the adapter's own static configuration (the same values
    /// it was built with), never from a live request. This is what lets a
    /// caller render a capability-gated control (or compute it for an API
    /// response) without waiting on a network round trip, and means a
    /// plane that's currently `unreachable` still reports honest
    /// capabilities — "what this provider can do" and "is it up right now"
    /// are different questions. See [`Capabilities`]'s own doc comment for
    /// the discipline every field follows.
    fn capabilities(&self) -> Capabilities;

    async fn health(&self) -> Result<Health, OrchError>;
    async fn status(&self) -> Result<FleetStatus, OrchError>;
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError>;
    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError>;
    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError>;
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError>;
    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError>;
    /// `since` is the opaque cursor a previous call's [`TracesPage::next`]
    /// returned (`None` to start from the beginning). The returned
    /// [`TracesPage::next`] must be persisted and passed back verbatim next
    /// time — never parsed, decoded, or recomputed by the caller.
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError>;
    // Write side, gated behind TACK_ORCH_ENABLE.
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    /// Trigger a full pipeline dispatch for `project` — `POST
    /// /dispatch/{project}`. `vars` is sent as the request body verbatim (a
    /// plain `{name: value}` JSON object) and becomes the pipeline's
    /// resolved variable namespace on the plane's side; this trait has no
    /// opinion on its shape beyond "an object", since that's the plane's
    /// own pipeline definition to interpret.
    ///
    /// Success carries the plane's own run id as `Ok(String)` — a
    /// different kind of id from what [`ControlPlane::enqueue_task`]
    /// returns: a pipeline *run* ([`ControlPlane::get_run`]), not a pod
    /// *task* ([`ControlPlane::list_tasks`]).
    ///
    /// **The id can come back before the dispatched work actually runs.**
    /// docket's implementation creates the run record and responds
    /// immediately; the pipeline itself executes afterwards, off the
    /// request thread. A caller that needs to know whether the dispatch was
    /// *accepted for execution* rather than merely *recorded* has to poll
    /// [`ControlPlane::get_run`] or [`ControlPlane::traces`] with the
    /// returned id — this method's `Err` only ever means the plane refused
    /// to record the run at all (bad credentials, an unresolvable variable,
    /// the plane unreachable), never that the run subsequently failed.
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    /// Grant (`grant: true`) or deny (`grant: false`) a pending approval —
    /// `POST /approvals/{token}`. Success
    /// carries docket's own resulting [`ApprovalState`] (`Granted`/`Denied`;
    /// modeled as `Unknown` rather than assumed if docket's wording ever
    /// changes) — mirrors the real response body, `{"ok":true,"token":...,
    /// "state":...}`. The `channel` docket records
    /// alongside the decision in its hash-chained audit log is
    /// **not** a parameter here — every caller of this trait is Tack itself,
    /// so `adapters::docket`'s implementation sends the fixed value `"tack"`
    /// (verified against `approval.APPROVAL_CHANNELS`, which already lists
    /// it) rather than threading a value no caller would ever vary through
    /// every layer above this trait.
    ///
    /// An already-decided token is [`OrchError::AlreadyDecided`], not a
    /// panic-worthy failure — see that variant's doc comment. An unknown
    /// token (or an illegal decision on one, which docket reports the same
    /// way) is [`OrchError::NotFound`].
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<ApprovalState, OrchError>;

    /// Provision a fresh pod from a blueprint — `POST /pods`, verified live
    /// against `core/pod_provisioning.py`/`serve.py::_handle_post_pods`.
    /// See [`ProvisionPodParams`] for the request shape.
    ///
    /// **docket provisions atomically: either every member is created, or
    /// none are.** `core/pod_provisioning.py`'s module doc states the
    /// contract explicitly — `provision_members` tears down every member
    /// (and any pod-level port range / scratch dir) created during a
    /// *failing* call before raising, so by the time this method returns
    /// `Err`, docket itself has already rolled back whatever it started.
    /// The one exception is [`OrchError::AlreadyExists`] (HTTP 409): raised
    /// *before* anything is touched (`PodAlreadyExistsError` is checked
    /// first, before even the blueprint name is resolved), so it also
    /// leaves nothing new behind — it means a pod already existed under
    /// this name, not that this call partially created one. Either way, a
    /// caller of this method never needs to (and cannot, over HTTP — docket
    /// has no `DELETE`/teardown route) undo a *successful* call; only the
    /// caller's own side of a multi-step flow (e.g. a Tack project record
    /// created moments earlier) can still need rolling back.
    async fn provision_pod(&self, params: ProvisionPodParams) -> Result<ProvisionedPod, OrchError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

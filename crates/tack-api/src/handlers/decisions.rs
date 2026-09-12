//! Operator decision-resolution repository/service/handler
//! module. Registered in `handlers.rs` and merged into the operator router
//! in `router.rs`'s `operator_execution_routes` — **before** the
//! `require_token` layer is applied and **with** `inject_operator_principal`
//! layered directly on top, exactly like every other route that function
//! merges.
//!
//! Design notes: docs/dev-notes/tack-api/handlers/decisions.md

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tack_db::{Repository, repo::execution::ExecutionClock};
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};

/// `docs/contracts/runner-v1/limits.json`'s `decision_answer_bytes_max`,
/// mirrored exactly (pinned against the live fixture by
/// `decision_answer_limit_matches_frozen_fixture` below) — the same limit
/// `handlers/runner_protocol.rs`'s `create_decision` documents as bounding
/// "an operator's decision *answer*". That card's own comment names this
/// exact limit as belonging to decision resolution, not runner-side
/// creation, which this module implements.
const DECISION_ANSWER_BYTES_MAX: u64 = 32_768;

/// State for this module's local router, constructed
/// from the shared API state the same way `operator_execution_routes`
/// constructs `executions::OperatorExecutionState` today.
#[derive(Clone)]
pub struct DecisionOperatorState {
    pub repo: Repository,
    pub clock: Arc<dyn ExecutionClock>,
    /// `TACK_EXECUTION_DECISION_TOKEN`. `None` means
    /// "not configured on this server" — the fail-closed default, same
    /// posture as `AppState::config.orch_approval_token` before
    /// `handlers::orch::require_approval_token` ever compares anything. See
    /// [`require_decision_token`].
    pub decision_token: Option<String>,
}

impl DecisionOperatorState {
    /// `decision_token` defaults to `None` (fail-closed) — a caller must
    /// opt in via [`Self::with_decision_token`] to ever allow a resolve
    /// through.
    pub fn with_clock(repo: Repository, clock: Arc<dyn ExecutionClock>) -> Self {
        Self {
            repo,
            clock,
            decision_token: None,
        }
    }

    /// Builder, mirroring `runner_protocol::RunnerProtocolState::with_artifact_storage_root`'s
    /// established convention for an additive, post-construction config
    /// value.
    pub fn with_decision_token(mut self, token: Option<String>) -> Self {
        self.decision_token = token;
        self
    }
}

/// Static request-correlation id placeholder, matching
/// `executions::OPERATOR_REQUEST_ID`'s own convention — no per-request
/// correlation id is wired into these error envelopes yet.
const OPERATOR_REQUEST_ID: &str = "req_operator_decisions";

/// Builds the stable v1 error envelope via `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` so it can never drift from
/// `docs/contracts/runner-v1/errors/*.json`. `details` follows the per-code
/// shape documented in `docs/contracts/runner-v1/README.md`.
fn error(
    status: StatusCode,
    code: StableErrorCode,
    message: &str,
    details: Value,
) -> (StatusCode, Json<Value>) {
    let envelope = ProtocolErrorEnvelope::new(code, message, OPERATOR_REQUEST_ID, details);
    (
        status,
        Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
}

fn internal_error() -> (StatusCode, Json<Value>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        StableErrorCode::InternalError,
        "Could not resolve decision",
        json!({}),
    )
}

/// Header carrying the operator's `TACK_EXECUTION_DECISION_TOKEN` on a
/// decision-resolution request. Deliberately not
/// `Authorization` (already spoken for by the ordinary `TACK_API_TOKEN`
/// Bearer gate — this is a second, independent credential, not a
/// replacement for it) and deliberately a header, not a request-body field,
/// mirroring `handlers::orch::APPROVAL_TOKEN_HEADER` exactly — so it never
/// ends up echoed into a JSON log line the way a body field might.
pub const DECISION_TOKEN_HEADER: &str = "x-tack-decision-token";

/// Byte-wise constant-time equality, duplicated verbatim from
/// `crate::middleware::constant_time_eq` rather than imported — this module
/// is loaded standalone via `#[path]` inside
/// `crates/tack-api/tests/runner_protocol/decisions.rs` (a module tree with
/// no `middleware` module of its own; see this
/// file's own module doc comment on why it stays deliberately decoupled from
/// other files), so a `crate::middleware::...` path would fail to resolve
/// there even though it resolves fine once this module is wired into the
/// real crate. Same reasoning as this file's existing
/// `canonical_json`/`canonical_string` duplication of `executions.rs`'s
/// identical pair.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    std::hint::black_box(
        a.iter()
            .zip(b.iter())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0,
    )
}

/// Resolving a decision releases whatever the harness/runner is blocked on
/// — a materially higher-privilege action than the ordinary operator
/// `x-tack-principal` gate already covers (which only proves "this caller
/// cleared `require_token`"), exactly the same argument
/// `handlers::orch::require_approval_token`'s doc comment makes for granting
/// a docket approval. This function mirrors that one's implementation and
/// rationale exactly, including the safe-default direction:
///
/// **The safe default when `TACK_EXECUTION_DECISION_TOKEN` is unset: always
/// reject.** There is deliberately no "no secret configured, so skip the
/// check" branch the way `middleware::require_token`'s ordinary Bearer gate
/// has for an unset `TACK_API_TOKEN` ("pure-local mode, allow everything").
/// An unconfigured `TACK_EXECUTION_DECISION_TOKEN` must mean "nothing on
/// this server is configured to resolve a decision" — never "anyone holding
/// the ordinary API token can."
///
/// The error details carry `required_scope: "operator:decisions"`, matching
/// `docs/contracts/runner-v1/errors/forbidden.json`'s frozen example
/// byte-for-byte in shape — this is the real, separately-scoped credential
/// that fixture's wording calls for (see the module doc comment's
/// `TACK_EXECUTION_DECISION_TOKEN` section).
fn require_decision_token(
    state: &DecisionOperatorState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(expected) = &state.decision_token else {
        return Err(error(
            StatusCode::FORBIDDEN,
            StableErrorCode::Forbidden,
            "resolving a decision requires TACK_EXECUTION_DECISION_TOKEN to be configured on this server",
            json!({"required_scope": "operator:decisions"}),
        ));
    };
    let provided = headers
        .get(DECISION_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    match provided {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => Ok(()),
        _ => Err(error(
            StatusCode::FORBIDDEN,
            StableErrorCode::Forbidden,
            &format!("missing or invalid {DECISION_TOKEN_HEADER} header"),
            json!({"required_scope": "operator:decisions"}),
        )),
    }
}

/// Reads the operator principal `inject_operator_principal` (`middleware.rs`)
/// sets on every request once this router is mounted behind `require_token`.
/// Deliberately reads *only* this header — never `Authorization` — which is
/// what makes a runner bearer credential structurally powerless here (see
/// this file's module doc comment).
fn principal(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "An authenticated operator principal is required",
                json!({}),
            )
        })
}

/// Recursively rebuilds a `Value`, forcing every object through
/// `FromIterator` so key order can never affect the serialized comparison
/// used for idempotent-replay detection — mirrors
/// `handlers/executions.rs`'s identical `canonical_json`/`canonical_string`
/// pair (duplicated here rather than imported, to avoid coupling this module
/// to that file).
fn canonical_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect(),
        ),
        other => other,
    }
}

fn canonical_string(value: &Value) -> String {
    serde_json::to_string(&canonical_json(value.clone())).unwrap_or_default()
}

pub fn routes(state: DecisionOperatorState) -> Router {
    Router::new()
        .route(
            "/attempts/{attempt_id}/decisions/{decision_id}/resolve",
            post(resolve_decision),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------
// Repository layer: `BEGIN IMMEDIATE` per CLAUDE.md's read-then-write rule
// — this is a read-then-write site outside `repo/execution.rs`/`repo.rs`.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// No `execution_decisions` row exists for this exact
    /// `(attempt_id, decision_id)` pair — covers both "never existed" and
    /// "exists, but under a different attempt" (cross-attempt access). Both
    /// write nothing; the caller cannot distinguish them, by design (an
    /// attacker guessing another attempt's `decision_id` learns nothing).
    NotFound,
    /// The submitted `answer.option_id` is non-empty but not one of this
    /// decision's own recorded `options` (only checked when `options` is
    /// non-empty — a freeform decision has none to check against).
    InvalidOption,
    /// Terminal, fail-closed: this decision is expired (either already
    /// recorded as such, or just transitioned to `expired` by this very
    /// call) and can never be resolved with an operator's answer, no matter
    /// what that answer is.
    Expired {
        resolved_at: String,
        resolved_by: Value,
    },
    /// Already resolved with a *different* answer than the one just
    /// submitted — a genuine conflict, not a replay.
    IdempotencyConflict { stored_answer: Value },
    /// A fresh resolution, or a byte-identical idempotent replay of one
    /// (`replayed` distinguishes the two; both return the same fields).
    Resolved {
        resolved_at: String,
        resolved_by: Value,
        answer: Value,
        replayed: bool,
    },
    /// Defensive: a `state` value neither this module nor
    /// `create_execution_decision`/`create_decision` ever writes. Never
    /// silently treated as any of the above.
    UnknownState(String),
}

/// Resolves (or fail-closed-expires) exactly one decision, scoped to the
/// exact `(attempt_id, decision_id)` pair — the only thing that makes
/// cross-attempt access denial possible. One `BEGIN IMMEDIATE` transaction:
/// the SELECT and the conditional UPDATE below never run as two separate
/// un-transacted statements, which is what would let a concurrent resolve
/// (or a concurrent expiry sweep, see [`expire_overdue_decisions`]) land a
/// second, conflicting write between this function's own read and write.
pub async fn resolve_decision_row(
    pool: &SqlitePool,
    attempt_id: &str,
    decision_id: &str,
    submitted_answer: &Value,
    resolved_by: &Value,
    now: DateTime<Utc>,
) -> Result<ResolveOutcome, sqlx::Error> {
    let now_s = now.to_rfc3339();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query(
        "SELECT id, state, options, answer, expires_at, resolved_at, resolved_by \
         FROM execution_decisions WHERE attempt_id = ? AND decision_id = ?",
    )
    .bind(attempt_id)
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(ResolveOutcome::NotFound);
    };
    let row_id: String = row.get("id");
    let state: String = row.get("state");

    match state.as_str() {
        "expired" => {
            let resolved_at: Option<String> = row.get("resolved_at");
            let resolved_by_raw: Option<String> = row.get("resolved_by");
            tx.commit().await?;
            Ok(ResolveOutcome::Expired {
                resolved_at: resolved_at.unwrap_or_default(),
                resolved_by: resolved_by_raw
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .unwrap_or(Value::Null),
            })
        }
        "resolved" => {
            let stored_answer_raw: Option<String> = row.get("answer");
            let stored_answer: Value = stored_answer_raw
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or(Value::Null);
            let stored_resolved_at: String = row
                .get::<Option<String>, _>("resolved_at")
                .unwrap_or_default();
            let stored_resolved_by_raw: Option<String> = row.get("resolved_by");
            let stored_resolved_by: Value = stored_resolved_by_raw
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or(Value::Null);
            tx.commit().await?;
            if canonical_string(&stored_answer) == canonical_string(submitted_answer) {
                Ok(ResolveOutcome::Resolved {
                    resolved_at: stored_resolved_at,
                    resolved_by: stored_resolved_by,
                    answer: stored_answer,
                    replayed: true,
                })
            } else {
                Ok(ResolveOutcome::IdempotencyConflict { stored_answer })
            }
        }
        "pending" => {
            let expires_at: Option<String> = row.get("expires_at");
            let is_overdue = expires_at
                .as_deref()
                .is_some_and(|value| value <= now_s.as_str());
            if is_overdue {
                let system_resolved_by = json!({"kind": "system", "subject_id": "expiry"});
                let resolved_by_json = serde_json::to_string(&system_resolved_by).unwrap();
                sqlx::query(
                    "UPDATE execution_decisions SET state='expired', answer=NULL, resolved_at=?, resolved_by=?, updated_at=? \
                     WHERE id=? AND state='pending'",
                )
                .bind(&now_s)
                .bind(&resolved_by_json)
                .bind(&now_s)
                .bind(&row_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return Ok(ResolveOutcome::Expired {
                    resolved_at: now_s,
                    resolved_by: system_resolved_by,
                });
            }

            let options_json: String = row.get("options");
            let options: Vec<Value> = serde_json::from_str(&options_json).unwrap_or_default();
            let option_id = submitted_answer
                .get("option_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let option_known = options
                .iter()
                .any(|opt| opt.get("option_id").and_then(Value::as_str) == Some(option_id));
            if !options.is_empty() && !option_known {
                tx.commit().await?;
                return Ok(ResolveOutcome::InvalidOption);
            }

            let answer_json = serde_json::to_string(submitted_answer).unwrap();
            let resolved_by_json = serde_json::to_string(resolved_by).unwrap();
            sqlx::query(
                "UPDATE execution_decisions SET state='resolved', answer=?, resolved_at=?, resolved_by=?, updated_at=? \
                 WHERE id=? AND state='pending'",
            )
            .bind(&answer_json)
            .bind(&now_s)
            .bind(&resolved_by_json)
            .bind(&now_s)
            .bind(&row_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(ResolveOutcome::Resolved {
                resolved_at: now_s,
                resolved_by: resolved_by.clone(),
                answer: submitted_answer.clone(),
                replayed: false,
            })
        }
        other => {
            tx.commit().await?;
            Ok(ResolveOutcome::UnknownState(other.to_string()))
        }
    }
}

/// Fail-closed bulk expiry sweep: transitions every still-`pending` decision
/// whose `expires_at` has passed into `state='expired'` with a `system`
/// `resolved_by` — the same terminal shape [`resolve_decision_row`] writes
/// lazily for a single row. Not called by this module's own handler,
/// which only ever touches one row at a time — wired instead as a periodic
/// caller in `execution_runtime.rs`.
pub async fn expire_overdue_decisions(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let now_s = now.to_rfc3339();
    let system_resolved_by = json!({"kind": "system", "subject_id": "expiry"});
    let resolved_by_json = serde_json::to_string(&system_resolved_by).unwrap();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let result = sqlx::query(
        "UPDATE execution_decisions SET state='expired', answer=NULL, resolved_at=?, resolved_by=?, updated_at=? \
         WHERE state='pending' AND expires_at IS NOT NULL AND expires_at <= ?",
    )
    .bind(&now_s)
    .bind(&resolved_by_json)
    .bind(&now_s)
    .bind(&now_s)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------
// HTTP handler.
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ResolveDecisionResponse {
    pub protocol_version: u32,
    pub decision_id: String,
    /// Always `"resolved"` on a 200 — an expired/not-found/conflicting
    /// decision is a distinct error response, never a 200 with a different
    /// state string, so a caller need not switch on this field.
    pub state: String,
    pub answer: Value,
    pub resolved_at: String,
    pub resolved_by: Value,
    /// `true` when this response is a byte-identical idempotent replay of an
    /// already-committed resolution rather than a fresh write.
    pub replayed: bool,
}

fn validate_answer(value: &Value) -> Result<Value, (StatusCode, Json<Value>)> {
    let answer = value.get("answer").cloned().ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer is required",
            json!({"field": "answer"}),
        )
    })?;
    if !answer.is_object() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer must be an object",
            json!({"field": "answer"}),
        ));
    }
    let option_id_ok = answer
        .get("option_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    if !option_id_ok {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer.option_id is required and must be a non-empty string",
            json!({"field": "answer.option_id"}),
        ));
    }
    if let Some(text) = answer.get("text")
        && !text.is_null()
        && !text.is_string()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "answer.text must be a string or null",
            json!({"field": "answer.text"}),
        ));
    }
    let answer_bytes = serde_json::to_vec(&answer).unwrap_or_default().len() as u64;
    if answer_bytes > DECISION_ANSWER_BYTES_MAX {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            StableErrorCode::PayloadTooLarge,
            "The answer exceeds a protocol limit",
            json!({"limit": "decision_answer_bytes_max", "maximum": DECISION_ANSWER_BYTES_MAX}),
        ));
    }
    Ok(answer)
}

/// `POST /attempts/{attempt_id}/decisions/{decision_id}/resolve` —
/// operator-only (see this file's module doc comment). Never reachable by a
/// runner bearer credential; never resolves an item's status directly.
pub async fn resolve_decision(
    State(state): State<DecisionOperatorState>,
    headers: HeaderMap,
    Path((attempt_id, decision_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<ResolveDecisionResponse>, (StatusCode, Json<Value>)> {
    require_decision_token(&state, &headers)?;
    let principal_id = principal(&headers)?;

    let value: Value = serde_json::from_slice(&body).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "Request body must be valid JSON",
            json!({"field": "body"}),
        )
    })?;
    let answer = validate_answer(&value)?;

    let now = state.clock.now();
    let resolved_by = json!({"kind": "operator", "subject_id": principal_id});
    let outcome = resolve_decision_row(
        state.repo.pool(),
        &attempt_id,
        &decision_id,
        &answer,
        &resolved_by,
        now,
    )
    .await
    .map_err(|err| {
        tracing::error!(
            attempt_id = %attempt_id,
            decision_id = %decision_id,
            error = %err,
            "decision resolve query failed"
        );
        internal_error()
    })?;

    match outcome {
        ResolveOutcome::NotFound => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "not_found",
                "decision resolve rejected"
            );
            Err(error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "The requested resource does not exist",
                json!({"resource": "decision"}),
            ))
        }
        ResolveOutcome::InvalidOption => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "invalid_option",
                "decision resolve rejected"
            );
            Err(error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "answer.option_id is not one of this decision's options",
                json!({"field": "answer.option_id"}),
            ))
        }
        ResolveOutcome::Expired { .. } => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "expired",
                "decision resolve rejected: fail-closed expiry"
            );
            Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::DecisionExpired,
                "The decision expired without a valid answer",
                json!({"decision_id": decision_id}),
            ))
        }
        ResolveOutcome::IdempotencyConflict { .. } => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "idempotency_conflict",
                "decision resolve rejected: conflicting replay"
            );
            Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::IdempotencyConflict,
                "The decision was already resolved with a different answer",
                json!({"decision_id": decision_id}),
            ))
        }
        ResolveOutcome::Resolved {
            resolved_at,
            resolved_by,
            answer,
            replayed,
        } => {
            tracing::info!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                outcome = "resolved",
                replayed,
                "decision resolved"
            );
            Ok(Json(ResolveDecisionResponse {
                protocol_version: 1,
                decision_id,
                state: "resolved".to_string(),
                answer,
                resolved_at,
                resolved_by,
                replayed,
            }))
        }
        ResolveOutcome::UnknownState(raw_state) => {
            tracing::error!(
                attempt_id = %attempt_id,
                decision_id = %decision_id,
                state = %raw_state,
                "decision resolve: unrecognized stored state"
            );
            Err(internal_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_answer_limit_matches_frozen_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../docs/contracts/runner-v1/limits.json"
        ))
        .expect("fixture parses");
        assert_eq!(
            fixture["decision_answer_bytes_max"].as_u64().unwrap(),
            DECISION_ANSWER_BYTES_MAX
        );
    }

    #[test]
    fn canonical_string_is_stable_across_key_order() {
        let a = json!({"option_id": "allow_once", "text": null});
        let b = json!({"text": null, "option_id": "allow_once"});
        assert_eq!(canonical_string(&a), canonical_string(&b));
        let c = json!({"option_id": "deny", "text": null});
        assert_ne!(canonical_string(&a), canonical_string(&c));
    }

    #[test]
    fn principal_requires_a_non_empty_header() {
        let mut headers = HeaderMap::new();
        let (status, body) = principal(&headers).unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body.0["error"]["code"], "unauthorized");

        headers.insert("x-tack-principal", "".parse().unwrap());
        let (status, _) = principal(&headers).unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        headers.insert("x-tack-principal", "operator:local".parse().unwrap());
        assert_eq!(principal(&headers).unwrap(), "operator:local");
    }

    #[test]
    fn validate_answer_rejects_missing_and_empty_option_id_and_bad_text() {
        assert!(validate_answer(&json!({})).is_err());
        assert!(validate_answer(&json!({"answer": "not-an-object"})).is_err());
        assert!(validate_answer(&json!({"answer": {"option_id": ""}})).is_err());
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": 5}})).is_err()
        );
        assert!(validate_answer(&json!({"answer": {"option_id": "allow_once"}})).is_ok());
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": null}})).is_ok()
        );
        assert!(
            validate_answer(&json!({"answer": {"option_id": "allow_once", "text": "ok"}})).is_ok()
        );
    }
}

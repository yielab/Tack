// Wire-format boundary for scoped decision resolution:
// `POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`, mounted in
// production at `crates/tack-api/src/handlers/decisions.rs`. Every shape
// below is copied field-for-field from that handler's
// `ResolveDecisionResponse` and its `validate_answer`'s accepted
// request-body shape (`{"answer": {"option_id", "text"?}}`).
//
// `decisionsApi.list` calls the discovery route — `GET
// /executions/{request_id}/attempts/{attempt_number}/decisions`
// (`crates/tack-api/src/handlers/attempt_lists.rs`), returning every
// `execution_decisions` row for that attempt, `pending`/`resolved`/`expired`
// alike, oldest first. It carries the ordinary operator gate only — never
// `TACK_EXECUTION_DECISION_TOKEN`, which stays scoped to resolution. This
// file's `DecisionRecord` type matches that response's `data` rows
// field-for-field.
//
// `decisionsApi.resolve` is the actual, mounted, tested resolve mutation —
// which is why a caller can still resolve a decision by id even without
// having listed it first.

import { apiOrigin, ApiError, request } from '../api/client';

// ─── Decision record shape (forward-declared; see header) ─────────────────

export interface DecisionOption {
  option_id: string;
  label: string;
}

/** `execution_decisions.state` — `pending`/`resolved`/`expired` are the only
 *  values anything in this codebase ever writes (`decisions.rs`'s own
 *  `UnknownState` defensive arm proves this exhaustively server-side), but
 *  this stays an open string, matching every other server-reported
 *  lifecycle value in this domain (`ExecutionSummary.state`'s own doc
 *  comment): an unrecognised value renders instead of failing to parse. */
export type DecisionState = 'pending' | 'resolved' | 'expired' | (string & {});

export interface DecisionAnswer {
  option_id: string;
  text?: string | null;
}

export interface DecisionResolvedBy {
  kind: string;
  subject_id: string;
}

/** One `execution_decisions` row, as returned by `decisionsApi.list`. */
export interface DecisionRecord {
  decision_id: string;
  attempt_id: string;
  kind: string;
  prompt: string;
  options: DecisionOption[];
  metadata: unknown;
  expires_at: string | null;
  state: DecisionState;
  answer: DecisionAnswer | null;
  resolved_at: string | null;
  resolved_by: DecisionResolvedBy | null;
  created_at: string;
  updated_at: string;
}

// ─── Resolve (`POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`) ─

export interface ResolveDecisionResult {
  protocol_version: number;
  decision_id: string;
  /** Always `"resolved"` on a 200 — an expired/not-found/conflicting
   *  decision is a distinct thrown `ApiError`, never a 200 with a different
   *  state string (see `decisions.rs`'s own `ResolveDecisionResponse` doc
   *  comment). */
  state: string;
  answer: DecisionAnswer;
  resolved_at: string;
  resolved_by: DecisionResolvedBy;
  /** `true` when this response is a byte-identical idempotent replay of an
   *  already-committed resolution rather than a fresh write. */
  replayed: boolean;
}

/** Header carrying `TACK_EXECUTION_DECISION_TOKEN` — mirrors
 *  `decisions.rs`'s own `DECISION_TOKEN_HEADER` constant byte-for-byte. */
const DECISION_TOKEN_HEADER = 'x-tack-decision-token';
const DECISION_TOKEN_STORAGE_KEY = 'tack_execution_decision_token';

/**
 * The operator's own copy of `TACK_EXECUTION_DECISION_TOKEN` — a **second**,
 * higher-privilege secret the server holds, structurally identical to
 * `features/approvals/api.ts`'s `approvalTokenStore` for
 * `TACK_ORCH_APPROVAL_TOKEN` (resolving a decision "releases whatever the
 * harness/runner is blocked on" — `decisions.rs`'s own doc comment makes the
 * identical argument granting an approval does). Session-only, scoped to the
 * configured API origin, never sent automatically — only
 * `decisionsApi.resolve` ever reads it, and only on an actual resolve call,
 * never implicitly.
 */
export const decisionTokenStore = {
  get(): string | null {
    try {
      return sessionStorage.getItem(`${DECISION_TOKEN_STORAGE_KEY}:${apiOrigin()}`);
    } catch {
      return null;
    }
  },
  set(token: string | null): void {
    try {
      const key = `${DECISION_TOKEN_STORAGE_KEY}:${apiOrigin()}`;
      if (token) sessionStorage.setItem(key, token);
      else sessionStorage.removeItem(key);
    } catch {
      /* ignore — sessionStorage may be unavailable */
    }
  },
};

/**
 * True when a resolve was rejected because `x-tack-decision-token` was
 * missing/wrong, or `TACK_EXECUTION_DECISION_TOKEN` isn't configured on the
 * server at all — `decisions.rs`'s `require_decision_token` is fail-closed
 * (rejects even when unset, never "no secret configured, allow anyway"), so
 * this is the honest, expected shape of "decisions cannot be resolved on
 * this deployment" your card's brief names — never swallowed as a generic
 * error. Mirrors `features/approvals/api.ts#isApprovalTokenRejected` exactly.
 */
export function isDecisionTokenRejected(err: unknown): boolean {
  return err instanceof ApiError && err.status === 403;
}

/** The decision expired before this resolve reached it (fail-closed
 *  expiry — `decisions.rs`'s own `ResolveOutcome::Expired`), or the
 *  attempt/decision id pair does not exist. Distinguished from
 *  {@link isDecisionIdempotencyConflict} — both are HTTP 409, so `code` is
 *  what actually distinguishes them (never bare `status`, matching
 *  `isOrchestrationDisabledError`'s own precedent for two failures sharing
 *  a status code). */
export function isDecisionExpired(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409 && err.code === 'decision_expired';
}

/** Already resolved with a genuinely different answer than the one just
 *  submitted — a real conflict, not a replay (a byte-identical resubmission
 *  instead returns 200 with `replayed: true`, never an error). */
export function isDecisionIdempotencyConflict(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409 && err.code === 'idempotency_conflict';
}

/** No decision exists under this exact `(attempt_id, decision_id)` pair —
 *  covers both "never existed" and "exists under a different attempt"
 *  (cross-attempt access is deliberately indistinguishable from
 *  not-existing server-side; see `decisions.rs`'s `ResolveOutcome::NotFound`
 *  doc comment). */
export function isDecisionNotFound(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

/** The submitted `answer.option_id` is not one of the decision's own
 *  declared `options` (only checked when `options` is non-empty). */
export function isDecisionInvalidOption(err: unknown): boolean {
  return err instanceof ApiError && err.status === 400;
}

interface DecisionListResponse {
  protocol_version: number;
  data: DecisionRecord[];
}

export const decisionsApi = {
  /** `GET /executions/{request_id}/attempts/{attempt_number}/decisions` —
   *  every decision raised against this attempt, oldest first (may be
   *  empty). Throws `ApiError` with status 404 if the request or attempt
   *  does not exist. */
  list: async (requestId: string, attemptNumber: number): Promise<DecisionRecord[]> => {
    const res = await request<DecisionListResponse>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(
        String(attemptNumber),
      )}/decisions`,
    );
    return res.data;
  },
  resolve: (attemptId: string, decisionId: string, answer: DecisionAnswer) => {
    const headers = new Headers();
    const token = decisionTokenStore.get();
    if (token) headers.set(DECISION_TOKEN_HEADER, token);
    return request<ResolveDecisionResult>(
      `/attempts/${encodeURIComponent(attemptId)}/decisions/${encodeURIComponent(decisionId)}/resolve`,
      { method: 'POST', headers, body: JSON.stringify({ answer }) },
    );
  },
};

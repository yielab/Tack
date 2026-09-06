// Wire-format boundary for the fleet-wide approvals inbox. Every
// assumption about `GET /api/approvals` /
// `POST /api/approvals/{token}`'s request/response shapes lives in this one
// file — `ApprovalsPage.tsx` and `format.ts` only ever import types and
// functions from here, never construct a request body or read a raw field
// name themselves, the same pattern `features/fleet/api.ts` and
// `shared/dispatch/api.ts` follow.
//
// Both routes and every field below are copied field-for-field from the real
// Rust handler (`crates/tack-api/src/handlers/orch.rs`'s
// `PendingApprovalResponse`/`PendingApprovalListResponse`/
// `DecideApprovalRequest`/`DecideApprovalResponse`) and `docs/openapi.json`
// — not a guess.

import { apiOrigin, request, ApiError, isOrchestrationDisabledError } from '../../shared/api/client';

/** One row of the fleet-wide approvals inbox, oldest-requested first.
 *  `item_id`/`item_title`/`item_status`/`project_id`/`project_name` are all
 *  `null` together for an **uncorrelated** approval — docket raised it but
 *  Tack couldn't attribute it to an item (a CLI-dispatched run, for
 *  instance). This is deliberately never filtered out: an approval Tack
 *  can't attribute is the one most likely to be silently blocking a fleet
 *  (the per-project Fleet view excludes these; this inbox is where they're
 *  meant to surface). */
export interface PendingApproval {
  token: string;
  control_plane_id: string;
  control_plane_name: string;
  item_id: string | null;
  item_title: string | null;
  item_status: string | null;
  project_id: string | null;
  project_name: string | null;
  remote_task_id: string | null;
  /** docket's `role` field — the closest thing to "which agent" docket's
   *  wire shape has (see `crates/tack-orch/src/reconciler.rs`'s ingestion
   *  comment). May be `null`. */
  agent: string | null;
  /** The gated action's description, already redacted by docket. */
  action: string | null;
  requested_at: string;
}

/**
 * `GET /api/approvals` response envelope. The backend still sends a
 * grant-availability boolean alongside `rows` (whether
 * `TACK_ORCH_APPROVAL_TOKEN` is configured) — deliberately not declared or
 * read here anymore. It was never a *provider* capability — `Capabilities`
 * describes what a control plane can do, not whether this Tack server holds
 * a decision-granting secret — so there is no real field in
 * `shared/orch/capabilities.ts` for it to become. Pre-emptively hiding
 * Grant/Deny based on a client-side guess also bought nothing:
 * `handlers/orch.rs`'s own doc comment on the field says the server
 * "enforces the real check independently on every decide call regardless of
 * what this flag says." So the UI now always renders the controls and lets
 * a real decide attempt fail with the server's actual 403 (see
 * {@link isApprovalTokenRejected} and `ApprovalsPage.tsx`'s
 * `confirmDecision`) — a real server answer, not a client-side prediction of
 * one, and one fewer thing this file has to keep in sync with the backend.
 */
export interface PendingApprovalListResponse {
  rows: PendingApproval[];
}

export type ApprovalDecisionActionValue = 'grant' | 'deny';

export interface DecideApprovalResponse {
  token: string;
  /** docket's own resulting state (`"granted"`/`"denied"`, or an
   *  unrecognised value shown as-is — same "never fail on an unknown
   *  remote value" discipline as everywhere else in this system). */
  state: string;
}

const APPROVAL_TOKEN_HEADER = 'X-Tack-Approval-Token';
const APPROVAL_TOKEN_STORAGE_KEY = 'tack_orch_approval_token';

/**
 * The operator's own copy of `TACK_ORCH_APPROVAL_TOKEN` — a **second**,
 * higher-privilege secret the server holds, deliberately separate from the
 * ordinary `TACK_API_TOKEN` `tokenStore` (`shared/api/client.ts`). It is
 * session-only and scoped to the configured API origin; it is never sent
 * automatically the way the Bearer
 * token is — `approvalsApi.decide` is the only thing that ever reads it,
 * and only when actually deciding an approval, never on a plain `list()`.
 * Nothing about this value is ever logged or echoed back by the server —
 * see {@link PendingApprovalListResponse}'s doc comment for why there is no
 * longer a pre-emptive "is granting even possible" signal from the list
 * call at all; the real answer only ever comes from an actual decide
 * attempt.
 */
export const approvalTokenStore = {
  get(): string | null {
    try {
      return sessionStorage.getItem(`${APPROVAL_TOKEN_STORAGE_KEY}:${apiOrigin()}`);
    } catch {
      return null;
    }
  },
  set(token: string | null): void {
    try {
      const key = `${APPROVAL_TOKEN_STORAGE_KEY}:${apiOrigin()}`;
      // Do not migrate the legacy persistent secret into the session.
      localStorage.removeItem(APPROVAL_TOKEN_STORAGE_KEY);
      if (token) sessionStorage.setItem(key, token);
      else sessionStorage.removeItem(key);
    } catch {
      /* ignore — sessionStorage may be unavailable */
    }
  },
};

/** True when the request failed because orchestration is disabled
 *  server-side — the default for every existing install. Delegates to
 *  `shared/api/client.ts#isOrchestrationDisabledError`, which distinguishes
 *  this from an ordinary 404 by a machine-readable
 *  `error.code`; kept as its own export so every existing caller
 *  (`ApprovalsPage.tsx`) keeps working unchanged. Note this only ever
 *  applies to `approvalsApi.list()`'s error — `approvalsApi.decide()`'s own
 *  403/409 (see {@link isApprovalTokenRejected} / {@link
 *  isApprovalAlreadyDecided} below) come from a different call site and
 *  never carry the `orchestration_disabled` code, so there's no ambiguity
 *  even though the raw status codes overlap. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}

/** True when a decision was rejected because `X-Tack-Approval-Token` was
 *  missing, wrong, or `TACK_ORCH_APPROVAL_TOKEN` isn't configured on the
 *  server at all — never a transport/server error. */
export function isApprovalTokenRejected(err: unknown): boolean {
  return err instanceof ApiError && err.status === 403;
}

/** True when docket reports the approval was already decided (granted,
 *  denied, or expired) by someone/something else before this request
 *  reached it — a normal race for a fleet-wide inbox, not a bug. The caller
 *  should drop the stale row, not show a scary error. */
export function isApprovalAlreadyDecided(err: unknown): boolean {
  return err instanceof ApiError && err.status === 409;
}

/** True when the token is unknown to Tack's own mirror, or docket itself no
 *  longer recognises it (e.g. an illegal decision on an already-resolved
 *  token) — same "drop the stale row" treatment as
 *  {@link isApprovalAlreadyDecided}. */
export function isApprovalGone(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

export const approvalsApi = {
  list: () => request<PendingApprovalListResponse>('/approvals'),
  decide: (token: string, action: ApprovalDecisionActionValue) => {
    const headers = new Headers();
    const approvalToken = approvalTokenStore.get();
    if (approvalToken) headers.set(APPROVAL_TOKEN_HEADER, approvalToken);
    return request<DecideApprovalResponse>(`/approvals/${encodeURIComponent(token)}`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ action }),
    });
  },
};

// Wire-format boundary for the embedded-runner control surface (ADR 0061
// decisions 2 and 6 — a UI-only user turns the in-process runner on/off and
// hands it a provider key, on a loopback bind, without a console). Every
// assumption about `GET`/`PUT /api/local-runner` and `GET`/`PUT`/`DELETE
// /api/local-runner/secrets(/{name})` lives in this one file — `
// ExecutionToggle.tsx` and `ProviderKeyPanel.tsx` only ever import types and
// functions from here, never construct a request body or read a raw wire
// field themselves. Mirrors the pattern `features/settings/
// orchestrationSettings/api.ts` set for the structurally identical
// orchestration on/off toggle.
//
// ── Why these routes can be a genuine 404 ───────────────────────────────
//
// Unlike `GET/PUT /api/settings/orchestration` (deliberately reachable even
// when orchestration is off, so an operator can discover the toggle),
// `/api/local-runner*` is absent — not present-and-refusing — on any
// non-loopback bind, or when the process serving the API never wired an
// embedded runner in at all (`crates/tack-api/src/router.rs`'s
// `local_runner_available` check, `build_router`). An embedded runner
// executes arbitrary agent processes on the host serving the UI, so it must
// not even be discoverable from outside the machine it runs on.
// `isLocalRunnerUnavailable` below is the one place that 404 is
// interpreted — both panels explain this is a remote-runner deployment
// instead of a bare error: `ExecutionToggle.tsx` points at the Advanced
// section's enrollment flow, `ProviderKeyPanel.tsx` at the console-only
// `tack runner secret set`, since a provider key genuinely is set on the
// runner's own machine regardless of embedded vs. remote.
//
// ── The `GET/PUT /api/local-runner` contract ────────────────────────────
//
//   GET  /api/local-runner  -> { enabled, state, since, catalog }
//     `enabled` is the persisted preference (survives a restart); `state`
//     ("running" | "stopped") is the live runtime, which can briefly lag
//     `enabled` right after boot. `catalog` is the provider-catalog
//     snapshot — see `CatalogSnapshot` below.
//   PUT  /api/local-runner  body: { enabled: boolean } -> 204
//     Persists the preference and starts/stops the runner to match, with no
//     restart.
//
// ── The `/api/local-runner/secrets` contract ────────────────────────────
//
//   GET    /api/local-runner/secrets            -> { data: SecretMeta[] }
//   PUT    /api/local-runner/secrets/{name}  body: { value: string } -> 204
//     Never echoes the value back, not even as a hash.
//   DELETE /api/local-runner/secrets/{name}      -> 204 (not an error if
//     already absent)
//
// This build has exactly one provider — Vercel AI Gateway — so every caller
// here hardcodes `VERCEL_AI_GATEWAY_SECRET_NAME` as `{name}` rather than
// exposing a picker for zero real choices.

import { ApiError, request } from '../../shared/api/client';

/** The one secret name this build's UI writes — `tack-runner`'s own
 *  default (`DEFAULT_VERCEL_AI_GATEWAY_SECRET`,
 *  `crates/tack-runner/src/config.rs`). Setting a value under this exact
 *  name is also what flips the Vercel provider on — see
 *  `EmbeddedRunnerControl::set_secret`'s own doc comment
 *  (`crates/tack-cli/src/local_runner.rs`). */
export const VERCEL_AI_GATEWAY_SECRET_NAME = 'vercel-ai-gateway/default';

export type LocalRunnerState = 'running' | 'stopped';

/** Mirrors `CatalogSnapshot` (`crates/tack-api/src/handlers/local_runner.rs`)
 *  field-for-field — the provider's own catalog, as the embedded runner's
 *  own probe last saw it, computed fresh on every `GET`. */
export type CatalogSnapshot =
  | { status: 'not_configured' }
  | { status: 'secret_unresolved' }
  | { status: 'unreachable'; http_status: number | null }
  | { status: 'configured'; model_count: number; checked_at: string };

export interface LocalRunnerStatus {
  enabled: boolean;
  state: LocalRunnerState;
  since: string | null;
  catalog: CatalogSnapshot;
}

export interface UpdateLocalRunnerBody {
  enabled: boolean;
}

export interface SecretMeta {
  name: string;
  /** `null` when the store holds this name but this process has no record
   *  of when it was set (e.g. `tack runner secret set` wrote it before this
   *  UI ever ran) — never a fabricated timestamp. */
  set_at: string | null;
}

export interface SecretListResult {
  data: SecretMeta[];
}

/** True when a request failed because this deployment has no reachable
 *  embedded-runner control surface — either the server isn't bound to
 *  loopback, or the process serving it never wired one in at all. Both
 *  render as a plain 404 with no error envelope (see this file's header) —
 *  the canonical check every caller of this API should use instead of
 *  matching on `err.status === 404` itself. */
export function isLocalRunnerUnavailable(err: unknown): boolean {
  return err instanceof ApiError && err.status === 404;
}

export const localRunnerApi = {
  get: () => request<LocalRunnerStatus>('/local-runner'),

  update: (enabled: boolean) =>
    request<void>('/local-runner', {
      method: 'PUT',
      body: JSON.stringify({ enabled } satisfies UpdateLocalRunnerBody),
    }),

  listSecrets: () => request<SecretListResult>('/local-runner/secrets'),

  setSecret: (name: string, value: string) =>
    request<void>(`/local-runner/secrets/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: JSON.stringify({ value }),
    }),

  removeSecret: (name: string) =>
    request<void>(`/local-runner/secrets/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),
};

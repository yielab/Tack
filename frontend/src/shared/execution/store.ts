// The shared item-execution store: optimistic cancellation with rollback
// and an explicit conflict/error state. This is the single reactive source
// of truth every consumer (the fleet/runner UI, the "Run with agent"
// surfaces) must read through, so every consumer sees one consistent state
// — no divergent copies of the same request/attempt. Components call
// `createExecutionStore()` once (typically via a context Provider the same
// way `shared/state/projectItemsContext.tsx` wraps `api.items.list`) and
// share the instance, rather than each maintaining its own fetch+signal.

import { createSignal } from 'solid-js';
import { ApiError } from '../api/client';
import {
  executionsApi,
  type CreateExecutionInput,
  type CreateExecutionResult,
  type ExecutionSummary,
  type RequeueExecutionInput,
} from './api';
import { attemptsApi, type AttemptSummary } from './attempts';
import { SequenceAllocator, VersionedCache } from './cache';
import type { ExecutionRealtime } from './realtime';

/** Every error this store surfaces is normalized to this shape — built from
 *  `ApiError` (status + optional stable `code`; see `types.ts`'s
 *  `StableErrorCode` header note on why `details`/`retryable` aren't
 *  available here) or, for a non-HTTP failure (e.g. a thrown non-`ApiError`
 *  in a test double), a bare message with `status: 0` and no `code`. */
export interface NormalizedExecutionError {
  status: number;
  code: string | undefined;
  message: string;
}

function normalizeError(err: unknown): NormalizedExecutionError {
  if (err instanceof ApiError) {
    return { status: err.status, code: err.code, message: err.message };
  }
  return { status: 0, code: undefined, message: err instanceof Error ? err.message : 'Unknown error' };
}

export type ListStatus = 'idle' | 'loading' | 'ready' | 'error';

/**
 * Mirrors `crates/tack-api/src/handlers/executions.rs`'s
 * `ListExecutionsQuery::MAX_LIMIT` — the server's hard cap on `?limit=`,
 * duplicated here because no generated contract carries this number (it is
 * a plain query parameter, not a schema field); the two can drift if the
 * server-side constant ever changes without this one following. The
 * app-wide preload (`loadList()` with no `itemId`) asks for this many rows
 * explicitly instead of leaving `limit` unset, so an item whose most recent
 * execution is older than the handler's *default* limit (200) still lands
 * in the shared cache as long as the install has fewer than this many
 * execution requests in total — raising the practical threshold at which
 * `RunWithAgentButton`'s badge can go stale from 200 to this number, though
 * not eliminating the class of bug: an install that ever exceeds this many
 * execution requests hits the same gap again, one order of magnitude later.
 */
export const EXECUTION_LIST_PRELOAD_LIMIT = 2000;

/**
 * Cancellation is modeled as its own small state machine layered on top of
 * `ExecutionSummary.cancellation_requested_at`, not merged into
 * `ExecutionState` — see `api.ts`'s header note on why the cancel
 * endpoint's own response `state` field is not trustworthy. `requested` is
 * the OR of "the server confirmed it" (via a fresh fetch's
 * `cancellation_requested_at`) and "we optimistically believe it": a
 * component that only cares "should I show a cancellation badge" reads
 * `requested`; one that needs to distinguish "still waiting on the server"
 * reads `pending`; one that needs to explain a failed cancel reads
 * `conflict`/`error`.
 */
export interface CancellationState {
  requested: boolean;
  pending: boolean;
  /** The last cancel attempt failed because the request had already
   *  reached a terminal state (`ApiError.code === 'conflict'`, matching
   *  `crates/tack-api/src/handlers/executions.rs`'s `request_cancellation`
   *  handler, which returns exactly this code for that case). An explicit,
   *  named outcome — never folded into `error` as a generic failure. */
  conflict: boolean;
  /** The last cancel attempt's non-conflict error, if any. */
  error: NormalizedExecutionError | undefined;
}

const EMPTY_CANCELLATION_STATE: CancellationState = {
  requested: false,
  pending: false,
  conflict: false,
  error: undefined,
};

/**
 * A request's full known state. `status: 'error'` with `summary: undefined`
 * means "we have never successfully fetched this request" (e.g. a bad id
 * passed to `loadOne`) — errors never render as empty data: a component
 * checks `status`, and an `'error'` record is never mistaken for "zero
 * data" or silently rendered as if it were a fresh, empty request.
 */
export interface ExecutionRequestRecord {
  status: 'ready' | 'error';
  summary: ExecutionSummary | undefined;
  error: NormalizedExecutionError | undefined;
  cancellation: CancellationState;
  fetchedAt: number;
}

/**
 * Attempts, read through `GET /executions/{id}/attempts` — see
 * `attempts.ts`'s header comment for how that route's shape is modeled.
 * Modeled the same way `ExecutionRequestRecord`/`ListStatus` are: an explicit state machine
 * so "never fetched", "fetching", "fetched, zero attempts exist", and
 * "fetch failed" are four genuinely different, never-conflated states — in
 * particular `ready` with an empty `data` array is NOT the same thing as
 * `idle` (never fetched), matching this store's own header-comment
 * discipline for `ExecutionRequestRecord`.
 */
export type AttemptAvailability =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; data: AttemptSummary[] }
  | { status: 'error'; error: NormalizedExecutionError };

const ATTEMPTS_IDLE: AttemptAvailability = { status: 'idle' };

export interface ExecutionStore {
  /** Every known request, keyed by `request_id`. Reactive — read inside a
   *  SolidJS tracking scope to re-render on any store mutation. */
  requests: () => ReadonlyMap<string, ExecutionRequestRecord>;
  /** Requests for one item, newest `created_at` first. */
  requestsForItem: (itemId: string) => ExecutionRequestRecord[];
  getRequest: (requestId: string) => ExecutionRequestRecord | undefined;
  listStatus: () => ListStatus;
  listError: () => NormalizedExecutionError | undefined;
  /** True once the app-wide preload (`loadList()` with no `itemId`) has
   *  come back with exactly `EXECUTION_LIST_PRELOAD_LIMIT` rows — meaning
   *  execution requests older than what this fetch covers may exist, so an
   *  on-screen item absent from the cache is genuinely unknown, not
   *  necessarily "never run." `false` whenever the last such fetch returned
   *  fewer rows than the cap (the table is provably covered in full) or no
   *  unscoped fetch has resolved yet. An item-scoped `loadList(itemId)`
   *  never sets this — that call is already exhaustive for its one item
   *  regardless of row count. */
  listMayBeIncomplete: () => boolean;
  /** Fetches into the shared cache. Omit `itemId` for the app-wide preload
   *  (bounded server-side by `list_executions`'s own default limit); pass
   *  one to fetch exactly that item's rows regardless of how many other
   *  requests the install has recorded — what a mounted `ExecutionTimeline`
   *  calls so its item's history is never silently truncated by the
   *  unscoped call's bound. Either call merges into the same
   *  `VersionedCache`, so `requestsForItem` sees the union of every fetch
   *  that has landed. */
  loadList: (itemId?: string) => Promise<void>;
  loadOne: (requestId: string) => Promise<void>;
  /** Creates the request, then immediately hydrates it into the store so a
   *  caller sees it appear without a second manual fetch or navigation.
   *  Resolves with the raw create result regardless of whether that
   *  hydration fetch succeeds. */
  create: (input: CreateExecutionInput) => Promise<CreateExecutionResult>;
  /** Optimistically marks the request as cancellation-pending, then
   *  confirms or rolls back against the real response. Rethrows on
   *  failure — the store's `cancellation` state is already updated by the
   *  time this rejects, so a caller may `.catch()` purely for its own
   *  side effects (e.g. a toast) without needing to re-derive anything. */
  cancel: (requestId: string) => Promise<void>;
  requeue: (requestId: string, input: RequeueExecutionInput) => Promise<void>;
  /** Reactive read of the last-known attempt list for one request — never
   *  triggers a fetch itself (mirrors `getRequest`'s pure-read contract).
   *  Returns `{status: 'idle'}` until a caller invokes {@link loadAttempts}
   *  at least once. */
  attemptsFor: (requestId: string) => AttemptAvailability;
  /** Fetches (or re-fetches) the attempt list for one request. Safe to call
   *  repeatedly — callers that only want a first, lazy load should check
   *  `attemptsFor(id).status === 'idle'` first (as `AttemptList.tsx` does);
   *  a caller that wants to force a refresh (e.g. after resolving a
   *  decision) may call this unconditionally. */
  loadAttempts: (requestId: string) => Promise<void>;
  /** Wires an `ExecutionRealtime` subscription (see `realtime.ts`) to this
   *  store's refetch paths. Returns an unsubscribe function; safe to call
   *  once per store/subscription pair. */
  connectRealtime: (realtime: ExecutionRealtime) => () => void;
}

/**
 * Every fetch that can write into `cache` (`loadOne`, every row of
 * `loadList`, `requeue`'s merge) shares this ONE sequence key rather than
 * one-per-request-id. A per-request-id counter cannot order a targeted
 * `loadOne('exec_1')` against a `loadList()` that also happens to return
 * `exec_1`, because `loadList()` doesn't know which request ids it will
 * receive until the response arrives — there is nothing to pre-allocate a
 * per-id counter against at issue time. A single shared counter, allocated
 * at each operation's *issue* time and applied to every row that operation
 * writes, orders every fetch against every other fetch regardless of which
 * endpoint or how many keys it touches — exactly what "a stale event can
 * never overwrite a newer snapshot" requires. `VersionedCache.set` still
 * compares versions per key, so this remains correct per key; the shared
 * counter only changes what "newer" is measured against.
 */
const GLOBAL_SEQUENCE_KEY = '*';

export function createExecutionStore(): ExecutionStore {
  const cache = new VersionedCache<ExecutionSummary>();
  const clock = new SequenceAllocator();
  const cancellations = new Map<string, CancellationState>();
  const fetchErrors = new Map<string, NormalizedExecutionError>();
  const inFlightCancel = new Set<string>();
  const attemptsCache = new Map<string, AttemptAvailability>();

  // A single bump signal drives reactivity for every accessor below. This
  // is coarser-grained than a per-key signal, but keeps exactly one
  // mutable source of truth (`cache`/`cancellations`/`fetchErrors`) instead
  // of duplicating state into a parallel SolidJS store — which is itself
  // part of "every consumer sees one consistent state": there is nowhere
  // for two copies to drift apart.
  const [bump, setBump] = createSignal(0);
  const touch = () => setBump((n) => n + 1);

  const [listStatus, setListStatus] = createSignal<ListStatus>('idle');
  const [listError, setListError] = createSignal<NormalizedExecutionError | undefined>(undefined);
  const [listMayBeIncomplete, setListMayBeIncomplete] = createSignal(false);

  function deriveCancellation(summary: ExecutionSummary | undefined, requestId: string): CancellationState {
    const local = cancellations.get(requestId) ?? EMPTY_CANCELLATION_STATE;
    const confirmed = summary?.cancellation_requested_at != null;
    return {
      requested: confirmed || local.requested,
      pending: !confirmed && local.pending,
      conflict: local.conflict,
      error: local.error,
    };
  }

  function recordFor(requestId: string): ExecutionRequestRecord | undefined {
    const summary = cache.get(requestId);
    const error = fetchErrors.get(requestId);
    if (!summary && !error) return undefined;
    return {
      status: summary ? 'ready' : 'error',
      summary,
      error,
      cancellation: deriveCancellation(summary, requestId),
      fetchedAt: Date.now(),
    };
  }

  /** Applies a fetched row through the version guard; returns whether it
   *  actually landed (false = dropped as a stale, out-of-order response).
   *  `version` must be allocated at the *issuing* operation's start (see
   *  `GLOBAL_SEQUENCE_KEY`'s doc comment) — never here, which would instead
   *  order writes by resolution time and defeat the whole guarantee. */
  function applyFetchedSummary(summary: ExecutionSummary, version: number): boolean {
    const applied = cache.set(summary.request_id, summary, version);
    if (applied) {
      fetchErrors.delete(summary.request_id);
      touch();
    }
    return applied;
  }

  function applyFetchError(requestId: string, err: unknown): void {
    // A fetch error never clears a previously known summary — only the
    // error map is set — so a real prior value is never downgraded to "no
    // data" just because a refresh attempt failed.
    fetchErrors.set(requestId, normalizeError(err));
    touch();
  }

  async function loadOne(requestId: string): Promise<void> {
    const version = clock.next(GLOBAL_SEQUENCE_KEY); // allocated now, before the network round-trip
    try {
      const { data } = await executionsApi.get(requestId);
      applyFetchedSummary(data, version);
    } catch (err) {
      applyFetchError(requestId, err);
      throw err;
    }
  }

  async function loadList(itemId?: string): Promise<void> {
    setListStatus('loading');
    const version = clock.next(GLOBAL_SEQUENCE_KEY); // one version for every row this call returns
    try {
      // The unscoped preload asks for `EXECUTION_LIST_PRELOAD_LIMIT`
      // explicitly rather than leaving `limit` unset (which would fall back
      // to the handler's much smaller default) — see that constant's own
      // doc comment. An item-scoped call is already exhaustive for its one
      // item regardless of row count, so it asks for nothing extra.
      const { data } = itemId
        ? await executionsApi.list(itemId)
        : await executionsApi.list(undefined, EXECUTION_LIST_PRELOAD_LIMIT);
      for (const row of data.data) applyFetchedSummary(row, version);
      setListStatus('ready');
      setListError(undefined);
      if (!itemId) setListMayBeIncomplete(data.data.length >= EXECUTION_LIST_PRELOAD_LIMIT);
    } catch (err) {
      setListStatus('error');
      setListError(normalizeError(err));
      throw err;
    }
  }

  async function create(input: CreateExecutionInput): Promise<CreateExecutionResult> {
    const result = await executionsApi.create(input);
    await loadOne(result.request_id).catch(() => {
      // The create itself succeeded server-side; a failed hydration fetch
      // is surfaced through the normal `fetchErrors` path on next read
      // rather than failing `create()`'s own promise.
    });
    return result;
  }

  async function cancel(requestId: string): Promise<void> {
    if (inFlightCancel.has(requestId)) return; // de-duplicate: one cancel in flight per key at a time
    inFlightCancel.add(requestId);
    cancellations.set(requestId, { requested: true, pending: true, conflict: false, error: undefined });
    touch();
    try {
      await executionsApi.cancel(requestId);
      // Do not touch `summary.state` here — see the module header note.
      // `pending` stays true until a fresh fetch confirms
      // `cancellation_requested_at`; `deriveCancellation` folds that in
      // automatically the next time `loadOne`/`loadList` resolves.
    } catch (err) {
      const normalized = normalizeError(err);
      const conflict = normalized.code === 'conflict';
      cancellations.set(requestId, {
        requested: false,
        pending: false,
        conflict,
        error: conflict ? undefined : normalized,
      });
      touch();
      throw err;
    } finally {
      inFlightCancel.delete(requestId);
    }
  }

  async function requeue(requestId: string, input: RequeueExecutionInput): Promise<void> {
    const version = clock.next(GLOBAL_SEQUENCE_KEY);
    const result = await executionsApi.requeue(requestId, input);
    // Unlike cancel's response, requeue's `state` genuinely is the fresh
    // authoritative value (III.1.1: `needs_operator -> queued` is a real,
    // operator-only transition and the handler only returns success after
    // committing it) — safe to merge directly.
    const current = cache.get(requestId);
    applyFetchedSummary(
      {
        request_id: result.request_id,
        item_id: current?.item_id ?? '',
        state: result.state,
        cancellation_requested_at: null,
        created_at: current?.created_at ?? new Date(0).toISOString(),
      },
      version,
    );
    cancellations.delete(requestId);
    touch();
  }

  function requests(): ReadonlyMap<string, ExecutionRequestRecord> {
    bump();
    const out = new Map<string, ExecutionRequestRecord>();
    for (const key of new Set([...cache.keys(), ...fetchErrors.keys()])) {
      const record = recordFor(key);
      if (record) out.set(key, record);
    }
    return out;
  }

  function getRequest(requestId: string): ExecutionRequestRecord | undefined {
    bump();
    return recordFor(requestId);
  }

  function requestsForItem(itemId: string): ExecutionRequestRecord[] {
    // Plain string comparison, not `localeCompare` — same determinism
    // reasoning as `capabilities.ts`'s model-id sort. ISO 8601 timestamps
    // compare correctly char-by-char, so this needs no date parsing either.
    return [...requests().values()]
      .filter((record) => record.summary?.item_id === itemId)
      .sort((a, b) => {
        const left = a.summary?.created_at ?? '';
        const right = b.summary?.created_at ?? '';
        if (left === right) return 0;
        return left < right ? 1 : -1; // newest first
      });
  }

  function attemptsFor(requestId: string): AttemptAvailability {
    bump();
    return attemptsCache.get(requestId) ?? ATTEMPTS_IDLE;
  }

  async function loadAttempts(requestId: string): Promise<void> {
    attemptsCache.set(requestId, { status: 'loading' });
    touch();
    try {
      const { data } = await attemptsApi.list(requestId);
      attemptsCache.set(requestId, { status: 'ready', data: data.data });
      touch();
    } catch (err) {
      attemptsCache.set(requestId, { status: 'error', error: normalizeError(err) });
      touch();
      throw err;
    }
  }

  function connectRealtime(realtime: ExecutionRealtime): () => void {
    return realtime.onInvalidate((event) => {
      if (event.scope === 'list') {
        void loadList();
        return;
      }
      void loadOne(event.requestId);
      // Only refresh attempts for a request this consumer already asked
      // about (`idle` means nobody has called `loadAttempts` for it yet) —
      // an unconditional refetch here would fetch attempt data for every
      // invalidated request even when nothing on screen reads it.
      if (attemptsCache.has(event.requestId)) void loadAttempts(event.requestId);
    });
  }

  return {
    requests,
    requestsForItem,
    getRequest,
    listStatus,
    listError,
    listMayBeIncomplete,
    loadList,
    loadOne,
    create,
    cancel,
    requeue,
    attemptsFor,
    loadAttempts,
    connectRealtime,
  };
}

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

/**
 * Structural equality, independent of a JS object's own key insertion
 * order. Two endpoints that both serialize "the same shape" (e.g. one row
 * of a list response vs. a single-resource GET) make no promise about key
 * order, so a naive `JSON.stringify` comparison can read byte-for-byte
 * identical data as "changed" purely from that ordering difference —
 * which is exactly what happened here: a request's summary compared equal
 * on every `GET .../executions/{id}` refresh against the previous refresh,
 * but not against the `GET .../executions?...` list row that first
 * populated it, wrongly treating the first realtime tick as a real change.
 */
function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, i) => deepEqual(item, b[i]));
  }
  const aRec = a as Record<string, unknown>;
  const bRec = b as Record<string, unknown>;
  const aKeys = Object.keys(aRec);
  const bKeys = Object.keys(bRec);
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((key) => Object.prototype.hasOwnProperty.call(bRec, key) && deepEqual(aRec[key], bRec[key]));
}

/**
 * Reuses a previous fetch's exact `AttemptSummary` object for any row a
 * refresh returns unchanged, rather than the freshly-deserialized one the
 * new response carries. SolidJS's `<For>` matches array items by reference
 * (`solid-js`'s `mapArray`), so a brand-new object for every row — even
 * one whose fields are identical — reads as "this attempt was removed and
 * a different one added," disposing and recreating everything rendered
 * under it (a `<For>` on a fresh array has no cheaper way to know two
 * objects are "the same row" without a shared reference or an explicit
 * key). Only a row whose content actually changed gets the new object, so
 * a real state transition still renders.
 */
function reuseUnchangedAttempts(previous: AttemptSummary[] | undefined, fresh: AttemptSummary[]): AttemptSummary[] {
  if (!previous || previous.length === 0) return fresh;
  const byId = new Map(previous.map((attempt) => [attempt.attempt_id, attempt]));
  return fresh.map((row) => {
    const prior = byId.get(row.attempt_id);
    return prior && deepEqual(prior, row) ? prior : row;
  });
}

/** Whether a refresh's (already row-deduplicated via
 *  {@link reuseUnchangedAttempts}) result is exactly the same set of
 *  attempt objects, in the same order, as what is already held — the
 *  condition under which `loadAttempts` must not write anything new. */
function sameAttempts(previous: AttemptSummary[], merged: AttemptSummary[]): boolean {
  return previous.length === merged.length && previous.every((attempt, i) => attempt === merged[i]);
}

export type ListStatus = 'idle' | 'loading' | 'ready' | 'error';

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
  /** Fetches one item's full request history into the shared cache,
   *  regardless of how many other requests the install has recorded —
   *  what a mounted `ExecutionTimeline` calls so its item's history is
   *  never silently truncated by any other consumer's bound. Merges into
   *  the same `VersionedCache` every other fetch does, so `requestsForItem`
   *  sees the union of every fetch that has landed. */
  loadList: (itemId: string) => Promise<void>;
  loadOne: (requestId: string) => Promise<void>;
  /**
   * Registers interest in one item's latest execution for as long as the
   * caller holds the returned unwatch function — what `RunWithAgentButton`
   * calls on mount (via `onMount`/`onCleanup`) instead of relying on any
   * shared, install-wide preload. Every `watchItem` call in the same
   * microtask (e.g. a page mounting many badges at once) coalesces into
   * exactly one batched `?item_ids=` request for the whole set, asking the
   * server the question a badge actually means — "the latest execution for
   * each of these item ids" — rather than "the most recent N requests
   * install-wide," a question whose answer degrades with install size.
   * Reference-counted: two callers watching the same id both need their own
   * unwatch called before the id stops being asked about. Also drives the
   * realtime `'list'`-scope refresh (`connectRealtime` below), which
   * re-asks for exactly the currently-watched set on each tick.
   */
  watchItem: (itemId: string) => () => void;
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

  // Reference-counted: two badges watching the same item id both need their
  // own unwatch called before the id drops out of the batch. Also read by
  // `connectRealtime`'s 'list'-scope handler, so a periodic tick refreshes
  // exactly the currently-watched set rather than an install-wide guess.
  const watchedItemIds = new Map<string, number>();
  let itemBatchScheduled = false;

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

  function cancellationEqual(a: CancellationState, b: CancellationState): boolean {
    return a.requested === b.requested && a.pending === b.pending && a.conflict === b.conflict && a.error === b.error;
  }

  // Memoizes the record actually handed out per request id, reused whole
  // when nothing about it changed — see `recordFor`'s own comment for why
  // this exists alongside `reuseUnchangedAttempts` above.
  const recordCache = new Map<string, ExecutionRequestRecord>();

  /**
   * `requestsForItem`/`requests` feed `ExecutionTimeline.tsx`'s `<For>`
   * directly, and SolidJS's `<For>` matches array items by reference (see
   * `reuseUnchangedAttempts`'s doc comment on `mapArray`) — so a record
   * rebuilt fresh on every read, as this used to do unconditionally, made
   * every mounted request row (and everything nested under it: the attempt
   * list, the decision inbox) look like a brand-new row on every store
   * mutation, including an unrelated realtime tick for a *different*
   * request. Reusing the previous record when its `summary`/`error`/
   * `cancellation` are all unchanged keeps that row's identity — and
   * everything mounted under it — stable across a no-op refresh.
   */
  function recordFor(requestId: string): ExecutionRequestRecord | undefined {
    const summary = cache.get(requestId);
    const error = fetchErrors.get(requestId);
    if (!summary && !error) {
      recordCache.delete(requestId);
      return undefined;
    }
    const cancellation = deriveCancellation(summary, requestId);
    const prior = recordCache.get(requestId);
    if (prior && prior.summary === summary && prior.error === error && cancellationEqual(prior.cancellation, cancellation)) {
      return prior;
    }
    const record: ExecutionRequestRecord = {
      status: summary ? 'ready' : 'error',
      summary,
      error,
      cancellation,
      fetchedAt: Date.now(),
    };
    recordCache.set(requestId, record);
    return record;
  }

  /** Applies a fetched row through the version guard; returns whether it
   *  actually landed (false = dropped as a stale, out-of-order response).
   *  `version` must be allocated at the *issuing* operation's start (see
   *  `GLOBAL_SEQUENCE_KEY`'s doc comment) — never here, which would instead
   *  order writes by resolution time and defeat the whole guarantee.
   *
   *  Reuses the previously-cached summary object when the freshly
   *  deserialized one is structurally identical to it, rather than storing
   *  the new one — see `recordFor`'s doc comment for why an unchanged row
   *  needs a stable reference, not just an unchanged value.
   *
   *  Normalizes to exactly `ExecutionSummary`'s five documented fields
   *  first: `GET /executions/{id}` (this store's `loadOne`) serializes an
   *  extra `protocol_version` alongside them that `GET /executions` (this
   *  store's `loadList`/`loadForItems`) does not carry per-row — an
   *  inconsistency between the two handlers, not a real change in the
   *  request — so comparing the raw payloads treated every first refresh
   *  from a list-sourced row to a get-sourced one as a change, every time,
   *  independent of whether anything the type actually documents differed. */
  function applyFetchedSummary(summaryRaw: ExecutionSummary, version: number): boolean {
    const summary: ExecutionSummary = {
      request_id: summaryRaw.request_id,
      item_id: summaryRaw.item_id,
      state: summaryRaw.state,
      cancellation_requested_at: summaryRaw.cancellation_requested_at,
      created_at: summaryRaw.created_at,
    };
    const previous = cache.get(summary.request_id);
    const toStore = previous && deepEqual(previous, summary) ? previous : summary;
    const applied = cache.set(summary.request_id, toStore, version);
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

  async function loadList(itemId: string): Promise<void> {
    setListStatus('loading');
    const version = clock.next(GLOBAL_SEQUENCE_KEY); // one version for every row this call returns
    try {
      const { data } = await executionsApi.list(itemId);
      for (const row of data.data) applyFetchedSummary(row, version);
      setListStatus('ready');
      setListError(undefined);
    } catch (err) {
      setListStatus('error');
      setListError(normalizeError(err));
      throw err;
    }
  }

  /** The batched fetch behind `watchItem` — never touches `listStatus`/
   *  `listError` (those track `ExecutionTimeline`'s own single-item fetch,
   *  a different call site). An id with no execution is simply absent from
   *  the response and never written to the cache — the server has already
   *  answered "no rows" for it authoritatively, so there is no ambiguity
   *  left for a caller to paper over. */
  async function loadForItems(itemIds: readonly string[]): Promise<void> {
    if (itemIds.length === 0) return;
    const version = clock.next(GLOBAL_SEQUENCE_KEY);
    try {
      const { data } = await executionsApi.list(undefined, undefined, itemIds);
      for (const row of data.data) applyFetchedSummary(row, version);
    } catch {
      // Both call sites fire this without awaiting it, so a rejection here
      // has nowhere to land and would surface as an unhandled rejection.
      // A badge that cannot refresh keeps whatever it last showed and
      // retries on the next realtime tick; there is no user action to
      // offer and no state worth invalidating on one failed poll.
    }
  }

  function watchItem(itemId: string): () => void {
    watchedItemIds.set(itemId, (watchedItemIds.get(itemId) ?? 0) + 1);
    // Coalesce every `watchItem` call within the same microtask (e.g. a
    // page mounting a dozen badges in one render pass) into one request —
    // `queueMicrotask` runs after all of them have registered but before
    // any other network round-trip could observe a half-registered set.
    if (!itemBatchScheduled) {
      itemBatchScheduled = true;
      queueMicrotask(() => {
        itemBatchScheduled = false;
        void loadForItems([...watchedItemIds.keys()]);
      });
    }
    return () => {
      const count = watchedItemIds.get(itemId);
      if (count === undefined) return;
      if (count <= 1) watchedItemIds.delete(itemId);
      else watchedItemIds.set(itemId, count - 1);
    };
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

  /**
   * `loading` is reserved for "nothing is known yet" (idle, or a previous
   * error) — never for "the data we have might be one round trip stale."
   * A request that already has `ready` data keeps showing exactly that
   * object, unconditionally, for the entire refresh; `attemptsCache` is
   * only written again once the fetch resolves, and only if the result
   * actually differs from what is already held.
   *
   * This is stricter than a `refreshing` flag on the `ready` variant would
   * be, and deliberately so: `attemptsFor()` is read as one plain value
   * (`ExecutionTimeline.tsx`'s `RequestRow` calls it inside a `createMemo`,
   * not through a `<For>`, which is the only place a reference change
   * without a content change is harmless — see `reuseUnchangedAttempts`'s
   * doc comment). SolidJS memos re-run their dependents whenever the
   * tracked value's *reference* changes, full stop; a `refreshing` flag
   * would need a new wrapper object to flip it, and that new reference
   * would re-trigger every downstream computation exactly as `loading`
   * did — including disposing and recreating the mounted attempt list and
   * its nested decision inbox — even though the underlying data never
   * changed. Skipping the write (not just skipping a status label) is the
   * only version of this fix that actually holds.
   */
  async function loadAttempts(requestId: string): Promise<void> {
    const existing = attemptsCache.get(requestId);
    const previousData = existing?.status === 'ready' ? existing.data : undefined;
    if (!previousData) {
      attemptsCache.set(requestId, { status: 'loading' });
      touch();
    }
    try {
      const { data } = await attemptsApi.list(requestId);
      const merged = reuseUnchangedAttempts(previousData, data.data);
      const unchanged = previousData !== undefined && sameAttempts(previousData, merged);
      if (!unchanged) {
        attemptsCache.set(requestId, { status: 'ready', data: merged });
        touch();
      }
    } catch (err) {
      attemptsCache.set(requestId, { status: 'error', error: normalizeError(err) });
      touch();
      throw err;
    }
  }

  function connectRealtime(realtime: ExecutionRealtime): () => void {
    return realtime.onInvalidate((event) => {
      if (event.scope === 'list') {
        // Refreshes exactly the currently-watched badge set, not an
        // install-wide guess — see `watchItem`'s own doc comment.
        void loadForItems([...watchedItemIds.keys()]);
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
    loadList,
    loadOne,
    watchItem,
    create,
    cancel,
    requeue,
    attemptsFor,
    loadAttempts,
    connectRealtime,
  };
}

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { ApiError } from '../api/client';
import { createExecutionStore } from './store';
import { executionsApi } from './api';
import type { ExecutionSummary } from './api';
import { attemptsApi } from './attempts';
import type { AttemptSummary } from './attempts';
import type { ExecutionRealtime, ExecutionInvalidationEvent } from './realtime';

vi.mock('./api', () => ({
  executionsApi: {
    list: vi.fn(),
    get: vi.fn(),
    create: vi.fn(),
    cancel: vi.fn(),
    requeue: vi.fn(),
  },
}));

vi.mock('./attempts', () => ({
  attemptsApi: {
    list: vi.fn(),
    events: vi.fn(),
  },
}));

const mockedApi = vi.mocked(executionsApi, true);
const mockedAttemptsApi = vi.mocked(attemptsApi, true);

function attemptSummary(overrides: Partial<AttemptSummary> = {}): AttemptSummary {
  return {
    attempt_id: 'att_1',
    request_id: 'exec_1',
    attempt_number: 1,
    runner_id: 'runner_1',
    fencing_token: 1,
    state: 'running',
    lease_issued_at: '2026-08-06T12:00:00Z',
    lease_expires_at: '2026-08-06T12:05:00Z',
    last_heartbeat_at: null,
    event_checkpoint: null,
    completion_id: null,
    workspace_id: null,
    base_revision: null,
    actual_execution: null,
    terminal_reason: null,
    usage: null,
    started_at: null,
    ended_at: null,
    created_at: '2026-08-06T12:00:00Z',
    updated_at: '2026-08-06T12:00:00Z',
    model_provenance: null,
    usage_economics: {
      model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
      runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' } },
    },
    ...overrides,
  };
}

function summary(overrides: Partial<ExecutionSummary> = {}): ExecutionSummary {
  return {
    request_id: 'exec_1',
    item_id: 'item_1',
    state: 'running',
    cancellation_requested_at: null,
    created_at: '2026-08-06T12:00:00Z',
    ...overrides,
  };
}

function withHeaders<T>(data: T): { data: T; headers: Headers } {
  return { data, headers: new Headers() };
}

/** A promise plus its resolve/reject, for controlling settlement order. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  vi.resetAllMocks();
});

/** Drains `watchItem`'s `queueMicrotask`-scheduled batch flush plus the
 *  mocked API promise's own resolution — a `setTimeout` macrotask always
 *  runs after every microtask already queued, so this is enough regardless
 *  of how many microtask hops `loadForItems` needs. */
const flush = () => new Promise((r) => setTimeout(r, 0));

describe('createExecutionStore — loadOne / loadList', () => {
  it('loadOne populates a ready record consumers can read', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary()));
    const store = createExecutionStore();
    await store.loadOne('exec_1');
    const record = store.getRequest('exec_1');
    expect(record?.status).toBe('ready');
    expect(record?.summary).toEqual(summary());
  });

  it('a record that was never fetched is undefined — not conflated with an error or empty-but-ready record', () => {
    const store = createExecutionStore();
    expect(store.getRequest('never-fetched')).toBeUndefined();
  });

  it('loadOne failure yields an explicit error record, distinguishable from "no data yet"', async () => {
    mockedApi.get.mockRejectedValue(new ApiError(404, 'Execution request does not exist', 'not_found'));
    const store = createExecutionStore();
    await expect(store.loadOne('missing')).rejects.toBeInstanceOf(ApiError);
    const record = store.getRequest('missing');
    expect(record?.status).toBe('error');
    expect(record?.summary).toBeUndefined();
    expect(record?.error).toEqual({ status: 404, code: 'not_found', message: 'Execution request does not exist' });
  });

  it('a refresh failure after a successful load keeps the last-known summary and surfaces the error alongside it', async () => {
    mockedApi.get.mockResolvedValueOnce(withHeaders(summary({ state: 'running' })));
    const store = createExecutionStore();
    await store.loadOne('exec_1');

    mockedApi.get.mockRejectedValueOnce(new ApiError(500, 'internal error', 'internal_error'));
    await expect(store.loadOne('exec_1')).rejects.toBeInstanceOf(ApiError);

    const record = store.getRequest('exec_1');
    expect(record?.status).toBe('ready'); // still trustworthy — real prior data, not wiped
    expect(record?.summary?.state).toBe('running');
    expect(record?.error?.code).toBe('internal_error');
  });

  it('loadList populates every row and flips listStatus loading -> ready', async () => {
    mockedApi.list.mockResolvedValue(
      withHeaders({ protocol_version: 1, data: [summary({ request_id: 'exec_1' }), summary({ request_id: 'exec_2' })] }),
    );
    const store = createExecutionStore();
    expect(store.listStatus()).toBe('idle');
    const promise = store.loadList('item_1');
    await promise;
    expect(store.listStatus()).toBe('ready');
    expect(store.requests().size).toBe(2);
    expect(store.getRequest('exec_2')?.summary?.request_id).toBe('exec_2');
  });

  it('loadList failure sets listStatus error + listError, without discarding rows fetched earlier', async () => {
    mockedApi.list.mockResolvedValueOnce(withHeaders({ protocol_version: 1, data: [summary()] }));
    const store = createExecutionStore();
    await store.loadList('item_1');

    mockedApi.list.mockRejectedValueOnce(new ApiError(500, 'boom'));
    await expect(store.loadList('item_1')).rejects.toBeInstanceOf(ApiError);
    expect(store.listStatus()).toBe('error');
    expect(store.listError()?.message).toBe('boom');
    expect(store.getRequest('exec_1')).toBeDefined(); // earlier row survives
  });

  it('loadList(itemId) forwards the item id to executionsApi.list and still merges into the shared cache', async () => {
    mockedApi.list.mockResolvedValue(
      withHeaders({ protocol_version: 1, data: [summary({ request_id: 'exec_1', item_id: 'item_1' })] }),
    );
    const store = createExecutionStore();
    await store.loadList('item_1');
    expect(mockedApi.list).toHaveBeenCalledWith('item_1');
    expect(store.getRequest('exec_1')?.summary?.item_id).toBe('item_1');
  });

  it('requestsForItem filters by item and sorts newest created_at first', async () => {
    mockedApi.list.mockResolvedValue(
      withHeaders({
        protocol_version: 1,
        data: [
          summary({ request_id: 'a', item_id: 'item_1', created_at: '2026-08-01T00:00:00Z' }),
          summary({ request_id: 'b', item_id: 'item_2', created_at: '2026-08-03T00:00:00Z' }),
          summary({ request_id: 'c', item_id: 'item_1', created_at: '2026-08-05T00:00:00Z' }),
        ],
      }),
    );
    const store = createExecutionStore();
    await store.loadList('item_1');
    const forItem1 = store.requestsForItem('item_1');
    expect(forItem1.map((r) => r.summary?.request_id)).toEqual(['c', 'a']);
  });
});

describe('createExecutionStore — watchItem() / loadForItems()', () => {
  it('coalesces every watchItem call in the same tick into exactly one batched request', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    store.watchItem('item_1');
    store.watchItem('item_2');
    store.watchItem('item_3');
    await flush();
    expect(mockedApi.list).toHaveBeenCalledTimes(1);
    expect(mockedApi.list).toHaveBeenCalledWith(undefined, undefined, ['item_1', 'item_2', 'item_3']);
  });

  it('populates the cache with the batched response, keyed by whichever item each row belongs to', async () => {
    mockedApi.list.mockResolvedValue(
      withHeaders({
        protocol_version: 1,
        data: [
          summary({ request_id: 'exec_1', item_id: 'item_1', state: 'queued' }),
          summary({ request_id: 'exec_2', item_id: 'item_2', state: 'succeeded' }),
        ],
      }),
    );
    const store = createExecutionStore();
    store.watchItem('item_1');
    store.watchItem('item_2');
    await flush();
    expect(store.requestsForItem('item_1')[0]?.summary?.state).toBe('queued');
    expect(store.requestsForItem('item_2')[0]?.summary?.state).toBe('succeeded');
  });

  it('an id with no execution stays absent from the cache — never conflated with "not yet fetched"', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    store.watchItem('item_1');
    await flush();
    expect(store.requestsForItem('item_1')).toEqual([]);
  });

  it('unwatching removes an item from the next batch, without affecting one watched later in a separate tick', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    const unwatchA = store.watchItem('item_a');
    await flush();
    expect(mockedApi.list).toHaveBeenLastCalledWith(undefined, undefined, ['item_a']);

    unwatchA();
    store.watchItem('item_b');
    await flush();
    expect(mockedApi.list).toHaveBeenLastCalledWith(undefined, undefined, ['item_b']);
  });

  it('two callers watching the same item both need to unwatch before it drops out of the batch', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    const unwatch1 = store.watchItem('item_1');
    await flush();
    const unwatch2 = store.watchItem('item_1');
    await flush();

    unwatch1();
    store.watchItem('item_2'); // forces a fresh batch to observe the current set
    await flush();
    expect(mockedApi.list).toHaveBeenLastCalledWith(undefined, undefined, ['item_1', 'item_2']);

    unwatch2();
    store.watchItem('item_3');
    await flush();
    expect(mockedApi.list).toHaveBeenLastCalledWith(undefined, undefined, ['item_2', 'item_3']);
  });

  it("an item whose only execution predates a large batch of others' rows still shows its real state — the badge asks by id, not by an install-wide row cap", async () => {
    // Mirrors the server-side acceptance proof
    // (`list_executions_item_ids_finds_an_item_whose_only_execution_predates_every_other_row`):
    // a fake that actually answers per requested id, the way the real
    // `item_ids` batch route does, rather than echoing back a fixed list
    // regardless of what was asked (which could not tell "asked for
    // everything" apart from "asked for exactly this one").
    const allRequests = new Map<string, ExecutionSummary>();
    allRequests.set('stale-item', summary({ request_id: 'exec_old', item_id: 'stale-item', state: 'succeeded' }));
    for (let i = 0; i < 2005; i += 1) {
      allRequests.set(`noise-item-${i}`, summary({ request_id: `exec_noise_${i}`, item_id: `noise-item-${i}`, state: 'running' }));
    }
    mockedApi.list.mockImplementation(async (_itemId, _limit, itemIds?: readonly string[]) =>
      withHeaders({
        protocol_version: 1,
        data: (itemIds ?? []).flatMap((id) => {
          const row = allRequests.get(id);
          return row ? [row] : [];
        }),
      }),
    );
    const store = createExecutionStore();
    store.watchItem('stale-item');
    await flush();
    expect(store.requestsForItem('stale-item')[0]?.summary?.state).toBe('succeeded');
  });
});

describe('createExecutionStore — ordering guard (stale response never overwrites a newer snapshot)', () => {
  it('a slow, earlier-issued fetch arriving after a later one must not overwrite it', async () => {
    const first = deferred<{ data: ExecutionSummary; headers: Headers }>();
    const second = deferred<{ data: ExecutionSummary; headers: Headers }>();
    mockedApi.get.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    const store = createExecutionStore();
    const call1 = store.loadOne('exec_1'); // issued first (sequence 1)
    const call2 = store.loadOne('exec_1'); // issued second (sequence 2)

    // Resolve the SECOND (newer) fetch first...
    second.resolve(withHeaders(summary({ state: 'succeeded' })));
    await call2;
    expect(store.getRequest('exec_1')?.summary?.state).toBe('succeeded');

    // ...then let the FIRST (older) fetch arrive late.
    first.resolve(withHeaders(summary({ state: 'running' })));
    await call1;

    // The stale, late-arriving response must never win.
    expect(store.getRequest('exec_1')?.summary?.state).toBe('succeeded');
  });
});

describe('createExecutionStore — create()', () => {
  it('hydrates the store immediately so the new request is visible without a second manual fetch', async () => {
    mockedApi.create.mockResolvedValue({ protocol_version: 1, request_id: 'exec_new', state: 'queued', replayed: false });
    mockedApi.get.mockResolvedValue(withHeaders(summary({ request_id: 'exec_new', state: 'queued' })));

    const store = createExecutionStore();
    const result = await store.create({
      item_id: 'item_1',
      idempotency_key: 'k1',
      selector_kind: 'exact_runner',
      selector_id: 'runr_1',
      agent_profile_id: 'ap_1',
      requested_harness_kind: 'codex',
      agent_profile_snapshot: {},
      repository_snapshot: {},
      permission_policy: {},
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 3600,
    });

    expect(result.request_id).toBe('exec_new');
    expect(store.getRequest('exec_new')?.summary?.state).toBe('queued');
  });

  it('still resolves even if the post-create hydration fetch fails', async () => {
    mockedApi.create.mockResolvedValue({ protocol_version: 1, request_id: 'exec_new', state: 'queued', replayed: false });
    mockedApi.get.mockRejectedValue(new ApiError(500, 'transient'));

    const store = createExecutionStore();
    await expect(
      store.create({
        item_id: 'item_1',
        idempotency_key: 'k1',
        selector_kind: 'fleet',
        selector_id: 'fleet_1',
        agent_profile_id: 'ap_1',
        requested_harness_kind: 'codex',
        agent_profile_snapshot: {},
        repository_snapshot: {},
        permission_policy: {},
        budgets: {},
        environment: {},
        metadata: {},
        timeout_seconds: 3600,
      }),
    ).resolves.toMatchObject({ request_id: 'exec_new' });
  });
});

describe('createExecutionStore — optimistic cancel with rollback and explicit conflict state', () => {
  it('optimistically marks pending immediately, before the network call resolves', async () => {
    const pending = deferred<{ protocol_version: number; request_id: string; state: string }>();
    mockedApi.cancel.mockReturnValue(pending.promise);
    mockedApi.get.mockResolvedValue(withHeaders(summary()));

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    const cancelPromise = store.cancel('exec_1');

    // Still in flight — optimistic state must already be visible.
    const duringFlight = store.getRequest('exec_1')?.cancellation;
    expect(duringFlight).toEqual({ requested: true, pending: true, conflict: false, error: undefined });

    pending.resolve({ protocol_version: 1, request_id: 'exec_1', state: 'cancellation_requested' });
    await cancelPromise;
  });

  it('a successful cancel leaves pending true until a fresh fetch confirms cancellation_requested_at', async () => {
    mockedApi.cancel.mockResolvedValue({ protocol_version: 1, request_id: 'exec_1', state: 'cancellation_requested' });
    mockedApi.get.mockResolvedValueOnce(withHeaders(summary({ cancellation_requested_at: null })));

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    await store.cancel('exec_1');

    let state = store.getRequest('exec_1')?.cancellation;
    expect(state?.requested).toBe(true);
    expect(state?.pending).toBe(true); // not yet confirmed by the server's own state column

    mockedApi.get.mockResolvedValueOnce(withHeaders(summary({ cancellation_requested_at: '2026-08-06T12:05:00Z' })));
    await store.loadOne('exec_1');

    state = store.getRequest('exec_1')?.cancellation;
    expect(state?.requested).toBe(true);
    expect(state?.pending).toBe(false); // confirmed — pending clears automatically
  });

  it('rolls back and sets an explicit conflict flag on a terminal-state 409, distinct from a generic error', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary({ state: 'succeeded' })));
    mockedApi.cancel.mockRejectedValue(
      new ApiError(409, 'Execution already reached a terminal state before cancellation could apply', 'conflict'),
    );

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    await expect(store.cancel('exec_1')).rejects.toBeInstanceOf(ApiError);

    const state = store.getRequest('exec_1')?.cancellation;
    expect(state).toEqual({ requested: false, pending: false, conflict: true, error: undefined });
  });

  it('rolls back with a generic error (not conflict) for a non-conflict failure', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary()));
    mockedApi.cancel.mockRejectedValue(new ApiError(500, 'network blip'));

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    await expect(store.cancel('exec_1')).rejects.toBeInstanceOf(ApiError);

    const state = store.getRequest('exec_1')?.cancellation;
    expect(state?.conflict).toBe(false);
    expect(state?.pending).toBe(false);
    expect(state?.requested).toBe(false);
    expect(state?.error).toEqual({ status: 500, code: undefined, message: 'network blip' });
  });

  it('de-duplicates a concurrent second cancel() call for the same request instead of racing', async () => {
    const pending = deferred<{ protocol_version: number; request_id: string; state: string }>();
    mockedApi.cancel.mockReturnValue(pending.promise);
    mockedApi.get.mockResolvedValue(withHeaders(summary()));

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    const first = store.cancel('exec_1');
    const second = store.cancel('exec_1'); // fires while the first is still in flight

    pending.resolve({ protocol_version: 1, request_id: 'exec_1', state: 'cancellation_requested' });
    await Promise.all([first, second]);

    expect(mockedApi.cancel).toHaveBeenCalledTimes(1);
  });
});

describe('createExecutionStore — requeue()', () => {
  it('merges the genuinely authoritative post-requeue state and clears cancellation state', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary({ state: 'needs_operator' })));
    mockedApi.requeue.mockResolvedValue({
      protocol_version: 1,
      request_id: 'exec_1',
      state: 'queued',
      recovered_from: 'needs_operator',
      replayed: false,
    });

    const store = createExecutionStore();
    await store.loadOne('exec_1');
    await store.requeue('exec_1', { recovery_key: 'rk', reason: 'confirmed dead process' });

    const record = store.getRequest('exec_1');
    expect(record?.summary?.state).toBe('queued');
    expect(record?.cancellation).toEqual({ requested: false, pending: false, conflict: false, error: undefined });
  });
});

describe('createExecutionStore — attemptsFor() / loadAttempts()', () => {
  it('is idle until loadAttempts is called — never conflated with "loaded, zero attempts"', () => {
    const store = createExecutionStore();
    expect(store.attemptsFor('exec_1')).toEqual({ status: 'idle' });
  });

  it('loadAttempts populates a ready record with real data', async () => {
    mockedAttemptsApi.list.mockResolvedValue(
      withHeaders({ protocol_version: 1, data: [attemptSummary(), attemptSummary({ attempt_id: 'att_2', attempt_number: 2 })] }),
    );
    const store = createExecutionStore();
    const promise = store.loadAttempts('exec_1');
    expect(store.attemptsFor('exec_1')).toEqual({ status: 'loading' });
    await promise;
    const record = store.attemptsFor('exec_1');
    expect(record.status).toBe('ready');
    expect(record.status === 'ready' && record.data).toHaveLength(2);
  });

  it('a genuinely empty attempt list is "ready" with zero rows, not "idle" or "error"', async () => {
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    await store.loadAttempts('exec_1');
    expect(store.attemptsFor('exec_1')).toEqual({ status: 'ready', data: [] });
  });

  it('a refresh of a ready entry never transitions it through loading — the exact same object stays visible for the entire in-flight window', async () => {
    const first = attemptSummary();
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [first] }));
    const store = createExecutionStore();
    await store.loadAttempts('exec_1');
    const initial = store.attemptsFor('exec_1');
    expect(initial).toEqual({ status: 'ready', data: [first] });

    const { promise, resolve } = deferred<{ data: { protocol_version: number; data: AttemptSummary[] }; headers: Headers }>();
    mockedAttemptsApi.list.mockReturnValue(promise);
    const refresh = store.loadAttempts('exec_1');

    // Reference equality, not just structural — the SAME object a
    // consumer's memo already holds, never a new one that merely looks
    // the same, since a new reference alone (independent of its status
    // label) is what re-triggers a reference-keyed consumer.
    expect(store.attemptsFor('exec_1')).toBe(initial);

    resolve(withHeaders({ protocol_version: 1, data: [first] }));
    await refresh;
    expect(store.attemptsFor('exec_1')).toBe(initial);
  });

  it('a refresh whose content is unchanged leaves attemptsFor() returning the exact same object, row objects included', async () => {
    const original = attemptSummary();
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [original] }));
    const store = createExecutionStore();
    await store.loadAttempts('exec_1');
    const initial = store.attemptsFor('exec_1');

    // A structurally-identical but distinct object, as a fresh HTTP
    // response would deserialize — never the same reference as `original`.
    const reFetched = attemptSummary();
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [reFetched] }));
    await store.loadAttempts('exec_1');

    expect(store.attemptsFor('exec_1')).toBe(initial);
    const record = store.attemptsFor('exec_1');
    expect(record.status === 'ready' && record.data[0]).toBe(original);
  });

  it('a refresh with a changed row returns a new ready object, so a real state transition still renders', async () => {
    const original = attemptSummary({ state: 'running' });
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [original] }));
    const store = createExecutionStore();
    await store.loadAttempts('exec_1');
    const initial = store.attemptsFor('exec_1');

    const updated = attemptSummary({ state: 'completed' });
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [updated] }));
    await store.loadAttempts('exec_1');

    const record = store.attemptsFor('exec_1');
    expect(record).not.toBe(initial);
    expect(record.status).toBe('ready');
    expect(record.status === 'ready' && record.data[0]).toBe(updated);
  });

  it('loadAttempts failure yields an explicit error record and rejects', async () => {
    mockedAttemptsApi.list.mockRejectedValue(new ApiError(500, 'boom', 'internal_error'));
    const store = createExecutionStore();
    await expect(store.loadAttempts('exec_1')).rejects.toBeInstanceOf(ApiError);
    const record = store.attemptsFor('exec_1');
    expect(record.status).toBe('error');
    expect(record.status === 'error' && record.error.message).toBe('boom');
  });

  it('a request nobody has asked about stays idle even after other requests load attempts', async () => {
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [attemptSummary()] }));
    const store = createExecutionStore();
    await store.loadAttempts('exec_1');
    expect(store.attemptsFor('exec_2')).toEqual({ status: 'idle' });
  });
});

describe('createExecutionStore — connectRealtime()', () => {
  function fakeRealtime() {
    const listeners = new Set<(e: ExecutionInvalidationEvent) => void>();
    const realtime: ExecutionRealtime = {
      status: () => 'active',
      onInvalidate: (cb) => {
        listeners.add(cb);
        return () => listeners.delete(cb);
      },
      dispose: () => listeners.clear(),
    };
    return { realtime, emit: (e: ExecutionInvalidationEvent) => listeners.forEach((cb) => cb(e)) };
  }

  it('a list-scope invalidation refreshes exactly the currently-watched badge set', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    store.watchItem('item_1');
    store.watchItem('item_2');
    await new Promise((r) => setTimeout(r, 0)); // the watch's own initial batch

    const { realtime, emit } = fakeRealtime();
    store.connectRealtime(realtime);
    mockedApi.list.mockClear();

    emit({ scope: 'list' });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedApi.list).toHaveBeenCalledTimes(1);
    expect(mockedApi.list).toHaveBeenCalledWith(undefined, undefined, ['item_1', 'item_2']);
  });

  it('a list-scope invalidation with nothing watched calls the API zero times, not with an empty item_ids', async () => {
    const store = createExecutionStore();
    const { realtime, emit } = fakeRealtime();
    store.connectRealtime(realtime);

    emit({ scope: 'list' });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedApi.list).not.toHaveBeenCalled();
  });

  it('a request-scope invalidation triggers loadOne() for exactly that id', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary({ request_id: 'exec_9' })));
    const store = createExecutionStore();
    const { realtime, emit } = fakeRealtime();
    store.connectRealtime(realtime);

    emit({ scope: 'request', requestId: 'exec_9' });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedApi.get).toHaveBeenCalledWith('exec_9');
  });

  it('a request-scope invalidation refreshes attempts only for a request that already loaded them', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary({ request_id: 'exec_9' })));
    mockedAttemptsApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [attemptSummary()] }));
    const store = createExecutionStore();
    const { realtime, emit } = fakeRealtime();
    store.connectRealtime(realtime);

    // Nobody has called loadAttempts('exec_9') yet — invalidation must not
    // fetch attempts for it out of nowhere.
    emit({ scope: 'request', requestId: 'exec_9' });
    await Promise.resolve();
    await Promise.resolve();
    expect(mockedAttemptsApi.list).not.toHaveBeenCalled();

    // Once a consumer has asked about this request's attempts, a later
    // invalidation refreshes them too.
    await store.loadAttempts('exec_9');
    mockedAttemptsApi.list.mockClear();
    emit({ scope: 'request', requestId: 'exec_9' });
    await Promise.resolve();
    await Promise.resolve();
    expect(mockedAttemptsApi.list).toHaveBeenCalledWith('exec_9');
  });

  it('the returned unsubscribe function detaches from the realtime source', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    store.watchItem('item_1'); // gives a 'list' emit something to refresh, if still subscribed
    await new Promise((r) => setTimeout(r, 0));

    const { realtime, emit } = fakeRealtime();
    const unsubscribe = store.connectRealtime(realtime);
    unsubscribe();

    mockedApi.list.mockClear();
    emit({ scope: 'list' });
    expect(mockedApi.list).not.toHaveBeenCalled();
  });
});

describe('createExecutionStore — reactivity', () => {
  it('requests() is reactive: a SolidJS memo re-derives after a mutation', async () => {
    mockedApi.get.mockResolvedValue(withHeaders(summary()));
    await createRoot(async (dispose) => {
      const store = createExecutionStore();
      const sizes: number[] = [];
      // Track manually via repeated reads inside a root, since createMemo's
      // async timing is awkward to assert against directly in a plain test.
      sizes.push(store.requests().size);
      await store.loadOne('exec_1');
      sizes.push(store.requests().size);
      expect(sizes).toEqual([0, 1]);
      dispose();
    });
  });
});

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { ApiError } from '../api/client';
import { createExecutionStore, EXECUTION_LIST_PRELOAD_LIMIT } from './store';
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
    const promise = store.loadList();
    await promise;
    expect(store.listStatus()).toBe('ready');
    expect(store.requests().size).toBe(2);
    expect(store.getRequest('exec_2')?.summary?.request_id).toBe('exec_2');
  });

  it('loadList failure sets listStatus error + listError, without discarding rows fetched earlier', async () => {
    mockedApi.list.mockResolvedValueOnce(withHeaders({ protocol_version: 1, data: [summary()] }));
    const store = createExecutionStore();
    await store.loadList();

    mockedApi.list.mockRejectedValueOnce(new ApiError(500, 'boom'));
    await expect(store.loadList()).rejects.toBeInstanceOf(ApiError);
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
    await store.loadList();
    const forItem1 = store.requestsForItem('item_1');
    expect(forItem1.map((r) => r.summary?.request_id)).toEqual(['c', 'a']);
  });

  it('loadList() (unscoped) asks the server for EXECUTION_LIST_PRELOAD_LIMIT rows instead of leaving limit unset', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    await store.loadList();
    expect(mockedApi.list).toHaveBeenCalledWith(undefined, EXECUTION_LIST_PRELOAD_LIMIT);
  });

  it('loadList(itemId) (scoped) still asks for exactly one argument, unaffected by the preload limit', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    await store.loadList('item_1');
    expect(mockedApi.list).toHaveBeenCalledWith('item_1');
  });

  it('an item touched before the server\'s DEFAULT_LIMIT (200) of newer requests still shows its real state, because the preload asks past that bound', async () => {
    const SERVER_DEFAULT_LIMIT = 200; // mirrors ListExecutionsQuery::DEFAULT_LIMIT, crates/tack-api/src/handlers/executions.rs
    // 205 rows, newest first, where the OLDEST row belongs to the one item
    // on screen this test cares about — a table seeded past the server's
    // default bound, exactly the acceptance scenario.
    const rows: ExecutionSummary[] = Array.from({ length: 205 }, (_, i) =>
      summary({
        request_id: `exec_${i}`,
        item_id: i === 204 ? 'stale-item' : `other-item-${i}`,
        state: i === 204 ? 'succeeded' : 'running',
        created_at: `2026-01-01T00:00:${String(204 - i).padStart(2, '0')}Z`,
      }),
    );
    // A fake that actually applies `limit` the way the real handler's
    // `ORDER BY created_at DESC LIMIT ?` does, which
    // `list_executions_limit_bounds_the_unscoped_response` pins on the
    // server side — every other test's plain
    // `mockResolvedValue` echoes back whatever rows it's handed regardless
    // of what limit was asked for, which cannot distinguish "asked for
    // 200" from "asked for 2000"; this one can.
    mockedApi.list.mockImplementation(async (_itemId?: string, limit?: number) =>
      withHeaders({ protocol_version: 1, data: rows.slice(0, limit ?? SERVER_DEFAULT_LIMIT) }),
    );
    const store = createExecutionStore();
    await store.loadList();
    expect(store.requestsForItem('stale-item')[0]?.summary?.state).toBe('succeeded');
  });

  it('listMayBeIncomplete() is true once the unscoped preload comes back at the row cap, false once it comes back short of it', async () => {
    const full: ExecutionSummary[] = Array.from({ length: EXECUTION_LIST_PRELOAD_LIMIT }, (_, i) =>
      summary({ request_id: `exec_${i}` }),
    );
    mockedApi.list.mockResolvedValueOnce(withHeaders({ protocol_version: 1, data: full }));
    const store = createExecutionStore();
    expect(store.listMayBeIncomplete()).toBe(false); // never fetched yet
    await store.loadList();
    expect(store.listMayBeIncomplete()).toBe(true);

    mockedApi.list.mockResolvedValueOnce(withHeaders({ protocol_version: 1, data: [summary()] }));
    await store.loadList();
    expect(store.listMayBeIncomplete()).toBe(false);
  });

  it('listMayBeIncomplete() is never set by an item-scoped loadList(itemId), even one returning many rows', async () => {
    const many: ExecutionSummary[] = Array.from({ length: EXECUTION_LIST_PRELOAD_LIMIT }, (_, i) =>
      summary({ request_id: `exec_${i}`, item_id: 'item_1' }),
    );
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: many }));
    const store = createExecutionStore();
    await store.loadList('item_1');
    expect(store.listMayBeIncomplete()).toBe(false);
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

  it('a list-scope invalidation triggers loadList()', async () => {
    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
    const store = createExecutionStore();
    const { realtime, emit } = fakeRealtime();
    store.connectRealtime(realtime);

    emit({ scope: 'list' });
    await Promise.resolve();
    await Promise.resolve();

    expect(mockedApi.list).toHaveBeenCalledTimes(1);
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

  it('the returned unsubscribe function detaches from the realtime source', () => {
    const store = createExecutionStore();
    const { realtime, emit } = fakeRealtime();
    const unsubscribe = store.connectRealtime(realtime);
    unsubscribe();

    mockedApi.list.mockResolvedValue(withHeaders({ protocol_version: 1, data: [] }));
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

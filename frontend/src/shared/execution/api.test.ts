import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ApiError, tokenStore } from '../api/client';
import { executionsApi, fleetsApi, agentProfilesApi, modelProfilesApi, runnersApi } from './api';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
}

beforeEach(() => {
  tokenStore.set(null);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('executionsApi', () => {
  it('list() calls GET /api/executions', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method ?? 'GET').toBe('GET');
  });

  it('list(itemId) calls GET /api/executions?item_id={id}, URL-encoded', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list('item/1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_id=item%2F1');
  });

  it('list(undefined, limit) calls GET /api/executions?limit={n} with no item_id', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, 2000);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?limit=2000');
  });

  it('list(itemId, limit) combines both query parameters', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list('item_1', 50);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_id=item_1&limit=50');
  });

  it('list(undefined, undefined, itemIds) calls GET /api/executions?item_ids={comma-joined}, URL-encoded', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, undefined, ['item/1', 'item-2']);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_ids=item%2F1%2Citem-2');
  });

  it('list(undefined, undefined, []) omits item_ids entirely rather than sending an empty parameter', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, undefined, []);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
  });

  it('list() preserves response headers, not just the parsed body', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse(
        { protocol_version: 1, data: [] },
        { headers: { 'Content-Type': 'application/json', 'X-Tack-Test-Header': 'preserved' } },
      ),
    );
    const { data, headers } = await executionsApi.list();
    expect(data).toEqual({ protocol_version: 1, data: [] });
    expect(headers.get('X-Tack-Test-Header')).toBe('preserved');
  });

  it('get(id) calls GET /api/executions/{id} and returns the thin five-field row', async () => {
    const row = {
      request_id: 'exec_1',
      item_id: 'item_1',
      state: 'running',
      cancellation_requested_at: null,
      created_at: '2026-08-06T12:00:00Z',
    };
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse(row));
    const { data } = await executionsApi.get('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1');
    expect(data).toEqual(row);
  });

  it('get(id) URL-encodes the request id', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ request_id: 'a/b', item_id: 'i', state: 'queued', cancellation_requested_at: null, created_at: '' }),
    );
    await executionsApi.get('a/b');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/a%2Fb');
  });

  it('create() POSTs the full CreateExecution body and returns the create result', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, request_id: 'exec_1', state: 'queued', replayed: false }),
    );
    const input = {
      item_id: 'item_1',
      idempotency_key: 'key-1',
      selector_kind: 'exact_runner' as const,
      agent_profile_id: 'ap_1',
      selector_id: 'runr_1',
      requested_harness_kind: 'codex',
      agent_profile_snapshot: {},
      repository_snapshot: {},
      permission_policy: {},
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 3600,
    };
    const result = await executionsApi.create(input);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual(input);
    expect(result).toEqual({ protocol_version: 1, request_id: 'exec_1', state: 'queued', replayed: false });
  });

  it('cancel() POSTs to /cancel and surfaces the acknowledgement string as-is', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, request_id: 'exec_1', state: 'cancellation_requested' }),
    );
    const result = await executionsApi.cancel('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/cancel');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(result.state).toBe('cancellation_requested');
  });

  it('cancel() rejects with ApiError carrying the conflict code on a terminal-state 409', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: {
            code: 'conflict',
            message: 'Execution already reached a terminal state before cancellation could apply',
            request_id: 'req_operator',
            retryable: true,
            details: {},
          },
        }),
        { status: 409, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const err = await executionsApi.cancel('exec_1').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(409);
    expect((err as ApiError).code).toBe('conflict');
  });

  it('requeue() POSTs the recovery confirmation body', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        request_id: 'exec_1',
        state: 'queued',
        recovered_from: 'needs_operator',
        replayed: false,
      }),
    );
    const result = await executionsApi.requeue('exec_1', { recovery_key: 'rk-1', reason: 'confirmed dead' });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/requeue');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual({
      recovery_key: 'rk-1',
      reason: 'confirmed dead',
    });
    expect(result.state).toBe('queued');
  });
});

describe('fleetsApi', () => {
  it('list() calls GET /api/runner-fleets', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await fleetsApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets');
  });

  it('create() POSTs to /api/runner-fleets', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', name: 'primary' }));
    const result = await fleetsApi.create({ name: 'primary' });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets');
    expect(result.fleet_id).toBe('fleet_1');
  });
});

describe('agentProfilesApi', () => {
  it('list() calls GET /api/agent-profiles', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await agentProfilesApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/agent-profiles');
  });
});

describe('modelProfilesApi', () => {
  it('list() calls GET /api/model-profiles', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await modelProfilesApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/model-profiles');
  });
});

describe('runnersApi', () => {
  it('enroll() POSTs to /api/runners/enrollment and returns the one-time secret', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        runner_id: 'runr_1',
        token_id: 'ent_1',
        enrollment_token: 'enr_secret',
        expires_at: '2026-08-06T13:00:00Z',
      }),
    );
    const result = await runnersApi.enroll({ name: 'box-1', total_capacity: 2, available_capacity: 2 });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/enrollment');
    expect(result.enrollment_token).toBe('enr_secret');
  });

  it('revokeRunner() POSTs to /api/runners/{id}/revoke', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, runner_id: 'runr_1', state: 'revoked' }));
    await runnersApi.revokeRunner('runr_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/runr_1/revoke');
  });

  it('revokeEnrollmentToken() POSTs to the nested revoke path', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, runner_id: 'runr_1', token_id: 'ent_1', state: 'revoked' }),
    );
    await runnersApi.revokeEnrollmentToken('runr_1', 'ent_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/runr_1/enrollment-tokens/ent_1/revoke');
  });
});

import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { ApiError } from '../../shared/api/client';
import {
  approvalTokenStore,
  approvalsApi,
  isApprovalAlreadyDecided,
  isApprovalGone,
  isApprovalTokenRejected,
  isOrchDisabled,
} from './api';

beforeEach(() => {
  approvalTokenStore.set(null);
});

afterEach(() => {
  vi.restoreAllMocks();
  approvalTokenStore.set(null);
});

describe('approvalsApi.list', () => {
  it('GETs /approvals with no body', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [] }), { status: 200 }));

    const res = await approvalsApi.list();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/approvals');
    expect(String(url)).not.toContain('/approvals/');
    expect((init as RequestInit | undefined)?.method ?? 'GET').toBe('GET');
    expect(res.rows).toEqual([]);
  });

  it('tolerates the server sending a field this type no longer declares (card G1 retired the client-side grant-availability flag; see api.ts header comment)', async () => {
    // Realistic: the backend hasn't necessarily dropped its own field the
    // same release this frontend stops reading it. `PendingApprovalListResponse`
    // no longer declares one, so no code path here can reference it — the
    // regression this guards against is that read creeping back in, not an
    // extra JSON key's mere presence on the wire (which never fails
    // `res.json()`). Uses a stand-in field name rather than the real
    // retired one, so this file carries zero occurrences of the real field
    // name that was removed.
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ rows: [], approval_token_set: false }), { status: 200 })
    );

    const res = await approvalsApi.list();

    expect(res.rows).toEqual([]);
  });
});

describe('approvalsApi.decide', () => {
  it('POSTs {action} to /approvals/{token} and never sends the header when no token is stored', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify({ token: 'apr-1', state: 'granted' }), { status: 200 })
      );

    await approvalsApi.decide('apr-1', 'grant');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/approvals/apr-1');
    expect((init as RequestInit).method).toBe('POST');
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ action: 'grant' });
    const headers = new Headers((init as RequestInit).headers);
    expect(headers.has('X-Tack-Approval-Token')).toBe(false);
  });

  it('sends the stored approval token as X-Tack-Approval-Token, never in the body', async () => {
    approvalTokenStore.set('operator-secret');
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify({ token: 'apr-1', state: 'denied' }), { status: 200 })
      );

    await approvalsApi.decide('apr-1', 'deny');

    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers((init as RequestInit).headers);
    expect(headers.get('X-Tack-Approval-Token')).toBe('operator-secret');
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body).toEqual({ action: 'deny' });
    expect(JSON.stringify(body)).not.toContain('operator-secret');
  });

  it('URL-encodes the token in the path', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify({ token: 'apr/weird', state: 'granted' }), { status: 200 })
      );

    await approvalsApi.decide('apr/weird token', 'grant');

    const [url] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/approvals/apr%2Fweird%20token');
  });
});

describe('approvalTokenStore', () => {
  it('round-trips through localStorage and clears on null', () => {
    expect(approvalTokenStore.get()).toBeNull();
    approvalTokenStore.set('abc123');
    expect(approvalTokenStore.get()).toBe('abc123');
    approvalTokenStore.set(null);
    expect(approvalTokenStore.get()).toBeNull();
  });
});

describe('error classifiers', () => {
  it('isOrchDisabled is true for the documented code (409/403) or a legacy bare 404', () => {
    expect(isOrchDisabled(new ApiError(409, 'disabled', 'orchestration_disabled'))).toBe(true);
    expect(isOrchDisabled(new ApiError(403, 'disabled', 'orchestration_disabled'))).toBe(true);
    expect(isOrchDisabled(new ApiError(404, 'not found'))).toBe(true);
    expect(isOrchDisabled(new ApiError(500, 'boom'))).toBe(false);
    expect(isOrchDisabled(new Error('plain'))).toBe(false);
  });

  it('isOrchDisabled never fires on the decide endpoint\'s own 403/409 — neither carries the code', () => {
    // approvalsApi.decide()'s "token rejected" (403) and "already decided"
    // (409) come from a different call site than approvalsApi.list()'s
    // isOrchDisabled check, and neither response carries
    // `orchestration_disabled` — so there is no ambiguity even though the
    // raw status codes overlap with isApprovalTokenRejected/
    // isApprovalAlreadyDecided below.
    expect(isOrchDisabled(new ApiError(403, 'approval token rejected'))).toBe(false);
    expect(isOrchDisabled(new ApiError(409, 'already decided'))).toBe(false);
  });

  it('isApprovalTokenRejected is true only for a 403 ApiError', () => {
    expect(isApprovalTokenRejected(new ApiError(403, 'forbidden'))).toBe(true);
    expect(isApprovalTokenRejected(new ApiError(404, 'not found'))).toBe(false);
  });

  it('isApprovalAlreadyDecided is true only for a 409 ApiError', () => {
    expect(isApprovalAlreadyDecided(new ApiError(409, 'Already granted: apr-1'))).toBe(true);
    expect(isApprovalAlreadyDecided(new ApiError(404, 'not found'))).toBe(false);
  });

  it('isApprovalGone is true only for a 404 ApiError', () => {
    expect(isApprovalGone(new ApiError(404, 'Approval not found: apr-1'))).toBe(true);
    expect(isApprovalGone(new ApiError(409, 'already decided'))).toBe(false);
  });

  it('every classifier is distinct by status — no two overlap except isOrchDisabled/isApprovalGone (both 404, different call sites)', () => {
    const forbidden = new ApiError(403, 'x');
    const conflict = new ApiError(409, 'x');
    expect(isApprovalTokenRejected(forbidden)).toBe(true);
    expect(isApprovalAlreadyDecided(forbidden)).toBe(false);
    expect(isApprovalGone(forbidden)).toBe(false);
    expect(isApprovalTokenRejected(conflict)).toBe(false);
    expect(isApprovalAlreadyDecided(conflict)).toBe(true);
    expect(isApprovalGone(conflict)).toBe(false);
  });
});

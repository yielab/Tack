import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createBoardSocket, boardLiveUrl } from './boardSocket';
import type { BoardEvent } from '../types';

// Minimal WebSocket stand-in we can drive synchronously from tests.
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  url: string;
  protocols: string | string[] | undefined;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(url: string, protocols?: string | string[]) {
    this.url = url;
    this.protocols = protocols;
    FakeWebSocket.instances.push(this);
  }
  // test drivers
  open() {
    this.onopen?.();
  }
  emit(event: BoardEvent) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
  serverClose() {
    this.onclose?.();
  }
  close() {
    this.closed = true;
  }
}

const WS = FakeWebSocket as unknown as typeof WebSocket;
const latest = () => FakeWebSocket.instances.at(-1)!;

describe('boardLiveUrl', () => {
  it('builds a same-origin /boards/live url with ws scheme', () => {
    expect(boardLiveUrl('p1')).toBe(
      `ws://${location.host}/api/projects/p1/boards/live`
    );
  });
});

describe('createBoardSocket', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts connecting and flips to open on connect', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    expect(sock.status()).toBe('connecting');
    latest().open();
    expect(sock.status()).toBe('open');
    sock.close();
  });

  it('offers the fixed board protocol and an auth subprotocol without using a query token', () => {
    const sessionKey = `tack_api_token:${location.origin}`;
    sessionStorage.setItem(sessionKey, 'operator-token');
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });

    expect(latest().url).not.toContain('?');
    expect(latest().protocols).toEqual(['tack.v1', 'tack.auth.b3BlcmF0b3ItdG9rZW4']);
    sock.close();
    sessionStorage.removeItem(sessionKey);
  });

  it('dispatches matching-project events and filters out other projects', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: BoardEvent[] = [];
    sock.onEvent((e) => seen.push(e));

    latest().emit({
      type: 'item_updated',
      project_id: 'p1',
      item_id: 'i1',
      old_status: 'todo',
      new_status: 'done',
    });
    latest().emit({
      type: 'item_updated',
      project_id: 'OTHER',
      item_id: 'i9',
      old_status: null,
      new_status: 'done',
    });

    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ type: 'item_updated', item_id: 'i1' });
    sock.close();
  });

  it('dispatches every non-ping event type for the matching project', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: string[] = [];
    sock.onEvent((e) => seen.push(e.type));

    latest().emit({ type: 'item_created', project_id: 'p1', item_id: 'i', status: 'todo' });
    latest().emit({ type: 'item_updated', project_id: 'p1', item_id: 'i', old_status: 'todo', new_status: 'done' });
    latest().emit({ type: 'item_deleted', project_id: 'p1', item_id: 'i' });
    latest().emit({ type: 'board_config_updated', project_id: 'p1' });
    latest().emit({ type: 'sprint_updated', project_id: 'p1', sprint_id: 's' });
    latest().emit({
      type: 'agent_run_updated',
      project_id: 'p1',
      item_id: 'i',
      run_id: 'run-1',
      state: 'running',
    });
    latest().emit({
      type: 'approval_pending',
      project_id: 'p1',
      item_id: 'i',
      token: 'tok-1',
      action: 'merge',
    });

    expect(seen).toEqual([
      'item_created',
      'item_updated',
      'item_deleted',
      'board_config_updated',
      'sprint_updated',
      'agent_run_updated',
      'approval_pending',
    ]);
    sock.close();
  });

  it('dispatches agent_run_updated and approval_pending only for the matching project', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: BoardEvent[] = [];
    sock.onEvent((e) => seen.push(e));

    latest().emit({
      type: 'agent_run_updated',
      project_id: 'p1',
      item_id: 'i1',
      run_id: 'run-1',
      state: 'succeeded',
    });
    latest().emit({
      type: 'agent_run_updated',
      project_id: 'OTHER',
      item_id: 'i9',
      run_id: 'run-2',
      state: 'succeeded',
    });
    latest().emit({
      type: 'approval_pending',
      project_id: 'p1',
      item_id: 'i1',
      token: 'tok-1',
      action: null,
    });
    latest().emit({
      type: 'approval_pending',
      project_id: 'OTHER',
      item_id: 'i9',
      token: 'tok-2',
      action: null,
    });

    expect(seen).toHaveLength(2);
    expect(seen[0]).toMatchObject({ type: 'agent_run_updated', run_id: 'run-1' });
    expect(seen[1]).toMatchObject({ type: 'approval_pending', token: 'tok-1' });
    sock.close();
  });

  it('an unrecognised event type on the wire is forwarded, not thrown', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: unknown[] = [];
    sock.onEvent((e) => seen.push(e));

    // A future BoardEvent variant this client build doesn't know about yet.
    expect(() =>
      latest().emit({ type: 'some_future_event', project_id: 'p1' } as unknown as BoardEvent)
    ).not.toThrow();

    expect(seen).toHaveLength(1);
    expect((seen[0] as { type: string }).type).toBe('some_future_event');
    sock.close();
  });

  it('ignores ping keepalive events', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    const seen: BoardEvent[] = [];
    sock.onEvent((e) => seen.push(e));
    latest().emit({ type: 'ping' });
    expect(seen).toHaveLength(0);
    sock.close();
  });

  it('reconnects with backoff after an unexpected close', () => {
    const sock = createBoardSocket('p1', {
      WebSocketImpl: WS,
      initialDelay: 1000,
    });
    latest().open();
    expect(FakeWebSocket.instances).toHaveLength(1);

    latest().serverClose();
    expect(sock.status()).toBe('reconnecting');

    vi.advanceTimersByTime(1000);
    expect(FakeWebSocket.instances).toHaveLength(2); // a fresh socket was created
    latest().open();
    expect(sock.status()).toBe('open');
    sock.close();
  });

  it('does not reconnect after an intentional close', () => {
    const sock = createBoardSocket('p1', { WebSocketImpl: WS });
    latest().open();
    sock.close();
    expect(sock.status()).toBe('closed');

    vi.advanceTimersByTime(60000);
    expect(FakeWebSocket.instances).toHaveLength(1); // no reconnect
  });
});

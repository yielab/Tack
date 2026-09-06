import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider, useExecutionStore } from './executionContext';

const flush = () => new Promise((r) => setTimeout(r, 0));

const disposers: Array<() => void> = [];

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('useExecutionStore', () => {
  it('throws outside a Provider — the same fail-fast contract projectItemsContext establishes', () => {
    let caught: unknown;
    const dispose = render(() => {
      try {
        useExecutionStore();
      } catch (err) {
        caught = err;
      }
      return null;
    }, document.createElement('div'));
    disposers.push(dispose);
    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toMatch(/ExecutionStoreProvider/);
  });
});

describe('ExecutionStoreProvider', () => {
  it('loads the execution list once on mount and shares one store instance across consumers', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [{ request_id: 'req-1', item_id: 'item-1', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' }],
        }),
        { status: 200 },
      ),
    );

    let seenByA: ReturnType<typeof useExecutionStore> | undefined;
    let seenByB: ReturnType<typeof useExecutionStore> | undefined;

    function ConsumerA() {
      seenByA = useExecutionStore();
      return null;
    }
    function ConsumerB() {
      seenByB = useExecutionStore();
      return null;
    }

    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <ConsumerA />
          <ConsumerB />
        </ExecutionStoreProvider>
      ),
      container,
    );
    disposers.push(() => {
      dispose();
      container.remove();
    });

    await flush();
    await flush();

    // Same instance — this is the mechanism behind "every consumer sees one
    // consistent state," inherited by every consumer this Provider serves.
    expect(seenByA).toBe(seenByB);
    expect(seenByA!.requests().has('req-1')).toBe(true);
    expect(fetchMock.mock.calls.some((c) => String(c[0]).includes('/executions'))).toBe(true);
  });

  it('disposes its realtime subscription on unmount without throwing', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <ExecutionStoreProvider>{null}</ExecutionStoreProvider>, container);
    await flush();
    expect(() => {
      dispose();
      container.remove();
    }).not.toThrow();
  });
});

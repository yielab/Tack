import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { useAgentActivityMap } from './useAgentActivityMap';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mockFetch(status: number, body: unknown) {
  return vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(body), { status }));
}

/** Renders `stateFor('item-1')` as text so the hook's reactive output is
 *  observable without a bespoke renderHook harness (none exists in this repo). */
function Host() {
  const map = useAgentActivityMap(() => 'proj-1');
  return <div data-testid="out">{map.stateFor('item-1')?.state ?? 'none'}</div>;
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Host />, container);
  return { container, dispose };
}

describe('useAgentActivityMap', () => {
  it('resolves a badge state for an item present in the bulk response', async () => {
    mockFetch(200, { rows: [{ item_id: 'item-1', remote_status: 'running', attempt: 1, updated_at: '2026-08-01T00:00:00Z' }] });
    const { container, dispose } = mount();
    await flush();
    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('running');
    dispose();
  });

  it('renders nothing (undefined) for an item absent from the bulk response — no agent activity, no chip', async () => {
    mockFetch(200, { rows: [{ item_id: 'item-OTHER', remote_status: 'running', attempt: 1, updated_at: '2026-08-01T00:00:00Z' }] });
    const { container, dispose } = mount();
    await flush();
    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('none');
    dispose();
  });

  it('fails open on a 404 (orchestration disabled) — treated as no rows, not an error', async () => {
    mockFetch(404, { error: { status: 404, message: 'not found' } });
    const { container, dispose } = mount();
    await flush();
    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('none');
    dispose();
  });

  it('fails open on a 500 as well — a badge is never worth degrading the board over', async () => {
    mockFetch(500, { error: { status: 500, message: 'boom' } });
    const { container, dispose } = mount();
    await flush();
    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('none');
    dispose();
  });

  // `orchAvailable()`'s own behavior does not change based on what it's
  // safe to gate on (see the doc comment above `orchAvailable` in
  // `useAgentActivityMap.ts`). This locks in the one thing every remaining
  // caller (e.g. `Sprints.tsx`, gating "is there bulk activity data to show
  // at all") still needs true.
  it('orchAvailable reflects only "did the bulk fetch resolve without error" — false while loading, true once it succeeds', async () => {
    let orchAvailableNow: () => boolean = () => false;
    function HostWithAvailability() {
      const map = useAgentActivityMap(() => 'proj-1');
      orchAvailableNow = map.orchAvailable;
      return <div data-testid="out">{map.stateFor('item-1')?.state ?? 'none'}</div>;
    }
    const container = document.createElement('div');
    document.body.appendChild(container);
    mockFetch(200, { rows: [] });
    expect(orchAvailableNow()).toBe(false);
    const dispose = render(() => <HostWithAvailability />, container);
    await flush();
    expect(orchAvailableNow()).toBe(true);
    dispose();
  });

  it('orchAvailable stays false on a 404 (orchestration disabled) — never a signal to gate a privileged control on', async () => {
    let orchAvailableNow: () => boolean = () => true;
    function HostWithAvailability() {
      const map = useAgentActivityMap(() => 'proj-1');
      orchAvailableNow = map.orchAvailable;
      return <div data-testid="out">{map.stateFor('item-1')?.state ?? 'none'}</div>;
    }
    const container = document.createElement('div');
    document.body.appendChild(container);
    mockFetch(404, { error: { status: 404, message: 'not found' } });
    const dispose = render(() => <HostWithAvailability />, container);
    await flush();
    expect(orchAvailableNow()).toBe(false);
    dispose();
  });

  // Card B4 (Wave 2, realtime broadcast): callers wire `refetch` to
  // `AgentRunUpdated`/`ApprovalPending` WebSocket events so a badge updates
  // without a page refresh — this proves `refetch` actually re-hits the
  // network and the memo picks up the new response.
  it('refetch re-fetches the bulk rows and updates stateFor', async () => {
    let refetchNow: () => void = () => {};
    function HostWithRefetch() {
      const map = useAgentActivityMap(() => 'proj-1');
      refetchNow = map.refetch;
      return <div data-testid="out">{map.stateFor('item-1')?.state ?? 'none'}</div>;
    }
    const container = document.createElement('div');
    document.body.appendChild(container);
    const spy = mockFetch(200, { rows: [] });
    const dispose = render(() => <HostWithRefetch />, container);
    await flush();
    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('none');

    spy.mockResolvedValue(
      new Response(
        JSON.stringify({
          rows: [{ item_id: 'item-1', remote_status: 'failed', attempt: 1, updated_at: '2026-08-01T00:00:00Z' }],
        }),
        { status: 200 }
      )
    );
    refetchNow();
    await flush();

    expect(container.querySelector('[data-testid="out"]')!.textContent).toBe('failed');
    dispose();
  });
});

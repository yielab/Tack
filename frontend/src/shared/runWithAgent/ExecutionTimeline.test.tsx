import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider } from '../state/executionContext';
import ExecutionTimeline from './ExecutionTimeline';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

/** The top-level request `<li>` rows only — `AttemptList.tsx` nests its own
 *  `<li>` elements per attempt inside each request row, so a plain
 *  `querySelectorAll('li')` would pick up both levels and break any test
 *  asserting positional order or count. Scoped to the outer request `<ul>`'s
 *  direct children via `:scope`. */
function requestRows(container: HTMLElement): HTMLLIElement[] {
  return [...container.querySelectorAll<HTMLLIElement>('ul.space-y-3 > li')];
}

const ROWS = [
  { request_id: 'req-running', item_id: 'item-1', state: 'running', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
  { request_id: 'req-needs-operator', item_id: 'item-1', state: 'needs_operator', cancellation_requested_at: null, created_at: '2025-01-02T00:00:00Z' },
  { request_id: 'req-mystery', item_id: 'item-1', state: 'some_future_state', cancellation_requested_at: null, created_at: '2025-01-03T00:00:00Z' },
  { request_id: 'req-other-item', item_id: 'item-OTHER', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
];

const ATTEMPT_ROW = {
  attempt_id: 'att-1',
  request_id: 'req-running',
  attempt_number: 1,
  runner_id: 'runner-1',
  fencing_token: 1,
  state: 'running',
  lease_issued_at: '2025-01-01T00:00:00Z',
  lease_expires_at: '2025-01-01T00:05:00Z',
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
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  model_provenance: null,
  usage_economics: {
    model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
    runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' } },
  },
};

let cancelCalls: string[];
let requeueBody: unknown;

function mockFetch(): typeof fetch {
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith('/cancel') && init?.method === 'POST') {
      cancelCalls.push(url);
      return new Response(JSON.stringify({ protocol_version: 1, request_id: 'req-running', state: 'cancellation_requested' }), { status: 200 });
    }
    if (url.endsWith('/requeue') && init?.method === 'POST') {
      requeueBody = JSON.parse(String(init.body));
      return new Response(
        JSON.stringify({ protocol_version: 1, request_id: 'req-needs-operator', state: 'queued', recovered_from: 'needs_operator', replayed: false }),
        { status: 200 },
      );
    }
    if (url.includes('/attempts') && url.includes('req-running')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [ATTEMPT_ROW] }), { status: 200 });
    }
    if (url.includes('/attempts')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    }
    if (url.includes('/executions')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: ROWS }), { status: 200 });
    }
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function mount(itemId = 'item-1') {
  cancelCalls = [];
  requeueBody = undefined;
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ExecutionTimeline itemId={itemId} />
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('ExecutionTimeline', () => {
  it('shows the empty state for an item with zero requests', async () => {
    const c = mount('item-with-nothing');
    await flush();
    await flush();
    expect(c.textContent).toContain('No execution requests yet');
  });

  it('lists only requests for the given item, newest first, with a state badge', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).not.toContain('req-other-item');
    const ids = requestRows(c).map((li) => li.textContent);
    // newest (req-mystery, 01-03) first, oldest (req-running, 01-01) last.
    expect(ids[0]).toContain('req-mystery');
    expect(ids[ids.length - 1]).toContain('req-running');
    expect(c.textContent).toContain('Running');
    expect(c.textContent).toContain('Needs operator');
  });

  it('an unrecognised lifecycle state renders its raw value with a visible caveat, never throwing', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).toContain('some_future_state');
    expect(c.textContent).toContain('unrecognised state');
  });

  it('fetches and renders real attempts for a request that has some, and an honest "no attempts yet" for one that has none', async () => {
    const c = mount('item-1');
    for (let i = 0; i < 6; i++) await flush();

    const runningRow = requestRows(c).find((li) => li.textContent?.includes('req-running'))!;
    expect(runningRow.textContent).toContain('Attempt #1');
    expect(runningRow.textContent).toContain('runner-1');

    const opRow = requestRows(c).find((li) => li.textContent?.includes('req-needs-operator'))!;
    expect(opRow.textContent).toContain('No attempts yet.');
  });

  it('Cancel is offered for a non-terminal request and calls the store cancel path', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    const runningRow = requestRows(c).find((li) => li.textContent?.includes('req-running'))!;
    const cancelBtn = [...runningRow.querySelectorAll('button')].find((b) => b.textContent === 'Cancel')!;
    cancelBtn.click();
    await flush();
    await flush();
    expect(cancelCalls.some((u) => u.includes('req-running'))).toBe(true);
  });

  it('Reconcile is offered ONLY for a needs_operator request, and requires an explicit recovery key + reason', async () => {
    const c = mount('item-1');
    await flush();
    await flush();

    const runningRow = requestRows(c).find((li) => li.textContent?.includes('req-running'))!;
    expect([...runningRow.querySelectorAll('button')].some((b) => b.textContent?.includes('Reconcile'))).toBe(false);

    const opRow = requestRows(c).find((li) => li.textContent?.includes('req-needs-operator'))!;
    const reconcileBtn = [...opRow.querySelectorAll('button')].find((b) => b.textContent?.includes('Reconcile'))!;
    reconcileBtn.click();
    await flush();

    const confirmBtn = [...opRow.querySelectorAll('button')].find((b) => b.textContent === 'Confirm requeue') as HTMLButtonElement;
    // No silent auto-retry: the confirm control starts disabled until both
    // fields are filled by hand.
    expect(confirmBtn.disabled).toBe(true);

    const inputs = [...opRow.querySelectorAll('input[type="text"], input:not([type])')] as HTMLInputElement[];
    expect(inputs.length).toBeGreaterThanOrEqual(2);
    inputs[0].value = 'recovery-key-123';
    inputs[0].dispatchEvent(new Event('input', { bubbles: true }));
    inputs[1].value = 'confirmed safe to requeue';
    inputs[1].dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    expect(confirmBtn.disabled).toBe(false);
    confirmBtn.click();
    await flush();
    await flush();

    expect(requeueBody).toEqual({ recovery_key: 'recovery-key-123', reason: 'confirmed safe to requeue' });
  });
});

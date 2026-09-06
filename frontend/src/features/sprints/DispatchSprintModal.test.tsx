import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DispatchSprintModal from './DispatchSprintModal';
import type { Sprint } from '../../shared/types';

// The modal is the confirmation gate for a privileged, sprint-wide action —
// the dry-run preview must show
// BEFORE any real dispatch is possible, and the outcome taxonomy must never
// be flattened in the post-dispatch summary. `Modal` renders via `<Portal>`
// onto `document.body` (see `shared/ui/Modal.tsx`), so every query below
// goes through `document`, not the render-target container.
//
// Mock payloads below match the REAL contract, reconciled
// against `docs/openapi.json`: `max_in_flight` is a query
// parameter, every sprint item is always present with an `order` and a
// `decision`, and the response carries a server-computed `summary`.

const SPRINT: Sprint = {
  id: 'sprint-1',
  project_id: 'p1',
  name: 'Sprint 7',
  status: 'active',
  goal: null,
  start_date: null,
  end_date: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

const flush = () => new Promise((r) => setTimeout(r, 0));
async function waitFor(predicate: () => boolean, timeoutMs = 3000) {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error('waitFor timed out');
    await flush();
  }
}

const disposers: Array<() => void> = [];

function mount(props: { sprint: Sprint | null; onClose: () => void; onDispatched?: () => void }) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <DispatchSprintModal {...props} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

function dialog(): HTMLElement {
  return document.querySelector('[role="dialog"]') as HTMLElement;
}

describe('DispatchSprintModal', () => {
  it('renders nothing when sprint is null', () => {
    mount({ sprint: null, onClose: () => {} });
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('fetches the dry-run plan with NO query string on open (no cap override yet), and shows the would-dispatch preview, never dispatching by itself', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          sprint_id: 'sprint-1',
          max_in_flight: 5,
          summary: {
            total: 3,
            dispatched: 0,
            waiting_approval: 0,
            blocked: 0,
            already_in_flight: 0,
            waiting_on_dependencies: 1,
            not_eligible: 0,
            no_dispatch_policy: 0,
            would_dispatch: 2,
            errored: 0,
          },
          items: [
            { item_id: 'a', title: 'Design schema', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
            { item_id: 'b', title: 'Build API', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
            { item_id: 'c', title: 'Write docs', status: 'Ready', order: 2, decision: 'waiting_on_dependencies', blocked_by: ['a'], policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          ],
        }),
        { status: 200 },
      ),
    );

    mount({ sprint: SPRINT, onClose: () => {} });
    await flush();
    await flush();

    // Fetched the dry-run endpoint with NO query string (no override given),
    // and never a real dispatch.
    const dryRunCall = fetchMock.mock.calls.find((c) => String(c[0]).includes('/sprints/sprint-1/dispatch/dry-run'));
    expect(dryRunCall).toBeTruthy();
    expect(String(dryRunCall![0])).not.toContain('?');
    expect(fetchMock.mock.calls.every((c) => (c[1] as RequestInit | undefined)?.method !== 'POST')).toBe(true);

    const d = dialog();
    expect(d.textContent).toContain('Design schema');
    expect(d.textContent).toContain('Build API');
    // The item that isn't ready is named, with why, not silently hidden.
    expect(d.textContent).toContain('1 item not dispatched this run');
    expect(d.textContent).toContain('Write docs');
    expect(d.textContent).toContain('Waiting on dependencies');
    // Confirm button reflects the real would-dispatch count (2), not the sprint's total (3).
    expect(d.textContent).toContain('Confirm dispatch (2)');
    // The cap field is pre-filled from the server's resolved default.
    const capInput = d.querySelector<HTMLInputElement>('input[type="number"]')!;
    expect(capInput.value).toBe('5');
  });

  it('shows the disabled empty state on a 404 (orchestration off) and never offers to confirm', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { message: 'not found' } }), { status: 404 }),
    );

    mount({ sprint: SPRINT, onClose: () => {} });
    await waitFor(() => !dialog().textContent!.includes('Loading dispatch plan'));

    const d = dialog();
    expect(d.textContent).toContain('Agent-fleet orchestration is disabled');
    expect(d.querySelector('button')?.textContent).not.toContain('Confirm dispatch');
  });

  it('confirming calls POST /sprints/{id}/dispatch?max_in_flight=N (query param, no body) and shows per-decision counts straight off the server summary, never folding waiting_approval into dispatched', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes('/dry-run')) {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              sprint_id: 'sprint-1',
              max_in_flight: 2,
              summary: {
                total: 2,
                dispatched: 0,
                waiting_approval: 0,
                blocked: 0,
                already_in_flight: 0,
                waiting_on_dependencies: 0,
                not_eligible: 0,
                no_dispatch_policy: 0,
                would_dispatch: 2,
                errored: 0,
              },
              items: [
                { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
                { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
              ],
            }),
            { status: 200 },
          ),
        );
      }
      if (url.includes('/sprints/sprint-1/dispatch') && init?.method === 'POST') {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              sprint_id: 'sprint-1',
              max_in_flight: 2,
              summary: {
                total: 2,
                dispatched: 1,
                waiting_approval: 1,
                blocked: 0,
                already_in_flight: 0,
                waiting_on_dependencies: 0,
                not_eligible: 0,
                no_dispatch_policy: 0,
                would_dispatch: 0,
                errored: 0,
              },
              items: [
                { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'dispatched', blocked_by: null, policy_id: null, message: null, status_applied: 'In Progress', status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
                { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'waiting_approval', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: 'tok-1', current_status: null, dispatch_from: null, error: null, task: null },
              ],
            }),
            { status: 200 },
          ),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const onDispatched = vi.fn();
    mount({ sprint: SPRINT, onClose: () => {}, onDispatched });
    await flush();
    await flush();

    const confirmBtn = [...dialog().querySelectorAll('button')].find((b) => b.textContent?.includes('Confirm dispatch'))!;
    confirmBtn.click();
    await flush();
    await flush();

    const dispatchCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).includes('/sprints/sprint-1/dispatch') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(dispatchCall).toBeTruthy();
    // Query param, not a JSON body — the real contract; a body here would be silently ignored server-side.
    expect(String(dispatchCall![0])).toContain('/sprints/sprint-1/dispatch?max_in_flight=2');
    expect((dispatchCall![1] as RequestInit).body).toBeUndefined();
    expect(onDispatched).toHaveBeenCalledTimes(1);

    const text = dialog().textContent!;
    expect(text).toContain('1 dispatched');
    expect(text).toContain('1 waiting on approval');
    // Never a single merged "2 dispatched".
    expect(text).not.toContain('2 dispatched');
  });

  it('editing the in-flight cap overrides the value sent to the real dispatch call', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes('/dry-run')) {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              sprint_id: 'sprint-1',
              max_in_flight: 5,
              summary: { total: 1, dispatched: 0, waiting_approval: 0, blocked: 0, already_in_flight: 0, waiting_on_dependencies: 0, not_eligible: 0, no_dispatch_policy: 0, would_dispatch: 1, errored: 0 },
              items: [
                { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
              ],
            }),
            { status: 200 },
          ),
        );
      }
      if (init?.method === 'POST') {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              sprint_id: 'sprint-1',
              max_in_flight: 9,
              summary: { total: 1, dispatched: 1, waiting_approval: 0, blocked: 0, already_in_flight: 0, waiting_on_dependencies: 0, not_eligible: 0, no_dispatch_policy: 0, would_dispatch: 0, errored: 0 },
              items: [
                { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'dispatched', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
              ],
            }),
            { status: 200 },
          ),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    mount({ sprint: SPRINT, onClose: () => {} });
    await flush();
    await flush();

    const capInput = dialog().querySelector<HTMLInputElement>('input[type="number"]')!;
    capInput.value = '9';
    capInput.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    const confirmBtn = [...dialog().querySelectorAll('button')].find((b) => b.textContent?.includes('Confirm dispatch'))!;
    confirmBtn.click();
    await flush();
    await flush();

    const fetchMock = vi.mocked(globalThis.fetch);
    const dispatchCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).includes('/sprints/sprint-1/dispatch') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(String(dispatchCall![0])).toContain('max_in_flight=9');
  });
});

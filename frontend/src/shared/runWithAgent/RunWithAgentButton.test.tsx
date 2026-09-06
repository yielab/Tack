import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, useSearchParams } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import { EXECUTION_LIST_PRELOAD_LIMIT } from '../execution/store';
import RunWithAgentButton from './RunWithAgentButton';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

/** Last search params seen by a component mounted inside the test router —
 *  `MemoryRouter`'s history is in-memory, not `window.location`, so this is
 *  how a test observes where `setSearchParams` navigated (mirrors
 *  `ItemDetailDrawer.dispatch.test.tsx`'s own `Host`-reads-params pattern). */
let lastSearchParams: Record<string, string | string[] | undefined> = {};
function ParamsProbe() {
  const [params] = useSearchParams();
  lastSearchParams = params;
  return null;
}

/** One active runner reporting `codex`/`openai` — enough that the modal's
 *  "agent execution is off" state never intercepts a test that isn't
 *  specifically about it (that state is `RunWithAgentModal.test.tsx`'s own
 *  concern, not this button/chip's). */
const RUNNER_CAPS = {
  harnesses: [{ harness_kind: 'codex', installed_version: '1.0.0', probe_error: null, probed_at: '2026-08-06T12:00:00Z', model_combinations: [] }],
  concurrency: { total: 1, available: 1 },
  limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
  features: {},
};
const RUNNER = {
  runner_id: 'runner-1', name: 'Dev laptop', state: 'active', labels: null, labels_raw: '{}',
  total_capacity: 1, available_capacity: 1, capability_snapshot: RUNNER_CAPS,
  capability_snapshot_raw: JSON.stringify(RUNNER_CAPS), protocol_version: 1, runner_version: '0.1.0',
  last_heartbeat_at: '2026-08-06T12:00:00Z', revoked_at: null, fleet_ids: [],
  created_at: '2026-08-06T12:00:00Z', updated_at: '2026-08-06T12:00:00Z',
};

function mockFetch(opts: { executions?: unknown[] } = {}): typeof fetch {
  const executions = opts.executions ?? [];
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/runner-fleets')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/runners')) return new Response(JSON.stringify({ protocol_version: 1, data: [RUNNER] }), { status: 200 });
    if (url.includes('/projects/')) return new Response(JSON.stringify({ id: 'project-1', name: 'P', default_model: null }), { status: 200 });
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: executions }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function mount(
  props: { compact?: boolean; showStateChip?: boolean } = {},
  fetchOpts: Parameters<typeof mockFetch>[0] = {},
) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch(fetchOpts));
  lastSearchParams = {};
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <MemoryRouter>
          <Route
            path="/"
            component={() => (
              <>
                <ParamsProbe />
                <RunWithAgentButton itemId="item-1" itemTitle="Fix login bug" projectId="project-1" {...props} />
              </>
            )}
          />
        </MemoryRouter>
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

describe('RunWithAgentButton', () => {
  it('compact mode renders an icon-only trigger with an item-specific accessible name — distinct from DispatchCardMenu\'s "⋮" kebab', () => {
    const c = mount({ compact: true });
    const trigger = c.querySelector('button[aria-label="Run with agent: Fix login bug"]');
    expect(trigger).toBeTruthy();
    expect(trigger?.textContent).toBe('▶');
    expect(c.querySelector('[aria-haspopup="menu"]')).toBeNull();
  });

  it('labeled mode renders a "Run with agent" button', () => {
    const c = mount({ compact: false });
    expect(c.textContent).toContain('Run with agent');
  });

  it('clicking the trigger opens the shared modal, titled with the item', async () => {
    const c = mount({ compact: true });
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).toBeTruthy();
    expect(dialog?.textContent).toContain('Run with agent: Fix login bug');
  });

  it('Cancel closes the modal', async () => {
    const c = mount({ compact: true });
    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeTruthy();
    const cancelBtn = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Cancel');
    cancelBtn!.click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('a click on the trigger does not bubble to a parent click handler (Board/Sprint card click-to-open)', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
    const parentClick = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <MemoryRouter>
            <Route
              path="/"
              component={() => (
                <div onClick={parentClick}>
                  <RunWithAgentButton itemId="item-1" itemTitle="Fix login bug" projectId="project-1" compact />
                </div>
              )}
            />
          </MemoryRouter>
        </ExecutionStoreProvider>
      ),
      container,
    );
    disposers.push(() => {
      dispose();
      container.remove();
    });
    (container.querySelector('button') as HTMLButtonElement).click();
    await flush();
    expect(parentClick).not.toHaveBeenCalled();
  });

  it('showStateChip renders the item\'s most recent execution state and clicking it opens the Execution tab, without opening the modal', async () => {
    const c = mount(
      { compact: true, showStateChip: true },
      { executions: [{ request_id: 'req-1', item_id: 'item-1', state: 'queued', cancellation_requested_at: null, created_at: '2026-01-01T00:00:00Z' }] },
    );
    await flush();
    const chip = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Queued');
    expect(chip).toBeTruthy();
    chip!.click();
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(lastSearchParams.tab).toBe('execution');
    expect(lastSearchParams.item).toBe('item-1');
  });

  it('showStateChip renders nothing when the item has no execution requests', async () => {
    const c = mount({ compact: true, showStateChip: true });
    await flush();
    expect([...c.querySelectorAll('button')].some((b) => b.textContent === 'Queued')).toBe(false);
  });

  it('showStateChip renders an explicit "Unknown" chip, not silence, when the item is absent from a preload that hit its row cap', async () => {
    // The preload comes back at exactly EXECUTION_LIST_PRELOAD_LIMIT rows,
    // none belonging to "item-1" — the shape a real install produces once
    // it has more execution requests than the preload's cap covers, with
    // this item's own most recent one older than every row returned.
    const full = Array.from({ length: EXECUTION_LIST_PRELOAD_LIMIT }, (_, i) => ({
      request_id: `req-${i}`,
      item_id: `other-item-${i}`,
      state: 'queued',
      cancellation_requested_at: null,
      created_at: '2026-01-01T00:00:00Z',
    }));
    const c = mount({ compact: true, showStateChip: true }, { executions: full });
    await flush();
    const chip = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Unknown');
    expect(chip).toBeTruthy();
    expect([...c.querySelectorAll('button')].some((b) => b.textContent === 'Queued')).toBe(false);
  });

  it('showStateChip still renders nothing (not "Unknown") when the item has no executions and the preload came back short of its cap', async () => {
    const c = mount({ compact: true, showStateChip: true }, { executions: [] });
    await flush();
    expect([...c.querySelectorAll('button')].some((b) => b.textContent === 'Unknown')).toBe(false);
  });

  it('a screen of many cards, each with showStateChip, still issues exactly one /executions fetch — the shared preload this card measures, not one per card', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch({ executions: [] }));
    const container = document.createElement('div');
    document.body.appendChild(container);
    const ITEM_COUNT = 12; // stand-in for "a screen of items" on a real Board
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <MemoryRouter>
            <Route
              path="/"
              component={() => (
                <>
                  {Array.from({ length: ITEM_COUNT }, (_, i) => (
                    <RunWithAgentButton itemId={`item-${i}`} itemTitle={`Item ${i}`} projectId="project-1" compact showStateChip />
                  ))}
                </>
              )}
            />
          </MemoryRouter>
        </ExecutionStoreProvider>
      ),
      container,
    );
    disposers.push(() => {
      dispose();
      container.remove();
    });
    await flush();
    const executionCalls = fetchMock.mock.calls.filter(([input]) => String(input).includes('/executions'));
    expect(executionCalls).toHaveLength(1);
    // The one call asks for the wider preload bound, not the server's
    // smaller default — the "after" half of this card's measurement.
    expect(String(executionCalls[0][0])).toBe(`/api/executions?limit=${EXECUTION_LIST_PRELOAD_LIMIT}`);
  });
});

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, useSearchParams } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import type { Resource } from 'solid-js';
import ItemDetailDrawer from './ItemDetailDrawer';

// Dispatch-to-agents on the item-detail drawer. A separate file from
// `ItemDetailDrawer.test.tsx` so each mock fetch
// map can vary the `/agent-activity` and `/dispatch` responses per test
// without complicating the base drawer test's fixtures.
//
// The drawer's content is rendered via `<Portal>` (see `shared/ui/Drawer.tsx`)
// straight onto `document.body`, NOT inside the render-target `container` —
// so every query below goes through `document`, matching
// `ItemDetailDrawer.test.tsx`'s own convention (`document.querySelector('[role="dialog"]')`).

const ITEM = {
  id: 'item-1',
  project_id: 'p1',
  title: 'My item',
  item_type: 'task',
  status: 'todo',
  priority: 'medium',
  tags: [],
  estimate_unit: 'story_points',
  sort_order: 0,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
};

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => null) as unknown as Resource<null>,
  workflow: () => ({
    workflow_type: 'kanban',
    statuses: [
      { name: 'todo', category: 'todo', order: 0 },
      { name: 'done', category: 'done', order: 1 },
    ],
  }),
  vocabulary: () => ({}),
  refetch: () => {},
};

const flush = () => new Promise((r) => setTimeout(r, 0));

let dispatchResponse: unknown;
let agentActivityStatus: number;

function mockFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input);
  if (url.endsWith('/api/items/item-1')) {
    return Promise.resolve(
      new Response(JSON.stringify({ item: ITEM, roles: [], dependencies: [] }), {
        status: 200,
        headers: { ETag: '"item-1:1"' },
      }),
    );
  }
  if (url.includes('/agent-activity')) {
    return Promise.resolve(
      new Response(
        agentActivityStatus === 200
          ? JSON.stringify({ attempts: [], approvals: [] })
          : JSON.stringify({ error: { message: 'not found' } }),
        { status: agentActivityStatus },
      ),
    );
  }
  if (url.endsWith('/api/items/item-1/dispatch') && init?.method === 'POST') {
    return Promise.resolve(new Response(JSON.stringify(dispatchResponse), { status: 200 }));
  }
  if (url.includes('/sprints')) {
    return Promise.resolve(new Response(JSON.stringify([]), { status: 200 }));
  }
  // `ExecutionStoreProvider` loads this once on mount.
  if (url.includes('/executions')) {
    return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
  }
  return Promise.resolve(new Response(JSON.stringify({}), { status: 200 }));
}

beforeEach(() => {
  agentActivityStatus = 200;
  dispatchResponse = { outcome: 'dispatched', task: null };
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch as typeof fetch);
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function Host() {
  const [, setSearchParams] = useSearchParams();
  return (
    <>
      <button data-testid="open" onClick={() => setSearchParams({ item: 'item-1' })}>
        open
      </button>
      <ItemDetailDrawer />
    </>
  );
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ProjectContext.Provider value={projectValue}>
          <MemoryRouter>
            <Route path="/" component={Host} />
          </MemoryRouter>
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  return { container, dispose };
}

async function openDrawer(container: HTMLElement) {
  container.querySelector<HTMLButtonElement>('[data-testid="open"]')!.click();
  await flush();
  await flush();
}

function findDispatchButton(): HTMLButtonElement | undefined {
  return [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Dispatch to agents'));
}

describe('ItemDetailDrawer — dispatch to agents', () => {
  it('shows the "Dispatch to agents" control once the agent-activity probe succeeds (orchestration enabled)', async () => {
    const { container, dispose } = mount();
    await openDrawer(container);

    expect(document.querySelector('[role="dialog"]')).toBeTruthy();
    expect(findDispatchButton()).toBeTruthy();

    dispose();
  });

  it('renders no dispatch control at all when the probe 404s (orchestration disabled)', async () => {
    agentActivityStatus = 404;
    const { container, dispose } = mount();
    await openDrawer(container);
    await flush();
    await flush();

    expect(document.querySelector('[role="dialog"]')).toBeTruthy();
    expect(findDispatchButton()).toBeUndefined();

    dispose();
  });

  it('a successful dispatch calls POST /items/{id}/dispatch and shows a "Dispatched" outcome note', async () => {
    dispatchResponse = {
      outcome: 'dispatched',
      task: {
        remote_task_id: 't1',
        remote_status: 'pending',
        attempt: 1,
        dispatched_at: '2026-01-01T00:00:00Z',
        trusted: true,
      },
    };
    const { container, dispose } = mount();
    await openDrawer(container);

    findDispatchButton()!.click();
    await flush();
    await flush();

    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog.textContent).toContain('Dispatched');

    dispose();
  });

  it('a waiting_approval outcome is shown distinctly — never as "Dispatched"', async () => {
    dispatchResponse = {
      outcome: 'waiting_approval',
      task: {
        remote_task_id: 't1',
        remote_status: 'waiting_approval',
        attempt: 1,
        dispatched_at: '2026-01-01T00:00:00Z',
        trusted: true,
      },
      approval_token: 'tok-1',
    };
    const { container, dispose } = mount();
    await openDrawer(container);

    findDispatchButton()!.click();
    await flush();
    await flush();

    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog.textContent).toContain('Waiting on approval');
    expect(dialog.textContent).not.toContain('Dispatched');

    dispose();
  });

  it('a blocked outcome names the policy that fired', async () => {
    dispatchResponse = {
      outcome: 'blocked',
      task: null,
      policy_id: 'prompt-injection',
      message: 'destructive shell command in task description',
    };
    const { container, dispose } = mount();
    await openDrawer(container);

    findDispatchButton()!.click();
    await flush();
    await flush();

    const dialog = document.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog.textContent).toContain('Blocked');
    expect(dialog.textContent).toContain('prompt-injection');

    dispose();
  });
});

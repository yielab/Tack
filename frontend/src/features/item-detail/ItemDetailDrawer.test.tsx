import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, useSearchParams } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import type { Resource } from 'solid-js';
import ItemDetailDrawer from './ItemDetailDrawer';

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

let fetchMock: ReturnType<typeof vi.spyOn>;
const flush = () => new Promise((r) => setTimeout(r, 0));

function jsonOf(url: string): unknown {
  // GET /api/items/{id} returns the detail envelope, matching the real handler
  // (crates/tack-api/src/handlers/items.rs) that api.items.get() unwraps.
  if (url.endsWith('/api/items/item-1')) return { item: ITEM, roles: [], dependencies: [] };
  if (url.includes('/sprints')) return [];
  // `ExecutionStoreProvider` loads this once on mount —
  // an empty, well-formed list so `store.ts#loadList` has real data to
  // iterate over rather than tripping on an undefined `data.data`.
  if (url.includes('/executions')) return { protocol_version: 1, data: [] };
  return {};
}

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
    const url = String(input);
    return Promise.resolve(
      new Response(JSON.stringify(jsonOf(url)), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ETag: '"item-1:1"' },
      }),
    );
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

/** Host renders the drawer plus buttons to drive the ?item= param. */
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

describe('ItemDetailDrawer', () => {
  it('opens from ?item=, populates from GET /items/{id}, and ESC closes + clears the param', async () => {
    const { container, dispose } = mount();

    // Closed initially.
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    container.querySelector<HTMLButtonElement>('[data-testid="open"]')!.click();
    await flush();

    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog).toBeTruthy();
    // Title input populated from the fetched item.
    const title = document.querySelector<HTMLInputElement>('input[aria-label="Item title"]');
    expect(title?.value).toBe('My item');
    // It fetched the item by id.
    expect(fetchMock.mock.calls.some((c) => String(c[0]).endsWith('/api/items/item-1'))).toBe(true);

    // ESC closes and clears the query param → drawer gone.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush();
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    dispose();
  });

  it('PATCHes /items/{id} when a header field changes', async () => {
    const { container, dispose } = mount();
    container.querySelector<HTMLButtonElement>('[data-testid="open"]')!.click();
    await flush();

    // Status is a row of pills; click the "done" pill.
    const donePill = [...document.querySelectorAll<HTMLButtonElement>('[role="group"][aria-label="Status"] button')]
      .find((b) => b.textContent === 'done')!;
    donePill.click();
    await flush();

    const patch = fetchMock.mock.calls.find(
      (c) => (c[1] as RequestInit)?.method === 'PATCH' && String(c[0]).endsWith('/api/items/item-1'),
    );
    expect(patch).toBeTruthy();
    expect(JSON.parse((patch![1] as RequestInit).body as string)).toEqual({ status: 'done' });

    dispose();
  });
});

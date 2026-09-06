import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ProjectItemsProvider } from '../../shared/state/projectItemsContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import type { Resource } from 'solid-js';
import Sprints from './Sprints';

// Regression test: the Sprints view's "Run
// sprint" button once resolved to 2 elements in a Playwright strict-mode
// locator. The root cause (confirmed by reading `Sprints.tsx` itself) is
// NOT a duplicate render for
// one sprint — `Sprints.tsx` renders exactly one "Run sprint" button per
// sprint, inside a single `<For each={activeSprints()}>`. It's two
// genuinely different, correctly-rendered buttons for two different
// sprints, both eligible (non-closed, has items, orchestration available).
// The Playwright helper that hit this (`createSprintWithItem`) always
// creates a fresh sprint against a project reused across an entire e2e spec
// file, so more than one eligible sprint accumulating there is expected,
// not a bug in the component.
//
// The fix: each button's accessible name now includes its own sprint's name
// (`Run sprint: <name>`), so screen-reader/voice-control users — and a
// `getByRole('button', { name })` locator — can tell the two apart, rather
// than sharing one ambiguous name. This test pins that behavior so it can't
// regress.

const flush = () => new Promise((r) => setTimeout(r, 0));

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => null) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
};

function item(id: string, sprintId: string) {
  return {
    id,
    project_id: 'p1',
    title: `Item ${id}`,
    item_type: 'task',
    status: 'todo',
    priority: 'medium',
    tags: [],
    estimate_unit: 'story_points',
    sort_order: 0,
    sprint_id: sprintId,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  };
}

function sprint(id: string, name: string) {
  return {
    id,
    project_id: 'p1',
    name,
    status: 'planning',
    goal: null,
    start_date: null,
    end_date: null,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
  };
}

let sprintsBody: unknown[];
let itemsBody: unknown[];

function mockFetch(input: RequestInfo | URL): Promise<Response> {
  const url = String(input);
  if (url.includes('/agent-activity')) {
    return Promise.resolve(new Response(JSON.stringify({ rows: [] }), { status: 200 }));
  }
  if (url.includes('/sprints')) {
    return Promise.resolve(new Response(JSON.stringify(sprintsBody), { status: 200 }));
  }
  if (url.includes('/items')) {
    return Promise.resolve(
      new Response(
        JSON.stringify({ data: itemsBody, total: itemsBody.length, page: 1, per_page: 200 }),
        { status: 200 },
      ),
    );
  }
  // `ExecutionStoreProvider` loads this once on mount.
  if (url.includes('/executions')) {
    return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
  }
  return Promise.resolve(new Response(JSON.stringify({}), { status: 200 }));
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

// `Sprints.tsx` reads `params.id` (route param), and `ProjectItemsProvider`
// keys its item fetch off the same param — so the router needs to be
// mounted already pointed at `/projects/p1/sprint`, not just at `/`.
// `MemoryRouter`'s default history always starts at `/`, and there's no
// documented "initial URL" prop, so seed one directly (mirrors
// `@solidjs/router`'s own `createMemoryHistory`, just with a non-`/` first
// entry) and pass it via the supported `history` prop.
function createMemoryHistoryAt(initialUrl: string) {
  const entries = [initialUrl];
  let index = 0;
  const listeners: Array<(v: string) => void> = [];
  return {
    get: () => entries[index],
    set: ({ value, replace }: { value: string; scroll?: boolean; replace?: boolean }) => {
      if (replace) entries[index] = value;
      else {
        entries.splice(index + 1, entries.length - index, value);
        index++;
      }
      listeners.forEach((l) => l(value));
    },
    back: () => {},
    forward: () => {},
    go: () => {},
    listen: (listener: (v: string) => void) => {
      listeners.push(listener);
      return () => {
        const i = listeners.indexOf(listener);
        if (i >= 0) listeners.splice(i, 1);
      };
    },
  };
}

function mountAtProject(projectId: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ProjectContext.Provider value={projectValue}>
          <MemoryRouter history={createMemoryHistoryAt(`/projects/${projectId}/sprint`)}>
            <Route
              path="/projects/:id/sprint"
              component={() => (
                <ProjectItemsProvider>
                  <Sprints />
                </ProjectItemsProvider>
              )}
            />
          </MemoryRouter>
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  return { container, dispose };
}

function runSprintButtons(): HTMLButtonElement[] {
  return [...document.querySelectorAll('button')].filter((b) =>
    b.textContent?.trim() === 'Run sprint',
  );
}

describe('Sprints — "Run sprint" accessible naming', () => {
  it('renders one distinctly-named button per eligible sprint (not a duplicate for one sprint)', async () => {
    sprintsBody = [sprint('s1', 'Sprint Alpha'), sprint('s2', 'Sprint Beta')];
    itemsBody = [item('i1', 's1'), item('i2', 's2')];
    vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch as typeof fetch);

    const { container, dispose } = mountAtProject('p1');
    await flush();
    await flush();
    await flush();

    const buttons = runSprintButtons();
    expect(buttons).toHaveLength(2);

    const names = buttons.map((b) => b.getAttribute('aria-label'));
    expect(names).toContain('Run sprint: Sprint Alpha');
    expect(names).toContain('Run sprint: Sprint Beta');
    // Distinct — the whole point of the fix.
    expect(new Set(names).size).toBe(2);
    // Each accessible name still contains the visible label text (WCAG 2.5.3
    // Label in Name) so voice-control users saying "click run sprint" still
    // match.
    for (const name of names) expect(name).toMatch(/^Run sprint/);

    dispose();
    container.remove();
  });

  it('still names the button after its sprint even when there is only one', async () => {
    sprintsBody = [sprint('s1', 'Sprint Solo')];
    itemsBody = [item('i1', 's1')];
    vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch as typeof fetch);

    const { container, dispose } = mountAtProject('p1');
    await flush();
    await flush();
    await flush();

    const buttons = runSprintButtons();
    expect(buttons).toHaveLength(1);
    expect(buttons[0].getAttribute('aria-label')).toBe('Run sprint: Sprint Solo');

    dispose();
    container.remove();
  });
});

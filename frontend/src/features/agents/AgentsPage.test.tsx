import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import AgentsPage from './AgentsPage';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const LOCAL_RUNNER_OFF = { enabled: false, state: 'stopped', since: null, catalog: { status: 'not_configured' } };

function mockFetch(overrides: Record<string, unknown> = {}) {
  const responses: Record<string, unknown> = {
    '/api/local-runner': LOCAL_RUNNER_OFF,
    '/api/local-runner/secrets': { data: [] },
    '/api/runners': { protocol_version: 1, data: [] },
    '/api/projects': [{ id: 'p1', name: 'Demo project', default_model: null }],
    '/api/executions': { protocol_version: 1, data: [] },
    ...overrides,
  };
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    const urlPath = url.split('?')[0]; // match on path alone — the execution preload now appends `?limit=`
    for (const [path, body] of Object.entries(responses)) {
      if (urlPath.endsWith(path)) {
        return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
      }
    }
    if (/\/api\/projects\/[^/]+$/.test(url)) {
      return Promise.resolve(new Response(JSON.stringify({ id: 'p1', name: 'Demo project', default_model: null }), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route
          path="/"
          component={() => (
            <ExecutionStoreProvider>
              <AgentsPage />
            </ExecutionStoreProvider>
          )}
        />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

/** Text a reader would actually see — strips copy-paste command blocks
 *  (`<pre>`/`<code>`), which necessarily name real CLI commands
 *  (`tack runner secret set`, `npm install -g @openai/codex`) the vendor or
 *  this project's own CLI define, not narrative vocabulary. Advanced's own
 *  content is never in this text at all while collapsed — it isn't
 *  rendered, not merely hidden. */
function visibleProseText(container: HTMLElement): string {
  const clone = container.cloneNode(true) as HTMLElement;
  clone.querySelectorAll('pre, code').forEach((el) => el.remove());
  return clone.textContent ?? '';
}

const FORBIDDEN_VOCABULARY = ['runner', 'fleet', 'enroll', 'heartbeat', 'capacity', 'lease', 'harness'];

describe('AgentsPage — default screen vocabulary', () => {
  it('never renders runner/fleet/enroll/heartbeat/capacity/lease/harness while Advanced is collapsed', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    const text = visibleProseText(container).toLowerCase();
    const hits = FORBIDDEN_VOCABULARY.filter((word) => text.includes(word));
    expect(hits, `forbidden vocabulary found on the default screen: ${hits.join(', ')}`).toEqual([]);
  });

  it('Advanced starts collapsed, and its own vocabulary appears once opened', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).not.toContain('Enroll a runner');
    const advancedButton = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Advanced'),
    )!;
    advancedButton.click();
    await flush();
    expect(container.textContent).toContain('Enroll a runner');
  });
});

describe('AgentsPage — composition', () => {
  it('renders every numbered step and mounts both ExecutionToggle and ProviderKeyPanel', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('Agent execution on this machine'); // ExecutionToggle
    expect(container.textContent).toContain('Vercel AI Gateway key'); // ProviderKeyPanel
    expect(container.textContent).toContain('Agents on this machine');
    expect(container.textContent).toContain('Default model');
    expect(container.textContent).toContain('Test run');
  });

  it('shows both known harnesses as "Not found" with an install command when agent execution is off', async () => {
    mockFetch();
    const { container } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain("Turn on agent execution above to see what's installed here.");
  });

  it('lists a harness as installed, with its version, once this machine has an active runner reporting it', async () => {
    mockFetch({
      '/api/local-runner': { enabled: true, state: 'running', since: '2026-01-01T00:00:00Z', catalog: { status: 'not_configured' } },
      '/api/runners': {
        protocol_version: 1,
        data: [
          {
            runner_id: 'runr_1',
            name: 'local-abc',
            state: 'active',
            labels: null,
            labels_raw: '{}',
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: {
              harnesses: [
                { harness_kind: 'codex', installed_version: '9.9.2', probe_error: null, probed_at: '', model_combinations: [] },
                { harness_kind: 'claude-code', installed_version: null, probe_error: '`claude` was not found on PATH', probed_at: '', model_combinations: [] },
              ],
            },
            capability_snapshot_raw: '{}',
            protocol_version: 1,
            runner_version: '0.1.0',
            last_heartbeat_at: null,
            revoked_at: null,
            fleet_ids: [],
            created_at: '',
            updated_at: '',
          },
        ],
      },
    });
    const { container } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Installed v9.9.2');
    expect(container.textContent).toContain('Not found');
    expect(container.textContent).toContain('npm install -g @anthropic-ai/claude-code');
  });
});

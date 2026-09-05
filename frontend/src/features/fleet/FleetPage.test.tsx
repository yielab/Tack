import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import FleetPage from './FleetPage';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={FleetPage} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

function mockFetch(status: number, body: unknown) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(new Response(JSON.stringify(body), { status })),
  );
}

describe('FleetPage — orchestration disabled (404, the default for every existing install)', () => {
  it('shows the disabled explanation, not the "register a plane" empty state', async () => {
    mockFetch(404, { error: { status: 404, message: 'not found' } });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Agent-fleet orchestration is disabled');
    // Card E2 (Phase 39): the flag is now UI-toggleable, so the empty state
    // links to the guided setup instead of naming an env var to set by hand.
    expect(container.textContent).toContain('Set up orchestration');
    expect(container.textContent).not.toContain('No control planes registered');
  });
});

describe('FleetPage — enabled, nothing registered yet (200, empty rows)', () => {
  it('explains how to register a control plane', async () => {
    mockFetch(200, { rows: [] });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No control planes registered');
    expect(container.textContent).toContain('TACK_ORCH_ENABLE');
    expect(container.textContent).toMatch(/POST \/api\/control-planes/);
  });
});

describe('FleetPage — request failure (not a 404)', () => {
  it('shows a retry-able error state distinct from both empty states', async () => {
    mockFetch(500, { error: { status: 500, message: 'boom' } });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain("Couldn't load fleet status");
    expect(container.textContent).not.toContain('No control planes registered');
    expect(container.textContent).not.toContain('Agent-fleet orchestration is disabled');
  });

  it('retries the request when the Retry button is clicked', async () => {
    const fetchMock = mockFetch(500, { error: { status: 500, message: 'boom' } });
    const { container } = mount();
    await flush();
    const retryBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Retry'),
    )!;
    retryBtn.click();
    await flush();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});

describe('FleetPage — rows present', () => {
  const rows = [
    {
      project_id: 'proj-1',
      project_name: 'Adapta',
      control_plane_id: 'cp-1',
      control_plane_name: 'home-docket',
      control_plane_kind: 'docket',
      health: 'healthy',
      last_seen_at: new Date().toISOString(),
      consecutive_failures: 0,
      gateway: 'active',
      roster: [{ id: 'a1', name: 'Backend Dev', role: 'backend', model: 'claude-sonnet-5' }],
      last_activity_at: new Date().toISOString(),
      tokens_in: 1000,
      tokens_out: 500,
      cost_usd_estimated: 1.5,
      pricing_snapshot_at: '2026-07-01T00:00:00Z',
      budget_usd: 25,
      pending_approval_count: 2,
    },
  ];

  it('renders one row per project with the estimated-cost wording', async () => {
    mockFetch(200, { rows });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Adapta');
    expect(container.textContent).toMatch(/estimated/);
    expect(container.querySelectorAll('tbody tr').length).toBe(1);
  });

  it('renders column headers for every field the card requires', async () => {
    mockFetch(200, { rows });
    const { container } = mount();
    await flush();
    const headerText = container.textContent ?? '';
    for (const col of ['Project', 'Pod health', 'Roster', 'Last activity', 'Burn vs budget', 'Gateway', 'Approvals']) {
      expect(headerText).toContain(col);
    }
  });
});

// The harness-agnostic runner/fleet/agent-profile/model-profile UI that
// used to open this page (`RunnerFleetSection`) now lives under the Agents
// page's Advanced section instead — see `features/agents/AgentsPage.tsx`
// and `AdvancedSection.tsx`. This page is the legacy Docket control-plane
// roster only, fully covered by the describe blocks above and by
// `FleetRow.test.tsx`.
describe('FleetPage — fires only the legacy control-plane request on mount', () => {
  it('does not fire any request beyond the one legacy /api/fleet GET', async () => {
    const fetchMock = mockFetch(404, { error: { status: 404, message: 'not found' } });
    mount();
    await flush();
    const urls = fetchMock.mock.calls.map((c) => String(c[0]));
    expect(urls.every((u) => u.endsWith('/api/fleet'))).toBe(true);
  });
});

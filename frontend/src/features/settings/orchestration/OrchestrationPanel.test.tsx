import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import OrchestrationPanel from './OrchestrationPanel';

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
        <Route path="/" component={() => <OrchestrationPanel projectId="proj-1" />} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

/** Routes each request to a body by matching a substring in the URL —
 *  `OrchestrationPanel` fans out to 2-3 endpoints depending on link state. */
function routedFetch(routes: Array<[match: string, status: number, body: unknown]>) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    for (const [match, status, body] of routes) {
      if (url.includes(match)) {
        return Promise.resolve(new Response(JSON.stringify(body), { status }));
      }
    }
    return Promise.resolve(new Response(JSON.stringify({ error: { status: 404, message: 'no route' } }), { status: 404 }));
  });
}

describe('OrchestrationPanel — orchestration disabled (404, the default)', () => {
  it('shows the disabled explanation and no form/panels', async () => {
    routedFetch([['/orch-link', 404, { error: { status: 404, message: 'not found' } }]]);
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain('Agent-fleet orchestration is disabled');
    expect(container.querySelector('form')).toBeNull();
    dispose();
  });
});

describe('OrchestrationPanel — request failure other than 404', () => {
  it('shows a retry state, not the disabled explanation', async () => {
    routedFetch([['/orch-link', 500, { error: { status: 500, message: 'boom' } }]]);
    const { container, dispose } = mount();
    await flush();
    expect(container.textContent).toContain("Couldn't load orchestration status");
    expect(container.textContent).not.toContain('disabled');
    dispose();
  });
});

describe('OrchestrationPanel — unlinked project', () => {
  it('renders the link form, not the budget/policy panels', async () => {
    routedFetch([
      ['/orch-link', 200, { linked: false, link: null }],
      ['/control-planes', 200, [{ id: 'cp-1', name: 'docket-1', kind: 'docket', health: 'unknown' }]],
    ]);
    const { container, dispose } = mount();
    await flush();
    await flush();
    expect(container.textContent).toContain('Link this project to a control plane');
    expect(container.querySelector('form')).not.toBeNull();
    expect(container.querySelector('#orch-budget-heading')).toBeNull();
    expect(container.querySelector('#orch-policy-heading')).toBeNull();
    dispose();
  });
});

describe('OrchestrationPanel — linked project', () => {
  const link = {
    project_id: 'proj-1',
    control_plane_id: 'cp-1',
    remote_project: 'remote-1',
    pipeline_file: null,
    blueprint: null,
    auto_dispatch: false,
    budget_usd: 50,
    // A value distinct from the real backend constant — proves the panel
    // renders whatever the API returns rather than a string hardcoded here.
    compatibility_label: 'legacy-docket:test-fixture-v9',
    compatibility_policy: 'Fixture policy text used only to pin that this comes from the API.',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  };

  it('renders both the Budget and Policy panels with honest caveats, never a "paused" claim', async () => {
    routedFetch([
      ['/orch-link', 200, { linked: true, link }],
      [
        '/orch-budget',
        200,
        {
          linked: true,
          control_plane_id: 'cp-1',
          control_plane_name: 'docket-1',
          health: 'healthy',
          budget_usd: 50,
          tokens_in: 1000,
          tokens_out: 500,
          cost_usd_estimated: 40,
          pricing_snapshot_at: null,
        },
      ],
      [
        '/orch-policy',
        200,
        {
          linked: true,
          control_plane_id: 'cp-1',
          control_plane_name: 'docket-1',
          health: 'healthy',
          scoped_to_control_plane_only: true,
          scraped_at: '2026-01-01T00:00:00Z',
          tool_calls: [
            { decision: 'allow', count: 8 },
            { decision: 'deny', count: 2 },
          ],
          denial_rate: 0.2,
          policy_hits: [{ policy_id: 'no-secrets', hook: 'pre_tool_call', action: 'deny', count: 2 }],
          approvals_by_channel: [{ channel: 'tack', outcome: 'granted', count: 3 }],
        },
      ],
    ]);

    const { container, dispose } = mount();
    await flush();
    await flush();
    await flush();

    expect(container.textContent).toContain('Budget');
    expect(container.textContent).toContain('Policy');
    // Honest cost figure — never presented as spend.
    expect(container.textContent).toContain('estimated');
    // The compounding-estimate caveat must accompany the progress figure.
    expect(container.textContent).toContain('estimate of a fraction of an estimate');
    // The CLI-only pause remedy is named, but nothing claims to know the pod IS paused.
    expect(container.textContent).toContain('docket profile');
    expect(container.textContent).not.toMatch(/is (currently )?paused/i);
    // The control-plane-wide scoping caveat is shown for policy data.
    expect(container.textContent).toContain('control plane');
    expect(container.textContent).toContain('20% of tool calls denied');
    expect(container.textContent).toContain('no-secrets');
    // The compatibility label/policy come from the fetched link, not a
    // string hardcoded in the component — see `link`'s fixture values above.
    expect(container.textContent).toContain('legacy-docket:test-fixture-v9');
    expect(container.textContent).toContain(
      'Fixture policy text used only to pin that this comes from the API.'
    );
    dispose();
  });
});

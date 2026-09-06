import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import RunWithAgentModal from './RunWithAgentModal';
import type { RunnerCapabilities } from '../execution';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const FLEET = { fleet_id: 'fleet-1', name: 'Primary Fleet', concurrency_limit: 5, default_policy: null };
/** Carries a `default_model` convention matching `RUNNER`'s own declared
 *  `codex`/`openai`/`opaque/model-alpha` combination below, so an Auto
 *  submission through this profile resolves to a real, supported pair —
 *  most tests in this file need that to be true to submit at all, since an
 *  Auto request that resolves to nothing is blocked before it ever reaches
 *  the wire (`shared.ts#gateHarnessModelSelection`). */
const PROFILE = {
  agent_profile_id: 'profile-1',
  name: 'Reviewer',
  instructions: 'Review the diff',
  tool_policy: { read: true },
  limits: { default_model: { provider: 'openai', model_id: 'opaque/model-alpha' } },
};
/** A second profile — with only one candidate, `RunWithAgentModal` auto-selects
 *  it (the same "an unambiguous single choice needs no picker" reasoning the
 *  target picker applies), so any test proving "no agent profile selected"
 *  needs at least two to keep that a real, unresolved choice. */
const PROFILE_2 = { agent_profile_id: 'profile-2', name: 'Fixer', instructions: 'Fix the bug', tool_policy: {}, limits: null };

/** One active runner reporting `codex`/`openai`/`opaque/model-alpha`, with no
 *  `model_passthrough` attestation — the common shape most tests start from. */
function runnerCapabilitySnapshot(overrides: Record<string, unknown> = {}) {
  return {
    harnesses: [
      {
        harness_kind: 'codex',
        installed_version: '1.0.0',
        probe_error: null,
        probed_at: '2026-08-06T12:00:00Z',
        model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'], discovery: 'reported' }],
      },
    ],
    concurrency: { total: 1, available: 1 },
    limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
    features: {
      cancel: { support: 'advisory', reason: null },
      resume: { support: 'unsupported', reason: null },
      decisions: { support: 'supported', reason: null },
      artifacts: { support: 'supported', reason: null },
      usage: { support: 'advisory', reason: null },
    },
    ...overrides,
  };
}

function runnerRow(id: string, name: string, snapshot: Record<string, unknown>, fleetIds: string[] = []) {
  return {
    runner_id: id,
    name,
    state: 'active',
    labels: null,
    labels_raw: '{}',
    total_capacity: 1,
    available_capacity: 1,
    capability_snapshot: snapshot,
    capability_snapshot_raw: JSON.stringify(snapshot),
    protocol_version: 1,
    runner_version: '0.1.0',
    last_heartbeat_at: '2026-08-06T12:00:00Z',
    revoked_at: null,
    fleet_ids: fleetIds,
    created_at: '2026-08-06T12:00:00Z',
    updated_at: '2026-08-06T12:00:00Z',
  };
}

const RUNNER = runnerRow('runner-1', 'Dev laptop', runnerCapabilitySnapshot());

let lastCreateBody: unknown;

function mockFetch(
  opts: {
    runners?: unknown[];
    fleets?: unknown[];
    agentProfiles?: unknown[];
    project?: unknown;
  } = {},
): typeof fetch {
  const runners = opts.runners ?? [RUNNER];
  const fleets = opts.fleets ?? [FLEET];
  const agentProfiles: unknown[] = [...(opts.agentProfiles ?? [PROFILE])];
  const project = opts.project ?? { id: 'project-1', name: 'P', default_model: null };
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.includes('/runner-fleets')) return new Response(JSON.stringify({ protocol_version: 1, data: fleets }), { status: 200 });
    if (url.includes('/agent-profiles') && init?.method === 'POST') {
      const body = JSON.parse(String(init.body));
      const created = { agent_profile_id: 'profile-new', name: body.name, instructions: body.instructions, tool_policy: body.tool_policy ?? {}, limits: null };
      agentProfiles.push(created);
      return new Response(JSON.stringify({ protocol_version: 1, ...created }), { status: 200 });
    }
    if (url.includes('/agent-profiles')) return new Response(JSON.stringify({ protocol_version: 1, data: agentProfiles }), { status: 200 });
    if (url.includes('/runners')) return new Response(JSON.stringify({ protocol_version: 1, data: runners }), { status: 200 });
    if (url.includes('/projects/')) return new Response(JSON.stringify(project), { status: 200 });
    if (url.endsWith('/executions') && init?.method === 'POST') {
      lastCreateBody = JSON.parse(String(init.body));
      return new Response(JSON.stringify({ protocol_version: 1, request_id: 'req-9', state: 'queued', replayed: false }), { status: 200 });
    }
    if (url.includes('/executions/req-9')) {
      return new Response(
        JSON.stringify({ request_id: 'req-9', item_id: 'item-1', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' }),
        { status: 200 },
      );
    }
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function Host(props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void }) {
  const [open, setOpen] = createSignal(true);
  (Host as unknown as { lastSetOpen?: (v: boolean) => void }).lastSetOpen = setOpen;
  return (
    <ExecutionStoreProvider>
      {/* `<A href="/agents">` (the "agent execution is off" state) needs a
       *  router context to resolve its path — every real mount point
       *  (Board/item-detail/Sprint) already renders under `app/routes.tsx`. */}
      <MemoryRouter>
        <Route
          path="/"
          component={() => (
            <RunWithAgentModal
              isOpen={open()}
              onClose={() => setOpen(false)}
              itemId="item-1"
              itemTitle="Fix login bug"
              projectId="project-1"
              onCreated={props.onCreated}
              capabilities={props.capabilities}
            />
          )}
        />
      </MemoryRouter>
    </ExecutionStoreProvider>
  );
}

function mount(
  props: { capabilities?: () => RunnerCapabilities[]; onCreated?: (id: string) => void } = {},
  fetchOpts: Parameters<typeof mockFetch>[0] = {},
) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch(fetchOpts));
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Host {...props} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

// Only `FieldShell`-rendered labels carry a `for` attribute (the plain
// "Fleet" / "Exact runner" / model-mode radio labels wrap their `<input>`
// directly instead, per native `<label>` association) — filtering on that
// attribute avoids matching a radio's own label text by substring.
function select(labelText: string): HTMLSelectElement {
  const label = [...document.querySelectorAll('label[for]')].find((l) => l.textContent?.trim().startsWith(labelText));
  const id = label!.getAttribute('for')!;
  return document.getElementById(id) as HTMLSelectElement;
}

function field(labelText: string): HTMLInputElement {
  const label = [...document.querySelectorAll('label[for]')].find((l) => l.textContent?.trim().startsWith(labelText));
  const id = label!.getAttribute('for')!;
  return document.getElementById(id) as HTMLInputElement;
}

function setSelect(el: HTMLSelectElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function setField(el: HTMLInputElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function submitButton(): HTMLButtonElement {
  return [...document.querySelectorAll('button')].find((b) => b.textContent === 'Run') as HTMLButtonElement;
}

/** Repository is a read-only summary by default (no free-text field visible
 *  — this card's own acceptance bar) — every test that needs to type a
 *  remote must expand it first, exactly as an operator would click
 *  "Change for this run". */
function expandRepository(): void {
  const btn = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Change for this run');
  btn!.click();
}

function modelModeRadio(index: number): HTMLInputElement {
  return [...document.querySelectorAll('input[type="radio"][name="model-mode"]')][index] as HTMLInputElement;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  lastCreateBody = undefined;
});

describe('RunWithAgentModal', () => {
  it('renders with the item name in the title, and every always-present section labeled', async () => {
    mount();
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Run with agent: Fix login bug');
    expect(dialog.textContent).toContain('Agent');
    expect(dialog.textContent).toContain('Repository');
    expect(dialog.textContent).toContain('Permissions & budget');
  });

  it('"agent execution is off" renders instead of the form when zero runners have ever enrolled', async () => {
    mount({}, { runners: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Agent execution is off');
    expect(dialog.textContent).toContain('Turn it on');
    expect(dialog.querySelector('form')).toBeNull();
  });

  it('hides the target picker and auto-selects the one active runner when it is the only target — no free-text id anywhere', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).not.toContain('Where it runs');
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ selector_kind: 'exact_runner', selector_id: 'runner-1' });
  });

  it('shows a combined machine/group picker, by name, when more than one target exists', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Where it runs');
    const picker = select('Machine or group');
    const labels = [...picker.options].map((o) => o.textContent);
    expect(labels).toContain(FLEET.name);
    expect(labels).toContain(RUNNER.name);
    // Never a raw id in the option text.
    expect(labels.some((l) => l?.includes(RUNNER.runner_id))).toBe(false);
  });

  it('manually picking a specific runner from the combined picker actually selects it (not just an auto-selected single target)', async () => {
    // Regression coverage: an earlier version of this picker's `<option>`
    // values used a `runner:` prefix that its own `onInput` parser didn't
    // recognise (it checked for `exact_runner`), so a real pick silently
    // cleared the selection — the native `<select>` still *displayed* the
    // picked option (browser-owned DOM state), while `selectorId` reset to
    // `''` and the submit button stayed disabled forever. Only an E2E run
    // against the real picker caught it; this test pins the fix so it can't
    // regress silently again.
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    setSelect(select('Machine or group'), `exact_runner:${RUNNER.runner_id}`);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ selector_kind: 'exact_runner', selector_id: RUNNER.runner_id });
  });

  it('submit is disabled with every missing-field reason listed when the form is empty', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET], agentProfiles: [PROFILE, PROFILE_2] });
    await flush();
    expect(submitButton().disabled).toBe(true);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Select where this runs.');
    expect(dialog.textContent).toContain('Select an agent profile.');
    expect(dialog.textContent).toContain('Enter a repository remote.');
  });

  it('offers "Create default profile" inline when no agent profile exists yet, and selects the new one', async () => {
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('No agent profile exists yet.');
    const createBtn = [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Create default profile')!;
    createBtn.click();
    await flush();
    await flush();
    expect(dialog.textContent).not.toContain('No agent profile exists yet.');
    expect(select('Agent profile').value).toBe('profile-new');
  });

  it('the Repository fieldset shows a read-only summary — no free-text field visible by default — until "Change for this run"', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('No repository configured for this run yet.');
    expect(dialog.querySelector('input[placeholder="git@github.com:org/repo.git"]')).toBeNull();
    const changeBtn = [...dialog.querySelectorAll('button')].find((b) => b.textContent === 'Change for this run')!;
    changeBtn.click();
    await flush();
    expect(dialog.querySelector('input[placeholder="git@github.com:org/repo.git"]')).not.toBeNull();
  });

  it('with a project model default configured, "Project default — …" is offered and selected automatically', async () => {
    mount({}, { runners: [RUNNER], fleets: [], project: { id: 'project-1', name: 'P', default_model: { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' } } });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Project default — openai / opaque/model-alpha');
    expect(modelModeRadio(0).checked).toBe(true);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    submitButton().click();
    await flush();
    await flush();
    expect(lastCreateBody).toMatchObject({ requested_model_provider: 'openai', requested_model_id: 'opaque/model-alpha' });
  });

  it('with no project model default, "Auto" is selected and no "Project default" option is offered', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).not.toContain('Project default —');
    expect(modelModeRadio(1).checked).toBe(true); // "Auto" is the only other radio besides "Choose…"
  });

  it('with no default model configured at any tier, Auto is blocked and names the fix, with a link to set one', async () => {
    // PROFILE_2 carries no `default_model` convention, and this mount's
    // project/fleets carry none either — the "every tier absent" state the
    // scheduler's AutoSelectNotVerified rejects unconditionally.
    mount({}, { runners: [RUNNER], fleets: [], agentProfiles: [PROFILE_2] });
    await flush();
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(modelModeRadio(1).checked).toBe(true); // Auto, the default with no project opinion
    expect(submitButton().disabled).toBe(true);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain('Unsupported');
    expect(dialog.textContent).toMatch(/no agent profile, project, or fleet default model is configured/i);
    const fixLink = [...dialog.querySelectorAll('a')].find((a) => a.textContent === 'Set a default model for this project');
    expect(fixLink).toBeTruthy();
    expect(fixLink!.getAttribute('href')).toBe('/projects/project-1/settings?tab=agents');
  });

  it('with the project pinned explicitly to Auto, the same request is blocked with a distinct, tier-naming reason', async () => {
    mount(
      {},
      {
        runners: [RUNNER],
        fleets: [],
        agentProfiles: [PROFILE_2],
        project: { id: 'project-1', name: 'P', default_model: { kind: 'auto' } },
      },
    );
    await flush();
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(true);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toMatch(/project is explicitly set to auto/i);
  });

  it('"Choose…" lists the target\'s own reported model combinations, and picking one is real, gate-supported data', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    modelModeRadio(0).click(); // "Choose…" is first when there is no project default
    await flush();
    const modelSelect = select('Model');
    const labels = [...modelSelect.options].map((o) => o.textContent);
    expect(labels.some((l) => l?.includes('openai / opaque/model-alpha'))).toBe(true);
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    setSelect(modelSelect, '0');
    await flush();
    // The "Choose…" list is built from the same live runner data the gate
    // itself reads (`targetCapabilities()` feeds both) — every option it
    // offers already passes `gateHarnessModelSelection` by construction, so
    // picking one enables submit with no separate capabilities override.
    expect(submitButton().disabled).toBe(false);
    expect(document.body.textContent).toContain('Supported');
  });

  it('a custom model id is only offered when the harness attests model_passthrough: supported', async () => {
    mount({}, { runners: [RUNNER], fleets: [] });
    await flush();
    modelModeRadio(0).click();
    await flush();
    expect([...select('Model').options].map((o) => o.textContent)).not.toContain('Other (type a model id)');

    disposers.pop()!();
    document.body.innerHTML = '';
    const passthroughRunner = runnerRow('runner-2', 'Passthrough box', runnerCapabilitySnapshot({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.0.0',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
          model_passthrough: { support: 'supported', reason: 'forwards the model id verbatim' },
        },
      ],
    }));
    mount({}, { runners: [passthroughRunner], fleets: [] });
    await flush();
    modelModeRadio(0).click();
    await flush();
    const options = [...select('Model').options].map((o) => o.textContent);
    expect(options).toContain('Other (type a model id)');
    setSelect(select('Model'), '__custom__');
    await flush();
    expect(field('Model id')).toBeTruthy();
  });

  it('proves the gate is load-bearing: injecting a real supported combination allows submit', async () => {
    const caps: RunnerCapabilities[] = [
      {
        runner_version: '0.1.0',
        reported_at: '2026-08-06T12:00:00Z',
        labels: {},
        concurrency: { total: 1, available: 1 },
        harnesses: [
          {
            harness_kind: 'codex',
            installed_version: '1.0.0',
            probe_error: null,
            probed_at: '2026-08-06T12:00:00Z',
            model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'], discovery: 'reported' }],
          },
        ],
        features: {
          cancel: { support: 'advisory', reason: null },
          resume: { support: 'unsupported', reason: null },
          decisions: { support: 'supported', reason: null },
          artifacts: { support: 'supported', reason: null },
          usage: { support: 'advisory', reason: null },
        },
        limits: { event_payload_bytes_max: 1024, artifact_content_bytes_max: 1024 },
      },
    ];
    mount({ capabilities: () => caps }, { runners: [RUNNER], fleets: [] });
    await flush();
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    modelModeRadio(0).click();
    await flush();
    setSelect(select('Model'), '0');
    await flush();
    expect(submitButton().disabled).toBe(false);
    expect(document.body.textContent).toContain('Supported');
  });

  it('submitting posts the exact CreateExecutionInput shape, closes, and calls onCreated', async () => {
    const onCreated = vi.fn();
    mount({ onCreated }, { runners: [RUNNER], fleets: [] });
    await flush();
    setSelect(select('Agent profile'), PROFILE.agent_profile_id);
    expandRepository();
    setField(field('Remote'), 'git@example.com:org/repo.git');
    await flush();
    expect(submitButton().disabled).toBe(false);
    submitButton().click();
    await flush();
    await flush();

    expect(lastCreateBody).toMatchObject({
      item_id: 'item-1',
      selector_kind: 'exact_runner',
      selector_id: 'runner-1',
      agent_profile_id: 'profile-1',
      requested_harness_kind: 'codex',
      requested_model_provider: null,
      requested_model_id: null,
      repository_snapshot: { kind: 'git', remote: 'git@example.com:org/repo.git', base_revision: 'main', subdirectory: null },
      permission_policy: { tools: [], network: false },
    });
    expect(typeof (lastCreateBody as Record<string, unknown>).idempotency_key).toBe('string');
    expect(onCreated).toHaveBeenCalledWith('req-9');
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('every visible field has an accessible label (native <label for>) — the keyboard/a11y path this card requires', async () => {
    mount({}, { runners: [RUNNER], fleets: [FLEET] });
    await flush();
    const dialog = document.querySelector('[role="dialog"]')!;
    const inputs = [...dialog.querySelectorAll('input[id], select[id]')];
    expect(inputs.length).toBeGreaterThan(0);
    for (const input of inputs) {
      const id = input.getAttribute('id')!;
      const label = dialog.querySelector(`label[for="${id}"]`);
      expect(label, `no label for #${id}`).toBeTruthy();
    }
  });
});

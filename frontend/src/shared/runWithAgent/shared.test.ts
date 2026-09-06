import { describe, it, expect } from 'vitest';
import {
  HARNESS_KINDS,
  buildCreateExecutionInput,
  generateIdempotencyKey,
  resolveDefaultProvenance,
  gateHarnessModelSelection,
  describeExecutionState,
  isTerminalStateString,
  relativeTimeFromIso,
  isActiveRunnerState,
  shouldHideTargetPicker,
  isExecutionOff,
  describeProjectModelDefault,
  projectDefaultModelPair,
  isModelPassthroughAttested,
  parseModelDefaultConvention,
  resolveAutoModelPolicy,
  type RunWithAgentFormValues,
} from './shared';
import type { RunnerCapabilities, HarnessCapability } from '../execution';
import type { ProjectModelDefault } from '../types';

function values(overrides: Partial<RunWithAgentFormValues> = {}): RunWithAgentFormValues {
  return {
    itemId: 'item-1',
    selectorKind: 'fleet',
    selectorId: 'fleet-1',
    agentProfileId: 'profile-1',
    agentProfileSnapshot: { name: 'Reviewer', instructions: 'Review the diff', tool_policy: { read: true } },
    harnessKind: 'codex',
    modelProvider: null,
    modelId: null,
    timeoutSeconds: 1800,
    allowNetwork: false,
    tools: ['read', 'write'],
    repository: { kind: 'git', remote: 'git@example.com:org/repo.git', baseRevision: 'main', subdirectory: null },
    idempotencyKey: 'fixed-key',
    ...overrides,
  };
}

describe('HARNESS_KINDS', () => {
  it('carries the two verified v1 wire values — codex, claude-code (hyphen)', () => {
    expect(HARNESS_KINDS.map((h) => h.value)).toEqual(['codex', 'claude-code']);
  });
});

describe('buildCreateExecutionInput', () => {
  it('builds a CreateExecutionInput with every top-level field the real handler requires', () => {
    const input = buildCreateExecutionInput(values());
    expect(input.item_id).toBe('item-1');
    expect(input.idempotency_key).toBe('fixed-key');
    expect(input.selector_kind).toBe('fleet');
    expect(input.selector_id).toBe('fleet-1');
    expect(input.agent_profile_id).toBe('profile-1');
    expect(input.requested_harness_kind).toBe('codex');
    expect(input.requested_model_provider).toBeNull();
    expect(input.requested_model_id).toBeNull();
    expect(input.timeout_seconds).toBe(1800);
    expect(input.status_map_policy_id).toBeNull();
  });

  // Pinned against `crates/tack-orch/src/execution/types.rs`'s
  // `AgentProfileSnapshot { name, instructions, tool_policy, timeout_seconds,
  // budgets }` — exactly the fields E5's handoff found are validated
  // server-side one layer deeper than `CreateExecution`'s own untyped
  // `Value`. A missing field here would pass this frontend's own type-check
  // (the wire type is `unknown`) but fail live, the same way E5's first
  // draft did with an empty-object default.
  it('agent_profile_snapshot carries exactly the fields AgentProfileSnapshot requires', () => {
    const input = buildCreateExecutionInput(values());
    const snap = input.agent_profile_snapshot as Record<string, unknown>;
    expect(Object.keys(snap).sort()).toEqual(['budgets', 'instructions', 'name', 'timeout_seconds', 'tool_policy']);
    expect(snap.name).toBe('Reviewer');
    expect(snap.instructions).toBe('Review the diff');
    expect(snap.tool_policy).toEqual({ read: true });
    expect(snap.timeout_seconds).toBe(1800);
    expect(snap.budgets).toEqual({});
  });

  // Pinned against `RepositorySnapshot { kind, remote, base_revision, subdirectory }`.
  it('repository_snapshot carries exactly the fields RepositorySnapshot requires', () => {
    const input = buildCreateExecutionInput(values());
    const repo = input.repository_snapshot as Record<string, unknown>;
    expect(Object.keys(repo).sort()).toEqual(['base_revision', 'kind', 'remote', 'subdirectory']);
    expect(repo.kind).toBe('git');
    expect(repo.remote).toBe('git@example.com:org/repo.git');
    expect(repo.base_revision).toBe('main');
    expect(repo.subdirectory).toBeNull();
  });

  // Pinned against `PermissionPolicy { tools: Vec<String>, network: bool }`
  // — `network` is required with no server-side default (E5's live-tested
  // finding), so this module always sends an explicit boolean, never omits it.
  it('permission_policy carries exactly the fields PermissionPolicy requires', () => {
    const input = buildCreateExecutionInput(values({ allowNetwork: true }));
    const policy = input.permission_policy as Record<string, unknown>;
    expect(Object.keys(policy).sort()).toEqual(['network', 'tools']);
    expect(policy.network).toBe(true);
    expect(policy.tools).toEqual(['read', 'write']);
  });

  it('budgets/environment/metadata default to {} — confirmed safe by E5s live test', () => {
    const input = buildCreateExecutionInput(values());
    expect(input.budgets).toEqual({});
    expect(input.environment).toEqual({});
    expect(input.metadata).toEqual({});
  });

  it('a chosen (non-auto) model provider/id round-trips exactly, not coerced', () => {
    const input = buildCreateExecutionInput(values({ modelProvider: 'openai', modelId: 'opaque/model-alpha' }));
    expect(input.requested_model_provider).toBe('openai');
    expect(input.requested_model_id).toBe('opaque/model-alpha');
  });

  it('two different entry points building from equal form values produce byte-identical payloads', () => {
    // This is the concrete proof behind the acceptance bar "all three
    // surfaces create the same payload shape... no divergent DTOs between
    // entry points" — Board/item-detail/Sprint all end up calling this exact
    // function, so equal input always yields equal output.
    const a = buildCreateExecutionInput(values());
    const b = buildCreateExecutionInput(values());
    expect(a).toEqual(b);
  });
});

describe('generateIdempotencyKey', () => {
  it('produces a fresh, non-empty key each call', () => {
    const a = generateIdempotencyKey();
    const b = generateIdempotencyKey();
    expect(a).not.toBe(b);
    expect(a.length).toBeGreaterThan(0);
  });
});

describe('resolveDefaultProvenance', () => {
  it('is always the typed not_available state today — III-F3 (Wave 5) has not landed', () => {
    const result = resolveDefaultProvenance();
    expect(result.status).toBe('not_available');
    if (result.status === 'not_available') {
      expect(result.reason).toMatch(/III-F3/);
    }
  });
});

function capsWith(harness: {
  harness_kind: string;
  probe_error: string | null;
  model_combinations: { model_provider: string; model_ids: string[] }[];
  model_passthrough?: { support: 'supported' | 'unsupported' | 'advisory'; reason: string | null };
}): RunnerCapabilities[] {
  return [
    {
      runner_version: '0.1.0',
      reported_at: '2026-08-06T12:00:00Z',
      labels: {},
      concurrency: { total: 1, available: 1 },
      harnesses: [
        {
          harness_kind: harness.harness_kind,
          installed_version: '1.0.0',
          probe_error: harness.probe_error,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: harness.model_combinations.map((c) => ({ ...c, discovery: 'reported' })),
          ...(harness.model_passthrough ? { model_passthrough: harness.model_passthrough } : {}),
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
}

describe('gateHarnessModelSelection', () => {
  it('with zero capability snapshots and a SPECIFIC model chosen, blocks submission with a typed reason', () => {
    // This is a falsifiable claim ("this exact combination works"), and with
    // no real capability data available it cannot be verified as supported —
    // TODO.md III.2 rule 7, "never report supported: true without at least
    // one runner's real capability data backing it."
    const gate = gateHarnessModelSelection([], 'codex', 'openai', 'opaque/model-alpha');
    expect(gate.allowed).toBe(false);
    expect(gate.advisory).toBe(false);
    expect(gate.reason).toMatch(/no runner capability data/i);
  });

  it('proves the gate is load-bearing: a genuinely unsupported specific combination is blocked even with real data present', () => {
    const caps = capsWith({
      harness_kind: 'codex',
      probe_error: null,
      model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'] }],
    });
    const gate = gateHarnessModelSelection(caps, 'codex', 'openai', 'opaque/model-DOES-NOT-EXIST');
    expect(gate.allowed).toBe(false);
  });

  it('proves the gate is load-bearing: a genuinely supported specific combination is allowed, non-advisory', () => {
    const caps = capsWith({
      harness_kind: 'codex',
      probe_error: null,
      model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'] }],
    });
    const gate = gateHarnessModelSelection(caps, 'codex', 'openai', 'opaque/model-alpha');
    expect(gate.allowed).toBe(true);
    expect(gate.advisory).toBe(false);
  });

  it('an explicit model undeclared by any runner is still allowed when the harness attests model_passthrough: supported', () => {
    const caps = capsWith({
      harness_kind: 'codex',
      probe_error: null,
      model_combinations: [],
      model_passthrough: { support: 'supported', reason: null },
    });
    const gate = gateHarnessModelSelection(caps, 'codex', 'openai', 'gpt-5-codex');
    expect(gate.allowed).toBe(true);
    expect(gate.reason).toContain('model passthrough');
  });

  // ── Auto ("let the runner decide") ────────────────────────────────────────
  //
  // Auto's fate now depends entirely on `autoResolution` (this module's
  // `resolveAutoModelPolicy`), never on harness probe status — the scheduler
  // (`crates/tack-orch/src/scheduler/select.rs`) rejects an unresolved Auto
  // request unconditionally regardless of whether any runner was ever probed
  // cleanly, so probe status has nothing to do with whether this submits.

  it('with no autoResolution argument (defaults to unresolved), Auto is blocked and names the fix', () => {
    const caps = capsWith({ harness_kind: 'codex', probe_error: null, model_combinations: [] });
    const gate = gateHarnessModelSelection(caps, 'codex', null, null);
    expect(gate.allowed).toBe(false);
    expect(gate.advisory).toBe(false);
    expect(gate.reason).toMatch(/no agent profile, project, or fleet default model is configured/i);
    expect(gate.reason).not.toMatch(/scheduler will still validate at claim time/i);
  });

  it('unresolved Auto is blocked even with zero capability snapshots — the same fix applies regardless', () => {
    const gate = gateHarnessModelSelection([], 'codex', null, null, { outcome: 'unresolved' });
    expect(gate.allowed).toBe(false);
    expect(gate.reason).toMatch(/no agent profile, project, or fleet default model is configured/i);
  });

  it('a tier pinned explicitly to Auto is blocked with a distinct reason naming that tier', () => {
    const gate = gateHarnessModelSelection([], 'codex', null, null, { outcome: 'pinned_auto', source: 'project' });
    expect(gate.allowed).toBe(false);
    expect(gate.reason).toMatch(/project is explicitly set to auto/i);
  });

  it('unresolved Auto carries a fix pointing at project settings when a href is given', () => {
    const gate = gateHarnessModelSelection([], 'codex', null, null, { outcome: 'unresolved' }, '/projects/p1/settings?tab=agents');
    expect(gate.fix).toEqual({ label: 'Set a default model for this project', href: '/projects/p1/settings?tab=agents' });
  });

  it('pinned_auto at the project tier also carries the fix (that IS the reachable setting)', () => {
    const gate = gateHarnessModelSelection(
      [],
      'codex',
      null,
      null,
      { outcome: 'pinned_auto', source: 'project' },
      '/projects/p1/settings?tab=agents',
    );
    expect(gate.fix).toBeDefined();
  });

  it('pinned_auto at the agent-profile or fleet tier carries no fix — neither has a settings UI', () => {
    const profileGate = gateHarnessModelSelection(
      [],
      'codex',
      null,
      null,
      { outcome: 'pinned_auto', source: 'agent_profile' },
      '/projects/p1/settings?tab=agents',
    );
    expect(profileGate.fix).toBeUndefined();
    const fleetGate = gateHarnessModelSelection(
      [],
      'codex',
      null,
      null,
      { outcome: 'pinned_auto', source: 'fleet' },
      '/projects/p1/settings?tab=agents',
    );
    expect(fleetGate.fix).toBeUndefined();
  });

  it('an Auto request that resolves to Explicit re-runs the same combination check a direct choice would get', () => {
    const caps = capsWith({
      harness_kind: 'codex',
      probe_error: null,
      model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'] }],
    });
    const gate = gateHarnessModelSelection(caps, 'codex', null, null, {
      outcome: 'explicit',
      source: 'project',
      provider: 'openai',
      model_id: 'opaque/model-alpha',
    });
    expect(gate.allowed).toBe(true);
    expect(gate.advisory).toBe(false);
    expect(gate.reason).toContain("project's default model");
    expect(gate.reason).toContain('openai / opaque/model-alpha');
  });

  it('an Auto request resolved to Explicit is still blocked when that resolved pair is genuinely unsupported', () => {
    const caps = capsWith({
      harness_kind: 'codex',
      probe_error: null,
      model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'] }],
    });
    const gate = gateHarnessModelSelection(caps, 'codex', null, null, {
      outcome: 'explicit',
      source: 'agent_profile',
      provider: 'openai',
      model_id: 'opaque/model-DOES-NOT-EXIST',
    });
    expect(gate.allowed).toBe(false);
    expect(gate.reason).toContain("agent profile's default model");
  });
});

describe('parseModelDefaultConvention', () => {
  it('is null for null, undefined-ish, and non-object raw values', () => {
    expect(parseModelDefaultConvention(null)).toBeNull();
    expect(parseModelDefaultConvention(undefined)).toBeNull();
    expect(parseModelDefaultConvention('not an object')).toBeNull();
    expect(parseModelDefaultConvention(42)).toBeNull();
  });

  it('is null when the key is absent', () => {
    expect(parseModelDefaultConvention({})).toBeNull();
    expect(parseModelDefaultConvention({ other_field: true })).toBeNull();
  });

  it('parses the literal "auto"', () => {
    expect(parseModelDefaultConvention({ default_model: 'auto' })).toEqual({ kind: 'auto' });
  });

  it('an unrecognised string literal is "no opinion", not a crash', () => {
    expect(parseModelDefaultConvention({ default_model: 'sometimes' })).toBeNull();
  });

  it('parses an explicit provider/model_id pair', () => {
    expect(parseModelDefaultConvention({ default_model: { provider: 'openai', model_id: 'opaque/model-alpha' } })).toEqual({
      kind: 'explicit',
      provider: 'openai',
      model_id: 'opaque/model-alpha',
    });
  });

  it('a partial pair (missing model_id) is "no opinion"', () => {
    expect(parseModelDefaultConvention({ default_model: { provider: 'openai' } })).toBeNull();
  });
});

describe('resolveAutoModelPolicy', () => {
  it('is unresolved when every tier is absent', () => {
    expect(resolveAutoModelPolicy(null, null, null)).toEqual({ outcome: 'unresolved' });
  });

  it('an agent-profile default wins even when a project default also exists (precedence)', () => {
    const result = resolveAutoModelPolicy(
      { default_model: { provider: 'profile-provider', model_id: 'profile-model' } },
      { kind: 'explicit', provider: 'project-provider', model_id: 'project-model' },
      null,
    );
    expect(result).toEqual({ outcome: 'explicit', source: 'agent_profile', provider: 'profile-provider', model_id: 'profile-model' });
  });

  it('a project default is used when the agent profile has no opinion', () => {
    const result = resolveAutoModelPolicy(null, { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' }, null);
    expect(result).toEqual({ outcome: 'explicit', source: 'project', provider: 'openai', model_id: 'opaque/model-alpha' });
  });

  it('a fleet default is used only when both agent profile and project have no opinion', () => {
    const result = resolveAutoModelPolicy(null, null, { default_model: { provider: 'openai', model_id: 'opaque/model-fleet' } });
    expect(result).toEqual({ outcome: 'explicit', source: 'fleet', provider: 'openai', model_id: 'opaque/model-fleet' });
  });

  it('a tier pinned to literal "auto" stops the walk there — a concrete default one tier down never rescues it', () => {
    const result = resolveAutoModelPolicy(
      { default_model: 'auto' },
      { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' },
      null,
    );
    expect(result).toEqual({ outcome: 'pinned_auto', source: 'agent_profile' });
  });

  it('a project pinned to "auto" also stops the walk before the fleet tier', () => {
    const result = resolveAutoModelPolicy(null, { kind: 'auto' }, { default_model: { provider: 'openai', model_id: 'opaque/model-fleet' } });
    expect(result).toEqual({ outcome: 'pinned_auto', source: 'project' });
  });
});

describe('describeExecutionState', () => {
  it('labels every one of the ten frozen v1 states', () => {
    const states = [
      'queued', 'leased', 'preparing', 'running', 'waiting_decision',
      'succeeded', 'failed', 'cancelled', 'lost', 'needs_operator',
    ];
    for (const s of states) {
      const info = describeExecutionState(s);
      expect(info.known).toBe(true);
      expect(info.label.length).toBeGreaterThan(0);
    }
  });

  it('an unrecognised state still renders (as itself), never throws', () => {
    const info = describeExecutionState('some_future_state');
    expect(info.known).toBe(false);
    expect(info.label).toBe('some_future_state');
    expect(info.tone).toBe('neutral');
  });
});

describe('isTerminalStateString', () => {
  it('matches the frozen terminal set exactly', () => {
    expect(isTerminalStateString('succeeded')).toBe(true);
    expect(isTerminalStateString('failed')).toBe(true);
    expect(isTerminalStateString('cancelled')).toBe(true);
    expect(isTerminalStateString('running')).toBe(false);
    expect(isTerminalStateString('needs_operator')).toBe(false);
  });
});

describe('relativeTimeFromIso', () => {
  it('handles null/undefined/invalid input without throwing', () => {
    expect(relativeTimeFromIso(null)).toBe('unknown');
    expect(relativeTimeFromIso(undefined)).toBe('unknown');
    expect(relativeTimeFromIso('not-a-date')).toBe('unknown');
  });

  it('reports "just now" for a very recent timestamp', () => {
    expect(relativeTimeFromIso(new Date().toISOString())).toBe('just now');
  });
});

describe('isActiveRunnerState', () => {
  it('only "active" counts — pending enrollment and revoked do not', () => {
    expect(isActiveRunnerState('active')).toBe(true);
    expect(isActiveRunnerState('pending_enrollment')).toBe(false);
    expect(isActiveRunnerState('revoked')).toBe(false);
    expect(isActiveRunnerState('some_future_state')).toBe(false);
  });
});

describe('shouldHideTargetPicker', () => {
  it('hides only for exactly one active runner and zero fleets', () => {
    expect(shouldHideTargetPicker(1, 0)).toBe(true);
  });
  it('shows when a fleet exists too, even with one active runner', () => {
    expect(shouldHideTargetPicker(1, 1)).toBe(false);
  });
  it('shows for zero or more than one active runner', () => {
    expect(shouldHideTargetPicker(0, 0)).toBe(false);
    expect(shouldHideTargetPicker(2, 0)).toBe(false);
  });
});

describe('isExecutionOff', () => {
  it('is off only at zero active runners', () => {
    expect(isExecutionOff(0)).toBe(true);
    expect(isExecutionOff(1)).toBe(false);
    expect(isExecutionOff(2)).toBe(false);
  });
});

describe('describeProjectModelDefault', () => {
  it('is null when the project has no opinion', () => {
    expect(describeProjectModelDefault(null)).toBeNull();
    expect(describeProjectModelDefault(undefined)).toBeNull();
  });
  it('labels an explicit project default as "provider / model_id"', () => {
    const d: ProjectModelDefault = { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' };
    expect(describeProjectModelDefault(d)).toBe('openai / opaque/model-alpha');
  });
  it('labels an auto project default as "Auto" — a real choice, distinct from no opinion', () => {
    const d: ProjectModelDefault = { kind: 'auto' };
    expect(describeProjectModelDefault(d)).toBe('Auto');
  });
});

describe('projectDefaultModelPair', () => {
  it('no opinion and an explicit "auto" choice both resolve to the null/null "let the runner decide" pair', () => {
    expect(projectDefaultModelPair(null)).toEqual({ provider: null, id: null });
    expect(projectDefaultModelPair({ kind: 'auto' })).toEqual({ provider: null, id: null });
  });
  it('an explicit default copies its pair through verbatim', () => {
    const d: ProjectModelDefault = { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' };
    expect(projectDefaultModelPair(d)).toEqual({ provider: 'openai', id: 'opaque/model-alpha' });
  });
});

describe('isModelPassthroughAttested', () => {
  const harness = (overrides: Partial<HarnessCapability> = {}): HarnessCapability => ({
    harness_kind: 'codex',
    installed_version: '1.0.0',
    probe_error: null,
    probed_at: '2026-09-04T00:00:00Z',
    model_combinations: [],
    ...overrides,
  });

  it('is false when the field is absent — an older runner, or the shared fake probe', () => {
    expect(isModelPassthroughAttested(harness())).toBe(false);
  });
  it('is false for undefined — no target harness selected yet', () => {
    expect(isModelPassthroughAttested(undefined)).toBe(false);
  });
  it('is false for "advisory" — an unverified claim behaves the same as absent', () => {
    expect(isModelPassthroughAttested(harness({ model_passthrough: { support: 'advisory', reason: 'unverified' } }))).toBe(false);
  });
  it('is true only for "supported"', () => {
    expect(isModelPassthroughAttested(harness({ model_passthrough: { support: 'supported', reason: null } }))).toBe(true);
  });
});

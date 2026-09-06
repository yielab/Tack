import { type Component, createSignal, createResource, createMemo, createEffect, Show, For } from 'solid-js';
import { A } from '@solidjs/router';
import { Modal, Button, Field, Select, Badge } from '../ui';
import { toast } from '../ui/toast';
import { api } from '../api';
import {
  fleetsApi,
  agentProfilesApi,
  runnersApi,
  listModelCombinationsForHarness,
  listReportedHarnessKinds,
  type FleetSummary,
  type AgentProfileSummary,
  type AggregatedModelCombination,
  type RunnerCapabilities,
  type RunnerSummary,
} from '../execution';
import { useExecutionStore } from '../state/executionContext';
import {
  HARNESS_KINDS,
  buildCreateExecutionInput,
  generateIdempotencyKey,
  gateHarnessModelSelection,
  isActiveRunnerState,
  shouldHideTargetPicker,
  isExecutionOff,
  describeProjectModelDefault,
  projectDefaultModelPair,
  isModelPassthroughAttested,
  resolveAutoModelPolicy,
  type RunWithAgentFormValues,
} from './shared';

/** A model id typed by hand, unlocked only when the selected harness attests
 *  `model_passthrough: supported` for the current target (`shared.ts#isModelPassthroughAttested`). */
const CUSTOM_MODEL_VALUE = '__custom__';

export interface RunWithAgentModalProps {
  isOpen: boolean;
  onClose: () => void;
  itemId: string;
  itemTitle: string;
  /** The item's project — needed to read that project's model default
   *  (the `Project` model-policy tier, `GET /api/projects/{id}`). Every
   *  real caller (Board, item-detail, Sprint) already has this on the
   *  `Item` it is rendering; none of them ever construct a
   *  `CreateExecutionInput` themselves (this module's own header comment). */
  projectId: string;
  /** Called after a successful create, with the new request id — so a host
   *  (e.g. item-detail) can switch straight to the Execution tab. Never
   *  required: every surface already sees the new request appear via the
   *  shared store without this callback. */
  onCreated?: (requestId: string) => void;
  /**
   * Injectable runner capability snapshots for the submit gate
   * (`shared.ts#gateHarnessModelSelection`), overriding the live `GET
   * /runners` fetch below. Exposed as a prop purely so tests can inject
   * deterministic fixture data without a network round-trip and prove the
   * gate is load-bearing, not permanently permissive or permanently
   * blocked — every real call site (Board/item-detail/Sprint) omits it and
   * gets live data instead. The target picker (below) and the "agent
   * execution is off" state always read the live `GET /runners` fetch
   * regardless of this override — it replaces the gate's input only, never
   * which machines/groups exist.
   */
  capabilities?: () => RunnerCapabilities[];
}

/**
 * Adapts one `GET /runners` row into the `RunnerCapabilities` shape
 * `gateHarnessModelSelection`/`listModelCombinationsForHarness`/
 * `listReportedHarnessKinds` expect. The two shapes differ in exactly one
 * way: `RunnerCapabilities` (`docs/contracts/runner-v1/capabilities.json`'s
 * *standalone* report shape) nests `protocol_version`/`runner_version`
 * inside the capability payload itself, while `agent_runners` stores them
 * as sibling columns next to the *embedded* snapshot. Returns `null` for a
 * runner with no valid, structurally complete parsed snapshot — never
 * enrolled (the column default is the literal `'{}'`), a pending-enrollment
 * runner, or a genuinely corrupt stored value — skipped entirely rather
 * than a fabricated empty capability standing in for "no data".
 */
function runnerSummaryToCapabilities(runner: RunnerSummary): RunnerCapabilities | null {
  const snapshot = runner.capability_snapshot as
    | Omit<RunnerCapabilities, 'protocol_version' | 'runner_version'>
    | null;
  if (!snapshot || !Array.isArray(snapshot.harnesses) || !snapshot.concurrency || !snapshot.limits) {
    return null;
  }
  return {
    ...snapshot,
    protocol_version: runner.protocol_version,
    runner_version: runner.runner_version ?? '',
  };
}

/**
 * The ONE shared "Run with agent" modal — Board,
 * item-detail, and Sprint all mount this exact component rather than each
 * building their own form, which is what makes "all three surfaces create
 * the same payload shape" true by construction rather than by convention.
 *
 * Vocabulary/visual distinctness from the older Docket "dispatch" feature
 * (`shared/dispatch/**`): this modal's title, every
 * label, and its submit button all say "run" / "agent", never "dispatch" —
 * and it is a visually different modal (its own title bar, its own field
 * set) from `features/sprints/DispatchSprintModal.tsx`, not a themed
 * variant of it. The two features are deliberately unaware of each other.
 */
const RunWithAgentModal: Component<RunWithAgentModalProps> = (props) => {
  const store = useExecutionStore();

  const [project] = createResource(
    () => (props.isOpen ? props.projectId : undefined),
    (id) => api.projects.get(id),
  );
  const [fleets] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => fleetsApi.list().then((r) => r.data.data),
  );
  const [agentProfiles, { refetch: refetchAgentProfiles }] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => agentProfilesApi.list().then((r) => r.data.data),
  );
  // Always fetched live — this drives the target picker (names/ids, never
  // hand-typed) and the "agent execution is off" state, neither of which
  // `props.capabilities` (a gate-input override for tests) replaces.
  const [liveRunners] = createResource(
    () => (props.isOpen ? 'open' : undefined),
    () => runnersApi.list().then((r) => r.data.data),
  );

  const runnersData = (): RunnerSummary[] => (liveRunners.error !== undefined ? [] : (liveRunners() ?? []));
  const activeRunners = (): RunnerSummary[] => runnersData().filter((r) => isActiveRunnerState(r.state));

  // The submit gate's own input. Scoped to `targetCapabilities()` (the
  // selected runner, or every active member of a selected fleet) — never
  // the whole runner population — because that's exactly what the real
  // scheduler evaluates per candidate (`crates/tack-orch/src/scheduler/
  // select.rs`'s `evaluate_candidate`, called once per eligible runner
  // implied by the request's own selector, never against an unrelated
  // runner elsewhere in the fleet). `props.capabilities` still overrides
  // this outright for tests, matching its own doc comment: it replaces the
  // gate's input, never which machines/groups the picker itself shows.
  const capabilities = (): RunnerCapabilities[] => {
    if (props.capabilities) return props.capabilities();
    return targetCapabilities();
  };

  // Resources throw once errored — read through a safe accessor everywhere
  // (same pattern, and same reasoning, as `DispatchSprintModal.tsx#dryRunData`
  // and `ItemDetailDrawer.tsx#agentActivityData`: calling an errored Solid
  // resource accessor from within a reactive computation aborts that batch).
  const fleetsData = (): FleetSummary[] => (fleets.error !== undefined ? [] : (fleets() ?? []));
  const agentProfilesData = (): AgentProfileSummary[] =>
    agentProfiles.error !== undefined ? [] : (agentProfiles() ?? []);

  // "Agent execution is off" — see `shared.ts#isExecutionOff`'s own doc
  // comment for the signal this reads. Only claimed once the fetch has
  // actually resolved (never during the initial loading flash, and never on
  // a fetch error, which is "unknown", not "off").
  const executionOff = createMemo(
    () => !liveRunners.loading && liveRunners.error === undefined && isExecutionOff(activeRunners().length),
  );

  const [selectorKind, setSelectorKind] = createSignal<'fleet' | 'exact_runner'>('fleet');
  const [selectorId, setSelectorId] = createSignal('');
  const [agentProfileId, setAgentProfileId] = createSignal('');
  const [creatingProfile, setCreatingProfile] = createSignal(false);
  const [harnessKind, setHarnessKind] = createSignal(HARNESS_KINDS[0].value);
  const [modelMode, setModelMode] = createSignal<'project' | 'choose' | 'auto'>('auto');
  const [modelModeInitialized, setModelModeInitialized] = createSignal(false);
  const [chooseIndex, setChooseIndex] = createSignal('');
  const [customProvider, setCustomProvider] = createSignal('');
  const [customModelId, setCustomModelId] = createSignal('');
  const [timeoutSeconds, setTimeoutSeconds] = createSignal(3600);
  const [allowNetwork, setAllowNetwork] = createSignal(false);
  const [toolsText, setToolsText] = createSignal('');
  const [repoExpanded, setRepoExpanded] = createSignal(false);
  const [repoKind, setRepoKind] = createSignal('git');
  const [repoRemote, setRepoRemote] = createSignal('');
  const [repoBaseRevision, setRepoBaseRevision] = createSignal('main');
  const [repoSubdirectory, setRepoSubdirectory] = createSignal('');
  const [submitting, setSubmitting] = createSignal(false);
  const [idempotencyKey, setIdempotencyKey] = createSignal(generateIdempotencyKey());

  // Reset to a fresh form (including a fresh idempotency key) every time the
  // modal opens for a — possibly different — item, so a leftover selection
  // from a previous run is never silently resubmitted against a new item.
  createEffect(() => {
    if (!props.isOpen) return;
    setSelectorKind('fleet');
    setSelectorId('');
    setAgentProfileId('');
    setHarnessKind(HARNESS_KINDS[0].value);
    setModelMode('auto');
    setModelModeInitialized(false);
    setChooseIndex('');
    setCustomProvider('');
    setCustomModelId('');
    setTimeoutSeconds(3600);
    setAllowNetwork(false);
    setToolsText('');
    setRepoExpanded(false);
    setRepoKind('git');
    setRepoRemote('');
    setRepoBaseRevision('main');
    setRepoSubdirectory('');
    setIdempotencyKey(generateIdempotencyKey());
  });

  // "Hidden when exactly one machine is active — the common case": the one
  // active runner is used directly, with no id ever hand-typed or even
  // shown. Guarded on `!selectorId()` so it only ever fills a blank
  // selection (the reset effect above), never fights a later manual pick.
  createEffect(() => {
    if (!props.isOpen || selectorId()) return;
    const runners = activeRunners();
    if (runners.length === 1 && fleetsData().length === 0) {
      setSelectorKind('exact_runner');
      setSelectorId(runners[0].runner_id);
    }
  });

  const hideTargetPicker = createMemo(() => shouldHideTargetPicker(activeRunners().length, fleetsData().length));

  interface TargetOption { value: string; label: string; kind: 'fleet' | 'exact_runner'; id: string }
  const targetOptions = createMemo<TargetOption[]>(() => [
    ...fleetsData().map((f) => ({ value: `fleet:${f.fleet_id}`, label: f.name, kind: 'fleet' as const, id: f.fleet_id })),
    ...activeRunners().map((r) => ({ value: `exact_runner:${r.runner_id}`, label: r.name, kind: 'exact_runner' as const, id: r.runner_id })),
  ]);

  // The capability reports belonging only to the currently selected target
  // — a specific runner, or every active member of a fleet — so the
  // Harness list and the "Choose…" model list reflect what THIS target
  // reports, not the whole runner population.
  const targetCapabilities = createMemo((): RunnerCapabilities[] => {
    const kind = selectorKind();
    const id = selectorId();
    if (!id) return [];
    const runners =
      kind === 'exact_runner'
        ? activeRunners().filter((r) => r.runner_id === id)
        : activeRunners().filter((r) => r.fleet_ids.includes(id));
    return runners.map(runnerSummaryToCapabilities).filter((c): c is RunnerCapabilities => c !== null);
  });

  const harnessOptions = createMemo(() => {
    if (liveRunners.loading) return HARNESS_KINDS;
    const reported = listReportedHarnessKinds(targetCapabilities());
    const filtered = HARNESS_KINDS.filter((h) => reported.includes(h.value));
    // Never brick the form on absent/incomplete capability data — the real
    // enforcement is `gateHarnessModelSelection` below, unchanged.
    return filtered.length > 0 ? filtered : HARNESS_KINDS;
  });

  const selectedAgentProfile = createMemo(() =>
    agentProfilesData().find((p) => p.agent_profile_id === agentProfileId()),
  );

  // No project-level "default agent profile" storage exists — only
  // `projects.default_model`. The one default that IS honest
  // without it: when exactly one agent profile exists, use it — the same
  // "a real, unambiguous choice needs no picker" reasoning the target
  // picker above applies to a single active runner.
  createEffect(() => {
    if (!props.isOpen || agentProfileId()) return;
    const profiles = agentProfilesData();
    if (profiles.length === 1) setAgentProfileId(profiles[0].agent_profile_id);
  });

  const createDefaultProfile = async () => {
    setCreatingProfile(true);
    try {
      const result = await agentProfilesApi.create({
        name: 'Default',
        instructions: 'Complete the requested change and summarize what changed.',
        tool_policy: {},
      });
      await refetchAgentProfiles();
      setAgentProfileId(result.agent_profile_id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to create the default agent profile.');
    } finally {
      setCreatingProfile(false);
    }
  };

  const projectDefaultLabel = createMemo(() => describeProjectModelDefault(project()?.default_model ?? null));

  // Defaults to "Project default" the first time the project resource
  // resolves with an opinion, "Auto" otherwise — never fights a manual
  // choice afterwards (`modelModeInitialized`, reset alongside the rest of
  // the form on open).
  createEffect(() => {
    if (!props.isOpen || modelModeInitialized() || project.loading) return;
    setModelMode(projectDefaultLabel() ? 'project' : 'auto');
    setModelModeInitialized(true);
  });

  const modelCombos = createMemo<AggregatedModelCombination[]>(() =>
    listModelCombinationsForHarness(targetCapabilities(), harnessKind()),
  );

  const targetHarnessCapability = createMemo(() => {
    for (const cap of targetCapabilities()) {
      const h = cap.harnesses.find((h) => h.harness_kind === harnessKind());
      if (h) return h;
    }
    return undefined;
  });
  const passthroughAttested = createMemo(() => isModelPassthroughAttested(targetHarnessCapability()));

  const modelProvider = (): string | null => {
    if (modelMode() === 'project') return projectDefaultModelPair(project()?.default_model ?? null).provider;
    if (modelMode() === 'choose') {
      if (chooseIndex() === CUSTOM_MODEL_VALUE) return customProvider().trim() || null;
      const idx = Number(chooseIndex());
      return Number.isFinite(idx) ? (modelCombos()[idx]?.model_provider ?? null) : null;
    }
    return null;
  };
  const modelId = (): string | null => {
    if (modelMode() === 'project') return projectDefaultModelPair(project()?.default_model ?? null).id;
    if (modelMode() === 'choose') {
      if (chooseIndex() === CUSTOM_MODEL_VALUE) return customModelId().trim() || null;
      const idx = Number(chooseIndex());
      return Number.isFinite(idx) ? (modelCombos()[idx]?.model_id ?? null) : null;
    }
    return null;
  };

  // What an Auto ("let the runner decide") request would actually resolve to
  // for the currently selected agent profile / project / target — computed
  // fresh from data this modal already fetches for other purposes (the
  // agent profile list, the project resource, the fleet list), never a
  // second network round-trip. See `shared.ts#resolveAutoModelPolicy`'s doc
  // comment for why this needs three tiers, not just the project's own
  // default this fieldset already shows.
  const autoModelResolution = createMemo(() =>
    resolveAutoModelPolicy(
      selectedAgentProfile()?.limits,
      project()?.default_model ?? null,
      selectorKind() === 'fleet' ? fleetsData().find((f) => f.fleet_id === selectorId())?.default_policy : null,
    ),
  );

  const combinationGate = createMemo(() =>
    gateHarnessModelSelection(
      capabilities(),
      harnessKind(),
      modelProvider(),
      modelId(),
      autoModelResolution(),
      `/projects/${props.projectId}/settings?tab=agents`,
    ),
  );

  // Structural (non-capability) validation — every reason is shown, never a
  // silently-disabled control.
  const structuralErrors = createMemo((): string[] => {
    const errors: string[] = [];
    if (!selectorId().trim()) errors.push('Select where this runs.');
    if (!agentProfileId()) errors.push('Select an agent profile.');
    if (!harnessKind()) errors.push('Select a harness.');
    if (modelMode() === 'choose' && chooseIndex() === '') errors.push('Select a model, or switch to Auto.');
    if (modelMode() === 'choose' && chooseIndex() === CUSTOM_MODEL_VALUE && !customModelId().trim()) {
      errors.push('Enter a model id, or pick one from the list.');
    }
    if (!repoRemote().trim()) errors.push('Enter a repository remote.');
    if (!repoBaseRevision().trim()) errors.push('Enter a base revision.');
    if (!Number.isFinite(timeoutSeconds()) || timeoutSeconds() <= 0) errors.push('Timeout must be a positive number of seconds.');
    return errors;
  });

  const canSubmit = createMemo(
    () => structuralErrors().length === 0 && combinationGate().allowed && !submitting(),
  );

  const buildValues = (): RunWithAgentFormValues => {
    const profile = selectedAgentProfile();
    return {
      itemId: props.itemId,
      selectorKind: selectorKind(),
      selectorId: selectorId().trim(),
      agentProfileId: agentProfileId(),
      agentProfileSnapshot: {
        name: profile?.name ?? '',
        instructions: profile?.instructions ?? '',
        tool_policy: profile?.tool_policy ?? {},
      },
      harnessKind: harnessKind(),
      modelProvider: modelProvider(),
      modelId: modelId(),
      timeoutSeconds: timeoutSeconds(),
      allowNetwork: allowNetwork(),
      tools: toolsText()
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean),
      repository: {
        kind: repoKind(),
        remote: repoRemote().trim(),
        baseRevision: repoBaseRevision().trim(),
        subdirectory: repoSubdirectory().trim() || null,
      },
      idempotencyKey: idempotencyKey(),
    };
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!canSubmit()) return;
    setSubmitting(true);
    try {
      const input = buildCreateExecutionInput(buildValues());
      const result = await store.create(input);
      toast.success(result.replayed ? 'Reused an existing run for this item.' : 'Run started.');
      props.onCreated?.(result.request_id);
      props.onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start the run.');
    } finally {
      setSubmitting(false);
    }
  };

  const repoSummary = () =>
    repoRemote().trim()
      ? `${repoKind()} — ${repoRemote()} @ ${repoBaseRevision()}${repoSubdirectory() ? ` / ${repoSubdirectory()}` : ''}`
      : 'No repository configured for this run yet.';

  return (
    <Modal isOpen={props.isOpen} onClose={props.onClose} title={`Run with agent: ${props.itemTitle}`} size="lg">
      <Show
        when={!executionOff()}
        fallback={
          <div class="space-y-3 py-6 text-center text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <p>Agent execution is off.</p>
            <A
              href="/agents"
              class="inline-flex items-center gap-1 font-medium"
              style={{ color: 'var(--color-primary-600)' }}
            >
              Turn it on
            </A>
          </div>
        }
      >
        <form class="space-y-5" onSubmit={submit}>
          {/* ── Target ──────────────────────────────────────────────────── */}
          <Show when={!hideTargetPicker()}>
            <fieldset class="space-y-2">
              <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Where it runs
              </legend>
              <Select
                label="Machine or group"
                value={selectorId() ? `${selectorKind()}:${selectorId()}` : ''}
                onInput={(e) => {
                  const [kind, id] = e.currentTarget.value.split(/:(.*)/s);
                  if (kind === 'fleet' || kind === 'exact_runner') {
                    setSelectorKind(kind);
                    setSelectorId(id ?? '');
                  } else {
                    setSelectorId('');
                  }
                }}
                options={[
                  { value: '', label: liveRunners.loading ? 'Loading…' : 'Select where this runs' },
                  ...targetOptions().map((o) => ({ value: o.value, label: o.label })),
                ]}
              />
            </fieldset>
          </Show>

          {/* ── Agent ───────────────────────────────────────────────────── */}
          <fieldset class="space-y-3">
            <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Agent
            </legend>
            <Show
              when={agentProfiles.loading || agentProfilesData().length > 0}
              fallback={
                <div class="space-y-2">
                  <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>No agent profile exists yet.</p>
                  <Button size="sm" variant="secondary" loading={creatingProfile()} onClick={createDefaultProfile}>
                    Create default profile
                  </Button>
                </div>
              }
            >
              <Select
                label="Agent profile"
                value={agentProfileId()}
                onInput={(e) => setAgentProfileId(e.currentTarget.value)}
                options={[
                  { value: '', label: agentProfiles.loading ? 'Loading…' : 'Select an agent profile' },
                  ...agentProfilesData().map((p) => ({ value: p.agent_profile_id, label: p.name })),
                ]}
              />
            </Show>

            <Select
              label="Harness"
              value={harnessKind()}
              onInput={(e) => setHarnessKind(e.currentTarget.value)}
              options={harnessOptions().map((h) => ({ value: h.value, label: h.label }))}
            />

            <div class="space-y-2">
              <div class="flex flex-col gap-1.5 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                <Show when={projectDefaultLabel()}>
                  {(label) => (
                    <label class="flex items-center gap-1.5">
                      <input
                        type="radio"
                        name="model-mode"
                        checked={modelMode() === 'project'}
                        onChange={() => setModelMode('project')}
                      />
                      Project default — {label()}
                    </label>
                  )}
                </Show>
                <label class="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="model-mode"
                    checked={modelMode() === 'choose'}
                    onChange={() => setModelMode('choose')}
                  />
                  Choose…
                </label>
                <label class="flex items-center gap-1.5">
                  <input type="radio" name="model-mode" checked={modelMode() === 'auto'} onChange={() => setModelMode('auto')} />
                  Auto (let the runner decide)
                </label>
              </div>
              <Show when={modelMode() === 'choose'}>
                <Select
                  label="Model"
                  value={chooseIndex()}
                  onInput={(e) => setChooseIndex(e.currentTarget.value)}
                  options={[
                    { value: '', label: 'Select a model' },
                    ...modelCombos().map((c, i) => ({
                      value: String(i),
                      label: `${c.model_provider} / ${c.model_id} (${c.supportingRunnerCount} runner${c.supportingRunnerCount === 1 ? '' : 's'})`,
                    })),
                    ...(passthroughAttested() ? [{ value: CUSTOM_MODEL_VALUE, label: 'Other (type a model id)' }] : []),
                  ]}
                />
                <Show when={chooseIndex() === CUSTOM_MODEL_VALUE}>
                  <div class="grid grid-cols-2 gap-3">
                    <Field label="Provider" value={customProvider()} onInput={(e) => setCustomProvider(e.currentTarget.value)} />
                    <Field label="Model id" value={customModelId()} onInput={(e) => setCustomModelId(e.currentTarget.value)} />
                  </div>
                </Show>
              </Show>
              <CombinationGateNote gate={combinationGate()} />
            </div>
          </fieldset>

          {/* ── Repository ──────────────────────────────────────────────── */}
          <fieldset class="space-y-2">
            <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Repository
            </legend>
            <Show
              when={repoExpanded()}
              fallback={
                <div class="flex items-center justify-between gap-3 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                  <span>{repoSummary()}</span>
                  <Button type="button" size="sm" variant="secondary" onClick={() => setRepoExpanded(true)}>
                    Change for this run
                  </Button>
                </div>
              }
            >
              <div class="grid grid-cols-2 gap-3">
                <Field label="Kind" value={repoKind()} onInput={(e) => setRepoKind(e.currentTarget.value)} />
                <Field
                  label="Base revision"
                  value={repoBaseRevision()}
                  onInput={(e) => setRepoBaseRevision(e.currentTarget.value)}
                  placeholder="main"
                />
              </div>
              <Field
                label="Remote"
                value={repoRemote()}
                onInput={(e) => setRepoRemote(e.currentTarget.value)}
                placeholder="git@github.com:org/repo.git"
              />
              <Field
                label="Subdirectory"
                value={repoSubdirectory()}
                onInput={(e) => setRepoSubdirectory(e.currentTarget.value)}
                hint="Optional."
              />
            </Show>
          </fieldset>

          {/* ── Permissions & budget ────────────────────────────────────── */}
          <fieldset class="space-y-3">
            <legend class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Permissions &amp; budget
            </legend>
            <Field
              label="Timeout (seconds)"
              type="number"
              min="1"
              value={timeoutSeconds()}
              onInput={(e) => setTimeoutSeconds(Number(e.currentTarget.value))}
            />
            <Field
              label="Allowed tools"
              value={toolsText()}
              onInput={(e) => setToolsText(e.currentTarget.value)}
              hint="Comma-separated. Leave blank for none."
            />
            <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
              <input type="checkbox" checked={allowNetwork()} onChange={(e) => setAllowNetwork(e.currentTarget.checked)} />
              Allow network access
            </label>
          </fieldset>

          <Show when={structuralErrors().length > 0}>
            <ul class="space-y-1 text-xs" style={{ color: 'var(--color-danger-600)' }}>
              <For each={structuralErrors()}>{(msg) => <li>{msg}</li>}</For>
            </ul>
          </Show>

          <div class="flex justify-end gap-2 border-t pt-3" style={{ 'border-color': 'var(--color-border-light)' }}>
            <Button type="button" variant="secondary" onClick={props.onClose} disabled={submitting()}>
              Cancel
            </Button>
            <Button type="submit" loading={submitting()} disabled={!canSubmit()}>
              Run
            </Button>
          </div>
        </form>
      </Show>
    </Modal>
  );
};

const CombinationGateNote: Component<{ gate: ReturnType<typeof gateHarnessModelSelection> }> = (props) => (
  <div class="space-y-1">
    <p class="flex items-start gap-1.5 text-xs" style={{ color: props.gate.advisory ? 'var(--color-warning-700)' : props.gate.allowed ? 'var(--color-success-700)' : 'var(--color-danger-600)' }}>
      <Show when={!props.gate.allowed}>
        <Badge tone="danger">Unsupported</Badge>
      </Show>
      <Show when={props.gate.allowed && props.gate.advisory}>
        <Badge tone="warning">Unverified</Badge>
      </Show>
      <Show when={props.gate.allowed && !props.gate.advisory}>
        <Badge tone="success">Supported</Badge>
      </Show>
      <span>{props.gate.reason}</span>
    </p>
    <Show when={props.gate.fix}>
      {(fix) => (
        <A
          href={fix().href}
          class="inline-flex items-center gap-1 text-xs font-medium"
          style={{ color: 'var(--color-primary-600)' }}
        >
          {fix().label}
        </A>
      )}
    </Show>
  </div>
);

export default RunWithAgentModal;

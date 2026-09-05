import { type Component, Show, createEffect, createResource, createSignal, onCleanup, onMount } from 'solid-js';
import { api } from '../../shared/api';
import { Select } from '../../shared/ui';
import { runnersApi } from '../../shared/execution';
import { localRunnerApi, isLocalRunnerUnavailable } from './api';
import ExecutionToggle from './ExecutionToggle';
import HarnessStep from './steps/HarnessStep';
import ProviderStep from './steps/ProviderStep';
import ModelDefaultStep from './steps/ModelDefaultStep';
import TestRunStep from './steps/TestRunStep';
import AdvancedSection from './AdvancedSection';
import { countOtherActiveRunners, findThisMachineRunner, isThisMachineRunner } from './runnerObservations';
import { getRememberedAgentsProjectId, setRememberedAgentsProjectId } from './agentsProjectPreference';

/** How often this page re-polls `GET /api/runners` while mounted — enough
 *  to notice a toggle/re-check without the operator reloading, without
 *  hammering the endpoint. `ExecutionToggle` manages its own separate
 *  fetch of `/api/local-runner` with no way for a sibling to subscribe to
 *  it, so this is a plain poll rather than an event. */
const RUNNERS_POLL_MS = 1500;

/**
 * The Agents page — five numbered, API-observed steps from an installed
 * binary to a completed attempt, plus an Advanced section for running
 * agents on other machines. Composes `ExecutionToggle`/`ProviderKeyPanel`
 * (each independently fetches and owns its own state) with the harness,
 * provider-login, default-model and test-run steps, and the `runnerFleet/`
 * management tree.
 */
const AgentsPage: Component = () => {
  const [runnersResult, { refetch: refetchRunners }] = createResource(() => runnersApi.list());
  const [localRunnerStatus, { refetch: refetchLocalRunnerStatus }] = createResource(() => localRunnerApi.get());
  const localRunnerUnavailable = () => isLocalRunnerUnavailable(localRunnerStatus.error);

  // Turning the embedded runner off does not revoke or remove its
  // enrollment row (confirmed live: `state` stays `active`, only
  // `last_heartbeat_at` freezes) — so "this machine's own row" is only
  // trustworthy while `/api/local-runner` itself reports `running`.
  // Filtering it out otherwise, once, here keeps every step below (harness
  // list, provider logins, model-default union, test-run target) from
  // separately re-deriving the same check.
  const runners = () => {
    const all = runnersResult()?.data.data ?? [];
    if (localRunnerStatus()?.state === 'running') return all;
    return all.filter((r) => !isThisMachineRunner(r));
  };
  const thisMachineRunner = () => findThisMachineRunner(runners());
  const otherActiveCount = () => countOtherActiveRunners(runners());

  let pollHandle: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    pollHandle = setInterval(() => {
      void refetchRunners();
      void refetchLocalRunnerStatus();
    }, RUNNERS_POLL_MS);
  });
  onCleanup(() => {
    if (pollHandle) clearInterval(pollHandle);
  });

  const [rechecking, setRechecking] = createSignal(false);
  const recheck = async () => {
    setRechecking(true);
    try {
      await localRunnerApi.update(false);
      await localRunnerApi.update(true);
      await Promise.all([refetchRunners(), refetchLocalRunnerStatus()]);
    } finally {
      setRechecking(false);
    }
  };

  // Step 4/5 need a project; this route carries no `:id` (it is a global
  // page, not nested under `/projects/:id`), so it keeps its own
  // selection, remembered per browser.
  const [projects] = createResource(() => api.projects.list());
  const [selectedProjectId, setSelectedProjectId] = createSignal('');
  createEffect(() => {
    if (selectedProjectId()) return;
    const list = projects();
    if (!list || list.length === 0) return;
    const remembered = getRememberedAgentsProjectId();
    setSelectedProjectId(remembered && list.some((p) => p.id === remembered) ? remembered : list[0].id);
  });
  createEffect(() => {
    const id = selectedProjectId();
    if (id) setRememberedAgentsProjectId(id);
  });
  const [project, { refetch: refetchProject }] = createResource(selectedProjectId, (id) =>
    id ? api.projects.get(id) : null,
  );

  const [verifiedHarnessKinds, setVerifiedHarnessKinds] = createSignal<ReadonlySet<string>>(new Set());
  const markVerified = (harnessKind: string) => {
    setVerifiedHarnessKinds((prev) => new Set(prev).add(harnessKind));
  };

  return (
    <div class="max-w-3xl space-y-8">
      <div>
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Agents
        </h1>
        <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Turn on an agent, give it a model provider, and run a test — the whole path from
          an installed binary to a completed run.
        </p>
      </div>

      <Show when={(projects()?.length ?? 0) > 1}>
        <Select
          label="Project"
          value={selectedProjectId()}
          onChange={(e) => setSelectedProjectId(e.currentTarget.value)}
          options={(projects() ?? []).map((p) => ({ value: p.id, label: p.name }))}
        />
      </Show>

      <section class="space-y-2">
        <ExecutionToggle />
        <Show when={!localRunnerUnavailable() && otherActiveCount() > 0}>
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            {otherActiveCount()} other machine{otherActiveCount() === 1 ? ' is' : 's are'} running agents —{' '}
            <a href="#advanced" class="underline">see Advanced</a>.
          </p>
        </Show>
      </section>

      <HarnessStep thisMachineRunner={thisMachineRunner()} onRecheck={() => void recheck()} rechecking={rechecking()} />

      <ProviderStep thisMachineRunner={thisMachineRunner()} verifiedHarnessKinds={verifiedHarnessKinds()} />

      <ModelDefaultStep project={project()} runners={runners()} onSaved={() => void refetchProject()} />

      <TestRunStep project={project()} runners={runners()} onVerified={markVerified} />

      <AdvancedSection />
    </div>
  );
};

export default AgentsPage;

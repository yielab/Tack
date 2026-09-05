import { type Component, Show, createEffect, createSignal } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field } from '../../../shared/ui';
import { agentProfilesApi } from '../../../shared/execution';
import type { RunnerSummary } from '../../../shared/execution/api';
import type { Project } from '../../../shared/types';
import { useExecutionStore } from '../../../shared/state/executionContext';
import { buildCreateExecutionInput } from '../../../shared/runWithAgent/shared';
import ExecutionTimeline from '../../../shared/runWithAgent/ExecutionTimeline';
import { isThisMachineRunner, pickTestRunTarget } from '../runnerObservations';

export interface TestRunStepProps {
  project: Project | null | undefined;
  runners: readonly RunnerSummary[];
  /** Called once this step observes its own created request reach
   *  `succeeded` — the harness kind it ran is now confirmed working. */
  onVerified: (harnessKind: string) => void;
}

/** The literal instruction every test run sends — proves the resolved model
 *  back to the operator rather than asking them to trust the configuration. */
const TEST_RUN_INSTRUCTIONS = 'print which model you are and exit';
const TEST_RUN_ITEM_TITLE = 'Agent test';

/**
 * Step 5 — the only step that actually exercises the path end to end.
 * Creates one item, one execution against whichever active runner/harness
 * `pickTestRunTarget` selects, and renders `ExecutionTimeline` (`shared/
 * runWithAgent/**`, unedited) for it. No project-level repository default
 * exists anywhere in this tree yet — the remote and base revision are one
 * manual field each here too, matching the same gap the "Run with agent"
 * modal already has, rather than a default invented just for this screen.
 */
const TestRunStep: Component<TestRunStepProps> = (props) => {
  const store = useExecutionStore();
  const [itemId, setItemId] = createSignal<string | null>(null);
  const [remote, setRemote] = createSignal('');
  const [baseRevision, setBaseRevision] = createSignal('main');
  const [modelProvider, setModelProvider] = createSignal('');
  const [modelId, setModelId] = createSignal('');
  const [running, setRunning] = createSignal(false);
  const [ranHarnessKind, setRanHarnessKind] = createSignal<string | null>(null);

  const target = () => pickTestRunTarget(props.runners);

  // Pre-fill provider/model from the project's own explicit default the
  // first time one becomes available, without overwriting anything the
  // operator already typed.
  createEffect(() => {
    const defaultModel = props.project?.default_model;
    if (defaultModel && defaultModel.kind === 'explicit' && !modelProvider() && !modelId()) {
      setModelProvider(defaultModel.provider);
      setModelId(defaultModel.model_id);
    }
  });

  // Watches this step's own created request for a real, observed
  // "succeeded" state — the only signal that flips step 3's badge from
  // "present, unverified" to "verified" for the harness this run used.
  createEffect(() => {
    const id = itemId();
    const harnessKind = ranHarnessKind();
    if (!id || !harnessKind) return;
    const succeeded = store.requestsForItem(id).some((r) => r.summary?.state === 'succeeded');
    if (succeeded) props.onVerified(harnessKind);
  });

  const run = async () => {
    const project = props.project;
    const picked = target();
    if (!project || !picked) return;
    if (!remote().trim() || !modelProvider().trim() || !modelId().trim()) {
      toast.error('Repository remote, provider and model id are all required');
      return;
    }
    setRunning(true);
    try {
      const item = await api.items.create(project.id, { title: TEST_RUN_ITEM_TITLE });
      setItemId(item.id);

      const profiles = await agentProfilesApi.list();
      let agentProfileId = profiles.data.data[0]?.agent_profile_id;
      if (!agentProfileId) {
        const created = await agentProfilesApi.create({
          name: TEST_RUN_ITEM_TITLE,
          instructions: TEST_RUN_INSTRUCTIONS,
        });
        agentProfileId = created.agent_profile_id;
      }

      const input = buildCreateExecutionInput({
        itemId: item.id,
        selectorKind: 'exact_runner',
        selectorId: picked.runner.runner_id,
        agentProfileId,
        agentProfileSnapshot: { name: TEST_RUN_ITEM_TITLE, instructions: TEST_RUN_INSTRUCTIONS, tool_policy: {} },
        harnessKind: picked.harness.harness_kind,
        modelProvider: modelProvider().trim(),
        modelId: modelId().trim(),
        timeoutSeconds: 600,
        allowNetwork: false,
        tools: [],
        repository: { kind: 'git', remote: remote().trim(), baseRevision: baseRevision().trim() || 'main', subdirectory: null },
        idempotencyKey: `agents-page-test-run-${Date.now()}`,
      });
      await store.create(input);
      setRanHarnessKind(picked.harness.harness_kind);
      toast.success('Test run started');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to start the test run');
    } finally {
      setRunning(false);
    }
  };

  return (
    <section class="space-y-3">
      <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        Test run
      </h2>

      <Show
        when={props.project}
        fallback={
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Choose a project above to run a test.
          </p>
        }
      >
        <Show
          when={target()}
          fallback={
            <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              No active, installed agent to test — turn on agent execution (step 1) or
              install one (step 2) first.
            </p>
          }
        >
          {(picked) => (
            <Show
              when={!itemId()}
              fallback={
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                  Runs {picked().harness.harness_kind} on{' '}
                  {isThisMachineRunner(picked().runner) ? 'this machine' : picked().runner.name}.
                </p>
              }
            >
              <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                Creates an "{TEST_RUN_ITEM_TITLE}" item on this project and runs it against{' '}
                {picked().harness.harness_kind} on{' '}
                {isThisMachineRunner(picked().runner) ? 'this machine' : picked().runner.name}.
              </p>
              <div class="grid gap-3 sm:grid-cols-2">
                <Field label="Repository remote" placeholder="git@example.com:org/repo.git" value={remote()} onInput={(e) => setRemote(e.currentTarget.value)} />
                <Field label="Base revision" placeholder="main" value={baseRevision()} onInput={(e) => setBaseRevision(e.currentTarget.value)} />
                <Field label="Provider" placeholder="openai" value={modelProvider()} onInput={(e) => setModelProvider(e.currentTarget.value)} />
                <Field label="Model ID" placeholder="opaque/model-alpha" value={modelId()} onInput={(e) => setModelId(e.currentTarget.value)} />
              </div>
              <Button onClick={() => void run()} loading={running()} disabled={running()}>
                Run test
              </Button>
            </Show>
          )}
        </Show>
      </Show>

      <Show when={itemId()}>
        <ExecutionTimeline itemId={itemId()!} />
      </Show>
    </section>
  );
};

export default TestRunStep;

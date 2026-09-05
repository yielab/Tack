import { type Component, For, Show } from 'solid-js';
import { Badge, Button } from '../../../shared/ui';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { RunnerSummary } from '../../../shared/execution/api';
import type { HarnessCapability } from '../../../shared/execution/types';
import { findHarness, harnessesOf } from '../runnerObservations';
import { HARNESS_INSTALL_COMMAND } from '../constants';

export interface HarnessStepProps {
  /** This machine's own active runner row, or `null` when agent execution
   *  is off (step 1) — there is nothing to probe without it. */
  thisMachineRunner: RunnerSummary | null;
  /** Restarts the embedded runner to force a fresh probe — the only way
   *  this build can re-probe (`GET /api/runners` itself returns whatever
   *  the runner last reported at its own startup, never re-probes on
   *  read). */
  onRecheck: () => void;
  rechecking: boolean;
}

/** One harness row's status, entirely from the runner's own probe — never
 *  a check mark derived from anything this page assumes. A harness kind
 *  this runner's snapshot never mentions at all is treated the same as an
 *  explicit "not found on PATH": this build's two adapters always probe
 *  both known harnesses, so a missing entry only happens against an older
 *  or fake snapshot that never ran the probe. */
function statusLabel(harness: HarnessCapability | undefined): { text: string; tone: 'success' | 'neutral' | 'warning' } {
  if (harness && harness.probe_error === null && harness.installed_version) {
    return { text: `Installed v${harness.installed_version}`, tone: 'success' };
  }
  if (harness?.probe_error && !/not found on path/i.test(harness.probe_error)) {
    return { text: 'Could not check', tone: 'warning' };
  }
  return { text: 'Not found', tone: 'neutral' };
}

const HarnessStep: Component<HarnessStepProps> = (props) => {
  return (
    <section class="space-y-3">
      <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        Agents on this machine
      </h2>

      <Show
        when={props.thisMachineRunner}
        fallback={
          <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Turn on agent execution above to see what's installed here.
          </p>
        }
      >
        <div class="space-y-2">
          <For each={HARNESS_KINDS}>
            {(kind) => {
              const harness = () => findHarness(harnessesOf(props.thisMachineRunner), kind.value);
              const status = () => statusLabel(harness());
              return (
                <div class="flex flex-wrap items-center gap-2 rounded-lg border p-3 text-sm" style={{ 'border-color': 'var(--color-border-light)' }}>
                  <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>{kind.label}</span>
                  <Badge tone={status().tone}>{status().text}</Badge>
                  <Show when={status().text === 'Not found'}>
                    <code class="rounded px-2 py-0.5 font-mono text-xs" style={{ background: 'var(--color-chip)', color: 'var(--color-text-secondary)' }}>
                      {HARNESS_INSTALL_COMMAND[kind.value] ?? 'see the vendor\'s own install instructions'}
                    </code>
                  </Show>
                  <Show when={status().text === 'Could not check'}>
                    <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>{harness()?.probe_error}</span>
                  </Show>
                </div>
              );
            }}
          </For>
          <Button variant="secondary" size="sm" loading={props.rechecking} onClick={props.onRecheck}>
            Re-check
          </Button>
          <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            Re-checking restarts agent execution on this machine to run a fresh check.
          </p>
        </div>
      </Show>
    </section>
  );
};

export default HarnessStep;

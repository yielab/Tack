import { type Component, For, Show } from 'solid-js';
import { Badge } from '../../../shared/ui';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { RunnerSummary } from '../../../shared/execution/api';
import { findHarness, harnessesOf } from '../runnerObservations';
import { CANNOT_OBSERVE_VENDOR_LOGIN, HARNESS_LOGIN_COMMAND } from '../constants';
import ProviderKeyPanel from '../ProviderKeyPanel';

export interface ProviderStepProps {
  thisMachineRunner: RunnerSummary | null;
  /** Whether step 5's test run has, in this browser tab, completed at
   *  least once against the given harness kind. Never persisted — a reload
   *  is honestly "unverified" again, since nothing here reads back a
   *  completed run's history for every harness that ever existed: no
   *  status here is ever derived from file existence, and none of it is
   *  fabricated memory either. */
  verifiedHarnessKinds: ReadonlySet<string>;
}

/**
 * Step 3 — two independent paths to a credentialed agent, side by side.
 * "Use the agent's own login" is per-installed-harness and purely
 * instructional (Tack has no route that reads back a vendor login's
 * result); "Use Vercel AI Gateway" mounts `ProviderKeyPanel` unchanged.
 */
const ProviderStep: Component<ProviderStepProps> = (props) => {
  return (
    <section class="space-y-4">
      <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        Provider
      </h2>

      <div class="grid gap-4 md:grid-cols-2">
        <div class="space-y-3 rounded-lg border p-4" style={{ 'border-color': 'var(--color-border-light)' }}>
          <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Use the agent's own login
          </h3>
          <Show
            when={props.thisMachineRunner}
            fallback={
              <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                Turn on agent execution above to see which agents are installed here.
              </p>
            }
          >
            <For each={HARNESS_KINDS}>
              {(kind) => {
                const harness = () => findHarness(harnessesOf(props.thisMachineRunner), kind.value);
                const installed = () => harness()?.probe_error === null && harness()?.installed_version;
                const verified = () => props.verifiedHarnessKinds.has(kind.value);
                return (
                  <Show when={installed()}>
                    <div class="space-y-1.5 text-sm">
                      <div class="flex items-center gap-2">
                        <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>{kind.label}</span>
                        <Badge tone={verified() ? 'success' : 'neutral'}>
                          {verified() ? 'Verified' : 'Present, unverified'}
                        </Badge>
                      </div>
                      <pre
                        class="overflow-x-auto rounded-lg border p-2 font-mono text-xs"
                        style={{ 'border-color': 'var(--color-border-light)', color: 'var(--color-text-primary)' }}
                      >
                        {HARNESS_LOGIN_COMMAND[kind.value] ?? kind.value}
                      </pre>
                      <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                        {CANNOT_OBSERVE_VENDOR_LOGIN}
                      </p>
                    </div>
                  </Show>
                );
              }}
            </For>
          </Show>
        </div>

        <div class="rounded-lg border p-4" style={{ 'border-color': 'var(--color-border-light)' }}>
          <h3 class="mb-3 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Use Vercel AI Gateway
          </h3>
          <ProviderKeyPanel />
          <p class="mt-2 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            This is this one provider's own catalog — the count above is not the full
            picture of every model this machine can reach.
          </p>
        </div>
      </div>
    </section>
  );
};

export default ProviderStep;

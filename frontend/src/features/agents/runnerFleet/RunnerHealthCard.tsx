import { type Component, For, Show } from 'solid-js';
import { Badge } from '../../../shared/ui';
import { gateFeature, gateFeatureAcrossRunners, type FeatureName } from '../../../shared/execution';
import type { RunnerCapabilities } from '../../../shared/execution';
import { formatCapacity, formatLabelChips } from './format';

/**
 * A runner's connection state, as far as this build of Tack can actually
 * know it — deliberately NOT the richer `ControlPlaneHealth` union `../
 * format.ts`'s Docket-fleet sibling uses (`healthy | degraded | unreachable
 * | unknown | unconfigured`). There is no `GET /runners` (or any other
 * read-back) endpoint today (see `docs/agent-handoffs/part-iii/III-E2.md`
 * gap 1, confirmed again against `crates/tack-api/src/handlers/
 * runner_admin.rs::routes()` for this card — it registers only
 * enrollment/revocation POSTs), so Tack has no live heartbeat, capability
 * snapshot, or capacity reading for any runner to grade against. The only
 * two states this card can honestly produce today are `unconfirmed`
 * (enrolled this browser session; connection status unknown) and
 * `unconfigured` (used for a runner that was revoked, or a form filled in
 * without ever calling enroll — a placeholder identity). `stale` and
 * `healthy` are kept in the type — and fully exercised by this card's own
 * tests — purely so this component needs no further design work the moment
 * a real read endpoint exists to drive them; nothing in the current UI ever
 * constructs a `healthy` value.
 */
export type RunnerConnectionStatus = 'unconfirmed' | 'stale' | 'healthy' | 'unconfigured';

const STATUS_LABEL: Record<RunnerConnectionStatus, string> = {
  unconfirmed: 'Connection unconfirmed',
  stale: 'Stale',
  healthy: 'Healthy',
  unconfigured: 'Unconfigured',
};

const STATUS_TONE: Record<RunnerConnectionStatus, 'neutral' | 'warning' | 'success'> = {
  unconfirmed: 'neutral',
  stale: 'warning',
  healthy: 'success',
  unconfigured: 'warning',
};

const FEATURES: FeatureName[] = ['cancel', 'resume', 'decisions', 'artifacts', 'usage'];
const FEATURE_LABEL: Record<FeatureName, string> = {
  cancel: 'Cancel',
  resume: 'Resume',
  decisions: 'Decisions',
  artifacts: 'Artifacts',
  usage: 'Usage',
};

export interface RunnerHealthCardProps {
  name: string;
  runnerId: string;
  /** The ONLY input this card's health badge reads. Capability/capacity data
   *  below is display-only and must never upgrade the badge — see this
   *  file's `RunnerHealthCard.test.tsx` for the adversarial proof (a runner
   *  reporting full capability support while `unconfirmed` still shows no
   *  "Healthy" badge). */
  connectionStatus: RunnerConnectionStatus;
  /** Why `connectionStatus` is what it is — always populated, never blank,
   *  so an operator is never left guessing (TODO.md III.2 rule 7). */
  connectionReason: string;
  /** `null` when no capacity was ever recorded for this identity (e.g. a
   *  revoked runner this session never captured capacity for). */
  capacity: { total: number; available: number } | null;
  labels: unknown;
  /** `null` in every case this card can produce today — see this type's
   *  own doc comment. Wired so a future data source needs no further
   *  design work here. */
  capabilities: RunnerCapabilities | null;
}

/**
 * Read-only summary card for one runner identity: health/capacity/protocol/
 * harness display plus per-feature support values, each with a visible
 * reason (III-E3's acceptance bar). Reused by both the session-local
 * enrolled-runners list and (once a `capabilities` value has somewhere real
 * to come from) `RunnerFleetSection.tsx`'s eventual live roster.
 */
const RunnerHealthCard: Component<RunnerHealthCardProps> = (props) => {
  const labelChips = () => formatLabelChips(props.labels);

  return (
    <div
      class="rounded-lg border p-3"
      style={{ 'border-color': 'var(--color-border-light)' }}
      data-connection-status={props.connectionStatus}
    >
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {props.name}
        </span>
        <span class="font-mono text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          {props.runnerId}
        </span>
        <Badge tone={STATUS_TONE[props.connectionStatus]}>{STATUS_LABEL[props.connectionStatus]}</Badge>
      </div>
      <p class="mt-1 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.connectionReason}
      </p>

      <div class="mt-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        {props.capacity ? formatCapacity(props.capacity.total, props.capacity.available) : 'capacity unknown'}
      </div>

      <Show when={labelChips().length > 0}>
        <div class="mt-2 flex flex-wrap gap-1">
          <For each={labelChips()}>
            {(chip) => (
              <span
                class="font-mono text-[10.5px]"
                style={{
                  padding: '2px 7px',
                  'border-radius': '5px',
                  background: 'var(--color-chip)',
                  color: 'var(--color-text-secondary)',
                }}
              >
                {chip}
              </span>
            )}
          </For>
        </div>
      </Show>

      <div class="mt-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        Protocol version: {props.capabilities?.protocol_version ?? 'not reported'}
      </div>

      <div class="mt-2">
        <p class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>
          Harnesses
        </p>
        <Show
          when={props.capabilities && props.capabilities.harnesses.length > 0}
          fallback={
            <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              no harness capability data available
            </p>
          }
        >
          <ul class="mt-1 space-y-0.5">
            <For each={props.capabilities?.harnesses ?? []}>
              {(h) => (
                <li class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  <span class="font-mono">{h.harness_kind}</span> v{h.installed_version}
                  <Show when={h.probe_error}>
                    {(err) => (
                      <span style={{ color: 'var(--color-danger-600)' }}> — probe error: {err()}</span>
                    )}
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>

      <div class="mt-2">
        <p class="text-xs font-semibold" style={{ color: 'var(--color-text-tertiary)' }}>
          Feature support
        </p>
        <ul class="mt-1 space-y-0.5">
          <For each={FEATURES}>
            {(feature) => {
              const gate = () =>
                props.capabilities
                  ? gateFeature(props.capabilities, feature)
                  : gateFeatureAcrossRunners([], feature);
              return (
                <li class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  {FEATURE_LABEL[feature]}:{' '}
                  <Badge tone={gate().enabled ? 'success' : 'neutral'}>
                    {gate().enabled ? 'supported' : 'not supported'}
                  </Badge>
                  <Show when={gate().reason}>
                    <span style={{ color: 'var(--color-text-tertiary)' }}> — {gate().reason}</span>
                  </Show>
                </li>
              );
            }}
          </For>
        </ul>
      </div>
    </div>
  );
};

export default RunnerHealthCard;

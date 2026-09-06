import { type Component, type JSX, Show } from 'solid-js';
import Badge, { type BadgeTone } from './Badge';

/**
 * The chip's 5 visual states — a collapse of the wire's `TaskStatus` (see
 * `frontend/src/shared/agentActivity/format.ts#deriveAgentChipState` for the
 * mapping and the reasoning behind which raw statuses fold together). Kept
 * here, not in `shared/agentActivity/`, because this type describes what the
 * chip itself can render — the presentational contract — while the wire
 * interpretation lives with the wire boundary.
 */
export type AgentChipState = 'queued' | 'running' | 'waiting_approval' | 'done' | 'failed';

export const AGENT_STATE_LABEL: Record<AgentChipState, string> = {
  queued: 'Queued',
  running: 'Running',
  waiting_approval: 'Waiting on approval',
  done: 'Done',
  failed: 'Failed',
};

/** Reuses `Badge`'s existing, already-AA-audited tones — every tone's
 *  contrast is verified across all six palette×mode combinations — this
 *  chip introduces no new color pairing, so no fresh contrast audit is
 *  needed for it specifically. */
export const AGENT_STATE_TONE: Record<AgentChipState, BadgeTone> = {
  queued: 'neutral',
  running: 'info',
  waiting_approval: 'warning',
  done: 'success',
  failed: 'danger',
};

const DOT_COLOR: Record<AgentChipState, string> = {
  queued: 'var(--color-text-tertiary)',
  running: 'var(--color-info-600)',
  waiting_approval: 'var(--color-warning-600)',
  done: 'var(--color-success-600)',
  failed: 'var(--color-danger-600)',
};

export interface AgentStateChipProps {
  state: AgentChipState;
  /** Optional accessible/tooltip override. Useful when the caller wants to
   *  surface the raw `remote_status` even though it visually folds into one
   *  of the 5 states — e.g. a `blocked` task renders the `failed` chip but
   *  can still say "Blocked" in its title so a hover reveals the real story. */
  title?: string;
  class?: string;
}

/**
 * The single shared "this item has agent activity" indicator — used by the
 * item-detail Agent Activity tab, and the Board/List/Table badges, so there
 * is one implementation and never a per-view reimplementation.
 * 5 visually distinct states, each a token-driven color plus a text label so
 * the distinction never relies on color alone (WCAG 1.4.1), following the
 * same pattern as `frontend/src/features/fleet/HealthChip.tsx`.
 *
 * Renders nothing when `state` is absent at the call site — callers achieve
 * "no chip for an item with no agent activity" by simply not mounting this
 * component, not by passing a 6th "none" state into it.
 */
const AgentStateChip: Component<AgentStateChipProps> = (props) => {
  const badge = (): JSX.Element => (
    <Badge tone={AGENT_STATE_TONE[props.state]} class={props.class}>
      <span
        aria-hidden="true"
        style={{
          display: 'inline-block',
          width: '6px',
          height: '6px',
          'border-radius': '99px',
          background: DOT_COLOR[props.state],
          'margin-right': '5px',
          animation: props.state === 'running' ? 'tk-pulse 2s ease-in-out infinite' : 'none',
        }}
      />
      {AGENT_STATE_LABEL[props.state]}
    </Badge>
  );

  return (
    <Show when={props.title} fallback={badge()}>
      <span title={props.title}>{badge()}</span>
    </Show>
  );
};

export default AgentStateChip;

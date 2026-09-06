import { type Component, type JSX, For, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Badge } from '../../shared/ui';
import HealthChip from './HealthChip';
import CapabilityNote from '../../shared/orch/CapabilityNote';
import { gatePause } from '../../shared/orch/capabilities';
import type { FleetRow as FleetRowData, FleetGatewayState } from './api';
import { formatEstimatedCost, formatBudget, formatTokens, relativeTime, isStale } from './format';

export interface FleetRowProps {
  row: FleetRowData;
}

const GATEWAY_LABEL: Record<FleetGatewayState, string> = {
  active: 'Active',
  inactive: 'Inactive',
  unknown: 'Unknown',
};

const GATEWAY_TONE: Record<FleetGatewayState, 'success' | 'warning' | 'neutral'> = {
  active: 'success',
  inactive: 'warning',
  unknown: 'neutral',
};

const cellStyle: JSX.CSSProperties = {
  padding: '12px 14px',
  'vertical-align': 'top',
  'border-bottom': '1px solid var(--color-border-light)',
};

/**
 * One product row on the Fleet view: a Tack project, its linked control
 * plane's health, roster, activity, burn vs budget, gateway state, and
 * pending-approval count.
 *
 * The load-bearing decision here is the `stale()` branch. `unreachable` and
 * `unknown` planes have no trustworthy current reading — the row gets a
 * muted background (`--color-bg-subtle`, not a CSS `opacity` dim: reducing
 * opacity blends already-AA text tokens with whatever sits behind them and
 * can silently drop below the 4.5:1 contrast floor the axe gate enforces;
 * a background swap keeps every token at its normal, already-audited
 * contrast) and every count-like field (tokens, estimated cost, pending
 * approvals, gateway state) renders as an em dash with a caption naming
 * *why*, never as a bare `0` or `$0.00` that would read as a confident
 * current value. `degraded` is deliberately NOT treated as stale — its data
 * is only a poll or two old — so it keeps the plain row background and just
 * gets an amber health chip and a "Last seen …" caption, keeping it visually
 * distinct from both `healthy` and `unreachable`.
 */
const FleetRow: Component<FleetRowProps> = (props) => {
  const row = () => props.row;
  const stale = () => isStale(row().health);
  const gateway = (): FleetGatewayState => (stale() ? 'unknown' : row().gateway);

  return (
    <tr
      style={{ background: stale() ? 'var(--color-bg-subtle)' : 'transparent' }}
      data-health={row().health}
    >
      {/* Project */}
      <td style={cellStyle}>
        <A
          href={`/projects/${row().project_id}/overview`}
          style={{
            'font-weight': 600,
            'font-size': '13.5px',
            color: 'var(--color-text-primary)',
            'text-decoration': 'none',
          }}
        >
          {row().project_name}
        </A>
        <div style={{ 'font-size': '11.5px', color: 'var(--color-text-tertiary)', 'margin-top': '2px' }}>
          {row().control_plane_name} · {row().control_plane_kind}
        </div>
      </td>

      {/* Pod health */}
      <td style={cellStyle}>
        <HealthChip health={row().health} />
        <Show when={row().health !== 'healthy'}>
          <div style={{ 'font-size': '11px', color: 'var(--color-text-tertiary)', 'margin-top': '5px' }}>
            {row().health === 'unknown'
              ? 'Not yet connected'
              : row().health === 'unconfigured'
                ? 'Credentials missing — reconfigure this plane in Settings'
                : `Last seen ${relativeTime(row().last_seen_at)}`}
          </div>
        </Show>
        {/* Capability negotiation: read straight from the wire
            payload, never from `control_plane_kind`.
            `capabilities` is only `null` in the `unconfigured` case above,
            where there is nothing to ask. Pause is the capability every
            operator is most likely to reach for from this exact row (a
            plane pinned at its budget cap), so it's the one surfaced here;
            `ControlPlanesManager.tsx` shows the full set on the admin page. */}
        <Show when={row().capabilities}>
          {(caps) => <CapabilityNote label="Pause" gate={gatePause(caps())} />}
        </Show>
      </td>

      {/* Roster: roles + models */}
      <td style={cellStyle}>
        <Show
          when={row().roster.length > 0}
          fallback={
            <span style={{ 'font-size': '12px', color: 'var(--color-text-tertiary)' }}>
              {stale() ? 'No cached roster — plane unreachable' : 'No agents registered'}
            </span>
          }
        >
          <div style={{ display: 'flex', 'flex-wrap': 'wrap', gap: '4px' }}>
            <For each={row().roster}>
              {(agent) => (
                <span
                  title={agent.name}
                  style={{
                    'font-family': 'var(--font-mono)',
                    'font-size': '10.5px',
                    padding: '2px 7px',
                    'border-radius': '5px',
                    background: 'var(--color-chip)',
                    color: stale() ? 'var(--color-text-tertiary)' : 'var(--color-text-secondary)',
                  }}
                >
                  {agent.role} · {agent.model}
                </span>
              )}
            </For>
          </div>
          <Show when={stale()}>
            <div style={{ 'font-size': '10.5px', color: 'var(--color-text-tertiary)', 'margin-top': '4px' }}>
              last known roster — may be out of date
            </div>
          </Show>
        </Show>
      </td>

      {/* Last activity */}
      <td style={cellStyle}>
        <Show
          when={!stale()}
          fallback={
            <span style={{ 'font-size': '12px', color: 'var(--color-text-tertiary)' }}>
              — unavailable while {row().health}
            </span>
          }
        >
          <span style={{ 'font-size': '12.5px', color: 'var(--color-text-secondary)' }}>
            {row().last_activity_at ? relativeTime(row().last_activity_at) : 'No activity yet'}
          </span>
        </Show>
      </td>

      {/* Burn vs budget — tokens are the primary measure, at least as
          prominent as the dollar figure beneath them. */}
      <td style={cellStyle}>
        <Show
          when={!stale()}
          fallback={
            <div>
              <div style={{ 'font-size': '12.5px', 'font-weight': 600, color: 'var(--color-text-tertiary)' }}>—</div>
              <div style={{ 'font-size': '10.5px', color: 'var(--color-text-tertiary)' }}>
                no fresh estimate — plane {row().health}
              </div>
            </div>
          }
        >
          <div
            style={{
              'font-family': 'var(--font-mono)',
              'font-size': '12.5px',
              'font-weight': 600,
              color: 'var(--color-text-primary)',
            }}
          >
            {formatTokens(row().tokens_in)} in / {formatTokens(row().tokens_out)} out tokens
          </div>
          <div style={{ 'font-size': '11px', color: 'var(--color-text-secondary)', 'margin-top': '2px' }}>
            {formatEstimatedCost(row().cost_usd_estimated, row().pricing_snapshot_at)}
          </div>
          <div style={{ 'font-size': '10.5px', color: 'var(--color-text-tertiary)', 'margin-top': '1px' }}>
            of {formatBudget(row().budget_usd)} budget
          </div>
        </Show>
      </td>

      {/* Gateway state */}
      <td style={cellStyle}>
        <Badge tone={GATEWAY_TONE[gateway()]}>{GATEWAY_LABEL[gateway()]}</Badge>
      </td>

      {/* Pending approvals */}
      <td style={cellStyle}>
        <Show
          when={!stale()}
          fallback={<span style={{ 'font-size': '12px', color: 'var(--color-text-tertiary)' }}>—</span>}
        >
          <Show
            when={row().pending_approval_count > 0}
            fallback={<span style={{ 'font-size': '12.5px', color: 'var(--color-text-tertiary)' }}>0</span>}
          >
            <Badge tone="warning">{row().pending_approval_count} pending</Badge>
          </Show>
        </Show>
      </td>
    </tr>
  );
};

export default FleetRow;

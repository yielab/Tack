import { type Component, createResource, For, Show } from 'solid-js';
import { Badge, Button, EmptyState, Skeleton } from '../../../shared/ui';
import { orchestrationApi } from './api';
import { POLICY_SCOPE_CAVEAT, formatDenialRate, relativeTime } from './format';

export interface PolicyPanelProps {
  projectId: string;
}

const thStyle = {
  padding: '6px 10px',
  'text-align': 'left' as const,
  'font-size': '10.5px',
  'font-weight': 700,
  'letter-spacing': '.03em',
  'text-transform': 'uppercase' as const,
  color: 'var(--color-text-tertiary)',
  'border-bottom': '1px solid var(--color-border-light)',
};

const tdStyle = {
  padding: '6px 10px',
  'font-size': '12.5px',
  color: 'var(--color-text-primary)',
  'border-bottom': '1px solid var(--color-border-light)',
};

/**
 * Guardrail/tool-call/approval activity for this project's linked control
 * plane — sourced entirely from mirrored `/metrics` samples,
 * never a live call. **Every number here is control-plane-wide**,
 * not scoped to just this project — `POLICY_SCOPE_CAVEAT` is rendered first,
 * above every figure, not as a footnote, because it changes how every number
 * below it should be read.
 *
 * Chain-verification of the underlying audit log is out of scope here by
 * design — this panel links out to
 * `docket audit verify` as a command to run, rather than reimplementing
 * tamper detection in Rust.
 */
const PolicyPanel: Component<PolicyPanelProps> = (props) => {
  const [policy, { refetch }] = createResource(
    () => props.projectId,
    (id) => orchestrationApi.getPolicy(id)
  );

  const hasAnyData = () => {
    const p = policy();
    if (!p) return false;
    return p.tool_calls.length > 0 || p.policy_hits.length > 0 || p.approvals_by_channel.length > 0;
  };

  return (
    <section aria-labelledby="orch-policy-heading">
      <h2 id="orch-policy-heading" class="text-base font-semibold mb-3" style={{ color: 'var(--color-text-primary)' }}>
        Policy
      </h2>

      <Show when={policy.loading}>
        <Skeleton height="140px" />
      </Show>

      <Show when={!policy.loading && policy.error}>
        <EmptyState
          icon="⚠️"
          title="Couldn't load policy data"
          description="The request to the server failed."
          action={<Button onClick={() => void refetch()}>Retry</Button>}
        />
      </Show>

      <Show when={!policy.loading && !policy.error && policy()}>
        {(data) => (
          <div
            class="rounded-lg p-4 space-y-4"
            style={{ border: '1px solid var(--color-border-light)' }}
          >
            <p style={{ 'font-size': '11.5px', color: 'var(--color-text-tertiary)' }}>
              {POLICY_SCOPE_CAVEAT}
            </p>

            <Show
              when={hasAnyData()}
              fallback={
                <p style={{ 'font-size': '12.5px', color: 'var(--color-text-tertiary)' }}>
                  No guardrail metrics reported yet for this control plane.
                </p>
              }
            >
              <div style={{ 'font-size': '13px', color: 'var(--color-text-secondary)' }}>
                {formatDenialRate(data().denial_rate)}
                <Show when={data().scraped_at}>
                  {' '}
                  — last scraped {relativeTime(data().scraped_at)}
                </Show>
              </div>

              <Show when={data().tool_calls.length > 0}>
                <div class="overflow-x-auto">
                  <table style={{ 'border-collapse': 'collapse', width: '100%' }}>
                    <caption class="sr-only">Tool-call volume by gate decision</caption>
                    <thead>
                      <tr>
                        <th scope="col" style={thStyle}>Decision</th>
                        <th scope="col" style={thStyle}>Count</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={data().tool_calls}>
                        {(row) => (
                          <tr>
                            <td style={tdStyle}>
                              <Badge tone={row.decision === 'deny' ? 'danger' : row.decision === 'ask' ? 'warning' : 'neutral'}>
                                {row.decision}
                              </Badge>
                            </td>
                            <td style={tdStyle}>{row.count}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>

              <Show when={data().policy_hits.length > 0}>
                <div class="overflow-x-auto">
                  <table style={{ 'border-collapse': 'collapse', width: '100%' }}>
                    <caption class="sr-only">Guardrail policy hits by policy id</caption>
                    <thead>
                      <tr>
                        <th scope="col" style={thStyle}>Policy</th>
                        <th scope="col" style={thStyle}>Hook</th>
                        <th scope="col" style={thStyle}>Action</th>
                        <th scope="col" style={thStyle}>Count</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={data().policy_hits}>
                        {(row) => (
                          <tr>
                            <td style={{ ...tdStyle, 'font-family': 'var(--font-mono)' }}>{row.policy_id}</td>
                            <td style={tdStyle}>{row.hook}</td>
                            <td style={tdStyle}>{row.action}</td>
                            <td style={tdStyle}>{row.count}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>

              <Show when={data().approvals_by_channel.length > 0}>
                <div class="overflow-x-auto">
                  <table style={{ 'border-collapse': 'collapse', width: '100%' }}>
                    <caption class="sr-only">Approvals by channel and outcome</caption>
                    <thead>
                      <tr>
                        <th scope="col" style={thStyle}>Channel</th>
                        <th scope="col" style={thStyle}>Outcome</th>
                        <th scope="col" style={thStyle}>Count</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={data().approvals_by_channel}>
                        {(row) => (
                          <tr>
                            <td style={tdStyle}>{row.channel}</td>
                            <td style={tdStyle}>
                              <Badge tone={row.outcome === 'denied' ? 'danger' : 'success'}>{row.outcome}</Badge>
                            </td>
                            <td style={tdStyle}>{row.count}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>
            </Show>

            <p style={{ 'font-size': '11px', color: 'var(--color-text-tertiary)' }}>
              To verify the underlying audit log hasn't been tampered with, run{' '}
              <code style={{ 'font-family': 'var(--font-mono)' }}>docket audit verify</code> from the
              docket CLI — Tack does not reimplement chain verification.
            </p>
          </div>
        )}
      </Show>
    </section>
  );
};

export default PolicyPanel;

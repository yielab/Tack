import { type Component, For, Show, createMemo } from 'solid-js';
import { EmptyState, AgentStateChip, Badge } from '../../../shared/ui';
import type { ItemAgentActivity, ItemAgentAttempt, ItemAgentApproval } from '../../../shared/agentActivity/api';
import {
  deriveAgentChipState,
  formatTokens,
  formatEstimatedCost,
  relativeTime,
  eventTypeLabel,
  eventsTruncatedMessage,
} from '../../../shared/agentActivity/format';

export interface AgentActivityTabProps {
  /** Pre-fetched by `ItemDetailDrawer` (not self-fetched, unlike the other
   *  tabs) because the drawer must already know whether this item has any
   *  agent activity *before* deciding whether to show the tab at all: an
   *  item with no agent activity shows no chip and no
   *  empty tab — fetching once at the drawer and passing down here avoids
   *  a second, redundant request for the same data. */
  activity: ItemAgentActivity | null | undefined;
  loading: boolean;
}

/** `state === 'pending'` approvals, oldest first (act on the longest-waiting
 *  one first — same ordering rule B1's handoff notes for the fleet-wide inbox). */
function pendingApprovals(approvals: ItemAgentApproval[]): ItemAgentApproval[] {
  return approvals
    .filter((a) => a.state === 'pending')
    .slice()
    .sort((a, b) => a.requested_at.localeCompare(b.requested_at));
}

/** Timeline of hops/tool-calls/verdicts/rework/approvals/tokens/estimated
 *  cost for the item's `orch_tasks` rows, grouped by attempt, newest first. */
const AgentActivityTab: Component<AgentActivityTabProps> = (props) => {
  // Sorted defensively (highest `attempt` number first) rather than trusting
  // the wire order — `attempt` is a monotonically increasing per-item counter
  // (migration 021's PK note), so this is a reliable "newest first" without
  // parsing `dispatched_at`. The API is documented to already return this
  // order (`shared/agentActivity/api.ts`'s `ItemAgentActivity.attempts` doc
  // comment); sorting here just makes that a guarantee, not an assumption.
  const attempts = createMemo(() =>
    (props.activity?.attempts ?? []).slice().sort((a, b) => b.attempt - a.attempt),
  );
  const approvals = createMemo(() => props.activity?.approvals ?? []);
  const pending = createMemo(() => pendingApprovals(approvals()));

  const totals = createMemo(() => {
    const list = attempts();
    return {
      tokensIn: list.reduce((n, a) => n + a.tokens_in, 0),
      tokensOut: list.reduce((n, a) => n + a.tokens_out, 0),
      // Sum only known costs; an attempt still queued (never costed) has
      // `cost_usd_estimated: null` and must not be treated as a real $0 —
      // same "never a confident-looking zero" discipline the Fleet view
      // applies.
      costKnown: list.some((a) => a.cost_usd_estimated != null),
      costUsd: list.reduce((n, a) => n + (a.cost_usd_estimated ?? 0), 0),
      // A pricing-snapshot date only means something once one exists; today
      // every attempt's is `null` (see `shared/agentActivity/api.ts`), so
      // this stays `null` and the total-cost line says so honestly too.
      pricingSnapshotAt: list.find((a) => a.pricing_snapshot_at)?.pricing_snapshot_at ?? null,
    };
  });

  return (
    <div class="space-y-5">
      <Show when={props.loading}>
        <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading agent activity…
        </p>
      </Show>

      <Show when={!props.loading}>
        <Show when={pending().length > 0}>
          <section
            class="space-y-2 rounded-lg border p-3"
            style={{
              'background-color': 'var(--color-warning-100)',
              'border-color': 'var(--color-warning-600)',
            }}
          >
            <h3 class="text-sm font-semibold" style={{ color: 'var(--color-warning-700)' }}>
              {pending().length === 1 ? '1 approval pending' : `${pending().length} approvals pending`}
            </h3>
            <ul class="space-y-1">
              <For each={pending()}>
                {(a) => (
                  <li class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <span class="font-medium">{a.agent ?? 'Unknown agent'}</span>
                    {a.action ? ` — ${a.action}` : ''}
                    <span style={{ color: 'var(--color-text-tertiary)' }}>
                      {' · requested '}
                      {relativeTime(a.requested_at)}
                    </span>
                  </li>
                )}
              </For>
            </ul>
          </section>
        </Show>

        <Show
          when={attempts().length > 0}
          fallback={
            <EmptyState
              title="No agent activity yet"
              description="This item hasn't been dispatched to an agent fleet."
            />
          }
        >
          <section
            class="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-lg border px-3 py-2 text-sm"
            style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
          >
            <span style={{ color: 'var(--color-text-primary)', 'font-weight': 600 }}>
              {formatTokens(totals().tokensIn)} in / {formatTokens(totals().tokensOut)} out tokens
            </span>
            <span style={{ color: 'var(--color-text-secondary)' }}>
              {totals().costKnown
                ? formatEstimatedCost(totals().costUsd, totals().pricingSnapshotAt)
                : 'cost estimate unavailable'}
            </span>
          </section>

          {/* Honesty notice for the retention sweep:
              `events_truncated` means "this item has an attempt old enough
              that some event history may have been aged out," not "N events
              were deleted" — that count is unknowable from the daily
              aggregate (see `eventsTruncatedMessage`'s doc comment). Only
              rendered when true, so the common case has no banner and no
              layout shift. */}
          <Show when={props.activity?.events_truncated}>
            <p class="flex flex-wrap items-center gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              <Badge tone="info">Partial history</Badge>
              <span>{eventsTruncatedMessage(props.activity!.events_retention_days)}</span>
            </p>
          </Show>

          <ul class="space-y-3">
            <For each={attempts()}>{(attempt) => <AttemptCard attempt={attempt} approvals={approvals()} />}</For>
          </ul>
        </Show>
      </Show>
    </div>
  );
};

const AttemptCard: Component<{ attempt: ItemAgentAttempt; approvals: ItemAgentApproval[] }> = (props) => {
  const relatedApprovals = createMemo(() =>
    props.approvals.filter((a) => a.remote_task_id === props.attempt.remote_task_id),
  );

  return (
    <li
      class="space-y-2 rounded-lg border p-3"
      style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Attempt {props.attempt.attempt}
        </span>
        <AgentStateChip
          state={deriveAgentChipState(props.attempt.remote_status)}
          title={props.attempt.remote_status}
        />
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          dispatched {relativeTime(props.attempt.dispatched_at)}
        </span>
      </div>

      <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <span>
          {formatTokens(props.attempt.tokens_in)} in / {formatTokens(props.attempt.tokens_out)} out tokens
        </span>
        <span>{formatEstimatedCost(props.attempt.cost_usd_estimated, props.attempt.pricing_snapshot_at)}</span>
      </div>

      <Show when={props.attempt.run}>
        {(run) => (
          <div class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            Run <span style={{ 'font-family': 'var(--font-mono)' }}>{run().run_id}</span> · {run().source} ·{' '}
            {run().state}
            <Show when={run().state === 'failed' && run().error}>
              <p class="mt-1" style={{ color: 'var(--color-danger-600)' }}>
                {run().error}
              </p>
            </Show>
          </div>
        )}
      </Show>

      <Show when={relatedApprovals().length > 0}>
        <ul class="space-y-1 border-t pt-2" style={{ 'border-color': 'var(--color-border-light)' }}>
          <For each={relatedApprovals()}>
            {(a) => (
              <li class="flex items-center gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <Badge tone={a.state === 'pending' ? 'warning' : a.state === 'granted' ? 'success' : 'danger'}>
                  {a.state}
                </Badge>
                <span>{a.agent ?? 'Unknown agent'}{a.action ? ` — ${a.action}` : ''}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={props.attempt.events.length > 0}>
        <ul class="space-y-1 border-t pt-2" style={{ 'border-color': 'var(--color-border-light)' }}>
          <For each={props.attempt.events}>
            {(evt) => (
              <li class="flex items-center gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <span style={{ color: 'var(--color-text-tertiary)' }}>{relativeTime(evt.occurred_at)}</span>
                <span>{eventTypeLabel(evt.event_type)}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </li>
  );
};

export default AgentActivityTab;

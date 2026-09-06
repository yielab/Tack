import { type Component, createResource, createSignal, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Badge, Button, EmptyState, Skeleton } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { economicsApi, isOrchDisabled, type EconomicsSummaryResponse } from './api';
import {
  formatTokens,
  formatEstimatedCost,
  describeLeadTime,
  describeRework,
  describeCostPerItem,
} from './format';
import SliceTable from './SliceTable';

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

/** Links to the guided setup (card E2, Phase 39) instead of naming an
 *  environment variable to set by hand. */
const OrchDisabledEmptyState: Component = () => {
  const navigate = useNavigate();
  return (
    <EmptyState
      icon="🔌"
      title="Agent-fleet orchestration is disabled"
      description="Turn it on to compare agent vs. human token cost, lead time, and rework across completed work."
      action={
        <Button onClick={() => navigate('/settings?section=orchestration')}>
          Set up orchestration
        </Button>
      }
    />
  );
};

const ErrorState: Component<{ onRetry: () => void }> = (props) => (
  <EmptyState
    icon="⚠️"
    title="Couldn't load unit economics"
    description="The request to the server failed. Check your connection and try again."
    action={<Button onClick={props.onRetry}>Retry</Button>}
  />
);

const NoDataEmptyState: Component = () => (
  <EmptyState
    icon="📊"
    title="No completed items yet"
    description="Unit economics is computed from completed items — dispatch some work to agents, finish some by hand, and this page fills in as items complete."
  />
);

/** One label + value stat tile. Token counts render larger/first (rule 6): callers
 *  pass `emphasis` for the primary figure on a card, leaving cost secondary. */
const StatTile: Component<{ label: string; value: string; emphasis?: boolean; hint?: string }> = (props) => (
  <div
    class="rounded-lg p-4"
    style={{ border: '1px solid var(--color-border-light)', background: 'var(--color-bg-surface)' }}
  >
    <div class="text-xs font-semibold uppercase tracking-wide" style={{ color: 'var(--color-text-tertiary)' }}>
      {props.label}
    </div>
    <div
      class="mt-1"
      style={{
        'font-size': props.emphasis ? '20px' : '15px',
        'font-weight': props.emphasis ? 700 : 600,
        color: 'var(--color-text-primary)',
      }}
    >
      {props.value}
    </div>
    <Show when={props.hint}>
      <div class="mt-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        {props.hint}
      </div>
    </Show>
  </div>
);

/**
 * Unit economics dashboard — answers "what did each
 * product line cost, in tokens and estimated dollars, per shipped item, and how
 * often did agents need rework?"
 *
 * Three honesty rules enforced throughout this page, not just in the API layer:
 *
 * 1. Below `min_sample_size` (from the wire, currently 5), an average/rate is
 *    replaced by raw counts — see `format.ts#describeLeadTime`/`describeRework`/
 *    `describeCostPerItem`. No card on this page ever shows a derived figure a
 *    handful of items can't support.
 * 2. The lead-time selection-bias caveat renders directly under the comparison
 *    it qualifies, not in a linked doc (`overall.lead_time_selection_bias_note`,
 *    server-authored so the copy can't drift from the backend's own reasoning).
 * 3. The rework-rate truncation note renders whenever any attempt was excluded as
 *    stale, naming the retention window rather than a count this page cannot know.
 */
const EconomicsPage: Component = () => {
  const [summary, { refetch }] = createResource(() => economicsApi.summary());
  const [exporting, setExporting] = createSignal(false);

  const disabled = () => isOrchDisabled(summary.error);
  const failed = () => summary.error !== undefined && !disabled();
  const empty = () => (summary()?.overall.completed_item_count ?? 0) === 0;

  const exportAs = async (format: 'csv' | 'json') => {
    setExporting(true);
    try {
      if (format === 'csv') {
        const blob = await economicsApi.exportCsv();
        downloadBlob(blob, 'unit-economics.csv');
      } else {
        const data = await economicsApi.exportJson();
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        downloadBlob(blob, 'unit-economics.json');
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Export failed');
    } finally {
      setExporting(false);
    }
  };

  const overallCostPerItem = (s: EconomicsSummaryResponse) =>
    describeCostPerItem(
      s.overall.cost_usd_estimated_per_item,
      s.overall.pricing_snapshot_at,
      s.overall.agent_completed_count,
      s.min_sample_size,
    );

  return (
    <div>
      <div class="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
            Unit Economics
          </h1>
          <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Tokens, estimated cost, agent-vs-human lead time, and rework rate — per completed item, sliced by
            product line.
          </p>
        </div>
        <Show when={!summary.loading && !disabled() && !failed() && !empty()}>
          <div class="flex gap-2">
            <Button variant="secondary" disabled={exporting()} onClick={() => exportAs('csv')}>
              Download CSV
            </Button>
            <Button variant="secondary" disabled={exporting()} onClick={() => exportAs('json')}>
              Download JSON
            </Button>
          </div>
        </Show>
      </div>

      <Show when={summary.loading}>
        <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <Skeleton height="72px" />
          <Skeleton height="72px" />
          <Skeleton height="72px" />
        </div>
      </Show>

      <Show when={!summary.loading && disabled()}>
        <OrchDisabledEmptyState />
      </Show>

      <Show when={!summary.loading && failed()}>
        <ErrorState onRetry={refetch} />
      </Show>

      <Show when={!summary.loading && !disabled() && !failed() && empty()}>
        <NoDataEmptyState />
      </Show>

      <Show when={!summary.loading && !disabled() && !failed() && !empty() && summary()}>
        {(s) => (
          <div class="flex flex-col gap-6">
            {/* Overall stat tiles — tokens primary, cost secondary (rule 6). */}
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <StatTile
                label="Completed items"
                value={String(s().overall.completed_item_count)}
                hint={`${s().overall.agent_completed_count} agent-dispatched, ${s().overall.human_completed_count} human`}
                emphasis
              />
              <StatTile
                label="Tokens in / out"
                value={`${formatTokens(s().overall.tokens_in)} / ${formatTokens(s().overall.tokens_out)}`}
                emphasis
              />
              <StatTile
                label="Estimated cost (total)"
                value={formatEstimatedCost(s().overall.cost_usd_estimated, s().overall.pricing_snapshot_at)}
              />
              <StatTile label="Estimated cost per item" value={overallCostPerItem(s())} />
            </div>

            {/* Lead time comparison — selection-bias caveat lives right here, not in a doc. */}
            <section
              class="rounded-lg p-4"
              style={{ border: '1px solid var(--color-border-light)', background: 'var(--color-bg-surface)' }}
            >
              <h2 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Lead time: agent vs. human
              </h2>
              <div class="mt-3 grid grid-cols-1 gap-4 sm:grid-cols-2">
                <div>
                  <div class="text-xs font-semibold uppercase" style={{ color: 'var(--color-text-tertiary)' }}>
                    Agent-dispatched items
                  </div>
                  <div class="mt-1 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    {describeLeadTime(s().overall.agent_lead_time)}
                  </div>
                </div>
                <div>
                  <div class="text-xs font-semibold uppercase" style={{ color: 'var(--color-text-tertiary)' }}>
                    Human-completed items
                  </div>
                  <div class="mt-1 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    {describeLeadTime(s().overall.human_lead_time)}
                  </div>
                </div>
              </div>
              <p class="mt-3 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <Badge tone="info">Not a controlled comparison</Badge>{' '}
                {s().overall.lead_time_selection_bias_note}
              </p>
            </section>

            {/* Rework — definition and truncation caveat both travel with the number. */}
            <section
              class="rounded-lg p-4"
              style={{ border: '1px solid var(--color-border-light)', background: 'var(--color-bg-surface)' }}
            >
              <h2 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Rework rate
              </h2>
              <p class="mt-1 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                {describeRework(s().overall.rework)}
              </p>
              <p class="mt-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <strong>Definition:</strong> {s().overall.rework.definition}
              </p>
              <Show when={s().overall.rework.attempts_excluded_stale > 0}>
                <p class="mt-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  <Badge tone="warning">
                    {s().overall.rework.attempts_excluded_stale} attempt(s) excluded
                  </Badge>{' '}
                  {s().overall.rework.truncation_note}
                </p>
              </Show>
            </section>

            {/* Product-line comparison (task 38.2) — the headline slice. */}
            <section>
              <h2 class="mb-2 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                By project type
              </h2>
              <SliceTable
                caption="Unit economics by project type"
                dimensionLabel="Project type"
                slices={s().by_project_type}
                minSampleSize={s().min_sample_size}
              />
            </section>

            <section>
              <h2 class="mb-2 text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                By item type
              </h2>
              <SliceTable
                caption="Unit economics by item type"
                dimensionLabel="Item type"
                slices={s().by_item_type}
                minSampleSize={s().min_sample_size}
              />
            </section>

            <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              Figures below {s().min_sample_size} samples show raw counts instead of an average or rate. Rework
              history is limited to the last {s().events_retention_days} days of mirrored events; token, cost, and
              lead-time figures are not affected by that window.
            </p>
          </div>
        )}
      </Show>
    </div>
  );
};

export default EconomicsPage;

import { type Component, createResource, createSignal, Show } from 'solid-js';
import { Badge, Button, EmptyState, Field, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { formatEstimatedCost, formatTokens } from '../../../shared/agentActivity/format';
import { orchestrationApi, type OrchLink } from './api';
import {
  BUDGET_PAUSE_NOTE,
  BUDGET_PROGRESS_CAVEAT,
  HEALTH_LABEL,
  HEALTH_TONE,
  budgetProgress,
  formatBudgetCap,
  formatPercent,
} from './format';

export interface BudgetPanelProps {
  projectId: string;
  /** The current link — needed to resend `control_plane_id`/`remote_project`
   *  when saving a new budget cap (`PUT /orch-link` is a full upsert, not a
   *  partial patch). */
  link: OrchLink;
  /** Called after a successful budget-cap save so the parent's `link`
   *  resource (and this panel's own re-fetch) both reflect the new value. */
  onBudgetSaved: () => void;
}

/**
 * Budget cap vs. estimated spend to date, for this project's linked control
 * plane. Every dollar figure is Tack's own token-based estimate — see
 * `formatEstimatedCost`'s doc comment — never docket's
 * own bill (docket doesn't expose one over HTTP either; see
 * `crates/tack-api/src/handlers/orch.rs`'s `OrchBudgetResponse` doc comment).
 *
 * **No pause indicator.** See `./api.ts`'s header comment — docket's budget
 * auto-pause has no HTTP surface Tack can read attributably per project.
 * `BUDGET_PAUSE_NOTE` names the CLI remedy instead of guessing at a status.
 */
const BudgetPanel: Component<BudgetPanelProps> = (props) => {
  const [budget, { refetch }] = createResource(
    () => props.projectId,
    (id) => orchestrationApi.getBudget(id)
  );

  const [editing, setEditing] = createSignal(false);
  const [draftBudget, setDraftBudget] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const startEdit = () => {
    setDraftBudget(budget()?.budget_usd != null ? String(budget()!.budget_usd) : '');
    setEditing(true);
  };

  const saveBudget = async (e: Event) => {
    e.preventDefault();
    setSaving(true);
    try {
      await orchestrationApi.putLink(props.projectId, {
        control_plane_id: props.link.control_plane_id,
        remote_project: props.link.remote_project,
        budget_usd: draftBudget().trim() ? Number(draftBudget()) : null,
      });
      toast.success('Budget cap updated');
      setEditing(false);
      await refetch();
      props.onBudgetSaved();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update budget');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-labelledby="orch-budget-heading">
      <h2 id="orch-budget-heading" class="text-base font-semibold mb-3" style={{ color: 'var(--color-text-primary)' }}>
        Budget
      </h2>

      <Show when={budget.loading}>
        <Skeleton height="90px" />
      </Show>

      <Show when={!budget.loading && budget.error}>
        <EmptyState
          icon="⚠️"
          title="Couldn't load budget data"
          description="The request to the server failed."
          action={<Button onClick={() => void refetch()}>Retry</Button>}
        />
      </Show>

      <Show when={!budget.loading && !budget.error && budget()}>
        {(data) => (
          <div
            class="rounded-lg p-4 space-y-3"
            style={{ border: '1px solid var(--color-border-light)' }}
          >
            <Show when={data().health}>
              <Badge tone={HEALTH_TONE[data().health!]}>{HEALTH_LABEL[data().health!]}</Badge>
            </Show>

            {/* Tokens are the primary measure — at least as
                prominent as the dollar figure beneath them. */}
            <div
              style={{
                'font-family': 'var(--font-mono)',
                'font-size': '15px',
                'font-weight': 600,
                color: 'var(--color-text-primary)',
              }}
            >
              {formatTokens(data().tokens_in)} in / {formatTokens(data().tokens_out)} out tokens
            </div>
            <div style={{ 'font-size': '13px', color: 'var(--color-text-secondary)' }}>
              {formatEstimatedCost(data().cost_usd_estimated, data().pricing_snapshot_at)}
            </div>

            {/* Budget cap + inline editor */}
            <Show
              when={!editing()}
              fallback={
                <form onSubmit={(e) => void saveBudget(e)} class="flex items-end gap-2 max-w-xs">
                  <Field
                    label="Budget cap (USD)"
                    type="number"
                    min="0"
                    step="0.01"
                    value={draftBudget()}
                    onInput={(e) => setDraftBudget(e.currentTarget.value)}
                  />
                  <Button type="submit" size="sm" loading={saving()} disabled={saving()}>
                    Save
                  </Button>
                  <Button type="button" variant="secondary" size="sm" onClick={() => setEditing(false)}>
                    Cancel
                  </Button>
                </form>
              }
            >
              <div class="flex items-center gap-3">
                <span style={{ 'font-size': '13px', color: 'var(--color-text-secondary)' }}>
                  {formatBudgetCap(data().budget_usd)}
                </span>
                <Button variant="secondary" size="sm" onClick={startEdit}>
                  Edit cap
                </Button>
              </div>
            </Show>

            {/* Progress band — always accompanied by the compounding-estimate
                caveat, never a bare percentage. */}
            <Show when={budgetProgress(data().cost_usd_estimated, data().budget_usd)}>
              {(progress) => (
                <div>
                  <div
                    role="progressbar"
                    aria-valuenow={Math.round(Math.min(progress().fraction, 1) * 100)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-label="Estimated share of budget consumed"
                    style={{
                      height: '6px',
                      'border-radius': '99px',
                      background: 'var(--color-bg-subtle)',
                      overflow: 'hidden',
                    }}
                  >
                    <div
                      style={{
                        height: '100%',
                        width: `${Math.min(progress().fraction, 1) * 100}%`,
                        'border-radius': '99px',
                        background:
                          progress().tone === 'danger'
                            ? 'var(--color-danger-600)'
                            : progress().tone === 'warning'
                              ? 'var(--color-warning-600)'
                              : 'var(--color-success-600)',
                      }}
                    />
                  </div>
                  <p style={{ 'font-size': '11.5px', color: 'var(--color-text-tertiary)', 'margin-top': '4px' }}>
                    ~{formatPercent(progress().fraction)} of cap. {BUDGET_PROGRESS_CAVEAT}
                  </p>
                </div>
              )}
            </Show>

            <p style={{ 'font-size': '11.5px', color: 'var(--color-text-tertiary)' }}>
              {BUDGET_PAUSE_NOTE}
            </p>
          </div>
        )}
      </Show>
    </section>
  );
};

export default BudgetPanel;

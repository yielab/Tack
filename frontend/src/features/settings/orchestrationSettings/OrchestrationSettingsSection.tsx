import { type Component, createResource, createSignal, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Badge, Button, EmptyState, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { orchestrationSettingsApi } from './api';
import ControlPlanesManager from './ControlPlanesManager';
import ProjectLinker from './ProjectLinker';

const numberFmt = new Intl.NumberFormat();

const StatTile: Component<{ label: string; value: string }> = (p) => (
  <div class="rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)' }}>
    <div class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
      {p.label}
    </div>
    <div class="mt-0.5 text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
      {p.value}
    </div>
  </div>
);

const StepHeader: Component<{ n: number; title: string; done?: boolean; locked?: boolean }> = (p) => (
  <div class="flex items-center gap-2">
    <span
      class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-xs font-bold"
      style={{
        'background-color': p.done
          ? 'var(--color-success-100)'
          : p.locked
            ? 'var(--color-bg-subtle)'
            : 'var(--color-primary-100)',
        color: p.done
          ? 'var(--color-success-700)'
          : p.locked
            ? 'var(--color-text-tertiary)'
            : 'var(--color-primary-700)',
      }}
      aria-hidden="true"
    >
      {p.done ? '✓' : p.n}
    </span>
    <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
      {p.title}
    </h3>
    <Show when={p.locked}>
      <Badge tone="neutral">Enable orchestration first</Badge>
    </Show>
  </div>
);

/**
 * Settings → Orchestration — makes the agent-factory control center
 * discoverable. Everything in this file is
 * new; before it, there was no way to discover, enable, or set up
 * orchestration from the UI — an operator had to know to set
 * `TACK_ORCH_ENABLE` and restart the server, sight unseen.
 *
 * **Wire contract:** `GET/PUT /api/settings/orchestration` — the flag is
 * runtime-toggleable and DB-backed, following the existing Cloud Backup
 * precedent; see `./api.ts`'s header comment for the full field
 * rationale, especially why both `source` and `env_default` exist.
 *
 * **The guided setup is sequential because the backend actually is.**
 * `/control-planes` and `/projects/{id}/orch-link` are gated behind the same
 * `TACK_ORCH_ENABLE` check every other orchestration route uses
 * (`crates/tack-api/src/router.rs`'s `orch_routes`) — so Step 2 and Step 3
 * below are visually locked, not just suggested in order, until Step 1's
 * toggle is actually on. Rendering `ControlPlanesManager`/`ProjectLinker`
 * before that would just produce a wall of "disabled" errors from routes
 * that still 404/409 while the flag is off.
 *
 * **Enabling is deliberately not a bare switch.** The paragraph directly
 * under the heading names the concrete consequence — Tack begins polling a
 * configured control plane and can dispatch work to autonomous agents that
 * spend money — before the control that flips it. It also isn't behind a
 * confirmation dialog: the explanation
 * is permanent and unmissable rather than a one-time modal an operator
 * calls up once, reads, and never sees again — and turning the feature back
 * off is always a single, frictionless click, matching how every other
 * optional integration in this Settings page (Cloud Backup, GitHub sync)
 * already works.
 */
const OrchestrationSettingsSection: Component = () => {
  const [settings, { refetch }] = createResource(() => orchestrationSettingsApi.get());
  const [toggling, setToggling] = createSignal(false);

  const enabled = () => settings()?.enabled ?? false;
  const loadFailed = () => settings.error !== undefined;

  const setEnabled = async (next: boolean) => {
    if (toggling() || enabled() === next) return;
    setToggling(true);
    try {
      await orchestrationSettingsApi.update(next);
      await refetch();
      toast.success(
        next
          ? 'Orchestration enabled — Tack will start polling any registered control plane.'
          : 'Orchestration disabled. The reconciler stops on its next cycle; nothing is deleted.',
      );
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update orchestration settings');
    } finally {
      setToggling(false);
    }
  };

  return (
    <section
      id="orchestration-settings"
      class="space-y-4 border-t pt-6"
      style={{ 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex items-center gap-3">
        <h2 class="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Orchestration
        </h2>
        <Show when={!settings.loading && !loadFailed()}>
          <Badge tone={enabled() ? 'success' : 'neutral'}>{enabled() ? 'On' : 'Off'}</Badge>
        </Show>
      </div>

      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        Tack can hand work off to an agent-fleet control plane (e.g. a running{' '}
        <span style={{ color: 'var(--color-text-primary)' }}>docket</span> instance) — a Fleet view of
        agent activity, an approvals inbox, budget tracking, and dispatch straight from an item. Turning
        this on does something real: Tack begins <strong>polling</strong> the control planes you register
        below, and once a project is linked, agents can be <strong>dispatched</strong> to work its items —
        which can spend money. Turning it off stops the polling and dispatch; nothing already recorded is
        deleted.
      </p>

      <Show when={settings.loading}>
        <Skeleton height="140px" />
      </Show>

      <Show when={!settings.loading && loadFailed()}>
        <EmptyState
          icon="⚠️"
          title="Couldn't load orchestration settings"
          description="The request to the server failed. Check your connection and try again."
          action={<Button onClick={() => void refetch()}>Retry</Button>}
        />
      </Show>

      <Show when={!settings.loading && !loadFailed()}>
        {(() => {
          const s = settings()!;
          return (
            <div class="space-y-5">
              {/* Step 1 — enable */}
              <div class="space-y-2">
                <StepHeader n={1} title="Turn orchestration on" done={enabled()} />
                <div
                  class="flex flex-wrap items-center gap-2 pl-8"
                  role="group"
                  aria-label="Orchestration state"
                >
                  <Button
                    variant={enabled() ? 'primary' : 'secondary'}
                    size="sm"
                    disabled={toggling()}
                    aria-pressed={enabled()}
                    onClick={() => void setEnabled(true)}
                  >
                    On
                  </Button>
                  <Button
                    variant={!enabled() ? 'primary' : 'secondary'}
                    size="sm"
                    disabled={toggling()}
                    aria-pressed={!enabled()}
                    onClick={() => void setEnabled(false)}
                  >
                    Off
                  </Button>
                  <Show when={toggling()}>
                    <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                      Saving…
                    </span>
                  </Show>
                </div>
                <p class="pl-8 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                  <Show
                    when={s.source === 'env_default'}
                    fallback={
                      <>
                        Saved from this page, stored in the database — overriding the{' '}
                        <code class="font-mono">TACK_ORCH_ENABLE</code> environment default, which is
                        currently <strong>{s.env_default ? 'on' : 'off'}</strong> on this server.
                      </>
                    }
                  >
                    No database override has been saved yet — this value comes from the{' '}
                    <code class="font-mono">TACK_ORCH_ENABLE</code> environment variable on this server
                    (currently <strong>{s.env_default ? 'on' : 'off'}</strong>). Changing it here saves an
                    explicit override.
                  </Show>
                </p>
              </div>

              {/* Live status */}
              <div class="grid grid-cols-2 gap-2 pl-8 sm:grid-cols-4">
                <StatTile label="Reconciler" value={s.reconciler_running ? 'Running' : 'Stopped'} />
                <StatTile label="Control planes" value={numberFmt.format(s.control_plane_count)} />
                <StatTile label="Linked projects" value={numberFmt.format(s.linked_project_count)} />
                <StatTile label="Poll interval" value={`${s.poll_secs}s`} />
              </div>
              <p class="pl-8 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                Approval token (grant/deny from the Approvals inbox):{' '}
                {s.approval_token_set ? (
                  <Badge tone="success">Configured</Badge>
                ) : (
                  <Badge tone="neutral">Not configured</Badge>
                )}
                {!s.approval_token_set && (
                  <>
                    {' '}
                    — set <code class="font-mono">TACK_ORCH_APPROVAL_TOKEN</code> to allow granting or
                    denying approvals from the UI. Optional; the inbox itself works without it.
                  </>
                )}
              </p>

              {/* Step 2 — control planes */}
              <div class="space-y-2">
                <StepHeader n={2} title="Register a control plane" locked={!enabled()} />
                <div class="pl-8">
                  <Show
                    when={enabled()}
                    fallback={
                      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                        Turn orchestration on above to register a control plane.
                      </p>
                    }
                  >
                    <ControlPlanesManager pollSecs={s.poll_secs} onChanged={() => void refetch()} />
                  </Show>
                </div>
              </div>

              {/* Step 3 — link a project */}
              <div class="space-y-2">
                <StepHeader n={3} title="Link a project" locked={!enabled()} />
                <div class="pl-8">
                  <Show
                    when={enabled()}
                    fallback={
                      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                        Turn orchestration on above, then register a control plane, to link a project to
                        it.
                      </p>
                    }
                  >
                    <ProjectLinker onLinked={() => void refetch()} />
                  </Show>
                </div>
              </div>

              <p class="pl-8 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                Once a project is linked, its agent activity, budget, and guardrail policy live on that
                project's own{' '}
                <A href="/projects" class="underline">
                  Settings → Orchestration
                </A>{' '}
                tab. The <A href="/fleet" class="underline">Fleet</A>,{' '}
                <A href="/approvals" class="underline">
                  Approvals
                </A>
                , and <A href="/economics" class="underline">Economics</A> views in the sidebar pick up
                linked projects automatically.
              </p>
            </div>
          );
        })()}
      </Show>
    </section>
  );
};

export default OrchestrationSettingsSection;

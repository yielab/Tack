import {
  type Component,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  Match,
  Show,
  Switch,
} from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button, EmptyState, Field, Select, Modal, Badge } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import {
  provisioningApi,
  isOrchDisabled,
  BLUEPRINT_OPTIONS,
  type OrchBlueprint,
  type CreateProjectWithPodResponse,
} from './api';
import { formatBudgetCap, suggestRemoteProjectName, isFullPodShape } from './format';

type Step = 1 | 2 | 3 | 4;

/**
 * End-to-end "I want a new product" flow (Phase 37, card D4, task 37.4):
 * product type / template → pod shape → budget → verify command → an
 * explicit confirmation → `POST /api/templates/{id}/provision`. See
 * `handlers::provisioning`'s module doc (Rust) for the rollback design this
 * UI is built around — the short version: a failure before the pod exists
 * rolls the project back automatically (nothing to show here beyond the
 * error), and a failure *after* the pod exists is a distinct, non-error
 * outcome (`pod_created_link_failed`) this component renders as a warning
 * with a concrete next step, never as a red error.
 *
 * **Gating.** `GET /api/control-planes`
 * doubles as the "is orchestration even on" probe — see `api.ts`'s doc
 * comment on `listControlPlanes`. `orchAvailable()` is `false` while
 * loading and on *any* error, not just a 404 — the same conservative "if we
 * can't positively confirm it's on, don't show a privileged control"
 * posture `useAgentActivityMap.orchAvailable`/`ItemDetailDrawer`'s
 * `orchAvailable()` already use. Nothing below this
 * gate ever renders while it's `false`.
 *
 * **Confirmation, not a credential (see the Rust module doc for the full
 * reasoning).** There is no one-click path from opening this page to a pod
 * existing: step 3's "Provision" button only opens a confirmation `Modal`
 * naming the exact docket project name, blueprint, control plane, and
 * budget cap, with the literal words "This creates real infrastructure and
 * cannot be automatically undone." — the same non-reversible-action pattern
 * card D1 built for approval decisions and card C4 built for sprint
 * dispatch. The actual privilege gate is the ordinary `TACK_API_TOKEN` +
 * `TACK_ORCH_ENABLE` pair enforced server-side, deliberately *not*
 * `TACK_ORCH_APPROVAL_TOKEN` — see the Rust handler's module doc for why.
 */
const ProvisioningWizard: Component = () => {
  const navigate = useNavigate();

  const [controlPlanes, { refetch: refetchControlPlanes }] = createResource(() =>
    provisioningApi.listControlPlanes()
  );
  const [templates] = createResource(() => provisioningApi.listTemplates());

  const orchAvailable = () => !controlPlanes.loading && controlPlanes.error === undefined;
  // Guard `.latest` behind an explicit `.error` check rather than calling it
  // unconditionally. `resource.latest` — like calling the resource itself —
  // **throws once the resource has settled into an error state**, a real,
  // reproducible SolidJS behavior (confirmed live while building this
  // component: an unguarded `controlPlanes.latest ?? []` inside this memo
  // silently stopped the whole resource's reactive graph from updating on
  // a 404/500, wedging the page on the loading skeleton forever with no
  // console error — the same failure shape as any other
  // resource accessor called directly from a memo/JSX expression). These
  // two memos are the only place either resource's
  // value is read outside `.loading`/`.error`, so guarding here is enough.
  const controlPlaneList = createMemo(() =>
    controlPlanes.error !== undefined ? [] : (controlPlanes.latest ?? [])
  );
  const templateList = createMemo(() =>
    templates.error !== undefined ? [] : (templates.latest ?? [])
  );

  const [step, setStep] = createSignal<Step>(1);

  const [projectName, setProjectName] = createSignal('');
  const [projectDescription, setProjectDescription] = createSignal('');
  const [selectedTemplateId, setSelectedTemplateId] = createSignal('');

  const [controlPlaneId, setControlPlaneId] = createSignal('');
  const [remoteProject, setRemoteProject] = createSignal('');
  const [remoteProjectTouched, setRemoteProjectTouched] = createSignal(false);
  const [blueprint, setBlueprint] = createSignal<OrchBlueprint>('software');
  const [fullRoster, setFullRoster] = createSignal(false);
  const [budgetUsd, setBudgetUsd] = createSignal('');
  const [verifyCmd, setVerifyCmd] = createSignal('');

  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [submitting, setSubmitting] = createSignal(false);
  const [result, setResult] = createSignal<CreateProjectWithPodResponse | null>(null);
  const [submitError, setSubmitError] = createSignal<string | null>(null);

  const selectedTemplate = createMemo(() =>
    templateList().find((t) => t.id === selectedTemplateId())
  );

  // Keep the suggested docket project name in sync with the Tack project
  // name until the operator edits it directly — after that, their choice
  // always wins (never silently overwritten mid-typing).
  createEffect(() => {
    const name = projectName();
    if (!remoteProjectTouched()) {
      setRemoteProject(suggestRemoteProjectName(name));
    }
  });

  // Seed step 2/3 from the chosen template's `orchestration` defaults, if
  // it has one — a template that already declares a blueprint/budget/
  // verify command is exactly the "pipeline library" this wizard is meant
  // to draw on, not something the operator should
  // have to re-type. Only applied on template *change*, so re-selecting
  // the same template never clobbers an edit the operator already made.
  createEffect(() => {
    const orch = selectedTemplate()?.orchestration;
    if (!orch) return;
    setBlueprint(orch.blueprint);
    setFullRoster(isFullPodShape(orch.pod_shape));
    if (orch.budget_usd != null) setBudgetUsd(String(orch.budget_usd));
    if (orch.verify_cmd) setVerifyCmd(orch.verify_cmd);
  });

  const step1Valid = () => projectName().trim().length > 0 && selectedTemplateId().length > 0;
  const step2Valid = () => controlPlaneId().length > 0 && remoteProject().trim().length > 0;

  const resetForAnotherAttempt = () => {
    setResult(null);
    setSubmitError(null);
    setStep(1);
  };

  const submit = async () => {
    setSubmitting(true);
    setSubmitError(null);
    try {
      const res = await provisioningApi.provision(selectedTemplateId(), {
        name: projectName().trim(),
        description: projectDescription().trim() || null,
        provision_pod: {
          control_plane_id: controlPlaneId(),
          remote_project: remoteProject().trim(),
          blueprint: blueprint(),
          pod_shape: fullRoster() ? 'full' : null,
          budget_usd: budgetUsd().trim() ? Number(budgetUsd()) : null,
          verify_cmd: verifyCmd().trim() || null,
        },
      });
      setResult(res);
      setStep(4);
      if (res.provisioning.status === 'linked') {
        toast.success(`"${res.project.name}" created and linked to a live pod.`);
      } else {
        toast.warning('Project and pod were created, but linking needs a manual step.');
      }
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Provisioning failed');
      setStep(4);
      toast.error('Provisioning failed');
    } finally {
      setSubmitting(false);
      setConfirmOpen(false);
    }
  };

  return (
    <div class="mx-auto max-w-2xl px-4 py-8">
      <h1 class="text-xl font-semibold" style={{ color: 'var(--color-text-primary)' }}>
        Provision a new product
      </h1>
      <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        Create a Tack project from a template, provision a docket pod for it, and link the two —
        end to end, with the exact same rollback guarantee whether it succeeds or fails partway.
      </p>

      <Switch fallback={<div class="mt-8 h-24 animate-pulse rounded-lg" style={{ background: 'var(--color-bg-subtle)' }} />}>
        <Match when={controlPlanes.loading}>
          <div class="mt-8 h-24 animate-pulse rounded-lg" style={{ background: 'var(--color-bg-subtle)' }} />
        </Match>
        <Match when={!orchAvailable() && !isOrchDisabled(controlPlanes.error)}>
          <div class="mt-8">
            <EmptyState
              icon="⚠️"
              title="Could not check orchestration status"
              description="Something went wrong reaching the API. Try reloading the page."
              action={<Button onClick={() => refetchControlPlanes()}>Retry</Button>}
            />
          </div>
        </Match>
        <Match when={!orchAvailable() && isOrchDisabled(controlPlanes.error)}>
          <div class="mt-8">
            <EmptyState
              icon="🛰️"
              title="Orchestration is disabled"
              description="Turn it on to provision a new project with a docket pod already linked to it."
              action={
                <Button onClick={() => navigate('/settings?section=orchestration')}>
                  Set up orchestration
                </Button>
              }
            />
          </div>
        </Match>
        <Match when={orchAvailable() && controlPlaneList().length === 0}>
          <div class="mt-8">
            <EmptyState
              icon="🛰️"
              title="No control planes registered"
              description="Register a control plane (e.g. a running docket instance) first — via Settings, or POST /api/control-planes — then come back here to provision a pod on it."
            />
          </div>
        </Match>
        <Match when={orchAvailable() && controlPlaneList().length > 0}>
          <div>
            {/* ── Step indicator ─────────────────────────────────────── */}
            <ol class="mt-6 flex gap-2 text-xs" aria-label="Provisioning steps">
              <For each={['Project', 'Pod', 'Confirm', 'Result'] as const}>
                {(label, i) => (
                  <li
                    class="flex-1 rounded-md px-2 py-1.5 text-center font-medium"
                    style={{
                      background:
                        step() === i() + 1 ? 'var(--color-accent-soft)' : 'var(--color-bg-subtle)',
                      color:
                        step() === i() + 1
                          ? 'var(--color-accent-ink)'
                          : 'var(--color-text-secondary)',
                    }}
                    aria-current={step() === i() + 1 ? 'step' : undefined}
                  >
                    {i() + 1}. {label}
                  </li>
                )}
              </For>
            </ol>

            {/* ── Step 1 — Project & template ────────────────────────── */}
            <Show when={step() === 1}>
              <div class="mt-6 space-y-4">
                <Field
                  label="Project name"
                  required
                  value={projectName()}
                  onInput={(e) => setProjectName(e.currentTarget.value)}
                />
                <Field
                  label="Description (optional)"
                  value={projectDescription()}
                  onInput={(e) => setProjectDescription(e.currentTarget.value)}
                />
                <Select
                  label="Template"
                  required
                  value={selectedTemplateId()}
                  onChange={(e) => setSelectedTemplateId(e.currentTarget.value)}
                >
                  <option value="" disabled>
                    Select a template…
                  </option>
                  <For each={templateList()}>
                    {(t) => (
                      <option value={t.id}>
                        {t.name} ({t.project_type}){t.orchestration ? ' — has pod defaults' : ''}
                      </option>
                    )}
                  </For>
                </Select>
                <div class="flex justify-end">
                  <Button disabled={!step1Valid()} onClick={() => setStep(2)}>
                    Next: Pod
                  </Button>
                </div>
              </div>
            </Show>

            {/* ── Step 2 — Pod & control plane ───────────────────────── */}
            <Show when={step() === 2}>
              <div class="mt-6 space-y-4">
                <Select
                  label="Control plane"
                  required
                  value={controlPlaneId()}
                  onChange={(e) => setControlPlaneId(e.currentTarget.value)}
                >
                  <option value="" disabled>
                    Select a control plane…
                  </option>
                  <For each={controlPlaneList()}>
                    {(p) => (
                      <option value={p.id}>
                        {p.name} ({p.kind}, {p.health})
                      </option>
                    )}
                  </For>
                </Select>
                <Field
                  label="Docket project name"
                  required
                  value={remoteProject()}
                  onInput={(e) => {
                    setRemoteProjectTouched(true);
                    setRemoteProject(e.currentTarget.value);
                  }}
                  hint="The pod's identifier on the control plane — must be unique there. Suggested from the project name; edit freely."
                />
                <Select
                  label="Blueprint"
                  value={blueprint()}
                  onChange={(e) => setBlueprint(e.currentTarget.value as OrchBlueprint)}
                >
                  <For each={BLUEPRINT_OPTIONS}>
                    {(o) => <option value={o.value}>{o.label}</option>}
                  </For>
                </Select>
                <Show when={blueprint() === 'software'}>
                  <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <input
                      type="checkbox"
                      checked={fullRoster()}
                      onChange={(e) => setFullRoster(e.currentTarget.checked)}
                    />
                    Full roster (lead, implementer, reviewer, tester)
                  </label>
                </Show>
                <Field
                  label="Budget cap, USD (optional)"
                  type="number"
                  min="0"
                  step="0.01"
                  value={budgetUsd()}
                  onInput={(e) => setBudgetUsd(e.currentTarget.value)}
                  hint="A cap for this pod's Lead agent. Not enforced by Tack itself — docket applies it. Leave blank to use the blueprint's own default."
                />
                <Field
                  label="Verify command (optional)"
                  value={verifyCmd()}
                  onInput={(e) => setVerifyCmd(e.currentTarget.value)}
                  hint="Run after each Implementer hop; a non-zero exit blocks completion."
                />
                <div class="flex justify-between">
                  <Button variant="secondary" onClick={() => setStep(1)}>
                    Back
                  </Button>
                  <Button disabled={!step2Valid()} onClick={() => setStep(3)}>
                    Next: Review
                  </Button>
                </div>
              </div>
            </Show>

            {/* ── Step 3 — Review & confirm ──────────────────────────── */}
            <Show when={step() === 3}>
              <div class="mt-6 space-y-4">
                <SummaryRow label="Project" value={projectName()} />
                <SummaryRow label="Template" value={selectedTemplate()?.name ?? ''} />
                <SummaryRow
                  label="Control plane"
                  value={controlPlaneList().find((p) => p.id === controlPlaneId())?.name ?? ''}
                />
                <SummaryRow label="Docket project" value={remoteProject()} />
                <SummaryRow
                  label="Blueprint"
                  value={`${blueprint()}${fullRoster() ? ' (full roster)' : ''}`}
                />
                <SummaryRow label="Budget cap" value={formatBudgetCap(budgetUsd() ? Number(budgetUsd()) : null)} />
                <SummaryRow label="Verify command" value={verifyCmd() || '(none)'} />
                <div class="flex justify-between pt-2">
                  <Button variant="secondary" onClick={() => setStep(2)}>
                    Back
                  </Button>
                  <Button onClick={() => setConfirmOpen(true)}>Provision…</Button>
                </div>
              </div>

              <Modal isOpen={confirmOpen()} onClose={() => setConfirmOpen(false)} title="Confirm provisioning" size="sm">
                <div class="space-y-4">
                  <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    This will create a real docket pod named <strong>{remoteProject()}</strong> (blueprint{' '}
                    <strong>{blueprint()}</strong>) on control plane{' '}
                    <strong>{controlPlaneList().find((p) => p.id === controlPlaneId())?.name}</strong>,
                    with a budget cap of <strong>{formatBudgetCap(budgetUsd() ? Number(budgetUsd()) : null)}</strong>.
                  </p>
                  <p class="text-sm font-medium" style={{ color: 'var(--color-danger-700)' }}>
                    This creates real infrastructure and cannot be automatically undone.
                  </p>
                  <div class="flex justify-end gap-2">
                    <Button variant="secondary" onClick={() => setConfirmOpen(false)} disabled={submitting()}>
                      Cancel
                    </Button>
                    <Button onClick={() => void submit()} loading={submitting()} disabled={submitting()}>
                      Confirm &amp; provision
                    </Button>
                  </div>
                </div>
              </Modal>
            </Show>

            {/* ── Step 4 — Result ────────────────────────────────────── */}
            <Show when={step() === 4}>
              <div class="mt-6 space-y-4">
                <Show
                  when={result()}
                  fallback={
                    <div>
                      <EmptyState
                        icon="❌"
                        title="Provisioning failed"
                        description={submitError() ?? 'Something went wrong.'}
                      />
                      <div class="mt-4 flex justify-center">
                        <Button variant="secondary" onClick={resetForAnotherAttempt}>
                          Start over
                        </Button>
                      </div>
                    </div>
                  }
                >
                  {(r) => <ResultPanel result={r()} onDone={() => navigate(`/projects/${r().project.id}/board`)} />}
                </Show>
              </div>
            </Show>
          </div>
        </Match>
      </Switch>
    </div>
  );
};

const SummaryRow: Component<{ label: string; value: string }> = (p) => (
  <div class="flex items-baseline justify-between gap-4 text-sm">
    <span style={{ color: 'var(--color-text-secondary)' }}>{p.label}</span>
    <span class="text-right font-medium" style={{ color: 'var(--color-text-primary)' }}>
      {p.value}
    </span>
  </div>
);

const ResultPanel: Component<{ result: CreateProjectWithPodResponse; onDone: () => void }> = (props) => {
  const outcome = () => props.result.provisioning;
  const linked = () => outcome().status === 'linked';

  return (
    <div class="space-y-4">
      <div class="flex items-center gap-2">
        <Badge tone={linked() ? 'success' : 'warning'}>
          {linked() ? 'Linked' : 'Pod created — link needs a manual step'}
        </Badge>
      </div>
      <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
        Project <strong>{props.result.project.name}</strong> was created, and pod{' '}
        <strong>{outcome().remote_project}</strong> ({outcome().blueprint}) was provisioned with{' '}
        {outcome().members.length} member{outcome().members.length === 1 ? '' : 's'}:
      </p>
      <ul class="list-inside list-disc text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        <For each={outcome().members}>
          {(m) => (
            <li>
              {m.role} — {m.model}
            </li>
          )}
        </For>
      </ul>
      <Show when={outcome().warnings.length > 0}>
        <div
          class="rounded-lg border p-3 text-sm"
          style={{
            border: '1px solid var(--color-warning-600)',
            'background-color': 'var(--color-warning-100)',
            color: 'var(--color-warning-700)',
          }}
        >
          <For each={outcome().warnings}>{(w) => <p>{w}</p>}</For>
        </div>
      </Show>
      <div class="flex justify-end">
        <Button onClick={props.onDone}>Go to project</Button>
      </div>
    </div>
  );
};

export default ProvisioningWizard;

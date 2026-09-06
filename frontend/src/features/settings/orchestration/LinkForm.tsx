import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button, Field, Select, EmptyState } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { orchestrationApi } from './api';

export interface LinkFormProps {
  projectId: string;
  /** Called after a successful link so the parent can refetch and swap to
   *  the Budget/Policy panels. */
  onLinked: () => void;
}

/**
 * Minimal "link this project to a control plane" form —
 * `PUT /api/projects/{id}/orch-link` was reachable but no page ever called
 * it, so a fresh project had no way to get budget/policy data populated at
 * all short of `curl`. Deliberately minimal: control plane + remote project
 * name + optional budget cap. `status_map`/`auto_dispatch`/`blueprint` are
 * left at their defaults (no dispatch policy configured) — that's the
 * dispatch UI's territory, not this form's.
 */
const LinkForm: Component<LinkFormProps> = (props) => {
  const navigate = useNavigate();
  const [planes] = createResource(() => orchestrationApi.listControlPlanes());
  const [controlPlaneId, setControlPlaneId] = createSignal('');
  const [remoteProject, setRemoteProject] = createSignal('');
  const [budgetUsd, setBudgetUsd] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!controlPlaneId() || !remoteProject().trim()) return;
    setSaving(true);
    try {
      await orchestrationApi.putLink(props.projectId, {
        control_plane_id: controlPlaneId(),
        remote_project: remoteProject().trim(),
        budget_usd: budgetUsd().trim() ? Number(budgetUsd()) : null,
      });
      toast.success('Linked to control plane');
      props.onLinked();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to link project');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Show
      when={!planes.loading && (planes()?.length ?? 0) > 0}
      fallback={
        <Show when={!planes.loading}>
          <EmptyState
            icon="🛰️"
            title="No control planes registered"
            description="Register a control plane (e.g. a running docket instance) first, then come back here to link this project to it."
            action={
              <Button onClick={() => navigate('/settings?section=orchestration')}>
                Register a control plane
              </Button>
            }
          />
        </Show>
      }
    >
      <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-4">
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Link this project to a control plane to see its agent budget and guardrail policy
          activity below.
        </p>
        <Select
          label="Control plane"
          required
          value={controlPlaneId()}
          onChange={(e) => setControlPlaneId(e.currentTarget.value)}
        >
          <option value="" disabled>
            Select a control plane…
          </option>
          <For each={planes()}>{(p) => <option value={p.id}>{p.name} ({p.kind})</option>}</For>
        </Select>
        <Field
          label="Remote project name"
          required
          value={remoteProject()}
          onInput={(e) => setRemoteProject(e.currentTarget.value)}
          hint="The project/pod name as known to the control plane (e.g. docket's own project id)."
        />
        <Field
          label="Budget cap (USD, optional)"
          type="number"
          min="0"
          step="0.01"
          value={budgetUsd()}
          onInput={(e) => setBudgetUsd(e.currentTarget.value)}
          hint="A configured cap to compare estimated spend against — not enforced by Tack itself."
        />
        <Button type="submit" loading={saving()} disabled={saving() || !controlPlaneId()}>
          Link project
        </Button>
      </form>
    </Show>
  );
};

export default LinkForm;

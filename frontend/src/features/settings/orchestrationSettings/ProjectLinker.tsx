import { A } from '@solidjs/router';
import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { Badge, Select, Skeleton } from '../../../shared/ui';
import { api } from '../../../shared/api';
import { orchestrationApi } from '../orchestration/api';
import LinkForm from '../orchestration/LinkForm';

/**
 * Step 3 of the guided setup: pick a Tack project and link it to a control
 * plane. Deliberately reuses
 * `LinkForm.tsx` — a second link form
 * must not be built — rather than re-implementing `PUT /api/projects/{id}/
 * orch-link` here. `LinkForm` already handles "no control planes yet" on its
 * own, so this component's only job is the piece that didn't exist before:
 * choosing *which* project to link, since `LinkForm` has always required a
 * `projectId` prop and nothing upstream ever supplied one outside of
 * Project Settings' own Orchestration tab.
 */
export interface ProjectLinkerProps {
  /** Called after a successful link so the parent
   *  (`OrchestrationSettingsSection`) can refresh its own
   *  `linked_project_count`. */
  onLinked?: () => void;
}

const ProjectLinker: Component<ProjectLinkerProps> = (props) => {
  const [projects] = createResource(() => api.projects.list());
  const [selected, setSelected] = createSignal('');

  const [link, { refetch: refetchLink }] = createResource(
    selected,
    (id) => (id ? orchestrationApi.getLink(id) : Promise.resolve(undefined)),
  );

  return (
    <div class="max-w-md space-y-3">
      <Show when={projects.loading}>
        <Skeleton height="38px" />
      </Show>

      <Show when={!projects.loading}>
        <Show
          when={(projects() ?? []).length > 0}
          fallback={
            <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              No projects yet —{' '}
              <A href="/projects" class="underline">
                create one
              </A>{' '}
              first, then come back here to link it.
            </p>
          }
        >
          <Select
            label="Project to link"
            value={selected()}
            onChange={(e) => setSelected(e.currentTarget.value)}
          >
            <option value="" disabled>
              Select a project…
            </option>
            <For each={projects()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
          </Select>
        </Show>
      </Show>

      <Show when={selected()}>
        <Show when={link.loading}>
          <Skeleton height="120px" />
        </Show>

        <Show when={!link.loading && link()?.linked === true}>
          {(() => {
            const l = link();
            return (
              <div
                class="flex items-center justify-between gap-3 rounded-lg border p-3 text-sm"
                style={{ 'border-color': 'var(--color-border-light)' }}
              >
                <div>
                  <Badge tone="success">Already linked</Badge>
                  <p class="mt-1" style={{ color: 'var(--color-text-secondary)' }}>
                    Remote project <code class="font-mono">{l?.link?.remote_project}</code>
                  </p>
                </div>
                <A href={`/projects/${selected()}/settings?tab=orchestration`} class="underline text-sm">
                  Manage budget &amp; policy
                </A>
              </div>
            );
          })()}
        </Show>

        <Show when={!link.loading && link()?.linked === false}>
          <LinkForm
            projectId={selected()}
            onLinked={() => {
              void refetchLink();
              props.onLinked?.();
            }}
          />
        </Show>
      </Show>
    </div>
  );
};

export default ProjectLinker;

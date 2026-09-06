import { type Component, createResource, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Button, EmptyState, Skeleton } from '../../../shared/ui';
import { orchestrationApi, isOrchDisabled, type OrchLink } from './api';
import LinkForm from './LinkForm';
import BudgetPanel from './BudgetPanel';
import PolicyPanel from './PolicyPanel';

export interface OrchestrationPanelProps {
  projectId: string;
}

/**
 * Project Settings → Orchestration tab: budget and policy panels for this
 * project's linked control plane. No pause control anywhere on this page —
 * see `./api.ts`'s header comment for why that's a deliberate omission, not
 * an oversight.
 *
 * Gating follows the same `orchAvailable()` pattern used elsewhere: `false`
 * while loading and on ANY fetch error, not just a 404, so a control that's
 * about to 404 never renders. This panel's own `GET
 * /projects/{id}/orch-link` fetch doubles as that probe — the same
 * "reuse the already-in-flight request instead of adding a second probe"
 * reasoning `useAgentActivityMap.orchAvailable` documents.
 */
const OrchestrationPanel: Component<OrchestrationPanelProps> = (props) => {
  const navigate = useNavigate();
  const [linkRes, { refetch }] = createResource(
    () => props.projectId,
    (id) => orchestrationApi.getLink(id)
  );

  const orchAvailable = () => !linkRes.loading && linkRes.error === undefined;
  const disabled = () => isOrchDisabled(linkRes.error);
  const failed = () => linkRes.error !== undefined && !disabled();
  /** The linked `OrchLink`, or `undefined` when not (yet) linked/available —
   *  a plain accessor rather than a boolean-chain `when` expression, so
   *  `Show`'s callback gets a clean `OrchLink` type. */
  const link = (): OrchLink | undefined =>
    orchAvailable() && linkRes()?.linked ? (linkRes()?.link ?? undefined) : undefined;

  return (
    <div>
      <Show when={linkRes.loading}>
        <Skeleton height="120px" />
      </Show>

      <Show when={!linkRes.loading && disabled()}>
        <EmptyState
          icon="🔌"
          title="Agent-fleet orchestration is disabled"
          description="Turn it on from Settings to link this project to a control plane and configure a budget or view policy activity here."
          action={
            <Button onClick={() => navigate('/settings?section=orchestration')}>
              Set up orchestration
            </Button>
          }
        />
      </Show>

      <Show when={!linkRes.loading && failed()}>
        <EmptyState
          icon="⚠️"
          title="Couldn't load orchestration status"
          description="The request to the server failed. Check your connection and try again."
          action={<Button onClick={() => void refetch()}>Retry</Button>}
        />
      </Show>

      <Show when={orchAvailable() && linkRes()?.linked === false}>
        <LinkForm projectId={props.projectId} onLinked={() => void refetch()} />
      </Show>

      <Show when={link()}>
        {(l) => (
          <div class="space-y-8">
            <BudgetPanel
              projectId={props.projectId}
              link={l()}
              onBudgetSaved={() => void refetch()}
            />
            <PolicyPanel projectId={props.projectId} />
          </div>
        )}
      </Show>
    </div>
  );
};

export default OrchestrationPanel;

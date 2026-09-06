import { type Component, createResource, createSignal, createMemo, createEffect, onCleanup, untrack, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import Drawer from '../../shared/ui/Drawer';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import Button from '../../shared/ui/Button';
import { api } from '../../shared/api';
import { isItemVersionConflict } from '../../shared/api/items';
import { toast } from '../../shared/ui/toast';
import type { Item, UpdateItem } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { agentActivityApi } from '../../shared/agentActivity/api';
import { createBoardSocket } from '../../shared/realtime/boardSocket';
import { dispatchApi, type DispatchItemResponse } from '../../shared/dispatch/api';
import { notifyDispatchOutcome } from '../../shared/dispatch/notify';
import { dispatchOutcomeDetail } from '../../shared/dispatch/format';
import DispatchOutcomeNote from '../../shared/dispatch/DispatchOutcomeNote';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import ExecutionTimeline from '../../shared/runWithAgent/ExecutionTimeline';
import ItemHeader from './ItemHeader';
import DetailsTab from './tabs/DetailsTab';
import ActivityTab from './tabs/ActivityTab';
import DependenciesTab from './tabs/DependenciesTab';
import FilesTab from './tabs/FilesTab';
import FieldsTab from './tabs/FieldsTab';
import AgentActivityTab from './tabs/AgentActivityTab';

const BASE_TABS: TabItem[] = [
  { id: 'details', label: 'Details' },
  { id: 'activity', label: 'Activity' },
  // "Execution" — the newer, neutral execution
  // domain (`ExecutionRequest`/`ExecutionAttempt` via `tack-runner`). Always
  // present, unlike the legacy "Agent Activity" tab below (which only
  // appears once Docket activity actually exists) — an item with zero
  // execution requests is still a real, honest state worth a visible empty
  // tab (`ExecutionTimeline`'s own `EmptyState`), not a hidden one.
  { id: 'execution', label: 'Execution' },
  { id: 'dependencies', label: 'Dependencies' },
  { id: 'files', label: 'Files' },
  { id: 'fields', label: 'Fields' },
];

/**
 * Item detail drawer. Mounted once at the app root; opens whenever the
 * `?item=<id>` query param is present (deep-linkable / shareable), fetches the
 * item, and exposes inline header editing + a tab bar. Built on the kit Drawer
 * (ESC + focus return).
 */
/** Every tab id this drawer can land on directly, including the dynamic
 *  "Agent Activity" tab `tabs()` below only adds once activity exists —
 *  `?tab=` is honored regardless of whether that tab is showing yet. */
const DEEP_LINKABLE_TAB_IDS = new Set([...BASE_TABS.map((t) => t.id), 'agent']);

function tabFromSearchParam(value: string | string[] | undefined): string {
  return typeof value === 'string' && DEEP_LINKABLE_TAB_IDS.has(value) ? value : 'details';
}

const ItemDetailDrawer: Component = () => {
  const [searchParams, setSearchParams] = useSearchParams();
  const itemId = () => (searchParams.item as string | undefined) || undefined;

  const [item, { mutate, refetch }] = createResource(
    itemId,
    (id) => (id ? api.items.get(id) : null),
  );

  // Defaults to `details`, but a `?tab=` alongside `?item=` (the attempt-state
  // chip on a Board card, or `onCreated` switching this same drawer to
  // `execution` after a fresh run) opens straight to that tab instead.
  //
  // Tracks `itemId()` only, the same one-shot-per-open pattern the dispatch
  // reset effect below already uses — NOT `searchParams.tab` itself, and the
  // param is read `untrack`ed and immediately cleared once applied. Every
  // other "open an item" call site across the app (Timeline, Calendar,
  // List, `Board.tsx`'s own card-body click, `DependenciesTab`) calls
  // `setSearchParams({ item: id })` with no `tab` key, and `setSearchParams`
  // merges rather than replaces — so without the clear, a `tab=execution`
  // left over from one chip click would silently redirect every
  // subsequently opened item to the Execution tab. The one known gap this
  // leaves: re-clicking the chip for the item ALREADY open (itemId
  // unchanged) does not re-apply the tab — a narrower, and much less
  // surprising, edge case than the leak this avoids.
  const [activeTab, setActiveTab] = createSignal(tabFromSearchParam(untrack(() => searchParams.tab)));
  createEffect(() => {
    const id = itemId();
    if (!id) return;
    const requestedTab = untrack(() => searchParams.tab);
    setActiveTab(tabFromSearchParam(requestedTab));
    if (requestedTab !== undefined) setSearchParams({ tab: undefined }, { replace: true });
  });

  // Agent activity is fetched once here — not inside `AgentActivityTab`, unlike
  // every other tab — because the drawer needs to know whether the item HAS
  // any agent activity before deciding whether to show the tab at all: an
  // item with no agent activity shows no chip and no empty tab. A 404
  // (`TACK_ORCH_ENABLE` unset — the default install state) or any other
  // fetch failure is treated the same as "no
  // activity": the tab quietly doesn't appear rather than surfacing an error
  // for a feature most installs haven't turned on.
  const [agentActivity, { refetch: refetchAgentActivity }] = createResource(itemId, (id) =>
    id ? agentActivityApi.getForItem(id) : null,
  );

  // A mirrored agent run or
  // approval change for the item currently open in this drawer should update
  // the "Agent Activity" tab without a manual reopen. The socket needs the
  // item's *project*, not just its id, and that's only known once `item()`
  // has loaded — so this waits on the item resource rather than opening a
  // socket the instant the drawer does.
  createEffect(() => {
    const projectId = item()?.project_id;
    const id = itemId();
    if (!projectId || !id) return;
    const s = createBoardSocket(projectId);
    const off = s.onEvent((event) => {
      if (
        (event.type === 'agent_run_updated' || event.type === 'approval_pending') &&
        event.item_id === id
      ) {
        void refetchAgentActivity();
      }
    });
    onCleanup(() => { off(); s.close(); });
  });

  // Dispatch-to-agents control. Reuses
  // the per-item agent-activity fetch above as the "is orchestration even
  // enabled here" signal rather than adding a second probe: `orchAvailable`
  // is true only once that fetch has resolved WITHOUT error — a 404
  // (`TACK_ORCH_ENABLE` unset, the default install state)
  // or any other failure both mean "don't show a privileged control that's
  // about to fail," the same conservative posture `useAgentActivityMap`'s
  // own `orchAvailable` applies for the Board/List/Table badges.
  const orchAvailable = () => !agentActivity.loading && agentActivity.error === undefined;
  const [dispatching, setDispatching] = createSignal(false);
  const [lastDispatch, setLastDispatch] = createSignal<DispatchItemResponse | null>(null);
  // Clear any previous result when a different item opens, so a stale
  // "dispatched"/"blocked" note never appears to belong to the newly opened item.
  createEffect(() => { itemId(); setLastDispatch(null); });

  const dispatchToAgents = async () => {
    const current = item();
    if (!current) return;
    setDispatching(true);
    try {
      const res = await dispatchApi.dispatchItem(current.id);
      setLastDispatch(res);
      notifyDispatchOutcome(res);
      void refetchAgentActivity();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to dispatch item');
    } finally {
      setDispatching(false);
    }
  };

  // A Solid resource accessor THROWS once it has errored (calling
  // `agentActivity()` directly, as opposed to reading `.loading`/`.error`) —
  // calling the
  // errored accessor from inside a memo that runs in the same reactive batch
  // that just set the error aborts that batch, silently wedging sibling
  // computations (found the hard way via `DispatchSprintModal`'s equivalent
  // resource — see its `dryRunData` for the fuller explanation). Every read
  // of the resource's *value* goes through this safe accessor instead —
  // `undefined` once errored, exactly like `agentActivity()` would return if
  // it simply hadn't thrown.
  const agentActivityData = () => (agentActivity.error !== undefined ? undefined : agentActivity());

  const hasAgentActivity = () => {
    const a = agentActivityData();
    return !!a && ((a.attempts?.length ?? 0) > 0 || (a.approvals?.length ?? 0) > 0);
  };
  const tabs = createMemo((): TabItem[] => {
    if (!hasAgentActivity()) return BASE_TABS;
    // Placed right after "Activity" — agent activity is a variant of the
    // item's activity history, not an unrelated concern.
    const idx = BASE_TABS.findIndex((t) => t.id === 'activity');
    return [
      ...BASE_TABS.slice(0, idx + 1),
      { id: 'agent', label: 'Agent Activity' },
      ...BASE_TABS.slice(idx + 1),
    ];
  });

  const close = () => setSearchParams({ item: undefined });

  // Optimistic PATCH: apply locally, persist, then notify the host (or roll back).
  const patch = async (p: UpdateItem) => {
    const current = item();
    if (!current) return;
    mutate({ ...current, ...p } as Item);
    try {
      const updated = await api.items.update(current.id, p);
      mutate(updated);
      window.dispatchEvent(new CustomEvent(ITEM_UPDATED_EVENT, { detail: updated }));
    } catch (err) {
      void refetch(); // reconcile to the server value
      toast.error(
        isItemVersionConflict(err)
          ? 'This item changed elsewhere. It has been refreshed; review it and retry your edit.'
          : err instanceof Error ? err.message : 'Failed to update item',
      );
    }
  };

  // Debounce description edits so we don't PATCH on every keystroke.
  let descTimer: ReturnType<typeof setTimeout> | undefined;
  const onDescriptionChange = (html: string) => {
    if (descTimer) clearTimeout(descTimer);
    descTimer = setTimeout(() => void patch({ description: html }), 600);
  };

  return (
    <Drawer isOpen={!!itemId()} onClose={close} title="Item details" width="md">
      <Show
        when={item()}
        fallback={
          <p class="py-8 text-center text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
            {item.loading ? 'Loading…' : 'Item not found.'}
          </p>
        }
      >
        {(it) => (
          <div class="space-y-6">
            <ItemHeader item={it()} onPatch={patch} />

            {/* Run with agent — the newer, neutral
                execution surface (`ExecutionRequest`/`ExecutionAttempt` via
                `tack-runner`). Deliberately its own control, visually and
                structurally separate from the older "Dispatch to agents"
                block below (a different, older Docket-backed feature) — the
                two are not variants of one
                feature and never share a component. A successful run
                switches straight to the new "Execution" tab so the request
                that just appeared is immediately visible, without a page
                navigation. */}
            <div
              class="flex flex-wrap items-center gap-3 rounded-lg border p-3"
              style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
            >
              <RunWithAgentButton
                itemId={it().id}
                itemTitle={it().title}
                projectId={it().project_id}
                onCreated={() => setActiveTab('execution')}
              />
            </div>

            {/* Dispatch to agents — a
                privileged, outward-facing action, so it's a distinct,
                explicit control rather than folded into an existing button
                row. Only rendered once `orchAvailable()` positively confirms
                the feature is on for this install (off by
                default). The three distinct outcomes
                (queued/dispatched, policy-blocked, waiting-approval)
                render via the same `DispatchOutcomeNote` the sprint dispatch
                modal uses, so they read identically everywhere. */}
            <Show when={orchAvailable()}>
              <div
                class="flex flex-wrap items-center gap-3 rounded-lg border p-3"
                style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
              >
                <Button size="sm" onClick={dispatchToAgents} loading={dispatching()} disabled={dispatching()}>
                  Dispatch to agents
                </Button>
                <Show when={lastDispatch()}>
                  {(res) => <DispatchOutcomeNote decision={res().outcome} detail={dispatchOutcomeDetail(res())} />}
                </Show>
              </div>
            </Show>

            <Tabs tabs={tabs()} active={activeTab()} onChange={setActiveTab}>
              <Show when={activeTab() === 'details'}>
                <DetailsTab item={it()} onDescriptionChange={onDescriptionChange} />
              </Show>
              <Show when={activeTab() === 'activity'}>
                <ActivityTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'execution'}>
                <ExecutionTimeline itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'agent'}>
                <AgentActivityTab activity={agentActivityData()} loading={agentActivity.loading} />
              </Show>
              <Show when={activeTab() === 'dependencies'}>
                <DependenciesTab item={it()} />
              </Show>
              <Show when={activeTab() === 'files'}>
                <FilesTab itemId={it().id} />
              </Show>
              <Show when={activeTab() === 'fields'}>
                <FieldsTab item={it()} />
              </Show>
            </Tabs>
          </div>
        )}
      </Show>
    </Drawer>
  );
};

export default ItemDetailDrawer;

/** Helper for host views: the query patch that opens the drawer for an item. */
export const openItemParam = (itemId: string) => ({ item: itemId });

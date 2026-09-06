import { type Component, createSignal, createMemo, onMount, onCleanup, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Button, Badge } from '../ui';
import RunWithAgentModal from './RunWithAgentModal';
import { useExecutionStore } from '../state/executionContext';
import { describeExecutionState } from './shared';
import type { RunnerCapabilities } from '../execution';

export interface RunWithAgentButtonProps {
  itemId: string;
  itemTitle: string;
  /** The item's project (`Item.project_id`) — threaded through to the modal
   *  so it can read that project's model default. */
  projectId: string;
  /** Icon-only trigger for tight spaces (Board card header, Sprint lane
   *  row) — visually and structurally distinct from `shared/dispatch/
   *  DispatchCardMenu.tsx`'s "⋮" kebab menu: this is a single, explicit
   *  "run" action, not a menu that happens to contain one. `false` renders
   *  a labeled button (item-detail). */
  compact?: boolean;
  /** Shows a small badge, next to the trigger, for the item's most recent
   *  execution request state — registers this item with the shared
   *  store's batched fetch (`store.ts#watchItem`) rather than issuing a
   *  fetch of its own. Off by default; only the Board card mounts it
   *  today. Clicking it opens the item to its Execution tab
   *  (`?item=<id>&tab=execution`), the same tab a successful create
   *  switches item-detail to via `onCreated`. */
  showStateChip?: boolean;
  onCreated?: (requestId: string) => void;
  capabilities?: () => RunnerCapabilities[];
}

/**
 * The one trigger component every "Run with agent" entry point (Board,
 * item-detail, Sprint) mounts to open `RunWithAgentModal` — kept separate
 * from the modal itself so each host only needs one import and one prop set,
 * and so the open/close signal never leaks into a host component's own
 * state: a host gets an entry point and a slot, not a redesign.
 */
const RunWithAgentButton: Component<RunWithAgentButtonProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [, setSearchParams] = useSearchParams();
  const store = useExecutionStore();

  // Registers this item with the store's batched badge fetch for as long as
  // this component is mounted — never a per-card fetch of its own (see
  // `store.ts#watchItem`'s doc comment for how many badges mounting at once
  // still cost exactly one request).
  onMount(() => {
    const unwatch = store.watchItem(props.itemId);
    onCleanup(unwatch);
  });

  // The item's most recent execution request, read from the shared store.
  // `null` whenever there is none yet, or the one record fetched for it
  // errored (an errored record is a real, distinct state, never mistaken
  // for "no activity" — `ExecutionRequestRecord`'s own doc comment). Absence
  // from the cache is conclusive: `watchItem`'s batched fetch asks the
  // server by this exact id, so there is no bounded/truncated preload left
  // to be uncertain about.
  const latestState = createMemo(() => {
    const record = store.requestsForItem(props.itemId)[0];
    if (record?.status === 'ready' && record.summary) return describeExecutionState(record.summary.state);
    return null;
  });

  const openExecutionTab = (e: MouseEvent) => {
    e.stopPropagation();
    setSearchParams({ item: props.itemId, tab: 'execution' });
  };

  return (
    <>
      <Show when={props.showStateChip && latestState()}>
        {(state) => (
          <button
            type="button"
            aria-label={`Open the Execution tab for ${props.itemTitle}`}
            title="Open the Execution tab"
            onClick={openExecutionTab}
            style={{ background: 'transparent', border: 'none', cursor: 'pointer', padding: 0 }}
          >
            <Badge tone={state().tone}>{state().label}</Badge>
          </button>
        )}
      </Show>
      {props.compact ? (
        <button
          type="button"
          aria-label={`Run with agent: ${props.itemTitle}`}
          title="Run with agent"
          onClick={(e) => {
            e.stopPropagation();
            setOpen(true);
          }}
          style={{
            width: '18px',
            height: '18px',
            'border-radius': '4px',
            border: 'none',
            background: 'transparent',
            cursor: 'pointer',
            color: 'var(--color-text-tertiary)',
            'line-height': '1',
            'font-size': '12px',
            padding: 0,
          }}
          class="focus:outline-none focus-visible:ring-2"
        >
          ▶
        </button>
      ) : (
        <Button
          size="sm"
          variant="primary"
          onClick={(e) => {
            e.stopPropagation();
            setOpen(true);
          }}
        >
          Run with agent
        </Button>
      )}
      <RunWithAgentModal
        isOpen={open()}
        onClose={() => setOpen(false)}
        itemId={props.itemId}
        itemTitle={props.itemTitle}
        projectId={props.projectId}
        onCreated={props.onCreated}
        capabilities={props.capabilities}
      />
    </>
  );
};

export default RunWithAgentButton;

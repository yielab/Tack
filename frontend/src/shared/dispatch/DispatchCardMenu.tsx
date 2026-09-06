import { type Component, Show, createSignal, createEffect, onCleanup } from 'solid-js';
import { dispatchApi } from './api';
import { notifyDispatchOutcome } from './notify';
import { toast } from '../ui/toast';

export interface DispatchCardMenuProps {
  itemId: string;
  itemTitle: string;
  /** Whether dispatch controls should render at all. `false` for the
   *  overwhelmingly common "orchestration not enabled" case — the menu
   *  trigger itself doesn't mount, rather than mounting and
   *  failing when clicked: no dispatch controls,
   *  not a control that errors. Today's only caller (`Board.tsx`) feeds
   *  this from `useAgentActivityMap`'s `orchAvailable()`, which is
   *  the wrong signal — see that function's doc comment for why
   *  and for the real one (`Capabilities.dispatch`) this prop should read
   *  once a per-project capability value is reachable at that call site.
   *  `dispatch` itself carries no `reason` on the wire (it's a plain
   *  boolean, unlike `pause`/`resume`/...), so unlike `shared/orch
   *  /CapabilityNote.tsx` there is nothing to *show* here even once fixed —
   *  only whether the trigger mounts. */
  available: boolean;
  /** Called after a dispatch attempt settles (success or failure) so the
   *  caller can refresh its own agent-activity badge data. */
  onDispatched?: () => void;
}

/**
 * The per-item board card menu — today a single action
 * (Dispatch to agents), built as a real menu rather than a bare button so
 * more item actions can be added here later without a second component.
 * Stops propagation on every interaction so it never triggers the item
 * card's own click-to-open-drawer handler (`Board.tsx`'s `ItemCard`).
 */
const DispatchCardMenu: Component<DispatchCardMenuProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  let ref: HTMLDivElement | undefined;

  createEffect(() => {
    if (!open()) return;
    const onDocClick = (e: MouseEvent) => {
      if (ref && !ref.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('click', onDocClick);
    document.addEventListener('keydown', onKey);
    onCleanup(() => {
      document.removeEventListener('click', onDocClick);
      document.removeEventListener('keydown', onKey);
    });
  });

  const dispatch = async (e: MouseEvent) => {
    e.stopPropagation();
    setOpen(false);
    setBusy(true);
    try {
      const res = await dispatchApi.dispatchItem(props.itemId);
      notifyDispatchOutcome(res, props.itemTitle);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : `Failed to dispatch ${props.itemTitle}`);
    } finally {
      setBusy(false);
      props.onDispatched?.();
    }
  };

  return (
    <Show when={props.available}>
      <div ref={ref} class="relative" onClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          aria-haspopup="menu"
          aria-expanded={open() ? 'true' : 'false'}
          aria-label={`Actions for ${props.itemTitle}`}
          onClick={(e) => {
            e.stopPropagation();
            setOpen((v) => !v);
          }}
          disabled={busy()}
          style={{
            width: '18px',
            height: '18px',
            'border-radius': '4px',
            border: 'none',
            background: 'transparent',
            cursor: 'pointer',
            color: 'var(--color-text-tertiary)',
            'line-height': '1',
            'font-size': '13px',
            padding: 0,
          }}
          class="focus:outline-none focus-visible:ring-2"
        >
          ⋮
        </button>
        <Show when={open()}>
          <div
            role="menu"
            aria-label={`Actions for ${props.itemTitle}`}
            class="absolute right-0 z-10 mt-1 min-w-42 rounded-lg border py-1"
            style={{
              'background-color': 'var(--color-bg-elevated)',
              'border-color': 'var(--color-border-light)',
              'box-shadow': 'var(--shadow-lg)',
            }}
          >
            <button
              type="button"
              role="menuitem"
              onClick={dispatch}
              disabled={busy()}
              class="block w-full px-3 py-1.5 text-left text-xs disabled:opacity-50 focus:outline-none focus-visible:ring-2"
              style={{ color: 'var(--color-text-primary)' }}
            >
              {busy() ? 'Dispatching…' : 'Dispatch to agents'}
            </button>
          </div>
        </Show>
      </div>
    </Show>
  );
};

export default DispatchCardMenu;

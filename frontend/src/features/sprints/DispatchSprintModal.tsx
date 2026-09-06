import { type Component, Show, For, createResource, createSignal, createMemo, createEffect } from 'solid-js';
import { Modal, Button, Field, Badge, EmptyState } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import {
  dispatchApi,
  isOrchDisabled,
  type SprintDispatchItemResponse,
  type SprintDispatchResponse,
} from '../../shared/dispatch/api';
import DispatchOutcomeNote from '../../shared/dispatch/DispatchOutcomeNote';
import { summarizeSprintDispatchCounts, describeDispatchOutcome, sprintItemDetail } from '../../shared/dispatch/format';
import type { Sprint } from '../../shared/types';

export interface DispatchSprintModalProps {
  /** `null` closes the modal (mirrors `Modal`'s own `isOpen`, but keyed off
   *  which sprint rather than a bare boolean, since the dry-run fetch needs
   *  the sprint id). */
  sprint: Sprint | null;
  onClose: () => void;
  /** Called once a real (non-dry-run) dispatch completes — `status_map` may
   *  have moved items, so the host (`Sprints.tsx`) should refresh its item list. */
  onDispatched?: () => void;
}

/**
 * "Run sprint" — the dry-run preview IS
 * the confirmation step: there is no one-click path from the Sprints board
 * straight to a real dispatch. Opening this modal always shows the
 * dependency-ordered plan first; only an explicit "Confirm dispatch" click
 * on THIS screen calls the real endpoint. This is deliberate, not
 * incidental — confirmation for sprint-wide dispatch
 * is required, not optional, for exactly the reason a dry-run preview
 * matters here: dispatching a whole sprint to autonomous agents is exactly
 * the action someone wants to inspect before confirming.
 *
 * `POST /sprints/{id}/dispatch` and `GET /sprints/{id}/dispatch/dry-run`
 * are wired up here against the
 * real contract (`docs/openapi.json` — see
 * `shared/dispatch/api.ts`'s header comment
 * for the field-by-field detail). Two corrections
 * that would otherwise have been silent bugs: `max_in_flight` is a query
 * parameter, not a JSON body field (a body was never read server-side, so
 * the override would have simply been ignored); and every sprint item is
 * always present in the plan with a `decision`, not filtered down to a
 * `position: number | null` — there is no "excluded" bucket to compute,
 * `decision !== "would_dispatch"` already tells you which items won't run.
 */
const DispatchSprintModal: Component<DispatchSprintModalProps> = (props) => {
  const sprintId = () => props.sprint?.id;
  const [dryRun, { refetch: refetchDryRun }] = createResource(sprintId, (id) =>
    dispatchApi.dryRunSprintDispatch(id),
  );

  // A Solid resource accessor THROWS once it has errored (`dryRun()`, not
  // `.loading`/`.error`) — calling it directly from a memo that runs as part
  // of the same reactive batch that just set the error would throw INSIDE
  // that batch, which aborts propagation to sibling computations (including
  // the `<Show>` blocks below that are supposed to swap "Loading…" for the
  // disabled/error state) and leaves an unhandled rejection besides. Every
  // read of the resource's *value* goes through this one safe accessor
  // instead — `undefined` once errored, exactly like `dryRun()` would return
  // if it simply hadn't thrown.
  const dryRunData = () => (dryRun.error !== undefined ? undefined : dryRun());

  const [capOverride, setCapOverride] = createSignal<number | null>(null);
  const effectiveCap = (): number | null => capOverride() ?? dryRunData()?.max_in_flight ?? null;

  const [result, setResult] = createSignal<SprintDispatchResponse | null>(null);
  const [dispatching, setDispatching] = createSignal(false);

  // Reset per-open state whenever a different sprint (or none) becomes the target.
  createEffect(() => {
    sprintId();
    setResult(null);
    setCapOverride(null);
  });

  // "would_dispatch" is the dry-run-only marker for "every gate passed; a
  // real run would call docket for this one" — the actual planned set, in
  // dependency order. Everything else is a real (typed, explained) reason
  // it won't run this pass, not a silent omission.
  const planned = createMemo(() =>
    (dryRunData()?.items ?? [])
      .filter((i) => i.decision === 'would_dispatch')
      .sort((a, b) => a.order - b.order),
  );
  const notPlanned = createMemo(() =>
    (dryRunData()?.items ?? [])
      .filter((i) => i.decision !== 'would_dispatch')
      .sort((a, b) => a.order - b.order),
  );

  const disabled = () => isOrchDisabled(dryRun.error);
  const failed = () => dryRun.error !== undefined && !disabled();

  const confirm = async () => {
    const id = sprintId();
    if (!id) return;
    setDispatching(true);
    try {
      const res = await dispatchApi.dispatchSprint(id, effectiveCap() ?? undefined);
      setResult(res);
      props.onDispatched?.();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to dispatch sprint');
    } finally {
      setDispatching(false);
    }
  };

  return (
    <Modal isOpen={!!props.sprint} onClose={props.onClose} title={`Run sprint: ${props.sprint?.name ?? ''}`} size="lg">
      <div class="space-y-4">
        <Show when={dryRun.loading}>
          <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
            Loading dispatch plan…
          </p>
        </Show>

        <Show when={!dryRun.loading && disabled()}>
          <EmptyState
            icon="🔌"
            title="Agent-fleet orchestration is disabled"
            description="Set TACK_ORCH_ENABLE=true and register a control plane for this project to dispatch a sprint."
          />
        </Show>

        <Show when={!dryRun.loading && failed()}>
          <EmptyState
            icon="⚠️"
            title="Couldn't load the dispatch plan"
            description="The request to the server failed. Check your connection and try again."
            action={<Button onClick={() => refetchDryRun()}>Retry</Button>}
          />
        </Show>

        <Show when={!dryRun.loading && !disabled() && !failed() && dryRunData() && !result()}>
          <div class="space-y-3">
            <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              Dependency-ordered preview — this is the exact order a real dispatch will use. Nothing has been
              dispatched yet.
            </p>

            <Show
              when={planned().length > 0}
              fallback={
                <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
                  No items in this sprint are eligible for dispatch right now.
                </p>
              }
            >
              <ol class="max-h-72 space-y-1.5 overflow-y-auto">
                <For each={planned()}>
                  {(planItem) => (
                    <li
                      class="flex items-center gap-2 rounded-md border px-2.5 py-1.5 text-sm"
                      style={{ 'border-color': 'var(--color-border-light)' }}
                    >
                      <span
                        class="w-5 shrink-0 text-right text-xs"
                        style={{ color: 'var(--color-text-tertiary)', 'font-family': 'var(--font-mono)' }}
                      >
                        {planItem.order + 1}
                      </span>
                      <span class="flex-1 truncate" style={{ color: 'var(--color-text-primary)' }}>
                        {planItem.title}
                      </span>
                    </li>
                  )}
                </For>
              </ol>
            </Show>

            {/* Every excluded item names ITS OWN reason (decision + detail),
                not a generic "not eligible" — e.g. "Waiting on dependencies —
                waiting on 2 direct dependencies to finish" tells the operator
                exactly what to go check: the
                dry-run preview must explain why something isn't
                ready, not just that it isn't. */}
            <Show when={notPlanned().length > 0}>
              <details class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                <summary class="cursor-pointer select-none">
                  {notPlanned().length} item{notPlanned().length === 1 ? '' : 's'} not dispatched this run
                </summary>
                <ul class="mt-1.5 space-y-1.5 pl-3">
                  <For each={notPlanned()}>
                    {(item) => <NotPlannedRow item={item} />}
                  </For>
                </ul>
              </details>
            </Show>

            <div class="border-t pt-3" style={{ 'border-color': 'var(--color-border-light)' }}>
              <Field
                label="Max in-flight dispatches"
                type="number"
                min="1"
                max="20"
                class="w-56"
                value={effectiveCap() ?? ''}
                onInput={(e) => {
                  const v = e.currentTarget.value;
                  setCapOverride(v === '' ? null : Math.max(1, Number(v)));
                }}
                hint="Caps how many of this sprint's agents run at once (1–20; server default shown until changed)."
              />
            </div>
          </div>
        </Show>

        <Show when={result()}>{(res) => <DispatchResultsSummary response={res()} />}</Show>

        <div class="flex justify-end gap-2 border-t pt-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <Show when={!result()} fallback={<Button onClick={props.onClose}>Close</Button>}>
            <Button variant="secondary" onClick={props.onClose} disabled={dispatching()}>
              Cancel
            </Button>
            <Button
              onClick={confirm}
              loading={dispatching()}
              disabled={dispatching() || disabled() || failed() || planned().length === 0}
            >
              Confirm dispatch ({planned().length})
            </Button>
          </Show>
        </div>
      </div>
    </Modal>
  );
};

const NotPlannedRow: Component<{ item: SprintDispatchItemResponse }> = (props) => {
  const desc = () => describeDispatchOutcome(props.item.decision);
  const detail = () => sprintItemDetail(props.item);
  return (
    <li class="flex flex-wrap items-baseline gap-1.5">
      <span style={{ color: 'var(--color-text-secondary)' }}>{props.item.title}</span>
      <span>—</span>
      <span>{desc().label}</span>
      <Show when={detail()}>
        <span>({detail()})</span>
      </Show>
    </li>
  );
};

/** Post-dispatch results — every decision gets its own labeled count, read
 *  straight off the server's own `summary` (never re-derived by counting
 *  `items` client-side — see `summarizeSprintDispatchCounts`'s doc comment),
 *  so a single "N dispatched" can never be shown when some of those are
 *  actually waiting on a human or a dependency — "8 dispatched" when three
 *  are awaiting approval would misrepresent the state of the work — plus
 *  the full per-item breakdown below it. */
const DispatchResultsSummary: Component<{ response: SprintDispatchResponse }> = (props) => {
  const counts = createMemo(() => summarizeSprintDispatchCounts(props.response.summary));

  return (
    <div class="space-y-3">
      <div class="flex flex-wrap gap-2">
        <For each={counts()}>
          {({ decision, count }) => {
            const desc = describeDispatchOutcome(decision);
            return (
              <Badge tone={desc.tone}>
                {count} {desc.label.toLowerCase()}
              </Badge>
            );
          }}
        </For>
      </div>
      <ul class="max-h-72 space-y-1.5 overflow-y-auto">
        <For each={props.response.items}>
          {(item) => (
            <li class="rounded-md border px-2.5 py-1.5" style={{ 'border-color': 'var(--color-border-light)' }}>
              <DispatchOutcomeNote decision={item.decision} detail={sprintItemDetail(item)} title={item.title} />
            </li>
          )}
        </For>
      </ul>
    </div>
  );
};

export default DispatchSprintModal;

import { type Component, For, Show, createEffect, createMemo, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Field } from '../ui';
import { toast } from '../ui/toast';
import { useExecutionStore } from '../state/executionContext';
import AttemptList from './AttemptList';
import { describeExecutionState, isTerminalStateString, relativeTimeFromIso } from './shared';
import type { ExecutionRequestRecord } from '../execution';

export interface ExecutionTimelineProps {
  itemId: string;
}

/**
 * The request/attempt timeline plus cancel/reconcile controls. Reads
 * exclusively through the shared `useExecutionStore()` — no second fetch,
 * no local cache — so this component's view of a request is always the
 * same one the "Run with agent" modal and every other mounted consumer see.
 *
 * Renders every gap in the underlying data HONESTLY rather than papering
 * over it: `attemptsFor()` is an explicit state machine (idle / loading /
 * ready / error — see `store.ts`'s own header comment), so a genuinely
 * empty attempt list is never conflated with "still loading" or "the fetch
 * failed". `GET /executions/{id}/attempts` is wired in, so this component
 * shows real attempt data rather than a typed `not_available` placeholder.
 */
const ExecutionTimeline: Component<ExecutionTimelineProps> = (props) => {
  const store = useExecutionStore();
  const requests = createMemo(() => store.requestsForItem(props.itemId));

  // The app-wide preload (`ExecutionStoreProvider`) is bounded server-side
  // and exists for other consumers' "most recent state" badges — it can
  // legitimately miss this item's own older rows once the install has more
  // executions than that bound. An item-scoped fetch, re-run whenever
  // `props.itemId` changes, guarantees this tab's own history is complete
  // regardless of what else the install has recorded, without the caller
  // having to reason about the bound at all.
  createEffect(() => {
    void store.loadList(props.itemId);
  });

  return (
    <div class="space-y-3">
      <Show when={store.listStatus() === 'loading' && requests().length === 0}>
        <p class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading execution requests…
        </p>
      </Show>

      <Show when={store.listStatus() === 'error'}>
        {(() => {
          const err = store.listError();
          return (
            <p class="text-sm" style={{ color: 'var(--color-danger-600)' }}>
              Couldn't load execution requests{err ? `: ${err.message}` : '.'}
            </p>
          );
        })()}
      </Show>

      <Show
        when={requests().length > 0}
        fallback={
          <Show when={store.listStatus() !== 'loading'}>
            <EmptyState title="No execution requests yet" description="Use “Run with agent” to start one." />
          </Show>
        }
      >
        <ul class="space-y-3">
          <For each={requests()}>{(record) => <RequestRow record={record} />}</For>
        </ul>
      </Show>
    </div>
  );
};

const RequestRow: Component<{ record: ExecutionRequestRecord }> = (props) => {
  const store = useExecutionStore();
  const [reconcileOpen, setReconcileOpen] = createSignal(false);
  const [recoveryKey, setRecoveryKey] = createSignal('');
  const [reason, setReason] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const summary = () => props.record.summary;
  const state = () => summary()?.state ?? 'unknown';
  const stateInfo = createMemo(() => describeExecutionState(state()));

  const canCancel = createMemo(() => {
    const s = summary();
    if (!s) return false;
    return !isTerminalStateString(s.state) && !props.record.cancellation.pending && !props.record.cancellation.requested;
  });

  const canReconcile = createMemo(() => summary()?.state === 'needs_operator');

  const cancel = async () => {
    const s = summary();
    if (!s) return;
    setBusy(true);
    try {
      await store.cancel(s.request_id);
      toast.success('Cancellation requested.');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to request cancellation.');
    } finally {
      setBusy(false);
    }
  };

  // An ambiguous (needs_operator) state requires explicit operator action,
  // never an automatic silent retry: the recovery key and reason are always
  // typed in by hand, never pre-filled or defaulted — there is no
  // "reconcile" button that fires with zero arguments.
  const reconcile = async (e: Event) => {
    e.preventDefault();
    const s = summary();
    if (!s || !recoveryKey().trim() || !reason().trim()) return;
    setBusy(true);
    try {
      await store.requeue(s.request_id, { recovery_key: recoveryKey().trim(), reason: reason().trim() });
      toast.success('Execution requeued.');
      setReconcileOpen(false);
      setRecoveryKey('');
      setReason('');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to requeue.');
    } finally {
      setBusy(false);
    }
  };

  const attempts = createMemo(() => store.attemptsFor(summary()?.request_id ?? ''));

  // Lazy first load: fetch attempts once per request the moment its row is
  // rendered, never repeatedly — a subsequent render with the same `idle`
  // state (there isn't one, since `loadAttempts` always transitions past
  // `idle`) would otherwise re-trigger. Realtime refresh is handled by
  // `store.ts#connectRealtime` for any request this effect has already
  // asked about.
  createEffect(() => {
    const id = summary()?.request_id;
    if (id && attempts().status === 'idle') void store.loadAttempts(id);
  });

  return (
    <li class="space-y-2 rounded-lg border p-3" style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}>
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {summary()?.request_id ?? '—'}
        </span>
        <Badge tone={stateInfo().tone}>{stateInfo().label}</Badge>
        <Show when={!stateInfo().known}>
          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            (unrecognised state — showing raw value)
          </span>
        </Show>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          created {relativeTimeFromIso(summary()?.created_at)}
        </span>
        <Show when={props.record.cancellation.requested || props.record.cancellation.pending}>
          <Badge tone="warning">{props.record.cancellation.pending ? 'Cancellation pending' : 'Cancellation requested'}</Badge>
        </Show>
        <Show when={props.record.cancellation.conflict}>
          <Badge tone="danger">Already terminal — cancel had no effect</Badge>
        </Show>
      </div>

      <Show when={props.record.error}>
        {(err) => (
          <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
            {err().message}
          </p>
        )}
      </Show>

      {/* Attempt timeline — real data, from a wired endpoint.
          Every fetch state is rendered explicitly. */}
      <Show when={attempts().status === 'loading'}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading attempts…
        </p>
      </Show>
      <Show when={attempts().status === 'error'}>
        {(() => {
          const a = attempts();
          return a.status === 'error' ? (
            <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
              Couldn't load attempts: {a.error.message}
            </p>
          ) : null;
        })()}
      </Show>
      <Show when={attempts().status === 'ready'}>
        {(() => {
          const a = attempts();
          if (a.status !== 'ready') return null;
          return a.data.length > 0 ? (
            <AttemptList requestId={summary()?.request_id ?? ''} attempts={a.data} />
          ) : (
            <p class="rounded-md border border-dashed px-2 py-1.5 text-xs" style={{ color: 'var(--color-text-tertiary)', 'border-color': 'var(--color-border-light)' }}>
              No attempts yet.
            </p>
          );
        })()}
      </Show>

      <div class="flex flex-wrap items-center gap-2">
        <Show when={canCancel()}>
          <Button size="sm" variant="secondary" onClick={cancel} disabled={busy()} loading={busy()}>
            Cancel
          </Button>
        </Show>
        <Show when={canReconcile()}>
          <Button size="sm" variant="secondary" onClick={() => setReconcileOpen((v) => !v)} disabled={busy()}>
            Reconcile…
          </Button>
        </Show>
      </div>

      <Show when={reconcileOpen()}>
        <form class="space-y-2 rounded-md border p-2" style={{ 'border-color': 'var(--color-border-light)' }} onSubmit={reconcile}>
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            This request needs an operator's explicit decision before it can requeue — enter the recovery key and a
            reason (audited).
          </p>
          <Field
            label="Recovery key"
            value={recoveryKey()}
            onInput={(e) => setRecoveryKey(e.currentTarget.value)}
            required
          />
          <Field label="Reason" value={reason()} onInput={(e) => setReason(e.currentTarget.value)} required />
          <div class="flex justify-end gap-2">
            <Button type="button" size="sm" variant="secondary" onClick={() => setReconcileOpen(false)} disabled={busy()}>
              Cancel
            </Button>
            <Button type="submit" size="sm" disabled={busy() || !recoveryKey().trim() || !reason().trim()} loading={busy()}>
              Confirm requeue
            </Button>
          </div>
        </form>
      </Show>
    </li>
  );
};

export default ExecutionTimeline;

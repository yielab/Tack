import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, EmptyState, Field } from '../ui';
import { toast } from '../ui/toast';
import {
  decisionTokenStore,
  decisionsApi,
  isDecisionExpired,
  isDecisionIdempotencyConflict,
  isDecisionInvalidOption,
  isDecisionNotFound,
  isDecisionTokenRejected,
  type DecisionAnswer,
  type DecisionRecord,
  type ResolveDecisionResult,
} from '../execution';

export interface DecisionInboxProps {
  requestId: string;
  attemptNumber: number;
  /** Internal attempt id — distinct from `attemptNumber` — the resolve
   *  route (`POST /attempts/{attempt_id}/decisions/{decision_id}/resolve`)
   *  is scoped by, matching every other caller of that route in this
   *  codebase. */
  attemptId: string;
  /** Called after any successful resolve. The list is refetched
   *  immediately after, so a resolved row's badge updates without the
   *  caller needing to do anything. */
  onResolved?: (result: ResolveDecisionResult) => void;
}

/** Shared resolve call + user-facing toast — one place maps every distinct
 *  server outcome to a distinct message. */
async function resolveAndNotify(
  attemptId: string,
  decisionId: string,
  answer: DecisionAnswer,
  onResolved: ((result: ResolveDecisionResult) => void) | undefined,
): Promise<void> {
  try {
    const result = await decisionsApi.resolve(attemptId, decisionId, answer);
    toast.success(
      result.replayed
        ? 'Already resolved — this is the previously recorded answer.'
        : 'Decision resolved.',
    );
    onResolved?.(result);
  } catch (err) {
    if (isDecisionTokenRejected(err)) {
      toast.error(
        'Rejected — either this deployment has not configured decision resolution ' +
          '(TACK_EXECUTION_DECISION_TOKEN), or the token entered above is wrong.',
      );
    } else if (isDecisionExpired(err)) {
      toast.error('This decision already expired — it can no longer be resolved.');
    } else if (isDecisionIdempotencyConflict(err)) {
      toast.error('Already resolved with a different answer than the one just submitted.');
    } else if (isDecisionNotFound(err)) {
      toast.error('No decision with that id exists for this attempt.');
    } else if (isDecisionInvalidOption(err)) {
      toast.error("That answer wasn't one of this decision's declared options.");
    } else {
      toast.error(err instanceof Error ? err.message : 'Failed to resolve the decision.');
    }
    throw err;
  }
}

const DecisionRow: Component<{
  attemptId: string;
  decision: DecisionRecord;
  onResolved?: (result: ResolveDecisionResult) => void;
}> = (props) => {
  const [optionId, setOptionId] = createSignal('');
  const [text, setText] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  const isPending = () => props.decision.state === 'pending';
  const isExpired = () => props.decision.state === 'expired';
  const isResolved = () => props.decision.state === 'resolved';
  const hasOptions = () => props.decision.options.length > 0;

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!optionId().trim() || busy()) return;
    setBusy(true);
    try {
      await resolveAndNotify(
        props.attemptId,
        props.decision.decision_id,
        { option_id: optionId().trim(), text: text().trim() || null },
        props.onResolved,
      );
    } catch {
      /* already toasted by resolveAndNotify */
    } finally {
      setBusy(false);
    }
  };

  return (
    <li
      class="space-y-2 rounded-lg border p-3"
      style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex flex-wrap items-center gap-2">
        {/* Pending / expired / resolved are visually AND semantically
            distinct — three different tones, three different labels, never
            merged into one generic "decision" badge. */}
        <Show when={isPending()}>
          <Badge tone="warning">Pending</Badge>
        </Show>
        <Show when={isExpired()}>
          <Badge tone="danger">Expired</Badge>
        </Show>
        <Show when={isResolved()}>
          <Badge tone="success">Resolved</Badge>
        </Show>
        <Show when={!isPending() && !isExpired() && !isResolved()}>
          <Badge tone="neutral">{props.decision.state} (unrecognised)</Badge>
        </Show>
        <span class="text-xs" style={{ 'font-family': 'var(--font-mono)', color: 'var(--color-text-tertiary)' }}>
          {props.decision.decision_id}
        </span>
      </div>

      <p class="text-sm" style={{ color: 'var(--color-text-primary)' }}>
        {props.decision.prompt}
      </p>

      <Show when={isResolved()}>
        <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Answered: <span style={{ 'font-family': 'var(--font-mono)' }}>{props.decision.answer?.option_id}</span>
          {props.decision.answer?.text ? ` — "${props.decision.answer.text}"` : ''}
        </p>
      </Show>

      {/* Expired: the control is disabled and the reason is visible text
          right next to it, never a silently-greyed-out button (acceptance
          bar: "disabled controls name reason"). */}
      <Show when={isExpired()}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          This decision expired without an answer — it can no longer be resolved.
        </p>
      </Show>

      <Show when={isPending()}>
        <form class="space-y-2" onSubmit={submit}>
          <Show
            when={hasOptions()}
            fallback={
              <Field
                label="Answer (option id)"
                value={optionId()}
                onInput={(e) => setOptionId(e.currentTarget.value)}
                required
                hint="This decision declared no fixed options — enter any non-empty answer id."
              />
            }
          >
            <fieldset class="space-y-1.5">
              <legend class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
                Choose an answer
              </legend>
              <For each={props.decision.options}>
                {(opt) => (
                  <label class="flex items-center gap-1.5 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <input
                      type="radio"
                      name={`decision-${props.decision.decision_id}`}
                      value={opt.option_id}
                      checked={optionId() === opt.option_id}
                      onChange={() => setOptionId(opt.option_id)}
                    />
                    {opt.label}
                  </label>
                )}
              </For>
            </fieldset>
          </Show>
          <Field
            label="Details (optional)"
            value={text()}
            onInput={(e) => setText(e.currentTarget.value)}
          />
          <div class="flex items-center gap-2">
            <Button type="submit" size="sm" disabled={busy() || !optionId().trim()} loading={busy()}>
              Resolve
            </Button>
            <Show when={!optionId().trim()}>
              <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                Select or enter an answer to enable Resolve.
              </span>
            </Show>
          </div>
        </form>
      </Show>
    </li>
  );
};

/**
 * The decision inbox for one attempt. Fetches `GET
 * /executions/{request_id}/attempts/{attempt_number}/decisions` and renders
 * every row with pending/expired/resolved kept visually and semantically
 * distinct. A successful resolve refetches the list so the resolved row's
 * badge updates immediately, without the caller managing any state of its
 * own.
 *
 * The decision token field mirrors `features/approvals/ApprovalsPage.tsx`'s
 * own `TACK_ORCH_APPROVAL_TOKEN` entry exactly, including its reasoning:
 * always render the control and let a real resolve attempt's actual 403
 * answer "is this configured at all", rather than guessing client-side (see
 * `features/approvals/api.ts`'s `PendingApprovalListResponse` doc comment
 * for the fuller argument this mirrors).
 */
const DecisionInbox: Component<DecisionInboxProps> = (props) => {
  const [tokenInput, setTokenInput] = createSignal(decisionTokenStore.get() ?? '');
  const [decisions, { refetch }] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => decisionsApi.list(props.requestId, props.attemptNumber),
  );

  const saveToken = () => {
    decisionTokenStore.set(tokenInput().trim() || null);
    toast.success('Decision token saved for this browser session.');
  };

  const handleResolved = (result: ResolveDecisionResult) => {
    props.onResolved?.(result);
    void refetch();
  };

  return (
    <div class="space-y-3">
      <div
        class="flex flex-wrap items-end gap-2 rounded-lg p-2.5"
        style={{ border: '1px solid var(--color-border-light)', 'background-color': 'var(--color-bg-subtle)' }}
      >
        <Field
          label="Your decision token"
          type="password"
          autocomplete="off"
          placeholder="TACK_EXECUTION_DECISION_TOKEN"
          value={tokenInput()}
          onInput={(e) => setTokenInput(e.currentTarget.value)}
          hint="Stored only in this browser. Required to resolve — never sent when just viewing."
          class="min-w-56 flex-1"
        />
        <Button size="sm" variant="secondary" onClick={saveToken}>
          Save
        </Button>
      </div>

      <Show when={decisions.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading decisions…
        </p>
      </Show>
      <Show when={decisions.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load decisions: {decisions.error instanceof Error ? decisions.error.message : 'unknown error'}
        </p>
      </Show>
      <Show when={!decisions.loading && !decisions.error && (decisions() ?? []).length === 0}>
        <EmptyState title="No decisions raised yet" />
      </Show>
      <Show when={!decisions.loading && !decisions.error && (decisions() ?? []).length > 0}>
        <ul class="space-y-2">
          <For each={decisions()}>
            {(decision) => (
              <DecisionRow attemptId={props.attemptId} decision={decision} onResolved={handleResolved} />
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

export default DecisionInbox;

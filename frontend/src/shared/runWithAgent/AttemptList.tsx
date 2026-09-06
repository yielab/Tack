import { type Component, For, Show, createSignal } from 'solid-js';
import { Badge, Button } from '../ui';
import type { AttemptSummary } from '../execution';
import { describeModelProvenance, formatUsageEconomics } from './attemptFormat';
import ArtifactDownloadPanel from './ArtifactDownloadPanel';
import DecisionInbox from './DecisionInbox';
import EventTimeline from './EventTimeline';
import { describeExecutionState, relativeTimeFromIso } from './shared';

export interface AttemptListProps {
  requestId: string;
  attempts: AttemptSummary[];
}

const AttemptRow: Component<{ requestId: string; attempt: AttemptSummary }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const stateInfo = () => describeExecutionState(props.attempt.state);
  const provenance = () => describeModelProvenance(props.attempt.model_provenance);
  const economics = () => formatUsageEconomics(props.attempt.usage_economics);

  return (
    <li
      class="space-y-2 rounded-lg border p-3"
      style={{ 'background-color': 'var(--color-bg-subtle)', 'border-color': 'var(--color-border-light)' }}
    >
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Attempt #{props.attempt.attempt_number}
        </span>
        <Badge tone={stateInfo().tone}>{stateInfo().label}</Badge>
        <Show when={!stateInfo().known}>
          <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            (unrecognised state)
          </span>
        </Show>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          runner {props.attempt.runner_id}
        </span>
        <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          leased {relativeTimeFromIso(props.attempt.lease_issued_at)}
        </span>
      </div>

      {/* Model provenance — a distinct, honest tone per case, never a bare
          "matched" boolean. */}
      <div class="flex items-start gap-1.5 text-xs">
        <Badge tone={provenance().tone}>{provenance().label}</Badge>
        <span style={{ color: 'var(--color-text-secondary)' }}>{provenance().detail}</span>
      </div>

      {/* Usage/economics — every dollar figure honestly labeled, "Not
          measured" rendered as literal text, never $0.00. */}
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
        <dt style={{ color: 'var(--color-text-tertiary)' }}>Model/token cost</dt>
        <dd>{economics().modelTokenCostUsd}</dd>
        <dt style={{ color: 'var(--color-text-tertiary)' }}>Runner time</dt>
        <dd>{economics().runnerTime.wallClock}</dd>
        <dt style={{ color: 'var(--color-text-tertiary)' }}>Runner time cost</dt>
        <dd>{economics().runnerTime.costUsd}</dd>
      </dl>

      <Button size="sm" variant="ghost" onClick={() => setExpanded((v) => !v)} aria-expanded={expanded()}>
        {expanded() ? 'Hide details' : 'Show events, decisions & artifacts'}
      </Button>

      <Show when={expanded()}>
        <div class="space-y-3 border-t pt-3" style={{ 'border-color': 'var(--color-border-light)' }}>
          <section>
            <h4 class="mb-1 text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Timeline
            </h4>
            <EventTimeline requestId={props.requestId} attemptNumber={props.attempt.attempt_number} />
          </section>

          <section>
            <h4 class="mb-1 text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Decisions
            </h4>
            <DecisionInbox
              requestId={props.requestId}
              attemptNumber={props.attempt.attempt_number}
              attemptId={props.attempt.attempt_id}
            />
          </section>

          <section>
            <h4 class="mb-1 text-xs font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Artifacts
            </h4>
            <ArtifactDownloadPanel requestId={props.requestId} attemptNumber={props.attempt.attempt_number} />
          </section>
        </div>
      </Show>
    </li>
  );
};

/**
 * Every attempt made against one execution request, reading real data from
 * `store.ts#attemptsFor`/`loadAttempts`. Each row exposes model provenance,
 * usage economics, and (on demand, to avoid an eager fetch for every attempt
 * of every visible request) its normalized event timeline, decision inbox,
 * and artifact download action.
 */
const AttemptList: Component<AttemptListProps> = (props) => (
  <ul class="space-y-2">
    <For each={props.attempts}>{(attempt) => <AttemptRow requestId={props.requestId} attempt={attempt} />}</For>
  </ul>
);

export default AttemptList;

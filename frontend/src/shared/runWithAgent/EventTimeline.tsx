import { type Component, For, Show, createResource } from 'solid-js';
import { Badge, EmptyState } from '../ui';
import { attemptsApi, type EventSummary } from '../execution';
import { relativeTimeFromIso } from './shared';

export interface EventTimelineProps {
  requestId: string;
  attemptNumber: number;
}

/** Best-effort, human-readable rendering of an event's free-form `payload`
 *  (III.1.6: event `kind`/`payload` are harness/runner-defined, not a fixed
 *  schema — `docs/contracts/runner-v1/event-batch.request.json`'s own
 *  frozen example is just `{"stream": "summary", "text": "..."}`-shaped).
 *  Prefers a `text`/`message` string field when present (the common case in
 *  the frozen fixture), else falls back to compact JSON — never throws on
 *  an unexpected shape. */
function describeEventPayload(payload: unknown): string {
  if (payload === null || payload === undefined) return '';
  if (typeof payload === 'string') return payload;
  if (typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;
    if (typeof obj.text === 'string') return obj.text;
    if (typeof obj.message === 'string') return obj.message;
    try {
      return JSON.stringify(payload);
    } catch {
      return '(unrenderable payload)';
    }
  }
  return String(payload);
}

const EventRow: Component<{ event: EventSummary }> = (props) => (
  <li class="flex flex-col gap-0.5 border-l-2 py-1 pl-3" style={{ 'border-color': 'var(--color-border-medium)' }}>
    <div class="flex flex-wrap items-center gap-1.5 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
      <Badge tone="neutral">{props.event.source}</Badge>
      <span style={{ 'font-family': 'var(--font-mono)' }}>{props.event.kind}</span>
      <span>{relativeTimeFromIso(props.event.occurred_at)}</span>
      <span>#{props.event.sequence}</span>
    </div>
    <p class="text-sm break-words" style={{ color: 'var(--color-text-primary)' }}>
      {describeEventPayload(props.event.payload)}
    </p>
  </li>
);

/**
 * The normalized event timeline for one attempt, reading
 * `GET /executions/{id}/attempts/{n}/events`. Oldest first, matching the
 * handler's own documented ordering; renders every fetch outcome
 * (loading/empty/error) explicitly rather than papering over a gap the way
 * `ExecutionTimeline.tsx` once had to for the whole attempts surface.
 */
const EventTimeline: Component<EventTimelineProps> = (props) => {
  const [events] = createResource(
    () => `${props.requestId}:${props.attemptNumber}`,
    () => attemptsApi.events(props.requestId, props.attemptNumber).then((r) => r.data.data),
  );

  return (
    <div>
      <Show when={events.loading}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Loading events…
        </p>
      </Show>
      <Show when={events.error}>
        <p class="text-xs" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load events: {events.error instanceof Error ? events.error.message : 'unknown error'}
        </p>
      </Show>
      <Show when={!events.loading && !events.error && (events() ?? []).length === 0}>
        <EmptyState title="No events reported yet" />
      </Show>
      <Show when={!events.loading && !events.error && (events() ?? []).length > 0}>
        <ul class="space-y-1">
          <For each={events()}>{(event) => <EventRow event={event} />}</For>
        </ul>
      </Show>
    </div>
  );
};

export default EventTimeline;

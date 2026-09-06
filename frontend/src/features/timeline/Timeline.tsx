import {
  createSignal,
  createMemo,
  For,
  Show,
  createResource,
} from 'solid-js';
import { useParams, useNavigate, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { priorityColor } from '../../shared/ui/PriorityDot';
import type { Item, Priority } from '../../shared/types';

// ── Drag state ─────────────────────────────────────────────────────────────

type DragMode = 'move' | 'resize-start' | 'resize-end';

interface DragState {
  itemId: string;
  mode: DragMode;
  startClientX: number;
  origStart: Date;
  origEnd: Date;
}

interface DragPreview {
  newStart: Date;
  newEnd: Date;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function formatIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}T12:00:00.000Z`;
}

function addDays(d: Date, n: number): Date {
  return new Date(d.getTime() + n * 86_400_000);
}

// ── Component ──────────────────────────────────────────────────────────────

export default function Timeline() {
  const params = useParams();
  const projectId = params.id!;
  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();
  const { project } = useProject();

  const { items, refetch } = useProjectItems();

  const [blockedIds] = createResource(items, async (its) => {
    const blocked = new Set<string>();
    await Promise.all(
      (its ?? []).map(async (it) => {
        const deps = await api.dependencies.list(it.id).catch(() => []);
        for (const d of deps) {
          const blockedHere =
            (d.target_item_id === it.id && d.dependency_type === 'blocks') ||
            (d.source_item_id === it.id && d.dependency_type === 'is_blocked_by');
          if (blockedHere) blocked.add(it.id);
        }
      }),
    );
    return blocked;
  });
  const isBlocked = (id: string) => blockedIds()?.has(id) ?? false;

  const [currentDate, setCurrentDate] = createSignal(new Date());
  const [viewMode, setViewMode] = createSignal<'week' | 'month' | 'quarter'>('month');

  // Drag state
  const [dragState, setDragState] = createSignal<DragState | null>(null);
  const [previews, setPreviews] = createSignal<Map<string, DragPreview>>(new Map());

  // Container ref for pixel→day conversion
  let containerRef: HTMLDivElement | undefined;

  // ── Timeline range ─────────────────────────────────────────────────────

  const timelineRange = createMemo(() => {
    const mode = viewMode();
    const today = currentDate();
    let startDate: Date;
    let endDate: Date;

    if (mode === 'week') {
      startDate = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 7);
      endDate   = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 21);
    } else if (mode === 'month') {
      startDate = new Date(today.getFullYear(), today.getMonth() - 1, 1);
      endDate   = new Date(today.getFullYear(), today.getMonth() + 2, 0);
    } else {
      startDate = new Date(today.getFullYear(), today.getMonth() - 2, 1);
      endDate   = new Date(today.getFullYear(), today.getMonth() + 4, 0);
    }

    const dayCount = Math.ceil((endDate.getTime() - startDate.getTime()) / 86_400_000);
    return { startDate, endDate, dayCount };
  });

  // Convert pixel delta → whole-day delta
  const pixelsToDays = (deltaX: number): number => {
    if (!containerRef) return 0;
    const { dayCount } = timelineRange();
    const w = containerRef.getBoundingClientRect().width || 1;
    return Math.round(deltaX / (w / dayCount));
  };

  // ── Item positioning ───────────────────────────────────────────────────

  const positionItem = (item: Item, preview?: DragPreview) => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();

    const start = preview?.newStart ?? (item.started_at
      ? new Date(item.started_at)
      : new Date(item.created_at));

    const end = preview?.newEnd ?? (item.due_date
      ? new Date(item.due_date)
      : addDays(start, 7));

    const leftPercent  = Math.max(0, ((start.getTime() - startDate.getTime()) / totalMs) * 100);
    const widthPercent = Math.min(100 - leftPercent, ((end.getTime() - start.getTime()) / totalMs) * 100);
    const isVisible    = end >= startDate && start <= endDate;

    return { start, end, leftPercent, widthPercent: Math.max(0.5, widthPercent), isVisible };
  };

  const timelineItems = createMemo(() => {
    const allItems = items() || [];

    return allItems
      .map(item => {
        const pos = positionItem(item);
        return { ...item, ...pos };
      })
      .filter(item => item.isVisible)
      .sort((a, b) => a.start.getTime() - b.start.getTime());
  });

  // ── Drag handlers ──────────────────────────────────────────────────────

  const handleBarPointerDown = (
    e: PointerEvent,
    item: Item,
    mode: DragMode,
  ) => {
    e.preventDefault();
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);

    const start = item.started_at ? new Date(item.started_at) : new Date(item.created_at);
    const end   = item.due_date   ? new Date(item.due_date)   : addDays(start, 7);

    setDragState({ itemId: item.id, mode, startClientX: e.clientX, origStart: start, origEnd: end });
  };

  const handlePointerMove = (e: PointerEvent) => {
    const ds = dragState();
    if (!ds) return;

    const deltaDays = pixelsToDays(e.clientX - ds.startClientX);

    let newStart = ds.origStart;
    let newEnd   = ds.origEnd;

    if (ds.mode === 'move') {
      newStart = addDays(ds.origStart, deltaDays);
      newEnd   = addDays(ds.origEnd,   deltaDays);
    } else if (ds.mode === 'resize-start') {
      newStart = addDays(ds.origStart, deltaDays);
      if (newStart >= newEnd) newStart = addDays(newEnd, -1);
    } else {
      newEnd = addDays(ds.origEnd, deltaDays);
      if (newEnd <= newStart) newEnd = addDays(newStart, 1);
    }

    setPreviews(new Map([[ds.itemId, { newStart, newEnd }]]));
  };

  const handlePointerUp = async () => {
    const ds = dragState();
    if (!ds) return;

    const preview = previews().get(ds.itemId);
    setDragState(null);
    setPreviews(new Map());

    if (!preview) return;
    if (
      preview.newStart.getTime() === ds.origStart.getTime() &&
      preview.newEnd.getTime()   === ds.origEnd.getTime()
    ) return;

    try {
      const patch: Record<string, string | null> = {};
      if (ds.mode !== 'resize-end')   patch.started_at = formatIso(preview.newStart);
      if (ds.mode !== 'resize-start') patch.due_date   = formatIso(preview.newEnd);

      await api.items.update(ds.itemId, patch as any);
      toast.success('Dates updated');
      void refetch();
    } catch {
      toast.error('Failed to update dates');
    }
  };

  // ── Navigation ─────────────────────────────────────────────────────────

  const previousPeriod = () => {
    const mode = viewMode(), c = currentDate();
    if (mode === 'week')    setCurrentDate(new Date(c.getFullYear(), c.getMonth(), c.getDate() - 28));
    else if (mode === 'month')   setCurrentDate(new Date(c.getFullYear(), c.getMonth() - 3, 1));
    else                         setCurrentDate(new Date(c.getFullYear(), c.getMonth() - 6, 1));
  };

  const nextPeriod = () => {
    const mode = viewMode(), c = currentDate();
    if (mode === 'week')    setCurrentDate(new Date(c.getFullYear(), c.getMonth(), c.getDate() + 28));
    else if (mode === 'month')   setCurrentDate(new Date(c.getFullYear(), c.getMonth() + 3, 1));
    else                         setCurrentDate(new Date(c.getFullYear(), c.getMonth() + 6, 1));
  };

  const goToday = () => setCurrentDate(new Date());

  // ── Month markers & today line ─────────────────────────────────────────

  const monthMarkers = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();
    const markers: { label: string; leftPercent: number }[] = [];
    let cur = new Date(startDate.getFullYear(), startDate.getMonth(), 1);
    while (cur <= endDate) {
      markers.push({
        label: cur.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }),
        leftPercent: Math.max(0, ((cur.getTime() - startDate.getTime()) / totalMs) * 100),
      });
      cur = new Date(cur.getFullYear(), cur.getMonth() + 1, 1);
    }
    return markers;
  });

  const todayPosition = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();
    return Math.max(0, Math.min(100, ((Date.now() - startDate.getTime()) / totalMs) * 100));
  });

  // ── Style helpers ──────────────────────────────────────────────────────

  const getPriorityColor = (p: string) => priorityColor(p as Priority);

  // Done items are marked with a checkmark + strikethrough, not dimmed: the
  // bar carries `--color-text-inverse` text over a solid priority-color
  // fill, and CSS `opacity` fades an element's whole rendered subtree
  // (fill *and* text) toward whatever's behind it — collapsing the
  // fill/text contrast gap well under WCAG AA at any status opacity below
  // ~1 (verified down to ~2:1 at the old 0.45 "done" value, across all
  // priorities and all six palette×mode combos).
  const isDone = (status: string) =>
    project()?.workflow?.statuses?.find(s => s.name === status)?.category === 'done';

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div
      class="min-h-screen p-6"
      style={{ 'background-color': 'var(--color-bg-base)' }}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      <div class="max-w-full mx-auto">
        <div class="mb-6">
          <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>Timeline</h1>
          <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Drag bars to shift dates. Drag the left/right edge to resize start or due date.
          </p>
        </div>
        <Show when={!(items as any).loading && (items() ?? []).length === 0}>
          <EmptyState
            icon="📊"
            title="No items to display on the timeline"
            description="Add items in Board or List — they'll appear here once created."
            action={<Button onClick={() => navigate(`/projects/${projectId}/board`)}>Go to Board</Button>}
          />
        </Show>

        <Show when={(items() ?? []).length > 0}>
          {/* Controls */}
          <div
            class="rounded-lg p-4 mb-6"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-3">
                <Button variant="secondary" onClick={previousPeriod}>← Previous</Button>
                <Button size="sm" onClick={goToday}>Today</Button>
                <Button variant="secondary" onClick={nextPeriod}>Next →</Button>
              </div>
              <div class="flex items-center gap-1">
                <For each={(['week', 'month', 'quarter'] as const)}>
                  {(mode) => (
                    <button
                      onClick={() => setViewMode(mode)}
                      class="px-3 py-1.5 text-sm rounded-lg capitalize transition-colors"
                      style={viewMode() === mode
                        ? { 'background-color': 'var(--color-primary-600)', color: 'var(--color-on-accent)' }
                        : { 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                    >
                      {mode}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>

          {/* Gantt chart */}
          <div
            class="rounded-lg overflow-hidden"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            {/* Month header */}
            <div
              class="relative h-10 overflow-hidden"
              style={{
                'border-bottom': '1px solid var(--color-border-light)',
                'background-color': 'var(--color-bg-base)',
              }}
            >
              <For each={monthMarkers()}>
                {(m) => (
                  <div
                    class="absolute top-0 bottom-0"
                    style={{ left: `${m.leftPercent}%`, 'border-left': '1px solid var(--color-border-medium)' }}
                  >
                    <span
                      class="absolute top-2 left-2 text-xs font-semibold whitespace-nowrap"
                      style={{ color: 'var(--color-text-secondary)' }}
                    >
                      {m.label}
                    </span>
                  </div>
                )}
              </For>
              {/* Today marker */}
              <div
                class="absolute top-0 bottom-0 w-0.5 z-10"
                style={{ left: `${todayPosition()}%`, 'background-color': 'var(--color-primary-500)' }}
              />
            </div>

            {/* Bars */}
            <div
              ref={containerRef}
              class="relative p-4 space-y-2 min-h-64 select-none"
              style={{ cursor: dragState() ? (dragState()!.mode === 'move' ? 'grabbing' : 'ew-resize') : 'default' }}
            >
              <Show when={timelineItems().length === 0}>
                <div class="flex items-center justify-center h-48 text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
                  No items visible in this date range — navigate forward or backward.
                </div>
              </Show>

              <For each={timelineItems()}>
                {(item) => {
                  const preview = () => previews().get(item.id);
                  const pos = () => positionItem(item, preview());
                  const dragging = () => dragState()?.itemId === item.id;

                  return (
                    <div class="relative h-10 group">
                      {/* Row label: fixed left column via absolute positioning at negative offset
                          is tricky in a pure-% layout. Use a small title tooltip instead. */}
                      <div
                        class="absolute top-1 bottom-1 rounded-lg transition-shadow"
                        classList={{ 'shadow-lg ring-2 ring-white/50': dragging() }}
                        style={{
                          left:    `${pos().leftPercent}%`,
                          width:   `${pos().widthPercent}%`,
                          'background-color': getPriorityColor(item.priority),
                          cursor:  dragState()
                            ? (dragState()!.mode === 'move' ? 'grabbing' : 'ew-resize')
                            : 'grab',
                          outline: isBlocked(item.id) ? '2px solid var(--color-danger-600)' : 'none',
                          'outline-offset': '1px',
                        }}
                        onPointerDown={(e) => handleBarPointerDown(e, item, 'move')}
                        onClick={() => !dragState() && setSearchParams({ item: item.id })}
                        title={`${item.title}\n${pos().start.toLocaleDateString()} → ${pos().end.toLocaleDateString()}\n${item.status} · ${item.priority}${isBlocked(item.id) ? '\n⛔ Blocked' : ''}\n\nDrag to move · Drag edges to resize`}
                      >
                        {/* Left resize handle */}
                        <div
                          class="absolute left-0 top-0 bottom-0 w-2 rounded-l-lg cursor-ew-resize hover:bg-black/20 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                          onPointerDown={(e) => { e.stopPropagation(); handleBarPointerDown(e, item, 'resize-start'); }}
                        >
                          <div class="w-0.5 h-3 bg-white/70 rounded-full" />
                        </div>

                        {/* Label. Always full-opacity `--color-text-inverse` on the solid
                            priority fill — never dimmed (see isDone() above for why). Done
                            items get a checkmark + strikethrough instead of a fade. */}
                        <div
                          class="px-3 py-1 h-full flex items-center gap-1 pointer-events-none overflow-hidden"
                          style={{ color: 'var(--color-text-inverse)' }}
                        >
                          <Show when={isBlocked(item.id)}>
                            <span class="text-xs shrink-0" aria-label="blocked">⛔</span>
                          </Show>
                          <Show when={isDone(item.status)}>
                            <span class="text-xs shrink-0" aria-label="done">✓</span>
                          </Show>
                          <span
                            class="text-xs font-medium truncate"
                            style={{ 'text-decoration': isDone(item.status) ? 'line-through' : 'none' }}
                          >
                            {item.title}
                          </span>
                        </div>

                        {/* Right resize handle */}
                        <div
                          class="absolute right-0 top-0 bottom-0 w-2 rounded-r-lg cursor-ew-resize hover:bg-black/20 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                          onPointerDown={(e) => { e.stopPropagation(); handleBarPointerDown(e, item, 'resize-end'); }}
                        >
                          <div class="w-0.5 h-3 bg-white/70 rounded-full" />
                        </div>
                      </div>

                      {/* Date tooltip during drag */}
                      <Show when={dragging() && preview()}>
                        <div
                          class="absolute -top-6 text-xs px-2 py-0.5 rounded whitespace-nowrap z-20 pointer-events-none"
                          style={{
                            left: `${pos().leftPercent}%`,
                            'background-color': 'var(--color-bg-base)',
                            border: '1px solid var(--color-border-medium)',
                            color: 'var(--color-text-primary)',
                          }}
                        >
                          {preview()!.newStart.toLocaleDateString()} → {preview()!.newEnd.toLocaleDateString()}
                        </div>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Legend */}
          <div
            class="mt-4 rounded-lg p-4 flex flex-wrap gap-6 items-center"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            <For each={[
              { label: 'Critical', priority: 'critical' },
              { label: 'High',     priority: 'high' },
              { label: 'Medium',   priority: 'medium' },
              { label: 'Low',      priority: 'low' },
            ] as const}>
              {(p) => (
                <div class="flex items-center gap-2">
                  <div class="w-4 h-3 rounded" style={{ 'background-color': priorityColor(p.priority) }} />
                  <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{p.label}</span>
                </div>
              )}
            </For>
            <span class="text-xs ml-4" style={{ color: 'var(--color-text-tertiary)' }}>
              Done items show a ✓ and strikethrough. Blocked items have a red outline.
            </span>
          </div>
        </Show>
      </div>
    </div>
  );
}

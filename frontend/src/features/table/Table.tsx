import { createSignal, createMemo, For, Show, onMount, onCleanup, createEffect } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { createBoardSocket } from '../../shared/realtime/boardSocket';
import { AgentStateChip } from '../../shared/ui';
import { useAgentActivityMap, type AgentBadgeInfo } from '../../shared/agentActivity/useAgentActivityMap';
import type { Item, Priority, UpdateItem } from '../../shared/types';

// ── Pure helpers (exported for unit testing) ──────────────────────────────────

export type SortKey = 'title' | 'item_type' | 'status' | 'priority' | 'assignee' | 'due_date';
export type SortDir = 'asc' | 'desc';

/** Rank priorities so sorting goes Critical → … → None rather than alphabetic. */
const PRIORITY_RANK: Record<string, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
  none: 4,
};

/** Normalize an item type (string or `{ custom }`) to a plain key string. */
export function typeKey(t: Item['item_type']): string {
  return typeof t === 'string' ? t : t.custom;
}

function cellValue(item: Item, key: SortKey): string | number {
  if (key === 'priority') return PRIORITY_RANK[item.priority] ?? 99;
  if (key === 'due_date') return item.due_date ?? '￿'; // undated sorts last
  if (key === 'item_type') return typeKey(item.item_type).toLowerCase();
  return (item[key] ?? '').toString().toLowerCase();
}

/** Stable sort by a column; returns a new array (does not mutate input). */
export function sortItems(items: Item[], key: SortKey | null, dir: SortDir): Item[] {
  if (!key) return items.slice();
  const sorted = items.slice().sort((a, b) => {
    const av = cellValue(a, key);
    const bv = cellValue(b, key);
    if (av < bv) return -1;
    if (av > bv) return 1;
    // Tie-break on created_at for a stable, predictable order.
    return a.created_at.localeCompare(b.created_at);
  });
  return dir === 'desc' ? sorted.reverse() : sorted;
}

/** Case-insensitive filter across title, assignee, and status. */
export function filterItems(items: Item[], query: string): Item[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((it) =>
    [it.title, it.assignee ?? '', it.status].some((f) => f.toLowerCase().includes(q)),
  );
}

// ── Column config ─────────────────────────────────────────────────────────────

interface ColDef {
  key: SortKey;
  /** Vocabulary key for the header label, or a literal when undefined. */
  vocabKey?: string;
  label: string;
  editable: boolean;
}

const COLUMNS: ColDef[] = [
  { key: 'title', label: 'Title', editable: true },
  { key: 'item_type', label: 'Type', editable: false },
  { key: 'status', label: 'Status', editable: true },
  { key: 'priority', label: 'Priority', editable: true },
  { key: 'assignee', vocabKey: 'assignee', label: 'Assignee', editable: true },
  { key: 'due_date', label: 'Due', editable: true },
];

const PRIORITIES: Priority[] = ['critical', 'high', 'medium', 'low', 'none'];
const PRIORITY_EMOJI: Record<string, string> = {
  critical: '🔥', high: '⬆️', medium: '➡️', low: '⬇️', none: '—',
};

const COLS_STORAGE_KEY = 'tack_table_cols';
const DENSITY_STORAGE_KEY = 'tack_table_density';

export type Density = 'comfortable' | 'compact';

function loadHiddenCols(): Set<string> {
  try {
    const raw = localStorage.getItem(COLS_STORAGE_KEY);
    if (raw) return new Set(JSON.parse(raw) as string[]);
  } catch { /* ignore */ }
  return new Set();
}

function loadDensity(): Density {
  try {
    const v = localStorage.getItem(DENSITY_STORAGE_KEY);
    if (v === 'compact' || v === 'comfortable') return v;
  } catch { /* ignore */ }
  return 'comfortable';
}

// ── Component ─────────────────────────────────────────────────────────────────

export default function Table() {
  const params = useParams();
  const { items, loading, refetch } = useProjectItems();
  const { workflow } = useProject();
  const { t, typeMap } = useVocab();
  const agentActivity = useAgentActivityMap(() => params.id);

  const [sortKey, setSortKey] = createSignal<SortKey | null>(null);
  const [sortDir, setSortDir] = createSignal<SortDir>('asc');
  const [query, setQuery] = createSignal('');
  const [hidden, setHidden] = createSignal<Set<string>>(loadHiddenCols());
  const [density, setDensity] = createSignal<Density>(loadDensity());
  const [colMenuOpen, setColMenuOpen] = createSignal(false);

  // Row padding driven by the density toggle (comfortable default / compact).
  const cellPad = () => (density() === 'compact' ? 'px-3 py-0.5' : 'px-3 py-1.5');

  function toggleDensity() {
    setDensity((d) => {
      const next: Density = d === 'compact' ? 'comfortable' : 'compact';
      try { localStorage.setItem(DENSITY_STORAGE_KEY, next); } catch { /* ignore */ }
      return next;
    });
  }
  const [editing, setEditing] = createSignal<{ id: string; key: SortKey } | null>(null);
  const [saving, setSaving] = createSignal<string | null>(null);

  // Live updates from other surfaces (board drag, websocket) refetch the shared store.
  onMount(() => {
    const onUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onUpdated));
  });

  // Table has no live board
  // socket otherwise (unlike Board.tsx) — this connection exists solely to
  // keep the agent-activity badge chip current when a mirrored run/approval
  // changes, without inventing a broader live-refresh story that isn't
  // needed here.
  createEffect(() => {
    const pid = params.id;
    if (!pid) return;
    const s = createBoardSocket(pid);
    const off = s.onEvent((event) => {
      if (event.type === 'agent_run_updated' || event.type === 'approval_pending') {
        agentActivity.refetch();
      }
    });
    onCleanup(() => { off(); s.close(); });
  });

  const visibleColumns = createMemo(() => COLUMNS.filter((c) => !hidden().has(c.key)));

  const rows = createMemo(() =>
    sortItems(filterItems(items() ?? [], query()), sortKey(), sortDir()),
  );

  const statuses = createMemo(() => workflow()?.statuses?.map((s) => s.name) ?? []);

  const headerLabel = (c: ColDef) => (c.vocabKey ? t(c.vocabKey) : c.label);

  function toggleSort(key: SortKey) {
    if (sortKey() === key) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(key);
      setSortDir('asc');
    }
  }

  function toggleColumn(key: string) {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      try { localStorage.setItem(COLS_STORAGE_KEY, JSON.stringify([...next])); } catch { /* ignore */ }
      return next;
    });
  }

  async function commit(item: Item, key: SortKey, raw: string) {
    setEditing(null);
    const patch: UpdateItem = {};
    if (key === 'title') {
      const v = raw.trim();
      if (!v || v === item.title) return;
      patch.title = v;
    } else if (key === 'status') {
      if (!raw || raw === item.status) return;
      patch.status = raw;
    } else if (key === 'priority') {
      if (raw === item.priority) return;
      patch.priority = raw as Priority;
    } else if (key === 'assignee') {
      const v = raw.trim();
      if (v === (item.assignee ?? '')) return;
      patch.assignee = v === '' ? null : v;
    } else if (key === 'due_date') {
      const current = item.due_date ? item.due_date.slice(0, 10) : '';
      if (raw === current) return;
      patch.due_date = raw === '' ? null : `${raw}T00:00:00Z`;
    } else {
      return;
    }

    setSaving(item.id);
    try {
      await api.items.update(item.id, patch);
      await refetch();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Failed to update item');
    } finally {
      setSaving(null);
    }
  }

  const isEditing = (id: string, key: SortKey) => {
    const e = editing();
    return !!e && e.id === id && e.key === key;
  };

  return (
    <div class="flex flex-col gap-3">
      {/* Toolbar: filter + column picker */}
      <div class="flex items-center gap-2">
        <input
          type="search"
          placeholder="Filter…"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          aria-label="Filter items"
          class="px-3 py-1.5 rounded-md text-sm w-56"
          style={{
            background: 'var(--color-bg-subtle)',
            border: '1px solid var(--color-border-light)',
            color: 'var(--color-text-primary)',
          }}
        />
        <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          {rows().length} item{rows().length === 1 ? '' : 's'}
        </span>
        <button
          type="button"
          onClick={toggleDensity}
          aria-pressed={density() === 'compact'}
          title="Toggle row density"
          class="ml-auto px-3 py-1.5 rounded-md text-sm font-medium"
          style={{
            background: 'var(--color-bg-subtle)',
            border: '1px solid var(--color-border-light)',
            color: 'var(--color-text-secondary)',
          }}
        >
          {density() === 'compact' ? 'Comfortable' : 'Compact'}
        </button>
        <div class="relative">
          <button
            type="button"
            onClick={() => setColMenuOpen((o) => !o)}
            class="px-3 py-1.5 rounded-md text-sm font-medium"
            style={{
              background: 'var(--color-bg-subtle)',
              border: '1px solid var(--color-border-light)',
              color: 'var(--color-text-secondary)',
            }}
          >
            Columns ▾
          </button>
          <Show when={colMenuOpen()}>
            <div
              class="absolute right-0 mt-1 z-20 rounded-md py-1 min-w-40"
              style={{
                background: 'var(--color-bg-base)',
                border: '1px solid var(--color-border-light)',
                'box-shadow': '0 4px 12px var(--color-shadow)',
              }}
            >
              <For each={COLUMNS}>
                {(c) => (
                  <label class="flex items-center gap-2 px-3 py-1.5 text-sm cursor-pointer">
                    <input
                      type="checkbox"
                      checked={!hidden().has(c.key)}
                      onChange={() => toggleColumn(c.key)}
                    />
                    {headerLabel(c)}
                  </label>
                )}
              </For>
            </div>
          </Show>
        </div>
      </div>

      <Show
        when={!loading()}
        fallback={<div class="py-12 text-center text-sm" style={{ color: 'var(--color-text-secondary)' }}>Loading…</div>}
      >
        <Show
          when={rows().length > 0}
          fallback={
            <div class="py-12 text-center text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              No items {query() ? 'match your filter' : 'yet'}.
            </div>
          }
        >
          <div class="overflow-x-auto rounded-lg" style={{ border: '1px solid var(--color-border-light)' }}>
            <table class="w-full text-sm border-collapse">
              <thead>
                <tr style={{ background: 'var(--color-bg-subtle)' }}>
                  <For each={visibleColumns()}>
                    {(c) => (
                      <th
                        class="text-left px-3 py-2 font-medium select-none cursor-pointer whitespace-nowrap"
                        style={{ color: 'var(--color-text-secondary)' }}
                        onClick={() => toggleSort(c.key)}
                        aria-sort={sortKey() === c.key ? (sortDir() === 'asc' ? 'ascending' : 'descending') : 'none'}
                      >
                        {headerLabel(c)}
                        <Show when={sortKey() === c.key}>
                          <span aria-hidden="true"> {sortDir() === 'asc' ? '▲' : '▼'}</span>
                        </Show>
                      </th>
                    )}
                  </For>
                </tr>
              </thead>
              <tbody>
                <For each={rows()}>
                  {(item) => (
                    <tr
                      style={{
                        'border-top': '1px solid var(--color-border-light)',
                        opacity: saving() === item.id ? '0.5' : '1',
                      }}
                    >
                      <For each={visibleColumns()}>
                        {(c) => (
                          <td class={`${cellPad()} align-middle`}>
                            <Show
                              when={c.editable && isEditing(item.id, c.key)}
                              fallback={
                                <button
                                  type="button"
                                  class="text-left w-full truncate"
                                  style={{
                                    color: 'var(--color-text-primary)',
                                    cursor: c.editable ? 'text' : 'default',
                                  }}
                                  onClick={() => c.editable && setEditing({ id: item.id, key: c.key })}
                                >
                                  {renderCell(item, c, typeMap, agentActivity.stateFor(item.id))}
                                </button>
                              }
                            >
                              {renderEditor(item, c, statuses(), commit)}
                            </Show>
                          </td>
                        )}
                      </For>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>
    </div>
  );
}

// ── Cell rendering ────────────────────────────────────────────────────────────

function renderCell(
  item: Item,
  c: ColDef,
  typeMap: ReturnType<typeof useVocab>['typeMap'],
  agentInfo?: AgentBadgeInfo,
) {
  switch (c.key) {
    case 'title':
      return (
        <span class="inline-flex items-center gap-2">
          <span class="font-medium">{item.title}</span>
          <Show when={agentInfo}>
            {(info) => <AgentStateChip state={info().state} title={info().remoteStatus} />}
          </Show>
        </span>
      );
    case 'item_type': {
      const key = typeKey(item.item_type);
      const meta = typeMap()[key];
      return <span>{meta ? `${meta.emoji} ${meta.label}` : key}</span>;
    }
    case 'status':
      return <span>{item.status}</span>;
    case 'priority':
      return <span>{PRIORITY_EMOJI[item.priority] ?? ''} {item.priority}</span>;
    case 'assignee':
      return <span>{item.assignee || '—'}</span>;
    case 'due_date':
      return <span>{item.due_date ? item.due_date.slice(0, 10) : '—'}</span>;
  }
}

function renderEditor(
  item: Item,
  c: ColDef,
  statuses: string[],
  commit: (item: Item, key: SortKey, raw: string) => void,
) {
  const baseStyle = {
    background: 'var(--color-bg-base)',
    border: '1px solid var(--color-primary-500)',
    color: 'var(--color-text-primary)',
  };
  const onKey = (e: KeyboardEvent, el: HTMLInputElement | HTMLSelectElement) => {
    if (e.key === 'Enter') el.blur();
    if (e.key === 'Escape') { (el as HTMLInputElement).value = ''; commit(item, c.key, originalRaw(item, c.key)); }
  };

  if (c.key === 'status') {
    return (
      <select
        autofocus
        class="px-2 py-1 rounded w-full"
        style={baseStyle}
        onChange={(e) => commit(item, 'status', e.currentTarget.value)}
        onBlur={(e) => commit(item, 'status', e.currentTarget.value)}
      >
        <For each={statuses}>
          {(s) => <option value={s} selected={s === item.status}>{s}</option>}
        </For>
      </select>
    );
  }
  if (c.key === 'priority') {
    return (
      <select
        autofocus
        class="px-2 py-1 rounded w-full"
        style={baseStyle}
        onChange={(e) => commit(item, 'priority', e.currentTarget.value)}
        onBlur={(e) => commit(item, 'priority', e.currentTarget.value)}
      >
        <For each={PRIORITIES}>
          {(p) => <option value={p} selected={p === item.priority}>{p}</option>}
        </For>
      </select>
    );
  }
  if (c.key === 'due_date') {
    return (
      <input
        type="date"
        autofocus
        value={item.due_date ? item.due_date.slice(0, 10) : ''}
        class="px-2 py-1 rounded w-full"
        style={baseStyle}
        onBlur={(e) => commit(item, 'due_date', e.currentTarget.value)}
        onKeyDown={(e) => onKey(e, e.currentTarget)}
      />
    );
  }
  // title, assignee — free text
  return (
    <input
      type="text"
      autofocus
      value={c.key === 'assignee' ? (item.assignee ?? '') : item.title}
      class="px-2 py-1 rounded w-full"
      style={baseStyle}
      onBlur={(e) => commit(item, c.key, e.currentTarget.value)}
      onKeyDown={(e) => onKey(e, e.currentTarget)}
    />
  );
}

/** The current raw value of a cell, used to cancel an edit on Escape (no-op commit). */
function originalRaw(item: Item, key: SortKey): string {
  if (key === 'assignee') return item.assignee ?? '';
  if (key === 'due_date') return item.due_date ? item.due_date.slice(0, 10) : '';
  if (key === 'title') return item.title;
  return '';
}

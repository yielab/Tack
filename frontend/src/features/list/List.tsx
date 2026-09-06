import { createSignal, For, Show, createMemo, onMount, onCleanup } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import {
  DragDropProvider,
  DragDropSensors,
  SortableProvider,
  createSortable,
  closestCenter
} from '@thisbeyond/solid-dnd';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { type ItemTypeConfig } from '../../shared/vocab/vocab';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Item } from '../../shared/types';
import { Button, AgentStateChip } from '../../shared/ui';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { useAgentActivityMap, type AgentBadgeInfo } from '../../shared/agentActivity/useAgentActivityMap';
import { createBoardSocket } from '../../shared/realtime/boardSocket';
import { FiPlus, FiMenu, FiCheck, FiX, FiChevronRight, FiChevronDown, FiTrash2, FiMaximize2, FiMinimize2 } from 'solid-icons/fi';

type ItemType = 'epic' | 'feature' | 'task' | 'subtask' | 'bug' | 'requirement';
type ItemWithChildren = Item & { children: ItemWithChildren[]; level: number };

const PRIORITIES = [
  { value: 'critical', emoji: '🔥', label: 'Critical' },
  { value: 'high', emoji: '⬆️', label: 'High' },
  { value: 'medium', emoji: '➡️', label: 'Medium' },
  { value: 'low', emoji: '⬇️', label: 'Low' },
];

export default function List() {
  const params = useParams();
  const projectId = params.id;

  const { items, refetch } = useProjectItems();
  const { project } = useProject();
  const { types: vocabTypes } = useVocab();
  const types = createMemo(() => vocabTypes());
  const agentActivity = useAgentActivityMap(() => projectId);

  const [, setSearchParams] = useSearchParams();
  const [expandedItems, setExpandedItems] = createSignal<Set<string>>(new Set());
  const [creatingAt, setCreatingAt] = createSignal<{ parentId?: string } | null>(null);
  const [newItemTitle, setNewItemTitle] = createSignal('');
  const [newItemType, setNewItemType] = createSignal<ItemType>('task');
  const [newItemPriority, setNewItemPriority] = createSignal('medium');

  const organizedItems = createMemo(() => {
    const allItems = items() || [];
    const itemMap = new Map<string, ItemWithChildren>();
    const rootItems: ItemWithChildren[] = [];

    allItems.forEach(item => {
      itemMap.set(item.id, { ...item, children: [], level: 0 });
    });

    allItems.forEach(item => {
      const itemWithChildren = itemMap.get(item.id)!;
      if (item.parent_id) {
        const parent = itemMap.get(item.parent_id);
        if (parent) {
          itemWithChildren.level = parent.level + 1;
          parent.children.push(itemWithChildren);
        } else {
          rootItems.push(itemWithChildren);
        }
      } else {
        rootItems.push(itemWithChildren);
      }
    });

    const sortItems = (items: ItemWithChildren[]) => {
      items.sort((a, b) => a.created_at.localeCompare(b.created_at));
      items.forEach(item => sortItems(item.children));
    };
    sortItems(rootItems);

    return rootItems;
  });

  const flattenedItems = createMemo(() => {
    const result: ItemWithChildren[] = [];
    const expanded = expandedItems();

    const flatten = (items: ItemWithChildren[]) => {
      items.forEach(item => {
        result.push(item);
        if (expanded.has(item.id) && item.children.length > 0) {
          flatten(item.children);
        }
      });
    };

    flatten(organizedItems());
    return result;
  });

  const allExpandableIds = createMemo(() => {
    const ids: string[] = [];
    const collect = (items: ItemWithChildren[]) => {
      for (const item of items) {
        if (item.children.length > 0) {
          ids.push(item.id);
          collect(item.children);
        }
      }
    };
    collect(organizedItems());
    return ids;
  });

  const expandAll = () => setExpandedItems(new Set(allExpandableIds()));
  const collapseAll = () => setExpandedItems(new Set<string>());

  const toggleExpand = (itemId: string) => {
    setExpandedItems(prev => {
      const newSet = new Set(prev);
      if (newSet.has(itemId)) {
        newSet.delete(itemId);
      } else {
        newSet.add(itemId);
      }
      return newSet;
    });
  };

  const startCreating = (parentId?: string) => {
    setCreatingAt({ parentId });
    setNewItemTitle('');
    setNewItemType('task');
    setNewItemPriority('medium');
  };

  const cancelCreating = () => {
    setCreatingAt(null);
    setNewItemTitle('');
  };

  const handleCreate = async () => {
    const title = newItemTitle().trim();
    if (!title) return;

    const creating = creatingAt();
    if (!creating || !projectId) return;

    try {
      await api.items.create(projectId, {
        title,
        item_type: newItemType(),
        priority: newItemPriority() as any,
        parent_id: creating.parentId,
      });
      toast.success('Item created');
      await refetch();
      setCreatingAt(null);
      setNewItemTitle('');

      if (creating.parentId) {
        setExpandedItems(prev => new Set([...prev, creating.parentId!]));
      }
    } catch (err) {
      toast.error('Failed to create item');
    }
  };

  const handleDelete = async (item: Item) => {
    if (!confirm(`Delete "${item.title}"?`)) return;
    try {
      await api.items.remove(item.id);
      toast.success('Deleted');
      await refetch();
    } catch {
      toast.error('Failed to delete');
    }
  };

  const handleMove = (_itemId: string, _newParentId: string | null) => {
    // Re-parenting is not supported by the API: `PATCH /items/:id` (UpdateItem in
    // the OpenAPI contract) has no `parent_id` field, so the backend would
    // silently ignore the change. Surface that honestly rather than showing a
    // misleading "moved" toast. (Re-parenting needs a backend UpdateItem change.)
    toast.info("Re-parenting items isn't supported yet");
  };

  const onDragEnd = ({ draggable, droppable }: any) => {
    if (draggable && droppable && draggable.id !== droppable.id) {
      const draggedItem = flattenedItems().find(i => i.id === draggable.id);
      const targetItem = flattenedItems().find(i => i.id === droppable.id);

      if (draggedItem && targetItem) {
        handleMove(draggedItem.id, targetItem.parent_id || null);
      }
    }
  };

  // Open the item detail drawer (deep-linkable via ?item=).
  const handleViewItem = (item: Item) => {
    setSearchParams({ item: item.id });
  };

  // Refresh the list when the drawer edits an item.
  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });

  // List has no live board
  // socket otherwise (unlike Board.tsx) — this connection exists solely to
  // keep the agent-activity badge chip current when a mirrored run/approval
  // changes, without inventing a broader live-refresh story that isn't
  // needed here.
  onMount(() => {
    if (!projectId) return;
    const s = createBoardSocket(projectId);
    const off = s.onEvent((event) => {
      if (event.type === 'agent_run_updated' || event.type === 'approval_pending') {
        agentActivity.refetch();
      }
    });
    onCleanup(() => { off(); s.close(); });
  });

  return (
    <div class="h-full flex flex-col">
      {/* Modern Header */}
      <div class="sticky top-0 z-10 bg-[var(--color-bg-base)] border-b border-[var(--color-border-light)] shadow-sm">
        <div class="px-8 py-6">
          <div class="flex items-center justify-between">
            <div>
              <h1 class="text-3xl font-bold text-[var(--color-text-primary)] tracking-tight">
                {project()?.name || 'List View'}
              </h1>
              <p class="text-sm text-[var(--color-text-secondary)] mt-1 font-medium">
                {flattenedItems().length} {flattenedItems().length === 1 ? 'item' : 'items'}
              </p>
            </div>

            <div class="flex items-center gap-2">
              <Show when={allExpandableIds().length > 0}>
                <button
                  onClick={expandAll}
                  class="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg transition-colors"
                  style={{ color: 'var(--color-text-secondary)', 'background-color': 'var(--color-bg-subtle)' }}
                  onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--color-primary-600)')}
                  onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--color-text-secondary)')}
                  title="Expand all"
                >
                  <FiMaximize2 size={14} />
                  Expand all
                </button>
                <button
                  onClick={collapseAll}
                  class="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg transition-colors"
                  style={{ color: 'var(--color-text-secondary)', 'background-color': 'var(--color-bg-subtle)' }}
                  onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--color-primary-600)')}
                  onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--color-text-secondary)')}
                  title="Collapse all"
                >
                  <FiMinimize2 size={14} />
                  Collapse all
                </button>
              </Show>
              <Button onClick={() => startCreating()}>
                <FiPlus size={18} />
                New Item
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div class="flex-1 overflow-auto px-8 py-6">
        <Show when={projectId} fallback={
          <div class="flex items-center justify-center h-64">
            <div class="text-center">
              <div class="text-4xl mb-4">📋</div>
              <p class="text-[var(--color-text-secondary)] text-lg">Select a project from the sidebar</p>
            </div>
          </div>
        }>
          {/* Modern Items List */}
          <DragDropProvider onDragEnd={onDragEnd} collisionDetector={closestCenter}>
            <DragDropSensors />
            <SortableProvider ids={flattenedItems().map(i => i.id)}>
              <div class="max-w-6xl">
                <Show when={flattenedItems().length > 0} fallback={
                  <div class="flex items-center justify-center h-64 bg-[var(--color-bg-elevated)] rounded-xl border-2 border-dashed border-[var(--color-border-light)]">
                    <div class="text-center">
                      <div class="text-5xl mb-4">✨</div>
                      <p class="text-[var(--color-text-secondary)] text-lg mb-4">No items yet</p>
                      <Button variant="ghost" onClick={() => startCreating()}>
                        Create your first item
                      </Button>
                    </div>
                  </div>
                }>
                  <div class="space-y-1">
                    <For each={flattenedItems()}>
                      {(item) => (
                        <>
                          <ItemRow
                            item={item}
                            types={types()}
                            onToggleExpand={() => toggleExpand(item.id)}
                            onAddChild={() => startCreating(item.id)}
                            onDelete={() => handleDelete(item)}
                            onView={() => handleViewItem(item)}
                            isExpanded={expandedItems().has(item.id)}
                            agentInfo={agentActivity.stateFor(item.id)}
                          />

                          {/* Inline creation form */}
                          <Show when={creatingAt()?.parentId === item.id && expandedItems().has(item.id)}>
                            <CreateForm
                              level={item.level + 1}
                              types={types()}
                              title={newItemTitle()}
                              type={newItemType()}
                              priority={newItemPriority()}
                              onTitleChange={setNewItemTitle}
                              onTypeChange={setNewItemType}
                              onPriorityChange={setNewItemPriority}
                              onSave={handleCreate}
                              onCancel={cancelCreating}
                            />
                          </Show>
                        </>
                      )}
                    </For>

                    {/* Top-level create form */}
                    <Show when={creatingAt() && !creatingAt()?.parentId}>
                      <CreateForm
                        level={0}
                        types={types()}
                        title={newItemTitle()}
                        type={newItemType()}
                        priority={newItemPriority()}
                        onTitleChange={setNewItemTitle}
                        onTypeChange={setNewItemType}
                        onPriorityChange={setNewItemPriority}
                        onSave={handleCreate}
                        onCancel={cancelCreating}
                      />
                    </Show>
                  </div>
                </Show>
              </div>
            </SortableProvider>
          </DragDropProvider>
        </Show>
      </div>

    </div>
  );
}

// Modern Sortable Item Row
function ItemRow(props: {
  item: ItemWithChildren;
  types: ItemTypeConfig[];
  onToggleExpand: () => void;
  onAddChild: () => void;
  onDelete: () => void;
  onView: () => void;
  isExpanded: boolean;
  agentInfo?: AgentBadgeInfo;
}) {
  const sortable = createSortable(props.item.id);
  const [showActions, setShowActions] = createSignal(false);
  const itemType = (typeof props.item.item_type === 'string' ? props.item.item_type : 'task') as ItemType;
  const typeConfig = () => props.types.find(t => t.value === itemType) ?? props.types[2] ?? { emoji: '📝', label: itemType };
  const priorityConfig = PRIORITIES.find(p => p.value === props.item.priority) || PRIORITIES[2];
  const hasChildren = props.item.children.length > 0;

  // Indent based on level
  const indentStyle = () => ({
    'padding-left': `${props.item.level * 2.5}rem`
  });

  return (
    <div
      ref={sortable.ref}
      class={`
        group relative rounded-lg mb-2 transition-all duration-200
        ${sortable.isActiveDraggable ? 'opacity-50 scale-95 shadow-lg' : ''}
      `}
      style={{
        "background-color": "var(--color-bg-elevated)",
        "border": "1px solid var(--color-border-light)"
      }}
      onMouseEnter={(e) => {
        setShowActions(true);
        e.currentTarget.style.boxShadow = "0 4px 6px -1px rgba(0, 0, 0, 0.1)";
        e.currentTarget.style.borderColor = "var(--color-primary-200)";
      }}
      onMouseLeave={(e) => {
        setShowActions(false);
        e.currentTarget.style.boxShadow = "";
        e.currentTarget.style.borderColor = "var(--color-border-light)";
      }}
    >
      <div class="flex items-center gap-3 px-4 py-3" style={indentStyle()}>
        {/* Drag Handle */}
        <div {...sortable.dragActivators} class="cursor-grab active:cursor-grabbing flex-shrink-0">
          <FiMenu size={16} class="text-[var(--color-text-tertiary)] hover:text-brand transition-colors" />
        </div>

        {/* Expand/Collapse */}
        <button
          onClick={props.onToggleExpand}
          class={`flex-shrink-0 p-1 rounded transition-colors ${hasChildren ? '' : 'invisible'}`}
          style={{
            color: hasChildren ? "var(--color-text-secondary)" : undefined
          }}
          onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-bg-hover)"}
          onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "transparent"}
        >
          {props.isExpanded ? <FiChevronDown size={16} /> : <FiChevronRight size={16} />}
        </button>

        {/* Type Badge */}
        <div
          class="flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center text-lg"
          style={{ "background-color": "var(--color-primary-50)" }}
          title={typeConfig().label}
        >
          {typeConfig().emoji}
        </div>

        {/* Title - Clickable */}
        <button
          onClick={props.onView}
          class="flex-1 min-w-0 text-left group/title"
        >
          <div class="flex items-center gap-2">
            <span class="text-base font-medium text-[var(--color-text-primary)] group-hover/title:text-brand transition-colors truncate">
              {props.item.title}
            </span>
            <Show when={hasChildren}>
              <span
                class="flex-shrink-0 text-xs px-2 py-0.5 rounded-full"
                style={{
                  "background-color": "var(--color-bg-subtle)",
                  color: "var(--color-text-secondary)"
                }}
              >
                {props.item.children.length}
              </span>
            </Show>
          </div>
          <Show when={props.item.description}>
            <p class="text-xs text-[var(--color-text-tertiary)] truncate mt-1">
              {props.item.description?.replace(/<[^>]*>/g, '').substring(0, 80)}
            </p>
          </Show>
        </button>

        {/* Priority Badge */}
        <div
          class="flex-shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-md"
          style={{ "background-color": "var(--color-bg-subtle)" }}
        >
          <span class="text-sm">{priorityConfig.emoji}</span>
          <span class="text-xs font-medium" style={{ color: "var(--color-text-secondary)" }}>{priorityConfig.label}</span>
        </div>

        {/* Status Badge */}
        <span
          class="flex-shrink-0 text-xs px-3 py-1.5 rounded-full font-medium whitespace-nowrap"
          style={{
            "background-color": "var(--color-primary-100)",
            color: "var(--color-primary-700)"
          }}
        >
          {props.item.status}
        </span>

        {/* Agent activity chip */}
        <Show when={props.agentInfo}>
          {(info) => (
            <div class="flex-shrink-0">
              <AgentStateChip state={info().state} title={info().remoteStatus} />
            </div>
          )}
        </Show>

        {/* Tags */}
        <Show when={props.item.tags && props.item.tags.length > 0}>
          <div class="flex-shrink-0 flex gap-1">
            <For each={props.item.tags?.slice(0, 2)}>
              {(tag) => (
                <span
                  class="text-xs px-2 py-1 rounded-md font-medium"
                  style={{
                    "background-color": "var(--color-info-light)",
                    color: "var(--color-info)"
                  }}
                >
                  {tag}
                </span>
              )}
            </For>
          </div>
        </Show>

        {/* Actions */}
        <div class={`flex-shrink-0 flex items-center gap-1 transition-opacity ${showActions() ? 'opacity-100' : 'opacity-0'}`}>
          <button
            onClick={props.onAddChild}
            class="p-2 rounded-md transition-colors"
            style={{ color: "var(--color-text-tertiary)" }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = "var(--color-primary-600)";
              e.currentTarget.style.backgroundColor = "var(--color-primary-50)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = "var(--color-text-tertiary)";
              e.currentTarget.style.backgroundColor = "transparent";
            }}
            title="Add child item"
          >
            <FiPlus size={16} />
          </button>
          <button
            onClick={props.onDelete}
            class="p-2 rounded-md transition-colors"
            style={{ color: "var(--color-text-tertiary)" }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = "var(--color-danger)";
              e.currentTarget.style.backgroundColor = "var(--color-danger-light)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = "var(--color-text-tertiary)";
              e.currentTarget.style.backgroundColor = "transparent";
            }}
            title="Delete"
          >
            <FiTrash2 size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}

// Modern Inline Create Form
function CreateForm(props: {
  level: number;
  types: ItemTypeConfig[];
  title: string;
  type: ItemType;
  priority: string;
  onTitleChange: (v: string) => void;
  onTypeChange: (v: ItemType) => void;
  onPriorityChange: (v: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const indentStyle = () => ({
    'padding-left': `${props.level * 2.5}rem`
  });

  return (
    <div
      class="rounded-lg mb-2"
      style={{
        "background-color": "var(--color-primary-50)",
        border: "2px solid var(--color-primary-200)"
      }}
    >
      <div class="flex items-center gap-3 px-4 py-3" style={indentStyle()}>
        {/* Spacer for alignment */}
        <div class="w-4 flex-shrink-0" />
        <div class="w-6 flex-shrink-0" />

        {/* Type Selector */}
        <select
          value={props.type}
          onChange={(e) => props.onTypeChange(e.currentTarget.value as ItemType)}
          class="flex-shrink-0 text-sm px-3 py-2 border rounded-lg focus:ring-2"
          style={{
            "border-color": "var(--color-primary-300)",
            "background-color": "var(--color-bg-base)",
            color: "var(--color-text-primary)",
            "outline-color": "var(--color-primary-500)"
          }}
        >
          <For each={props.types}>
            {(t) => <option value={t.value}>{t.emoji} {t.label}</option>}
          </For>
        </select>

        {/* Priority Selector */}
        <select
          value={props.priority}
          onChange={(e) => props.onPriorityChange(e.currentTarget.value)}
          class="flex-shrink-0 text-sm px-3 py-2 border rounded-lg focus:ring-2"
          style={{
            "border-color": "var(--color-primary-300)",
            "background-color": "var(--color-bg-base)",
            color: "var(--color-text-primary)",
            "outline-color": "var(--color-primary-500)"
          }}
        >
          <For each={PRIORITIES}>
            {(p) => <option value={p.value}>{p.emoji} {p.label}</option>}
          </For>
        </select>

        {/* Title Input */}
        <input
          type="text"
          value={props.title}
          onInput={(e) => props.onTitleChange(e.currentTarget.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') props.onSave();
            if (e.key === 'Escape') props.onCancel();
          }}
          placeholder="Item title... (Enter to save, Esc to cancel)"
          class="flex-1 text-sm px-4 py-2 border rounded-lg focus:ring-2"
          style={{
            "border-color": "var(--color-primary-300)",
            "background-color": "var(--color-bg-base)",
            color: "var(--color-text-primary)",
            "outline-color": "var(--color-primary-500)"
          }}
          classList={{
            "placeholder-[var(--color-text-tertiary)]": true
          }}
          autofocus
        />

        {/* Actions */}
        <button
          onClick={props.onSave}
          class="flex-shrink-0 p-2 rounded-lg transition-colors shadow-sm"
          style={{
            color: "var(--color-on-accent)",
            "background-color": "var(--color-primary-600)"
          }}
          onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-primary-700)"}
          onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "var(--color-primary-600)"}
          title="Save (Enter)"
        >
          <FiCheck size={18} />
        </button>
        <button
          onClick={props.onCancel}
          class="flex-shrink-0 p-2 rounded-lg transition-colors"
          style={{ color: "var(--color-text-secondary)" }}
          onMouseEnter={(e) => e.currentTarget.style.backgroundColor = "var(--color-bg-hover)"}
          onMouseLeave={(e) => e.currentTarget.style.backgroundColor = "transparent"}
          title="Cancel (Esc)"
        >
          <FiX size={18} />
        </button>
      </div>
    </div>
  );
}

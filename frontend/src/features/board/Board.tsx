import { createMemo, For, Show, createSignal, createEffect, onMount, onCleanup, type Component } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { deriveBoard } from '../../shared/api/boards';
import type { BoardColumn, Item, BoardState, Priority } from '../../shared/types';
import CreateItemModal from '../../shared/ui/CreateItemModal';
import { createBoardSocket, type BoardSocket, type SocketStatus } from '../../shared/realtime/boardSocket';
import { useKeyboard, keyboardManager, type ShortcutContext } from '../../shared/keyboard/keyboard';
import { withOptimisticUpdate } from '../../shared/state/optimistic';
import { BoardSkeleton } from '../../shared/ui/SkeletonScreen';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import EmptyProjectGuide from '../../shared/ui/EmptyProjectGuide';
import { Avatar, AvatarStack, TypeBadge, PriorityDot, WipChip, AgentStateChip, typeKey } from '../../shared/ui';
import { IconPlus } from '../../shared/ui/icons';
import { estimateUnitSuffix } from '../../shared/estimateUnit';
import { useAgentActivityMap, type AgentBadgeInfo } from '../../shared/agentActivity/useAgentActivityMap';
import DispatchCardMenu from '../../shared/dispatch/DispatchCardMenu';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import FirstRunBanner from '../../shared/agents/FirstRunBanner';

/** Short, human id for the card header (real ids are UUIDs). */
function shortId(id: string): string {
  return id.replace(/-/g, '').slice(0, 6).toUpperCase();
}

const ItemCard: Component<{
  item: Item;
  typeLabel: string;
  onEdit: (item: Item) => void;
  agentInfo?: AgentBadgeInfo;
  dispatchAvailable: boolean;
  onDispatched: () => void;
}> = (props) => {
  const [isDragging, setIsDragging] = createSignal(false);

  const handleDragStart = (e: DragEvent) => {
    setIsDragging(true);
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', props.item.id);
  };
  const handleDragEnd = () => setIsDragging(false);
  const handleClick = () => { if (!isDragging()) props.onEdit(props.item); };

  const estimate = () => {
    const e = props.item.estimate;
    if (e == null) return null;
    const suffix = estimateUnitSuffix(props.item.estimate_unit);
    return suffix ? `${e} ${suffix}` : `${e}`;
  };

  return (
    <div
      draggable={true}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={handleClick}
      style={{
        background: 'var(--color-bg-base)',
        border: '1px solid var(--color-border-light)',
        'border-radius': '11px',
        padding: '11px 12px',
        cursor: 'pointer',
        'box-shadow': 'var(--shadow-sm)',
        transition: 'border-color .12s, box-shadow .12s, transform .12s',
        opacity: isDragging() ? 0.4 : 1,
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = 'var(--color-accent-line)';
        e.currentTarget.style.boxShadow = 'var(--shadow-lg)';
        e.currentTarget.style.transform = 'translateY(-1px)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = 'var(--color-border-light)';
        e.currentTarget.style.boxShadow = 'var(--shadow-sm)';
        e.currentTarget.style.transform = 'translateY(0)';
      }}
    >
      <div style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'margin-bottom': '7px' }}>
        <TypeBadge type={props.item.item_type} label={props.typeLabel} />
        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '10.5px', color: 'var(--color-text-tertiary)' }}>
          {shortId(props.item.id)}
        </span>
        <div style={{ flex: 1 }} />
        <Show when={props.agentInfo}>
          {(info) => <AgentStateChip state={info().state} title={info().remoteStatus} />}
        </Show>
        {/* "Run with agent" (TODO.md III-E4) — the new, neutral Part III
            execution surface. Deliberately a separate control from
            `DispatchCardMenu` below (the older, unrelated Part II Docket
            "dispatch" feature), per III.0's vocabulary rule: distinct icon,
            distinct label, distinct backend domain (`ExecutionRequest` via
            `tack-runner`, never `orch_tasks`). */}
        <RunWithAgentButton
          itemId={props.item.id}
          itemTitle={props.item.title}
          projectId={props.item.project_id}
          showStateChip
          compact
        />
        <DispatchCardMenu
          itemId={props.item.id}
          itemTitle={props.item.title}
          available={props.dispatchAvailable}
          onDispatched={props.onDispatched}
        />
      </div>
      <h4 style={{ 'font-size': '13px', 'font-weight': 600, margin: '0 0 9px', 'line-height': 1.35, color: 'var(--color-text-primary)' }}>
        {props.item.title}
      </h4>
      <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
        <Show when={props.item.priority !== 'none'}>
          <PriorityDot priority={props.item.priority as Priority} showLabel />
        </Show>
        <div style={{ flex: 1 }} />
        <Show when={estimate()}>
          <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '10px', 'font-weight': 500, color: 'var(--color-text-secondary)', background: 'var(--color-chip)', padding: '1px 6px', 'border-radius': '5px' }}>
            {estimate()}
          </span>
        </Show>
        <Show when={props.item.assignee}>
          <Avatar name={props.item.assignee!} size="sm" />
        </Show>
      </div>
    </div>
  );
};

const BoardColumnView: Component<{
  column: BoardColumn;
  dotColor: string;
  typeLabelOf: (item: Item) => string;
  onItemDrop: (itemId: string, newStatus: string) => void;
  onAddItem: (status: string) => void;
  onEditItem: (item: Item) => void;
  agentInfoOf: (itemId: string) => AgentBadgeInfo | undefined;
  dispatchAvailable: boolean;
  onItemDispatched: () => void;
}> = (props) => {
  const [isDragOver, setIsDragOver] = createSignal(false);

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
    setIsDragOver(true);
  };
  const handleDragLeave = () => setIsDragOver(false);
  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const itemId = e.dataTransfer!.getData('text/plain');
    if (itemId) props.onItemDrop(itemId, props.column.status);
  };

  return (
    <section style={{
      width: '282px', 'flex-shrink': 0, display: 'flex', 'flex-direction': 'column',
      'max-height': '100%', background: 'var(--color-bg-base)',
      border: '1px solid var(--color-border-light)', 'border-radius': '13px',
    }}>
      <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', padding: '12px 13px 10px' }}>
        <span style={{ width: '8px', height: '8px', 'border-radius': '99px', 'flex-shrink': 0, background: props.dotColor }} />
        <h3 style={{ 'font-size': '12.5px', 'font-weight': 700, margin: 0, 'letter-spacing': '.01em', color: 'var(--color-text-primary)' }}>
          {props.column.status}
        </h3>
        <span style={{ 'font-size': '11px', 'font-weight': 600, color: 'var(--color-text-tertiary)' }}>
          {props.column.items.length}
        </span>
        <div style={{ flex: 1 }} />
        <Show when={props.column.wip_limit != null}>
          <WipChip count={props.column.items.length} limit={props.column.wip_limit!} />
        </Show>
        <button
          onClick={() => props.onAddItem(props.column.status)}
          title="Add item"
          style={{
            width: '22px', height: '22px', 'border-radius': '6px', border: 'none',
            background: 'transparent', cursor: 'pointer', color: 'var(--color-text-tertiary)',
            display: 'flex', 'align-items': 'center', 'justify-content': 'center',
          }}
          onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--color-border-subtle)'; e.currentTarget.style.color = 'var(--color-text-primary)'; }}
          onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = 'var(--color-text-tertiary)'; }}
        >
          <IconPlus size={14} />
        </button>
      </div>
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        style={{
          flex: 1, 'overflow-y': 'auto', display: 'flex', 'flex-direction': 'column', gap: '8px',
          padding: '2px 11px 12px',
          'border-radius': '0 0 13px 13px',
          outline: isDragOver() ? '2px solid var(--color-accent-line)' : 'none',
          'outline-offset': '-4px',
        }}
      >
        <For each={props.column.items}>
          {(item) => (
            <ItemCard
              item={item}
              typeLabel={props.typeLabelOf(item)}
              onEdit={props.onEditItem}
              agentInfo={props.agentInfoOf(item.id)}
              dispatchAvailable={props.dispatchAvailable}
              onDispatched={props.onItemDispatched}
            />
          )}
        </For>
        <Show when={props.column.items.length === 0}>
          <div style={{ border: '1.5px dashed var(--color-border-light)', 'border-radius': '9px', padding: '18px', 'text-align': 'center', 'font-size': '11.5px', color: 'var(--color-text-tertiary)' }}>
            Drop items here
          </div>
        </Show>
      </div>
    </section>
  );
};

const Board: Component = () => {
  const params = useParams();
  const projectId = () => params.id;
  const { vocabulary, project } = useProject();
  const vocab = useVocab();
  const { items, loading, refetch } = useProjectItems();
  const [, setSearchParams] = useSearchParams();
  const agentActivity = useAgentActivityMap(projectId);

  const boardStateFromServer = createMemo((): BoardState | null => {
    const proj = project();
    const its = items();
    if (!proj || !its) return null;
    return deriveBoard(proj, its);
  });

  // status name → category, for the column dot color.
  const categoryOf = (status: string): 'todo' | 'in_progress' | 'done' =>
    project()?.workflow.statuses.find((s) => s.name === status)?.category ?? 'todo';
  const dotColor = (status: string): string => {
    switch (categoryOf(status)) {
      case 'in_progress': return 'var(--color-primary-600)';
      case 'done': return 'var(--color-success-600)';
      default: return 'var(--color-accent2)';
    }
  };
  const typeLabelOf = (item: Item) => vocab.t(typeKey(item.item_type));

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [selectedColumn, setSelectedColumn] = createSignal<string | null>(null);
  const [activeContext, setActiveContext] = createSignal<ShortcutContext>('board');
  const [editingItem, setEditingItem] = createSignal<Item | null>(null);
  const [modalMode, setModalMode] = createSignal<'create' | 'edit'>('create');
  const [optimisticBoardState, setOptimisticBoardState] = createSignal<BoardState | null>(null);

  const currentBoard = (): BoardState | null | undefined => optimisticBoardState() || boardStateFromServer();

  // Unique assignees across the board → toolbar avatar stack.
  const assignees = createMemo(() => {
    const set = new Set<string>();
    for (const col of currentBoard()?.columns ?? []) {
      for (const it of col.items) if (it.assignee) set.add(it.assignee);
    }
    return [...set];
  });
  const itemCount = () => (currentBoard()?.columns ?? []).reduce((n, c) => n + c.items.length, 0);

  // Real-time board updates via the reconnecting socket.
  const [sock, setSock] = createSignal<BoardSocket>();
  const socketStatus = (): SocketStatus => sock()?.status() ?? 'closed';

  createEffect(() => {
    const pid = projectId();
    if (!pid) { setSock(undefined); return; }
    const s = createBoardSocket(pid);
    const off = s.onEvent((event) => {
      void refetch();
      // Card B4 (Wave 2, realtime broadcast, task 34.5): a mirrored agent
      // run/approval change doesn't touch the item's status, so the plain
      // items `refetch()` above won't pick it up — the badge chip has its
      // own bulk resource (`useAgentActivityMap`), refreshed here on exactly
      // the two event types that mean its data is stale.
      if (event.type === 'agent_run_updated' || event.type === 'approval_pending') {
        agentActivity.refetch();
      }
    });
    setSock(s);
    onCleanup(() => { off(); s.close(); });
  });

  // Resync when the connection comes back after dropping.
  let wasReconnecting = false;
  createEffect(() => {
    const st = socketStatus();
    if (st === 'reconnecting') wasReconnecting = true;
    else if (st === 'open' && wasReconnecting) {
      wasReconnecting = false;
      void refetch();
    }
  });

  useKeyboard(activeContext);
  createEffect(() => setActiveContext(showCreateModal() ? 'modal' : 'board'));

  createEffect(() => {
    keyboardManager.register('board', {
      key: 'n',
      description: 'Create new item',
      action: () => {
        const firstColumn = currentBoard()?.columns[0];
        if (firstColumn) handleAddItem(firstColumn.status);
      },
    });
    keyboardManager.register('board', { key: 'r', description: 'Refresh board', action: () => refetch() });
    keyboardManager.register('modal', { key: 'Escape', description: 'Close modal', action: () => setShowCreateModal(false) });
  });

  const handleItemDrop = async (itemId: string, newStatus: string) => {
    const realBoard = boardStateFromServer();
    if (!realBoard) return;
    const item = realBoard.columns.flatMap((col) => col.items).find((i) => i.id === itemId);
    if (!item || item.status === newStatus) return;

    const optimisticBoard = {
      ...realBoard,
      columns: realBoard.columns.map((col) => ({
        ...col,
        items: col.items
          .filter((i) => i.id !== itemId)
          .concat(col.status === newStatus ? [{ ...item, status: newStatus }] : []),
      })),
    };

    await withOptimisticUpdate(
      () => api.items.update(itemId, { status: newStatus }),
      () => setOptimisticBoardState(optimisticBoard),
      async () => { setOptimisticBoardState(null); await refetch(); },
      {
        showSuccessToast: true,
        successMessage: `Item moved to ${newStatus}`,
        showErrorToast: true,
        errorMessage: 'Failed to move item',
      }
    );

    setOptimisticBoardState(null);
    await refetch();
  };

  const handleAddItem = (status: string) => {
    setModalMode('create');
    setEditingItem(null);
    setSelectedColumn(status);
    setShowCreateModal(true);
  };

  const handleEditItem = (item: Item) => setSearchParams({ item: item.id });

  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });

  const liveLabel = () =>
    socketStatus() === 'open' ? 'Live'
      : socketStatus() === 'connecting' || socketStatus() === 'reconnecting' ? 'Connecting…'
        : 'Offline';
  const liveColor = () =>
    socketStatus() === 'open' ? 'var(--color-success-600)'
      : socketStatus() === 'closed' ? 'var(--color-text-tertiary)'
        : 'var(--color-warning-600)';

  return (
    <div style={{ flex: 1, display: 'flex', 'flex-direction': 'column', 'min-width': 0, height: '100%' }}>
      {/* board toolbar */}
      <div style={{ 'flex-shrink': 0, padding: '13px 18px 11px', display: 'flex', 'align-items': 'center', gap: '10px', 'border-bottom': '1px solid var(--color-border-subtle)' }}>
        <h1 style={{ 'font-size': '17px', 'font-weight': 800, 'letter-spacing': '-.01em', margin: 0, color: 'var(--color-text-primary)' }}>
          {vocab.t('board')}
        </h1>
        <Show when={sock()}>
          <span style={{
            display: 'inline-flex', 'align-items': 'center', gap: '5px', 'font-size': '11px', 'font-weight': 600,
            color: liveColor(),
            background: socketStatus() === 'open' ? 'var(--color-success-100)' : 'var(--color-chip)',
            padding: '3px 8px', 'border-radius': '99px',
          }}>
            <span style={{ width: '6px', height: '6px', 'border-radius': '99px', background: liveColor(), animation: socketStatus() === 'open' ? 'tk-pulse 2s ease-in-out infinite' : 'none' }} />
            {liveLabel()}
          </span>
        </Show>
        <span style={{ 'font-size': '12px', color: 'var(--color-text-tertiary)' }}>·</span>
        <span style={{ 'font-size': '12px', color: 'var(--color-text-secondary)' }}>{itemCount()} items</span>
        <div style={{ flex: 1 }} />
        <Show when={assignees().length > 0}>
          <AvatarStack names={assignees()} max={5} />
        </Show>
      </div>

      <FirstRunBanner hasItems={itemCount() > 0} />

      <Show when={!loading()} fallback={<div style={{ padding: '16px 18px' }}><BoardSkeleton /></div>}>
        <Show
          when={projectId()}
          fallback={<div style={{ 'text-align': 'center', padding: '48px', color: 'var(--color-text-secondary)' }}>Please select a project from the Projects page</div>}
        >
          <Show when={(currentBoard()?.columns ?? []).length > 0 && (currentBoard()?.columns ?? []).every((col) => col.items.length === 0)}>
            <div style={{ padding: '16px 18px 0' }}>
              <EmptyProjectGuide onAddItem={() => handleAddItem(currentBoard()!.columns[0].status)} />
            </div>
          </Show>

          <div style={{ flex: 1, 'overflow-x': 'auto', 'overflow-y': 'hidden', padding: '16px 18px 18px' }}>
            <div style={{ display: 'flex', gap: '14px', height: '100%', 'align-items': 'flex-start' }}>
              <For each={currentBoard()?.columns || []}>
                {(column) => (
                  <BoardColumnView
                    column={column}
                    dotColor={dotColor(column.status)}
                    typeLabelOf={typeLabelOf}
                    onItemDrop={handleItemDrop}
                    onAddItem={handleAddItem}
                    onEditItem={handleEditItem}
                    agentInfoOf={agentActivity.stateFor}
                    dispatchAvailable={agentActivity.orchAvailable()}
                    onItemDispatched={agentActivity.refetch}
                  />
                )}
              </For>
            </div>
          </div>
        </Show>
      </Show>

      <Show when={projectId()}>
        <CreateItemModal
          isOpen={showCreateModal()}
          onClose={() => {
            setShowCreateModal(false);
            setSelectedColumn(null);
            setEditingItem(null);
            setModalMode('create');
          }}
          onSuccess={() => refetch()}
          projectId={projectId()!}
          vocabulary={vocabulary()}
          initialStatus={selectedColumn() || undefined}
          mode={modalMode()}
          existingItem={editingItem() || undefined}
        />
      </Show>
    </div>
  );
};

export default Board;

// Shared data hook backing the Board/List/Table badge chips. One bulk fetch
// per project (`GET /projects/{id}/agent-activity`, see `./api.ts`) instead
// of N per-item calls — the three views each mount this hook independently,
// so each gets its own resource/cache, but all three read the same wire
// boundary and the same state-derivation function, so there is exactly one
// `AgentStateChip` implementation, never a per-view reimplementation.

import { createResource, createMemo, type Accessor } from 'solid-js';
import { agentActivityApi, type AgentBadgeRow } from './api';
import { deriveAgentChipState } from './format';
import type { AgentChipState } from '../ui/AgentStateChip';

export interface AgentBadgeInfo {
  state: AgentChipState;
  /** Raw wire status, kept for a tooltip (e.g. distinguishing `blocked` from
   *  a plain `failed` even though both render the `failed` chip). */
  remoteStatus: string;
  attempt: number;
}

export interface AgentActivityMap {
  /** `undefined` = no agent activity for this item (render no chip). */
  stateFor: (itemId: string) => AgentBadgeInfo | undefined;
  /**
   * Re-fetch the bulk badge rows. Callers wire this to
   * `BoardEvent::AgentRunUpdated`/`ApprovalPending` arriving over
   * `shared/realtime/boardSocket.ts` so a badge updates without a page
   * refresh, the same way `useProjectItems`'s `refetch` already does for
   * the item list itself.
   */
  refetch: () => void;
  /**
   * `true` once this same bulk fetch has resolved successfully — i.e.
   * orchestration is enabled on this server and the project is reachable.
   * `false` while loading and on ANY failure, not just a 404 — badges have
   * always failed open (see {@link stateFor}'s own doc comment), but a
   * caller gating something more consequential than a missing chip should
   * not show it until it can positively confirm the fetch succeeded.
   *
   * **`Board.tsx` also feeds this into `DispatchCardMenu`'s `available`
   * prop (`dispatchAvailable={agentActivity.orchAvailable()}`) — that usage
   * is wrong and still live: this boolean only ever means "the bulk
   * agent-activity fetch did not error," i.e. "orchestration is on," never
   * "this project's control plane can dispatch." Those happen to coincide
   * while docket is the only adapter; they stop coinciding the moment a
   * second provider with `capabilities().dispatch === false` exists. The
   * real signal is `shared/orch/capabilities.ts`'s `Capabilities.dispatch`,
   * read from the project's linked control plane — but nothing reachable
   * from THIS hook (fed only a bare `projectId`) currently carries that
   * value without a second network call this hook does not make, and
   * `Board.tsx` (whose inner `ItemCard`/`BoardColumnView` components thread
   * the prop down to the menu) does not make it either. This function's
   * other, correct uses (gating "is there bulk activity data to show at
   * all," e.g. `Sprints.tsx`) are unaffected by fixing this.**
   */
  orchAvailable: () => boolean;
}

/**
 * Fetches the project's bulk agent-activity rows and exposes an
 * item-id → badge-info lookup. Fails open: a 404 (orchestration disabled,
 * the default for every existing install) or any other
 * request error is treated as "no rows", not surfaced as an error — a
 * badge's absence is never worth degrading Board/List/Table over.
 */
export function useAgentActivityMap(projectId: Accessor<string | undefined>): AgentActivityMap {
  const [resource, { refetch }] = createResource(projectId, (id) =>
    agentActivityApi.listForProject(id)
  );

  const byItem = createMemo(() => {
    const map = new Map<string, AgentBadgeRow>();
    if (resource.error) return map; // fail open — see doc comment above
    for (const row of resource()?.rows ?? []) map.set(row.item_id, row);
    return map;
  });

  return {
    stateFor: (itemId: string) => {
      const row = byItem().get(itemId);
      if (!row) return undefined;
      return {
        state: deriveAgentChipState(row.remote_status),
        remoteStatus: row.remote_status,
        attempt: row.attempt,
      };
    },
    refetch: () => void refetch(),
    orchAvailable: () => !resource.loading && resource.error === undefined,
  };
}

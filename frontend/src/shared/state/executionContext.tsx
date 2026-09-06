import { createContext, useContext, onCleanup, type ParentComponent } from 'solid-js';
import { createExecutionStore, createExecutionRealtime, type ExecutionStore } from '../execution';

// The single shared instance of the execution store: every consumer must
// call `useExecutionStore()` rather than `createExecutionStore()`
// independently, or they get divergent copies, defeating "one consistent
// state." This file is that Provider, following the exact shape
// `shared/state/projectItemsContext.tsx` already established for the same
// kind of "one fetch, shared via context" problem.
//
// Deliberately placed under `shared/state/`, not `shared/execution/**`
// (that module's own boundary) and not `frontend/src/features/execution/**`
// (`architecture.test.ts` forbids one `features/*` importing another
// `features/*` — Board/item-detail/Sprint all need this, so it has to live
// in `shared/*` regardless of which feature "owns" the execution domain
// conceptually; see `shared/runWithAgent/`'s own header comment for the
// fuller reasoning).

const ExecutionStoreContext = createContext<ExecutionStore>();

/**
 * Mount once, above every surface that needs execution data (Board,
 * item-detail, Sprint all live under the app shell — see `app/App.tsx`).
 * Fetches nothing itself on mount — each consumer drives its own fetch
 * (`ExecutionTimeline`'s item-scoped `loadList`, `RunWithAgentButton`'s
 * batched `watchItem`) — and wires the shared bounded-poll realtime
 * invalidation (`createExecutionRealtime`) so every consumer's view of a
 * request updates without a manual refetch or a page navigation.
 */
export const ExecutionStoreProvider: ParentComponent = (props) => {
  const store = createExecutionStore();

  const realtime = createExecutionRealtime({
    watchedRequestIds: () => [...store.requests().keys()],
  });
  const unsubscribe = store.connectRealtime(realtime);
  onCleanup(() => {
    unsubscribe();
    realtime.dispose();
  });

  return <ExecutionStoreContext.Provider value={store}>{props.children}</ExecutionStoreContext.Provider>;
};

export function useExecutionStore(): ExecutionStore {
  const ctx = useContext(ExecutionStoreContext);
  if (!ctx) throw new Error('useExecutionStore must be used within ExecutionStoreProvider');
  return ctx;
}

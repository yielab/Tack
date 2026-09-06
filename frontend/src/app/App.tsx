import { Router } from '@solidjs/router';
import Layout from './Layout';
import { routes } from './routes';
import { ExecutionStoreProvider } from '../shared/state/executionContext';

// `ExecutionStoreProvider` wraps the whole router so Board (under
// `WorkLayout`), Sprints (also under `WorkLayout`), and `ItemDetailDrawer`
// (mounted at the `Layout` root, a WorkLayout *sibling*) all share one
// `createExecutionStore()` instance. `Layout` itself is the app shell shared
// by every unrelated feature (Projects, Templates, Fleet, Settings, ...), so
// wrapping here — the smallest common ancestor that is NOT that shared
// internals file — keeps the footprint to a two-line, purely
// additive wrap instead of editing `Layout.tsx`'s body.
function App() {
  return (
    <ExecutionStoreProvider>
      <Router root={Layout}>{routes}</Router>
    </ExecutionStoreProvider>
  );
}

export default App;

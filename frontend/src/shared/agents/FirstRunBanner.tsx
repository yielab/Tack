import { type Component, Show, createResource, createSignal } from 'solid-js';
import { A } from '@solidjs/router';
import { runnersApi } from '../execution';

// Lives in `shared/**`, not `features/agents/**`: `Board.tsx`
// (`features/board/**`) mounts this, and `architecture.test.ts` forbids one
// `features/*` importing another — the same reason `shared/runWithAgent/**`
// exists instead of living under `features/execution/**`. Reads only
// `GET /api/runners` (`shared/execution`, already a legitimate shared
// import) rather than the Agents page's own local-runner client
// (`features/agents/api.ts`) — the embedded runner shows up
// as an ordinary active row there once it's on, so no second endpoint is
// needed to answer "is any agent execution active anywhere".

const DISMISSED_KEY = 'tack_agents_first_run_banner_dismissed';

function isDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === '1';
  } catch {
    return false;
  }
}

function dismiss(): void {
  try {
    localStorage.setItem(DISMISSED_KEY, '1');
  } catch {
    /* ignore — a convenience, not state */
  }
}

export interface FirstRunBannerProps {
  /** Whether the current project has at least one item — the banner is
   *  pointless on an empty board. Passed in rather than fetched here since
   *  `Board.tsx` already holds this. */
  hasItems: boolean;
}

/**
 * "This board can run its items with an agent" — shown on the Board when no
 * runner this page can observe is active anywhere, and the project has
 * items. Dismissable per browser (`localStorage`) — a convenience, not
 * state, so a dismissal on one machine/browser doesn't hide it for anyone
 * else.
 */
const FirstRunBanner: Component<FirstRunBannerProps> = (props) => {
  const [dismissed, setDismissed] = createSignal(isDismissed());
  const [runnersResult] = createResource(() => runnersApi.list());

  const anyActiveAgent = () => (runnersResult()?.data.data ?? []).some((r) => r.state === 'active');

  return (
    <Show when={props.hasItems && !dismissed() && !runnersResult.loading && !runnersResult.error && !anyActiveAgent()}>
      <div
        class="mx-[18px] mt-3 flex items-center gap-3 rounded-lg border px-4 py-2.5 text-sm"
        style={{ 'border-color': 'var(--color-accent-line)', background: 'var(--color-accent-soft)', color: 'var(--color-text-primary)' }}
      >
        <span class="flex-1">This board can run its items with an agent.</span>
        <A href="/agents" class="font-semibold underline">Turn on</A>
        <button
          type="button"
          class="text-xs"
          style={{ color: 'var(--color-text-tertiary)' }}
          aria-label="Dismiss"
          onClick={() => {
            dismiss();
            setDismissed(true);
          }}
        >
          Dismiss
        </button>
      </div>
    </Show>
  );
};

export default FirstRunBanner;

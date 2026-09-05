// The Agents page is a global route (`/agents`, not nested under
// `/projects/:id`) — steps 4 and 5 are project-scoped (a project's default
// model, an item created on it), so the page needs its own notion of
// "which project" independent of `useProject()` (route-`:id`-keyed,
// `shared/state/projectContext.tsx`). Remembered per browser, same pattern
// `shared/state/lastView.ts` already uses for the last-viewed lens.

const KEY = 'tack_agents_selected_project';

export function getRememberedAgentsProjectId(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function setRememberedAgentsProjectId(projectId: string): void {
  try {
    localStorage.setItem(KEY, projectId);
  } catch {
    /* ignore */
  }
}

// Pure formatting/derivation helpers for the Provisioning wizard — kept
// isolated and unit-tested, same discipline `features/fleet/format.ts` and
// `shared/agentActivity/format.ts` established.

/** `null`/`undefined` cap means "no budget cap set" — distinct from a `0`
 *  cap. This is an operator-set ceiling, not a derived spend figure, so it
 *  does **not** carry the "estimated" qualifier `cost_usd_estimated`
 *  requires — same scoping `features/fleet/format.ts`'s
 *  `formatBudget` already establishes
 *  (duplicated here rather than imported, per this codebase's established
 *  per-feature-file convention for small formatters). */
export function formatBudgetCap(budgetUsd: number | null | undefined): string {
  if (budgetUsd == null) return 'no budget cap';
  return budgetUsd.toLocaleString(undefined, { style: 'currency', currency: 'USD' });
}

/**
 * Suggest a docket-side `remote_project` identifier from the operator's
 * Tack project name — a starting point the operator can still edit, never
 * silently used as-is. docket treats `project` as an opaque identifier
 * (`core/pod_provisioning.py`), so this only needs to produce something
 * short, stable, and free of characters that would be awkward in a
 * filesystem path or shell command — it does not need to match any real
 * docket-side validation rule beyond "non-empty," which the server itself
 * still enforces.
 */
export function suggestRemoteProjectName(projectName: string): string {
  const slug = projectName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'new-project';
}

/** `true` iff `shape` (the free-text `pod_shape` field) means "full roster"
 *  — the only value docket's `POST /pods` accepts besides absent. Mirrors
 *  the Rust handler's own `eq_ignore_ascii_case("full")` check so the
 *  wizard's checkbox and the request it sends can never disagree about
 *  what "full roster" means. */
export function isFullPodShape(shape: string | null | undefined): boolean {
  return (shape ?? '').trim().toLowerCase() === 'full';
}

// Pure helpers over `GET /api/runners` (`shared/execution/api.ts`'s
// `RunnerSummary`) for the Agents page's steps 1, 2 and 4 — kept separate
// from any component so "which runner is this machine's own" and "what do
// active runners collectively support" are each one small, testable
// function instead of logic buried inside JSX.
//
// Live-observed convention this module depends on: the embedded/
// self-provisioned runner (`docs/CONFIG.md`'s "Embedded runner" section,
// ADR 0061 decision 6) is always named `local-<uuid>` — confirmed against a
// real `tack serve --with-runner` process across a restart (the name is
// stable, reused from the on-disk credential, never regenerated). There is
// no dedicated wire field for "is this the embedded runner"; this is the
// only observable signal, so `isThisMachineRunner` names it as a convention
// rather than a guaranteed contract — a future runner-v1 revision that adds
// an explicit field should replace this, not stack on top of it.

import type { RunnerSummary } from '../../shared/execution/api';
import type { HarnessCapability, ModelCombination, RunnerCapabilities } from '../../shared/execution/types';

export const LOCAL_RUNNER_NAME_PREFIX = 'local-';

export function isThisMachineRunner(runner: RunnerSummary): boolean {
  return runner.name.startsWith(LOCAL_RUNNER_NAME_PREFIX);
}

/** The active runner representing the embedded runner on this machine, or
 *  `null` when none is enrolled (never started, or revoked). Never used to
 *  claim "on" by itself — step 1's own state comes from `GET
 *  /api/local-runner`; this is only for step 2's per-harness listing, which
 *  has no other source. */
export function findThisMachineRunner(runners: readonly RunnerSummary[]): RunnerSummary | null {
  return runners.find((r) => r.state === 'active' && isThisMachineRunner(r)) ?? null;
}

/** Active runners that are not this machine's own — "on, other machines" in
 *  step 1. Counts every `state === 'active'` row regardless of how recent
 *  its heartbeat is: staleness has no agreed threshold anywhere else in
 *  this tree, and inventing one here would be a hard-coded status this
 *  codebase's rules forbid. A caller that wants
 *  to show staleness can read `last_heartbeat_at` directly. */
export function countOtherActiveRunners(runners: readonly RunnerSummary[]): number {
  return runners.filter((r) => r.state === 'active' && !isThisMachineRunner(r)).length;
}

/** `capability_snapshot` is an untyped, best-effort parse of a stored JSON
 *  column server-side (`RunnerSummary`'s own doc comment) — never assumed
 *  to have the expected shape. Returns `[]` rather than throwing when it
 *  doesn't parse as a `RunnerCapabilities`-shaped object with a `harnesses`
 *  array. */
export function harnessesOf(runner: RunnerSummary | null): HarnessCapability[] {
  const snapshot = runner?.capability_snapshot as Partial<RunnerCapabilities> | null | undefined;
  const harnesses = snapshot?.harnesses;
  return Array.isArray(harnesses) ? harnesses : [];
}

export function findHarness(
  harnesses: readonly HarnessCapability[],
  harnessKind: string,
): HarnessCapability | undefined {
  return harnesses.find((h) => h.harness_kind === harnessKind);
}

/** Step 4's model picker source: the union of `(model_provider, model_id)`
 *  pairs every active runner's every harness has actually reported,
 *  deduplicated. Empty today for both in-tree harnesses — neither CLI has a
 *  list-models command (`HarnessCapability.model_discovery_note`) — which
 *  is why the picker also needs the free-text fallback
 *  `anyHarnessAttestsPassthrough` gates. */
export function unionModelCombinations(runners: readonly RunnerSummary[]): ModelCombination[] {
  const seen = new Map<string, ModelCombination>();
  for (const runner of runners) {
    if (runner.state !== 'active') continue;
    for (const harness of harnessesOf(runner)) {
      for (const combo of harness.model_combinations ?? []) {
        const key = `${combo.model_provider} ${combo.model_ids.slice().sort().join(',')}`;
        if (!seen.has(key)) seen.set(key, combo);
      }
    }
  }
  return [...seen.values()];
}

/** Whether any active runner's any harness attests that it forwards an
 *  operator-specified model id verbatim (`model_passthrough.support ===
 *  'supported'`) — the gate for step 4's "type a model id" free-text
 *  fallback and step 5's own model field, mirroring
 *  `shared/runWithAgent/shared.ts#gateHarnessModelSelection`'s reading of
 *  the same field (that function gates a specific harness at run time; this
 *  gates the picker's affordance at configuration time). */
export function anyHarnessAttestsPassthrough(runners: readonly RunnerSummary[]): boolean {
  return runners
    .filter((r) => r.state === 'active')
    .some((r) => harnessesOf(r).some((h) => h.model_passthrough?.support === 'supported'));
}

/** The first active runner with at least one harness the probe reports
 *  installed (`probe_error === null`), this machine's own preferred first —
 *  step 5's "which runner runs the test" has no picker in the card, so this
 *  is the one auto-selection rule it needs. `null` when nothing qualifies:
 *  step 5 renders that honestly rather than guessing a runner id. */
export function pickTestRunTarget(
  runners: readonly RunnerSummary[],
): { runner: RunnerSummary; harness: HarnessCapability } | null {
  const active = runners.filter((r) => r.state === 'active');
  const ordered = [...active].sort((a, b) => (isThisMachineRunner(b) ? 1 : 0) - (isThisMachineRunner(a) ? 1 : 0));
  for (const runner of ordered) {
    const harness = harnessesOf(runner).find((h) => h.probe_error === null && h.installed_version);
    if (harness) return { runner, harness };
  }
  return null;
}

# VI-C11 handoff

- Base SHA / branch / final SHA: Base `891ae90` (`develop`). Branch
  `agent/vi-c11-precedence-fixture`, created explicitly from `891ae90` — the worktree's
  default branch (`worktree-agent-a3eeb37f7b35618ba`) was at `e5206c7`, which is an
  *ancestor* of `891ae90`, not a descendant (`git merge-base --is-ancestor e5206c7 891ae90`
  succeeds; the reverse fails). Dozens of commits behind, matching the dispatch's warning
  about recent waves starting stale. Recreated with
  `git checkout -b agent/vi-c11-precedence-fixture 891ae90` before any other work. Final
  SHA: none — nothing is committed (hard rule: no commit/push/merge/rebase). The working
  tree has exactly three new files and no modified tracked file; see `git status --porcelain`
  in "Measured numbers".
- Files changed (must equal ownership list): `crates/tack-orch/tests/model_policy_contract.rs`
  (new), `docs/contracts/model-policy/README.md` (new), `docs/contracts/model-policy/precedence-table.json`
  (new, generated — never hand-typed), `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts`
  (new), `docs/agent-handoffs/part-vi/VI-C11.md` (this file). Matches the card's ownership
  line exactly: "the shared fixture and the tests on both sides of it, plus the VI-C11
  handoff." Neither `crates/tack-orch/src/model_policy/**` nor
  `frontend/src/shared/runWithAgent/shared.ts` was touched (both were edited transiently
  for the two adversarial proofs below, then reverted with `git checkout --`; `git status`
  confirms zero diff on either file at handoff).
- Contract fixtures consumed: none from `docs/contracts/runner-v1/` — this card creates a
  new, sibling contract directory, `docs/contracts/model-policy/`, modeled on that
  directory's own README + pin-table pattern (`crates/tack-orch/tests/runner_contract/fixtures.rs`)
  but scoped to one JSON file rather than 46.
- Behavior implemented: none. Both `resolve_model_policy`/`ModelPolicyTier::ORDER`/
  `parse_model_default_convention` (Rust) and `resolveAutoModelPolicy` (TypeScript) are
  unchanged from `891ae90` — this card binds them to one fixture, it does not fix or touch
  either.
- Tests added and exact commands/results:
  - `crates/tack-orch/tests/model_policy_contract.rs::precedence_table_matches_committed_fixture`
    — drift gate: regenerates the 27-row table from the real `resolve_model_policy`/
    `parse_model_default_convention` and asserts it equals the committed
    `precedence-table.json` byte for byte. `cargo nextest run --workspace -E
    'binary(model_policy_contract)'` → `1 passed`. Regenerate with
    `UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'`.
  - `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts` — loads the same JSON
    file from disk and asserts `resolveAutoModelPolicy` agrees with all 27 rows (plus one
    row-count sanity check). `npx vitest run src/shared/runWithAgent/modelPolicyContract.test.ts`
    → `28 passed`.
  - Full gate: `cargo nextest run --workspace` → `1429 passed, 7 skipped`;
    `cd frontend && npm run type-check` → clean; `npx vitest run` → `91 files, 835 tests
    passed` (807 before this card + 28 new); `cargo clippy --workspace --all-targets -- -D
    warnings` → clean; `./scripts/check-comments.sh` → `✓ no board archaeology in crates/`;
    `./scripts/check-test-hygiene.sh` → `✓ tests take their temporary paths from a guard`.
- Failure/adversarial case proved: two, both required by the card's acceptance bar, both
  restored afterward. Full transcripts in "Measured numbers".
  1. **Precedence order.** Swapped `ModelPolicyTier::Project`/`AgentProfile` in
     `ModelPolicyTier::ORDER` (Rust only), regenerated the fixture with
     `UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run ...` (a Rust-only command — no
     TypeScript file touched), then ran the unchanged `modelPolicyContract.test.ts`: 12 of
     28 cases failed, every one a row where the two tiers disagreed on the winner (e.g.
     `ap_explicit__pj_explicit__fl_absent` expected `source: "project"`, actual
     `source: "agent_profile"`). Reverted `mod.rs`, regenerated the fixture again — it
     matched the pre-change file exactly (`git status --porcelain` showed no diff on the
     JSON afterward), TS suite back to 28/28.
  2. **`pinned_auto` stopping the walk.** Changed `resolve_model_policy`'s loop to `continue`
     past a `Some(ModelSelector::AutoSelect)` tier instead of stopping there (falling
     through as if it were absent), regenerated the fixture, ran the unchanged TS test: 13
     of 28 failed, every row that has a pinned-auto tier with a concrete value beneath it
     (e.g. `ap_auto__pj_explicit__fl_absent` expected `{outcome: "pinned_auto", source:
     "agent_profile"}`, actual `{outcome: "explicit", source: "project", ...}`). Reverted,
     regenerated, back to 28/28 with zero tracked-file diff.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the fixture covers only the three tiers an
  *Auto* request can resolve through (agent profile, project, fleet) in three states each
  (absent / pinned-auto / explicit) — 27 rows. It deliberately excludes the fourth Rust tier,
  `RequestOverride`: a request override is never present when resolving what Auto means, on
  both sides, by construction — encoding it would be "something the TS function does not
  consume," not something the wire fails to carry, so it was left out rather than padded in
  (the card's own stop condition did not fire; noted here so a later reader does not mistake
  the omission for an oversight). `parse_model_default_convention`'s tolerance for malformed
  JSON (missing key, wrong type, unrecognised literal) is intentionally out of this fixture's
  scope — both sides already carry independent, pre-existing tests for that
  (`wiring.rs`'s `#[cfg(test)] mod tests`, `shared.test.ts`'s `parseModelDefaultConvention`
  block); this card is about the precedence *walk*, not convention-parsing robustness, and
  duplicating that coverage a third time was not asked for.
- Secrets/logging review: N/A — no credentials, tokens, or log lines anywhere in this
  card's files. The fixture's provider/model values are synthetic sentinels
  (`agent-profile-provider`, `project-model`, etc.), matching the existing Rust test's own
  `sentinel()` convention.
- Safe merge order and likely conflicts: pure additions — no existing file's content
  changed. Safe to merge in any order relative to other Part VI/VII cards in flight; the
  only theoretical collision is another card also creating `docs/contracts/model-policy/`
  or `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts`, which nothing in
  `TODO.md`'s active boards does as of `891ae90`.
- Checklist: no unowned files touched; no live secret; no panic stub (the two `.expect()`
  calls in the generator are on data this same function just produced or on the committed
  fixture — a real read/parse failure, not a reachable production path); no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Rust and TypeScript check their model-precedence walk against one shared file, not two independent copies | `docs/contracts/model-policy/precedence-table.json` (27 rows) read by both `crates/tack-orch/tests/model_policy_contract.rs` and `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts` |
| The fixture is generated from real Rust behavior, never hand-typed | `UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'` → `1 passed`; drift gate re-run without the env var → `1 passed` |
| Reordering `ModelPolicyTier::ORDER` breaks the unchanged TypeScript test | 12/28 `modelPolicyContract.test.ts` cases failed after the Rust-only reorder + regenerate; see "Failure/adversarial case proved" for the exact diffs |
| The `pinned_auto` stop-the-walk rule is equally load-bearing | 13/28 cases failed after making `resolve_model_policy` skip a pinned-auto tier + regenerate |
| Neither implementation was changed by this card | `git status --porcelain` at handoff shows only the three new files + this handoff — zero diff on `crates/tack-orch/src/model_policy/**` or `frontend/src/shared/runWithAgent/shared.ts` |
| Full gate green at handoff | `cargo nextest run --workspace`: `1429 passed, 7 skipped`; clippy clean; `npm run type-check` clean; `npx vitest run`: `91 files, 835 tests passed`; `check-comments.sh`/`check-test-hygiene.sh` clean |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `cargo nextest run --workspace -E 'binary(model_policy_contract)'` (clean, non-update):
  `1 test run: 1 passed, 0 skipped`.
- `cd frontend && npx vitest run src/shared/runWithAgent/modelPolicyContract.test.ts`:
  `Test Files 1 passed (1)`, `Tests 28 passed (28)`.
- `cd frontend && npm run type-check`: exits clean, no output.
- `cargo nextest run --workspace`: `1429 tests run: 1429 passed, 7 skipped`.
- `cd frontend && npx vitest run` (whole suite): `91 files passed`, `835 tests passed`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `./scripts/check-comments.sh`: `✓ no board archaeology in crates/`.
- `./scripts/check-test-hygiene.sh`: `✓ tests take their temporary paths from a guard`.
- Proof 1 (reorder `ORDER`, swap `Project`/`AgentProfile`): TS run →
  `Test Files 1 failed (1)`, `Tests 12 failed | 16 passed (28)`. Sample:
  `ap_explicit__pj_explicit__fl_absent` expected
  `{outcome: 'explicit', source: 'project', provider: 'project-provider', model_id:
  'project-model'}`, received `{outcome: 'explicit', source: 'agent_profile', provider:
  'agent-profile-provider', model_id: 'agent-profile-model'}`.
- Proof 2 (`resolve_model_policy` skips a pinned-auto tier instead of stopping): TS run →
  `Test Files 1 failed (1)`, `Tests 13 failed | 15 passed (28)`. Sample:
  `ap_auto__pj_explicit__fl_absent` expected `{outcome: 'pinned_auto', source:
  'agent_profile'}`, received `{outcome: 'explicit', source: 'project', provider:
  'project-provider', model_id: 'project-model'}`.
- After each proof: `git checkout -- crates/tack-orch/src/model_policy/mod.rs`, then
  `UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'`
  → `1 passed`; `git status --porcelain` on the fixture path → no output (byte-identical).
- New files: `crates/tack-orch/tests/model_policy_contract.rs` 237 lines,
  `docs/contracts/model-policy/README.md` 30 lines,
  `docs/contracts/model-policy/precedence-table.json` 504 lines (27 rows, 3 tiers × 3
  states), `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts` 48 lines.

## What a stranger still cannot do

A stranger editing `resolve_model_policy` or `resolveAutoModelPolicy` directly still gets no
in-editor warning that the two must agree — the drift is caught only the next time the full
test suite actually runs (pre-push/CI), the same way `docs/openapi.json`'s own drift gate
works today. What changed is that the check now exists at all and requires no manual
cross-reading of both files to trust: running the ordinary gate commands (already required
before any merge) now fails loudly, by itself, if a Rust-side precedence change was not
matched with a regenerated fixture — where before this card, nothing anywhere would have
caught it.

## Surface-map delta

None. This card adds no capability and moves no §VI.0 surface-map row from console to UI —
it is a verification binding between two already-shipped implementations, not new
user-facing behavior.

## Context spent

- Tokens read before the first edit (cold start): not measured with a token-counting
  command (the `/tokens` skill was not invoked this session). Qualitatively: `CLAUDE.md`
  (repo + workspace), the two `TODO.md` extracts named in the dispatch (card VI-C11, §VI.0
  capsule), `crates/tack-orch/src/model_policy/mod.rs` and `wiring.rs` in full,
  `frontend/src/shared/runWithAgent/shared.ts` in full, its existing `shared.test.ts`'s
  `resolveAutoModelPolicy`/`parseModelDefaultConvention` blocks, `docs/contracts/runner-v1/README.md`,
  `crates/tack-orch/tests/runner_contract/fixtures.rs`, `crates/tack-api/tests/openapi_contract.rs`
  (the update/drift-gate pattern this card copies), `tack-core::models::ProjectModelDefault`
  and its generated TS counterpart in `schema.gen.ts`, and
  `crates/tack-orch/tests/scheduling/policy.rs` for precedent on importing `model_policy`
  types from an external test crate.
- Context size at handoff: moderate — one focused feature, no live server/runner rig, no
  multi-crate refactor.
- Files opened and not used: `crates/tack-orch/tests/scheduling/policy.rs` (read for the
  `use tack_orch::model_policy::...` import-path precedent only; nothing from it was copied
  — this card's fixture test needed no live `Repository`/database).
- Read-list lines that were wrong: none — both `awk` extraction ranges named in the dispatch
  (the VI-C11 card body, the §VI.0 capsule) matched exactly, and the branch-staleness check
  the dispatch warned about did in fact fire (see "Base SHA / branch / final SHA" above).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

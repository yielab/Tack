# VI-C10 handoff

- Base SHA / branch / final SHA: The worktree's default branch
  (`worktree-agent-a509875f6ddf2ff3b`) started at `e5206c7` — an *ancestor* of `develop`'s
  tip, not a descendant (429 files / 46,506 insertions / 9,502 deletions behind
  `13cdce5`; `git merge-base --is-ancestor 13cdce5 HEAD` failed). Recreated explicitly with
  `git checkout -b agent/vi-c10-frontend-comment-gate 13cdce5` before any other work. Final
  SHA: none — nothing is committed (hard rule: no commit/push/merge/rebase). `HEAD` still
  equals base `13cdce5`; every change described below sits uncommitted in the working tree.
- Files changed (must equal ownership list): `scripts/check-comments.sh` (its scope, per
  the card) plus 99 files under `frontend/src` — every file the extended gate flagged, no
  more. Full list: `git status --porcelain | grep '^ M'` in this worktree. No file outside
  `frontend/src`/`scripts/check-comments.sh` was touched; an incidental one-line fix to
  `.claude/skills/gate/SKILL.md` (a stale "only `.rs`" row) was drafted, then reverted to
  respect the card's explicit scope line — see "Known limitations" for the recommendation
  instead.
- Contract fixtures consumed: none.
- Behavior implemented: `scripts/check-comments.sh` now scans `frontend/src` (`*.ts`,
  `*.tsx`) in addition to `crates/` (`*.rs`), with a new "Handoff paths" category (a
  `docs/agent-handoffs` path is flagged even with no card id next to it) and broader
  comment/string detection for TypeScript conventions (block comments, JSX `{/* */}`
  openers, single-quote/backtick string literals). Every citation the extended gate found
  in `frontend/src` was rewritten to state the underlying fact without the board pointer,
  following the same "keep the knowledge, drop the pointer" rule already enforced on the
  Rust side. Two of those facts were themselves false, not just archaeological (see below),
  one callerless mechanism was deleted outright, and four comments pointed at files that no
  longer exist under the cited name (repointed, not deleted).
- Tests added and exact commands/results:
  - `./scripts/check-comments.sh` (default: `crates/ frontend/src`) → `✓ no board
    archaeology in crates/ frontend/src`. Runtime: combined ~0.12–0.13s across three
    repeated runs (`crates/` alone ~0.10s, `frontend/src` alone ~0.06s — not additive
    because of shared process/git overhead, but the frontend half's own marginal cost is
    roughly 0.03–0.05s on top of the Rust half).
  - `cd frontend && npm run type-check` → clean (`tsc -b`, no errors). Required `npm ci`
    first — this worktree had no `node_modules` (199 packages installed, 0 vulnerabilities).
  - `cd frontend && npx vitest run` → `91 files, 836 tests passed` (0 failed). Two test
    files needed a `MemoryRouter`/`Route` wrapper added to their `mount()` helper
    (`EnrollmentPanel.test.tsx`, `RunnerFleetSection.test.tsx`, `AgentsPage.test.tsx`) after
    `EnrollmentPanel.tsx` started using `<A href="/agents">` (SolidJS Router) to point at
    the live roster — `<A>` throws outside a `Route` and the first vitest run caught this
    with 5 failing tests before the wrap was added.
  - `cargo nextest run --workspace` (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C10`)
    → `1431 passed, 7 skipped`. No Rust file was touched; this run is a green baseline
    check, not evidence of a Rust change.
  - Three load-bearing proofs against `frontend/src/shared/runWithAgent/shared.ts` (added
    one line, ran the gate scoped to that file, captured the failure, reverted the line,
    confirmed the file and the whole gate were clean again before moving on — see "Failure/
    adversarial case proved").

- Failure/adversarial case proved: the acceptance bar asked for three temporary re-additions
  — a `TODO.md §`, a bare card id, and a handoff path — each proven to fail the gate, then
  reverted. All three ran against the same file/line to keep the diff minimal; the file was
  confirmed byte-identical to its pre-proof state afterward (`tail -3` matched, and the
  gate re-ran clean on the whole tree).

  1. **`TODO.md §`** — appended `// PROOF-OF-LOAD-BEARING-1: see TODO.md §6 for the full
     write-up.` → `./scripts/check-comments.sh frontend/src/shared/runWithAgent/shared.ts`:
     ```
     Board citations
       A reader with the code but not the board cannot use these. State the rule, bar or hazard itself.

       529:// PROOF-OF-LOAD-BEARING-1: see TODO.md §6 for the full write-up.
     ```
  2. **Bare card id** — replaced with `// PROOF-OF-LOAD-BEARING-2: this behavior was pinned
     by card VI-C9.` (a single-digit id — the gate's own regex requires a word boundary
     right after the digit, so a two-digit id like `VI-C10` does not match; this is stated
     plainly, not hidden, since it means the gate cannot see its own card number written
     this way — a real, minor gap, not a bug I introduced):
     ```
     Card ids
       Card names mean nothing outside the board. Say what the code does instead.

       529:// PROOF-OF-LOAD-BEARING-2: this behavior was pinned by card VI-C9.
     ```
  3. **Handoff path** — replaced with `// PROOF-OF-LOAD-BEARING-3: see
     docs/agent-handoffs/part-vi/README.md.`:
     ```
     Handoff paths
       A path under docs/agent-handoffs is board scaffolding even without a card id next to it. State the fact the handoff recorded, not where it lives.

       529:// PROOF-OF-LOAD-BEARING-3: see docs/agent-handoffs/part-vi/README.md.
     ```
  After proof 3, the line was deleted and `./scripts/check-comments.sh` (whole tree) was
  re-run: `✓ no board archaeology in crates/ frontend/src`.

- Schema/API/contract change requested from another owner: none.

- Known limitations or `not_measured` fields:
  - **The gate's own card-id regex can't see a two-digit card number.**
    `\b(I{1,3}|IV|V|VI{1,3})-[A-H][0-9]\b` requires a word boundary immediately after one
    digit; `VI-C10` fails to match because `1` and `0` are both word characters with no
    boundary between them (`VI-C9` matches fine — proof 2 above). This predates this card
    (inherited from the Rust-side regex, unchanged here) and is a real, minor blind spot,
    not something this card was asked to fix.
  - **Multi-line JSX block comments with unmarked continuation lines are invisible to the
    gate.** A `{/* opening line\n    continuation line with no leading //, /*, or * */}`
    has its continuation line detected as neither comment nor string by the line-based
    regex, since it carries no marker at all — just indented prose. Found this the hard way:
    after the gate went fully green, re-running the card's own measurement command
    (`grep -rnE 'agent-handoffs|TODO\.md|...' frontend/src`) still found 2 residual hits
    the gate had missed — `Sprints.tsx:394` and `BudgetPanel.tsx:151`, both continuation
    lines of a multi-line `{/* */}` comment. Both are fixed by hand (see "Measured
    numbers"); the gate itself was not changed to catch this class, because doing so
    correctly needs per-file, stateful "am I still inside an open comment" tracking, which
    is a different kind of script than the current fast, stateless, line-by-line grep
    pipeline — a real follow-up, not a five-minute fix. Recommend a dedicated card if this
    pattern recurs.
  - **`frontend/e2e/**` was not extended**, per the card's explicit instruction (another
    card owns two of those spec files right now). Recommendation, not a decision: the same
    decay this card fixed in `frontend/src` can recur in `frontend/e2e` unwatched, and nothing
    currently scans it.
  - **The `runnerFleet/` folder (`EnrollmentPanel.tsx`, `FleetsPanel.tsx`,
    `RunnerHealthCard.tsx`, `RunnerFleetSection.tsx`) is a session-local, no-live-data view
    that `AgentsPage.tsx` has since grown a live-`GET /runners`-backed replacement next to.**
    Fixing the false "no GET /runners" claims in this folder (see "Claim → evidence") only
    required stating the truth, not rebuilding the UI — but the folder itself now reads as
    partially superseded scope, not just stale prose. Whether to merge it into `AgentsPage`,
    delete the session-local roster in favor of the live one, or leave both is a design
    decision this card did not make (scope-discipline rule 2: acceptance is the
    specification, not an invitation to redesign). Flagging it here rather than acting on it.

- Secrets/logging review: n/a — no logging or secret-handling code touched.

- Safe merge order and likely conflicts: independent of every other Wave 17 card per the
  board (`shared.ts` is the one file VI-C9 also touched, and VI-C9 is already merged into
  `develop` at this card's base — confirmed the specific string VI-C9 was said to delete,
  `shared.ts`'s old `III-E2.md`/"Gap 1" mention, is already gone). No other in-flight card
  in `TODO.md` owns any file this diff touches. Straightforward to merge whenever picked up.

- Checklist: no unowned files (scope held to `scripts/check-comments.sh` +
  `frontend/src` exactly — see "Files changed"); no live secret; no panic stub; no blind
  retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `EnrollmentPanel.tsx`'s session-local runner list no longer says "Tack has no endpoint yet to list existing runners" (false — `GET /runners` exists) or renders `docs/agent-handoffs/part-iii/III-E3.md` in the UI. It now says the page doesn't check back with the server for live status, and links to the Agents page's live roster. | `git diff frontend/src/features/agents/runnerFleet/EnrollmentPanel.tsx`; `EnrollmentPanel.test.tsx`'s renamed assertion (`toMatch(/doesn't check back with the server/i)`) passes: `npx vitest run src/features/agents/runnerFleet/EnrollmentPanel.test.tsx` → all pass. |
| `FleetsPanel.tsx` no longer says "no route exposes `agent_fleet_members`" (false — `POST`/`DELETE .../members` exist) with "(requested in this card's handoff)". It now says this page can't show or edit fleet membership yet, without a citation. | `git diff frontend/src/features/agents/runnerFleet/FleetsPanel.tsx`; `FleetsPanel.test.tsx`'s renamed assertion passes. |
| `shared.ts`'s `DEFAULT_RESOLUTION_NOT_AVAILABLE_REASON` no longer tells an operator "profile/project/fleet default precedence (TODO.md III-F3, Wave 5) has not landed yet" — the precedence walk **has** landed (`crates/tack-orch/src/model_policy/mod.rs`, pinned by `docs/contracts/model-policy/precedence-table.json`, VI-C11). The whole mechanism (`DefaultProvenance`, `resolveDefaultProvenance`, the reason string) is deleted: `resolveDefaultProvenance` had exactly one caller anywhere in `frontend/src` — its own test — so it was a callerless mechanism describing a now-false state, not a string that needed a truer replacement. | `grep -rn "resolveDefaultProvenance\|DefaultProvenance" frontend/src` → zero hits after the edit (was: the type, the function, and one test, no production caller). `npx vitest run` still green (836 passed) with the test removed alongside it. |
| `shared.ts:253`'s citation (`docs/agent-handoffs/part-iii/III-E2.md`, "Gap 1") — the card's third named example — was already gone before this card started. | `grep -n "III-E2" frontend/src/shared/runWithAgent/shared.ts` → no output, on the very first read of the file (verified against the card's base, `13cdce5`, which already includes VI-C9's merge). |
| Every other `TODO.md`/card-id/handoff-path citation the extended gate can see, across 99 files, is gone. | `./scripts/check-comments.sh` → `✓ no board archaeology in crates/ frontend/src`. |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- **Citation count, the card's own measurement command**
  (`grep -rnE 'agent-handoffs|TODO\.md|\b(I{1,3}|IV|V|VI|VII)-[A-Z][0-9]\b' frontend/src`),
  re-run against `git HEAD` (base `13cdce5`) before any edit:
  **219 lines across 80 files** (`| wc -l` and `grep -l | wc -l`). The card's own text
  states 221/80 as of 2026-09-06 — this is a **2-line mismatch**, the finding the
  coordinator asked to surface if the number didn't reproduce. The file count (80) matched
  exactly; only the line count drifted. The most likely single cause, confirmed directly:
  `shared.ts` carried two citations on 2026-09-06 (`:183` and `:253`); VI-C9's merge (into
  this card's own base) already removed the `:253` one — accounting for at least 1 of the
  2 missing lines. The remaining 1-line gap wasn't traced further; it doesn't change this
  card's job, since the gate operates on whatever is actually present, not on the
  originally-quoted number.
- Same command, after every fix in this diff: **0 lines across 0 files** — confirmed twice,
  once right after the gate went green (which still found 2 residual hits the gate itself
  can't see — see "Known limitations") and once after those 2 were fixed by hand.
- Gate runtime: `time ./scripts/check-comments.sh` (whole tree, three repeated runs):
  `0.12s`, `0.13s`, `0.12s` (`user`+`sys` from `/usr/bin/time -f '%e s'`: `0.12`, `0.13`,
  `0.12`). Scoped alone: `crates/` only ≈ `0.10s`; `frontend/src` only ≈ `0.06s`. The two
  don't sum to the combined number (shared shell/git process overhead), so the honest
  statement is: the whole gate is ~0.12–0.13s, and the frontend half's own marginal cost on
  top of the ~0.1s Rust half is roughly 0.03–0.05s — both still far under the ~15s Rust test
  suite this runs alongside in CI/pre-push.
- `cd frontend && npm run type-check` → clean, no output beyond the `tsc -b` invocation
  line.
- `cd frontend && npx vitest run` → `Test Files 91 passed (91)`, `Tests 836 passed (836)`,
  `Duration 8.92s`.
- `cargo nextest run --workspace` (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C10`) →
  `Summary [ 14.716s] 1431 tests run: 1431 passed, 7 skipped`.
- Dead pointers repointed (files the gate's new cross-language check found cited under a
  name that no longer exists anywhere in the repo): 4.
  - `shared/agentActivity/useAgentActivityMap.ts` cited `Board.tsx`/`BoardColumnView.tsx`/
    `ItemCard.tsx` as three files; `BoardColumnView`/`ItemCard` are components defined
    *inside* `Board.tsx`, not separate files (confirmed: `grep -n "^const
    BoardColumnView\|^const ItemCard" frontend/src/features/board/Board.tsx` — both are
    there). Repointed to `Board.tsx` alone.
  - `shared/execution/api.ts` cited `crates/tack-api/tests/e6_routes_test.rs` — the
    per-request attempt/event route tests now live in
    `crates/tack-api/tests/handlers/operator_read_routes.rs` (confirmed by grepping that
    file for the exact route strings the old citation named).
  - `shared/execution/artifacts.ts` cited `crates/tack-api/tests/f6a_artifact_wiring_test.rs`
    — the artifact-download wiring proof now lives in
    `crates/tack-api/tests/wiring/artifact.rs` (same confirmation method).
  - `features/provisioning/api.ts` cited `ProvisioningOutcomeNote.tsx` — no such file exists
    (`git ls-files | grep -i ProvisioningOutcomeNote` → empty); the rendering it described
    is `ProvisioningWizard.tsx`'s `ResultPanel` component (confirmed: that component reads
    `props.result.provisioning.status === 'linked'`, the exact discrimination the old
    comment described).
  - One additional wrong-file citation the dead-pointer check does *not* catch (it names a
    real file, just the wrong one): `shared/execution/api.ts` said `capabilities.ts`'s
    `runnerSummaryToCapabilities` — that function is actually defined in
    `RunWithAgentModal.tsx` (`grep -rn "runnerSummaryToCapabilities" frontend/src` shows the
    real definition site). Repointed.
- Files modified: 99 under `frontend/src` + `scripts/check-comments.sh` (100 total,
  0 crates files, 0 untracked files — confirmed via `git status --porcelain | awk '{print
  $1}' | sort | uniq -c` → `101 M` including this handoff once saved).

## What a stranger still cannot do

Nothing changes here for an end user — this card is comment/gate hygiene plus three
corrected operator-facing strings, not a feature. A stranger who reads `EnrollmentPanel.tsx`
or `FleetsPanel.tsx` today gets an honest, current statement of what those two specific
screens can't do yet, instead of a stale, disprovable claim about the backend. A stranger
who adds a new `TODO.md §`/card-id/handoff-path citation to a `.ts`/`.tsx` file under
`frontend/src` now gets caught by `pre-push`/CI exactly the way a Rust doc comment already
was — that gap (the whole reason this card exists) is closed.

## Surface-map delta

None — this card doesn't move a row of §VI.0's surface map. It's infrastructure (the gate)
and hygiene (the citations), not a step in the onboarding path.

## Context spent

- Tokens read before the first edit (cold start): not measured against a stated block
  estimate — the card capsule instructed extracting only the card's own `TODO.md` section
  and the §VI.0 cold-start capsule (two `awk` extractions), which is what was done; no
  wider `TODO.md` read.
- Context size at handoff: not measured (no `/tokens` run this session).
- Files opened and not used: none tracked separately — every file opened was either edited
  or read specifically to verify a claim before or while editing it.
- Read-list lines that were wrong: n/a — this card's dispatch didn't hand out a specific
  read-list; work was driven by the gate's own output at each step instead.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

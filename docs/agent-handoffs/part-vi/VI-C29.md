# VI-C29 handoff

**Base SHA / branch / final SHA:** base `develop` at `2bccb82`, branch
`agent/vi-c29-e2e-comment-gate`, final SHA recorded at commit time below (see the two
commits on this branch — no amend, no rebase, nothing squashed).

**Files changed (must equal ownership list):** `scripts/check-comments.sh` (roots +
pointer-resolution fix), `frontend/e2e/{a11y,agent-assets,execution-attempt-detail,
run-with-agent,scheduler-e2e}.spec.ts` and `frontend/e2e/helpers.ts` (comment rewrites),
`CHANGELOG.md` (`[Unreleased]` bullet), this handoff. No file outside that list was
touched; nothing in `crates/`, `frontend/src`, or `TODO.md` was edited.

**Contract fixtures consumed:** none — this card touches shell/TS comments only, no wire
contract.

**Behavior implemented:** `scripts/check-comments.sh` with no arguments now scans
`crates/`, `frontend/src`, **and `frontend/e2e`** (`*.ts` — `frontend/e2e` has no
`.mjs`/`.js` files, confirmed by `find frontend/e2e -type f | sed 's/.*\.//' | sort |
uniq -c`, so `*.ts` in the existing `INCLUDES` array already covers it). The gate exits 0
against all three roots, together and individually.

**Failure/adversarial case proved:** reintroduced a card id (`VI-C29:`) at the top of
`frontend/e2e/execution-attempt-detail.spec.ts`'s header comment, ran
`./scripts/check-comments.sh frontend/e2e/execution-attempt-detail.spec.ts`, confirmed it
fails with exactly that line flagged under "Card ids", then removed it and confirmed the
gate passes again. Full transcript below under "Load-bearing proof."

**Schema/API/contract change requested from another owner:** none.

**Known limitations or `not_measured` fields:** the card-id regex
(`\b(I{1,3}|IV|V|VI{1,3})-[A-H][0-9]+\b`) requires a Roman-numeral Part prefix. Part
III's own bare letter+digit ids ("Card F1", "card C4", `F4 attempt detail` as test-data
labels) still appear in `frontend/e2e/helpers.ts` and elsewhere and are NOT caught by
this gate. I left them alone — widening the regex is outside this card's Acceptance and
would need checking against `crates/`/`frontend/src` for new false positives too. Noting
it here rather than fixing it, per scope discipline.

**Secrets/logging review:** not applicable — no runtime code, no logging paths touched.

**Safe merge order and likely conflicts:** low conflict risk. The only shared file is
`scripts/check-comments.sh`, which no other Part VI/VII card is known to be editing.
`CHANGELOG.md`'s `[Unreleased]` section is edited by many cards in parallel — the
integrator should expect a textual conflict there and can resolve by keeping both
bullets (mine is a new bullet under `### Changed`, not an edit to an existing one).

**Checklist:** no unowned files touched, no live secret, no panic stub, no blind retry.

---

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The comment gate now covers `frontend/e2e`, not just `crates/` and `frontend/src` | `./scripts/check-comments.sh` (no args) → `✓ no board archaeology in crates/ frontend/src frontend/e2e` |
| Each of the three roots still passes individually | `./scripts/check-comments.sh crates/` / `frontend/src` / `frontend/e2e` — all exit 0 |
| The extended gate is load-bearing, not a no-op | reintroduced `VI-C29:` into `execution-attempt-detail.spec.ts` line 16 → gate fails with that exact line flagged under "Card ids"; removed it → gate passes again (full transcript below) |
| 21 of the original 22 "pointers to files that do not exist" were a script bug, not stale comments | `crates/tack-orch/tests/scheduling/wiring.rs` and 20 other real, tracked `*.spec.ts` files are cited correctly; only `crates/tack-orch/tests/scheduler_wiring_test.rs` (renamed in `dfb8385`) was genuinely stale — see "The 22-pointer table" below |
| `cargo fmt`, clippy, test hygiene, and generated-file freshness are unaffected by a comments-only change | `.githooks/pre-push` run in full from the worktree root → `✓ pre-push checks passed` (transcript below) |
| The frontend still type-checks | `cd frontend && npm ci && npm run type-check` → no output (tsc success) |

A row with no evidence is a claim to delete, not a row to leave blank — every row above
has a command or transcript.

## Measured numbers

**Per-category count, `./scripts/check-comments.sh frontend/e2e` — before any change**
(unmodified script, run first, before touching anything):

| Category | Count |
|---|---|
| Board citations (`TODO.md §`) | 17 |
| Card ids (`VI-C\d+` etc.) | 28 |
| Board vocabulary in prose ("this card", "the handoff", "the acceptance bar") | **29** |
| Handoff paths (`docs/agent-handoffs/...`) | 4 |
| Pointers to files that do not exist | 22 |
| **Exit code** | **1** |

The card's own dispatch text quoted 28/28/17/4/22. Board vocabulary re-measured at **29**,
not 28 — a one-line discrepancy (the dispatch block undercounted by one; every line I
found is real board scaffolding, not a false positive). Per this repo's rule on
re-measuring quoted numbers before relying on them: re-measured, and the correction is
this row.

**After the pointer-resolution fix in `scripts/check-comments.sh`, before any comment
rewrite** (`./scripts/check-comments.sh frontend/e2e`):

| Category | Count |
|---|---|
| Board citations | 17 (unchanged — this category was never affected by the pointer bug) |
| Card ids | 28 (unchanged) |
| Board vocabulary in prose | 29 (unchanged) |
| Handoff paths | 4 (unchanged) |
| Pointers to files that do not exist | **1** (was 22 — 21 were the line-wrap fragment bug, see below) |
| **Exit code** | **1** |

**After every comment rewrite** (`./scripts/check-comments.sh` — no args, all three
roots; also run individually):

| Root | Exit code |
|---|---|
| `crates/` | 0 |
| `frontend/src` | 0 |
| `frontend/e2e` | 0 |
| all three together (the new default) | 0 |

Commands, in order, exactly as run:

```
./scripts/check-comments.sh frontend/e2e                    # before: exit 1, 100 total findings
./scripts/check-comments.sh frontend/e2e                    # after script fix only: exit 1, 79 total findings (69 unique lines)
./scripts/check-comments.sh                                 # after all rewrites: exit 0
./scripts/check-comments.sh crates/                         # exit 0
./scripts/check-comments.sh frontend/src                    # exit 0
./scripts/check-comments.sh frontend/e2e                    # exit 0
```

**Unique lines needing a rewrite, measured directly:** 69 —
`grep -oE 'frontend/e2e/[A-Za-z0-9_.-]+\.ts:[0-9]+' <post-script-fix output> | sort -u | wc -l`
run against the gate's output right after the script fix landed and before any comment
was touched. (A single line can appear in more than one category's list — e.g. a line
citing both `TODO.md §` and a card id — so the sum of the five category counts, 79,
overcounts the number of lines that actually needed editing.)

## The 22-pointer table

The bug: `dead_pointers()` extracts filename-looking tokens from every comment line, then
reports any that don't match a tracked `.rs`/`.ts`/`.tsx` basename. A citation broken
across a line wrap — `` `provider-key-panel` `` ending one line, `` `.spec.ts` `` starting
the next (`frontend/e2e/helpers.ts:611-612`) — extracts as the fragment `spec.ts`. That
fragment matches no real file, so the old code called it "dead," then built a search
pattern out of it and re-scanned every root — and `spec.ts` as a substring matches every
genuine `*.spec.ts` citation in the file. 21 of the 22 original findings were exactly
this: real, existing files reported as missing because the search pattern that found them
was garbage, not because the citations were wrong.

| Line (original numbering) | Cites | Verdict |
|---|---|---|
| `agent-assets.spec.ts:29` | `e2e/agent-assets.spec.ts` | resolved — false positive (substring match) |
| `agent-assets.spec.ts:261,265` | `hero-gif.spec.ts` | resolved — false positive |
| `provider-key-panel.spec.ts:18-19` | `execution-toggle.spec.ts`, `agents-page.spec.ts` | resolved — false positive |
| `a11y.spec.ts:1159,1181,1188,1202` | `run-with-agent.spec.ts`, `scheduler-e2e.spec.ts` | resolved — false positive |
| `run-with-agent.spec.ts:20,141` | `journey.spec.ts`, `a11y.spec.ts`, `scheduler-e2e.spec.ts` | resolved — false positive |
| `agents-page.spec.ts:15,101,102` | `execution-toggle.spec.ts`, `provider-key-panel.spec.ts` | resolved — false positive |
| `recovery-demo.spec.ts:167` | `screenshots.spec.ts`, `hero-gif.spec.ts` | resolved — false positive |
| `helpers.ts:415` | `execution-attempt-detail.spec.ts` | resolved — false positive |
| `helpers.ts:569` | `scheduler-e2e.spec.ts` | resolved — false positive |
| `helpers.ts:611-612` | `execution-toggle.spec.ts`, `provider-key-panel.spec.ts`, `agents-page.spec.ts` | resolved — false positive (this is the line whose OWN wrap produced the `spec.ts` fragment) |
| `execution-toggle.spec.ts:9-10` | `provider-key-panel.spec.ts`, `agents-page.spec.ts` | resolved — false positive |
| `scheduler-e2e.spec.ts:35` | `crates/tack-orch/tests/scheduler_wiring_test.rs` | **genuinely stale** — file renamed to `crates/tack-orch/tests/scheduling/wiring.rs` in commit `dfb8385` ("regroup test binaries by subject, one link per group"); confirmed via `git diff-tree -r -M50 --name-status dfb8385^ dfb8385` (`R085 ... scheduler_wiring_test.rs scheduling/wiring.rs`), and the two named tests (`an_unsaturated_fleet_still_allows_a_member_to_claim`, `a_saturated_fleet_concurrency_limit_blocks_a_fleet_selector_request`) plus `choose_request_for_runner` all still exist at the new path — **repointed** in `scheduler-e2e.spec.ts`'s header comment |

21 resolved as false positives (the fix), 1 repointed as genuinely stale. All 22
accounted for.

## Fix in `scripts/check-comments.sh`

Anchoring the final search pattern (`\b...\b` or similar) does not work: every real
filename also ends in `.ts`/`.tsx`/`.rs`, so a boundary at the start or end of the
fragment cannot distinguish a wrap artifact's tail from a whole name's tail — I tried
this first and it did not change the count (still 22). The actual fix filters the "dead"
candidate list itself: before turning a dead name into a search pattern, drop any
candidate that is the tail of some real tracked filename (`grep -qE -- "<candidate>\$"`
against the `known` list). A split citation always produces exactly this: a fragment that
matches no file on its own but is recognizably the end of one that exists. The one
genuinely dead name, `scheduler_wiring_test.rs`, is not a suffix of anything real, so it
survives the filter untouched.

## Load-bearing proof

```
$ sed -i '16s/.*/\/\/ VI-C29: The attempts\/events\/decisions\/artifacts UI on the Execution tab/' \
    frontend/e2e/execution-attempt-detail.spec.ts
$ ./scripts/check-comments.sh frontend/e2e/execution-attempt-detail.spec.ts

[1mCard ids[0m
  Card names mean nothing outside the board. Say what the code does instead.

  16:// VI-C29: The attempts/events/decisions/artifacts UI on the Execution tab
...
EXIT:1

$ sed -i '16s/.*/\/\/ The attempts\/events\/decisions\/artifacts UI on the Execution tab/' \
    frontend/e2e/execution-attempt-detail.spec.ts
$ ./scripts/check-comments.sh frontend/e2e/execution-attempt-detail.spec.ts
✓ no board archaeology in frontend/e2e/execution-attempt-detail.spec.ts
EXIT:0
```

## Comments rewritten, and what each kept

Every rewrite kept the underlying fact — why a test waits, what a fixture proves, what a
real product bug is — and dropped only the board pointer (card id, wave/phase number,
`TODO.md §`, or handoff path). No comment was deleted for content reasons; the "Stop if"
condition (a comment whose only content is a card reference with the underlying knowledge
already gone) did not apply anywhere in this file set — every flagged comment wrapped a
real, still-true fact about the code.

Two corrections found and fixed along the way (not required by the gate, but false once I
checked them against current code):

- **`a11y.spec.ts`'s Avatar test comment** claimed `<Avatar>` always renders "white
  initials." `frontend/src/shared/ui/Avatar.tsx`'s `textColorForHue` picks black or white
  per hue's own contrast — the "always white" framing described a bug that was already
  fixed. Rewritten to describe the current, dynamic behavior.
- **`agent-assets.spec.ts`'s two-machines comment** claimed `EnrollmentPanel.tsx`'s intro
  paragraph "literally renders a board handoff path" at line 210. It no longer does —
  `grep -n "agent-handoffs" frontend/src/features/agents/runnerFleet/EnrollmentPanel.tsx`
  finds nothing; the paragraph now points to the Agents page instead. The comment was
  describing a state that a prior `frontend/src` cleanup already fixed. Rewritten to drop
  the now-false claim.

Files touched, with the number of separate diff hunks each required
(`git diff --unified=0 -- <file> | grep -c '^@@'`) as a check against the list below:

- `frontend/e2e/a11y.spec.ts` — **19 hunks**: Avatar fixture, Settings → Orchestration,
  Fleet view, Dispatch UI, sprint-button disambiguation (two separate spots), Approvals
  inbox, Project Settings → Orchestration tab, Unit economics, Provisioning wizard, "Run
  with agent" section header, fresh-item note, exact-runner note, repository/model-choice
  inline notes (two spots), picker-selection note, attempt-detail panel header,
  graphite/light contrast note, blocked-dispatch note.
- `frontend/e2e/agent-assets.spec.ts` — **16 hunks**: file header, retry-loop note,
  git-fixture note (plus the fixture's own README string, a separate hunk), the full
  RE-RECORDING RECIPE block, the submit-gate product-bug note, the attempt.png
  collapsed-panel note, the two-machines verification note, the EnrollmentPanel crop
  note, and the `test.skip` reason string.
- `frontend/e2e/execution-attempt-detail.spec.ts` — **5 hunks**: file header, three
  inline notes.
- `frontend/e2e/helpers.ts` — **6 hunks**: `createSprintWithItem`'s doc comment,
  `createFleet`'s doc comment, the `capabilities()` doc comment, the attempt-detail
  section banner, `submitRunnerArtifact`'s doc comment, `createExecution`'s doc comment.
- `frontend/e2e/run-with-agent.spec.ts` — **11 hunks**: file header, five inline notes,
  the project-default-model test's repository-tier note, the sprint-trigger propagation
  note.
- `frontend/e2e/scheduler-e2e.spec.ts` — **4 hunks**: file header (also repoints the one
  genuinely stale file citation), one inline note.

## What a stranger still cannot do

Nothing changes for a stranger running Tack — this card only touched developer-facing
comments and a CI/pre-push script. A stranger installing the binary, connecting a runner,
or running the test suite sees identical behavior before and after this change.

## Context spent

- Tokens read before the first edit (cold start): read `.claude/skills/card/SKILL.md`
  whole, `.claude/scope-discipline.md` whole, `.claude/reporting-contract.md` whole, the
  card's ~35-line block from `TODO.md` (line-ranged), `scripts/check-comments.sh` whole
  (~150 lines), `docs/agent-handoffs/part-vi/TEMPLATE.md` whole, and one baseline gate
  run's output. No dispatch-plan README exists for this card (Part VI's dispatch plan
  doesn't cover VI-C29 specifically), so the generic §1 recipe in `card`'s SKILL.md
  applied.
- Context size at handoff: mid-range for a single-card session — the bulk was reading and
  rewriting six E2E spec files' comment blocks in place, plus one round of debugging the
  `dead_pointers()` bug directly in a scratch script before editing the real one.
- Files opened and not used: none — every file read (`a11y.spec.ts`,
  `agent-assets.spec.ts`, `execution-attempt-detail.spec.ts`, `helpers.ts`,
  `run-with-agent.spec.ts`, `scheduler-e2e.spec.ts`, `EnrollmentPanel.tsx`,
  `Avatar.tsx`, `router.rs` mentions checked by grep only) had at least one edit or one
  fact-check that changed a comment.
- Read-list lines that were wrong: the card text's own root-cause theory ("the check
  fails to resolve [sibling spec files] from an e2e root") did not match what the code
  actually does wrong — `dead_pointers()`'s basename matching is root-agnostic already;
  the real bug is a line-wrap fragment poisoning a substring search. Documented as a
  correction rather than silently fixed to match the card's framing.

## Proposed board row text

> **VI-C29 — done.** `check-comments.sh` now scans `frontend/e2e` by default. The
> "pointers to files that do not exist" bug was a line-wrap fragment (`spec.ts`) turned
> into a substring search, not a root-resolution problem — fixed by dropping any "dead"
> candidate that is the tail of a real tracked filename before it becomes a search
> pattern. 21 of 22 original pointer findings were this; the 1 real stale pointer
> (`scheduler_wiring_test.rs` → `scheduling/wiring.rs`) is repointed. 69 flagged lines
> across 6 spec files rewritten, gate proven load-bearing by reintroducing and removing a
> card id, `pre-push` green.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

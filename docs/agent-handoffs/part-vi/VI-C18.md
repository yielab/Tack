# VI-C18 handoff

**Base note: this worktree started on the wrong branch.** `git log --oneline -1` at dispatch
showed `worktree-agent-a8cbbcdc3c602ee05` at `e5206c7`, on a line unrelated to and not a
descendant of `495b00e` (`git merge-base --is-ancestor 495b00e e5206c7` fails; that commit's
own history is a different, older line — `IV-A6`/README-rewrite work, nothing to do with
Part VI). Local `develop` itself was already sitting exactly at `495b00e`, so recreated
explicitly: `git checkout -b agent/vi-c18-run-with-agent-spec develop`, confirmed
`git merge-base --is-ancestor 495b00e HEAD` succeeds and `git status --porcelain` was empty
before the first edit.

**Verdict: the test, not the gate — and the false thing it was asserting was never really
about Auto at all.** `isCombinationSupported`/`gateHarnessModelSelection` (both read-only for
this card, unmodified) are answering correctly on every run measured below: given the exact
project and runner they were actually handed, the runner genuinely does not declare the
model the gate says is `Unsupported`. The "Stop if" condition in this card's own text — the
gate blocking a combination the scheduler would accept — does not apply; nothing here argues
the gate is wrong.

What was actually false: this test's own assumption that `getOrCreateProject`'s "the shared
project" is a stable identity with no interfering `default_model`. It is not, under this
suite's own `fullyParallel: true` (`playwright.config.ts`) combined with
`crates/tack-db/src/repo/projects.rs`'s `list_projects` query, which orders
`ORDER BY updated_at DESC` (line 82). `getOrCreateProject` (`frontend/e2e/helpers.ts:43-55`)
returns `existing[0].id` — under that ordering, "the shared project" is not "the one project
every test agrees to reuse," it is **whichever project any currently-running test most
recently created or patched** — and dozens of other spec files call `getOrCreateProject` too.
This test's own sibling in the same file (`run-with-agent.spec.ts:153`, "the run form submits
the project's configured model default…") is one such patcher; `scheduler-e2e.spec.ts`'s "an
unsupported model is blocked client-side" test is another, and it was that second one's own
fixture value that I caught leaking into this test's DOM, live, twice (see "Live proof"
below).

- Base SHA / branch / final SHA: base `develop` at `495b00e` (recreated, see above), branch
  `agent/vi-c18-run-with-agent-spec`, not committed (card rule: no commit/push/merge/rebase).
- Files changed (matches ownership — the one owned spec file, plus this handoff):
  - `frontend/e2e/run-with-agent.spec.ts` — the `item-detail: submitting a run…` test
    (line 98) now explicitly checks "Choose…" and selects the target's own declared model
    (index `"0"` — `enrollRunner`'s fixture always reports exactly one combination) and
    asserts the badge reads "Supported" before checking the Run button is enabled. No
    assertion removed, no wait/retry/serialization added.
  - `docs/agent-handoffs/part-vi/VI-C18.md` (this file).
- Contract fixtures consumed: none.
- Behavior implemented: test-only — no product code touched (`frontend/src/shared/runWithAgent/
  shared.ts` and `frontend/src/shared/execution/capabilities.ts` were read, as instructed, to
  confirm the gate's logic, and are unmodified).
- Tests added and exact commands/results: no new test; one existing test's setup changed.
  Commands and pass/fail counts in "Measured numbers" below.
- Failure/adversarial case proved: reverted the fix (`git stash`) and re-ran
  `npx playwright test --project=chromium` against the identical (already-contaminated)
  `e2e.db` this session had built up — the original failure reproduced exactly (69 passed, 1
  failed, same test, same line, same `toBeEnabled` timeout). Restored the fix and re-ran: 70
  passed. The fix is load-bearing, not incidental.
- Schema/API/contract change requested from another owner: none directly requested, but see
  "What is left" — `getOrCreateProject`'s stability assumption is broken by product code
  (`list_projects`'s `ORDER BY updated_at DESC`) this card does not own and did not touch.
- Known limitations or `not_measured` fields:
  - Webkit not measured — matches every other Part VI/VII handoff's finding on this same
    sandbox (`browserType.launch` fails, missing system libraries, root-only fix). Not
    re-verified independently here; not this card's claim to make either way.
  - Firefox not run — the card's reproduction recipe and acceptance bar both name chromium
    only.
- Secrets/logging review: no secret introduced; no logging touched.
- Safe merge order and likely conflicts: touches only `run-with-agent.spec.ts`, additive
  within one test's body. No other in-flight Part VI/VII card is known to touch this file.
- Checklist: no unowned files touched (`git status --porcelain` shows exactly the one spec
  file), no live secret, no panic stub, no blind retry (no retry/wait logic added at all —
  the fix removes a dependency on ambient state rather than papering over a timing issue).

## Live proof: catching the contamination directly

Reproduced against a **fresh** `e2e.db` (deleted before the first run) using this card's own
exact recipe, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C18 npx playwright test
--project=chromium`, on the unmodified base (`495b00e`):

- Run 1: 69 passed, 1 failed — `run-with-agent.spec.ts:98`, `toBeEnabled` timeout on the Run
  button. The failure screenshot's modal read: **"Project default — openai /
  opaque/model-not-declared-1788699614121"**, badge **Unsupported**, reason "no runner
  reports this model id for this harness/provider."
- Run 2 (fresh full re-run, same recipe): identical failure, identical location. Screenshot
  this time: **"Project default — openai / opaque/model-not-declared-1788699828864"** —
  same fixture-name shape, different timestamp.

`opaque/model-not-declared-<timestamp>` is not a value this test, or anything in
`run-with-agent.spec.ts`, ever writes. It is minted verbatim at
`frontend/e2e/scheduler-e2e.spec.ts:222`:
```ts
const undeclaredModelId = `opaque/model-not-declared-${Date.now()}`;
```
in the "an unsupported model is blocked client-side" test — which deliberately uses
`createFreshProject` (never the shared one) precisely because, per its own doc comment,
`default_model` has no route back once set. That test's care in avoiding the shared project
does not help here: `getOrCreateProject`'s `existing[0]` is picked by `updated_at DESC`
across the whole `projects` table, so a project created *by `createFreshProject`* and then
immediately patched (making it the single most-recently-updated row in the entire table)
becomes indistinguishable from "the shared project" to any other concurrently-running test's
own `getOrCreateProject` call. This test's freshly-enrolled runner (declaring only
`opaque/model-alpha`) was being asked, honestly, whether it supported a model id it had never
heard of — because it had been handed someone else's project, not its own.

One further, weaker confirmation: after these two full-suite runs left `e2e.db` in a state
where the globally-most-recently-updated project already carries a mismatched
`default_model`, a **solo** run of just `run-with-agent.spec.ts` (`npx playwright test
e2e/run-with-agent.spec.ts --project=chromium`) also failed once, at the same location —
this card's own "passes alone" framing is real under the DB state it was measured against,
but not a structural guarantee; it depends on ambient DB state and on which of this file's
own tests happens to touch `getOrCreateProject`'s target project last, under
`fullyParallel: true`, even within a single file.

## The fix

`run-with-agent.spec.ts:98`'s test now selects the target's own declared model explicitly
(`Choose…` + Model index `"0"`) instead of leaving `modelMode` on whatever it silently
defaulted to (`"Auto"`, or "Project default" if the ambient project happened to have one).
This is the same convention every `getOrCreateProject`-using test in
`scheduler-e2e.spec.ts` already follows for the identical reason (`fillExactRunnerTarget`'s
callers all explicitly check "Choose…" and select `"0"` rather than trust an ambient
default) — not a new pattern invented for this card. The test's own claim — "a freshly
enrolled runner's declared combination lets an operator submit a run and see it appear,
without navigation" — is unchanged and, if anything, now more precisely what actually gets
asserted: previously this test never proved anything about the model gate at all, it only
happened to pass when an unrelated project's ambient state lined up by chance.

## Claim → evidence

| Claim | Evidence |
|---|---|
| The base full chromium suite fails deterministically at exactly `run-with-agent.spec.ts:98`, unmodified | Two fresh-`e2e.db` runs on `495b00e`, both 69 passed / 1 failed, same test, same assertion |
| The failure is caused by a project-identity leak across spec files, not a gate defect | Both failure screenshots show `Project default — openai / opaque/model-not-declared-<timestamp>` — a string minted only by `scheduler-e2e.spec.ts:222`'s own fixture, never by this file |
| The leak's mechanism is `getOrCreateProject`'s `existing[0]` under `list_projects`'s `updated_at DESC` ordering, not a fluke | `crates/tack-db/src/repo/projects.rs:82` (`ORDER BY updated_at DESC`), `frontend/e2e/helpers.ts:43-55` (`getOrCreateProject` returns `existing[0].id`), `playwright.config.ts` (`fullyParallel: true`) — read directly, not inferred |
| The gate itself is correct given what it was handed | `isCombinationSupported`'s reason text, verbatim in the screenshot: "no runner reports this model id for this harness/provider" — true of the actual runner/project pair the test's own `request` client had produced |
| The fix is load-bearing | Reverted it (`git stash`), re-ran the identical full-suite command against the same (already-contaminated) `e2e.db`: the original failure reproduced exactly. Restored, re-ran clean |
| The fix makes the full suite pass, repeatedly | 6 consecutive full chromium runs after the fix, 70/70 passed every time (exceeds the 3 the card requires) |
| Nothing was weakened to get there | `git diff` on the spec file: one assertion added (`Supported` badge), one structural setup step added (explicit model selection); the original `toBeEnabled()` assertion is untouched, no `test.slow()`, no added wait, no serialization |

## Measured numbers

All commands from `frontend/`, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C18`.

**Before the fix** (`npx playwright test --project=chromium`, base `495b00e`, fresh
`e2e.db` for the first of the two):

| Run | Result |
|---|---|
| 1 | 69 passed, **1 failed** — `run-with-agent.spec.ts:98` |
| 2 | 69 passed, **1 failed** — `run-with-agent.spec.ts:98` (same location, different contaminating value) |

Solo-file re-run afterward, same (now-contaminated) `e2e.db`,
`npx playwright test e2e/run-with-agent.spec.ts --project=chromium`: **1 of 1 also failed**,
same location — the "passes alone" framing does not hold once the shared project's identity
has already drifted, contrary to this card's own inherited claim of a clean 4/4 solo split;
re-measured rather than assumed, per this repo's own rule on load-bearing numbers.

**After the fix**, same command, repeated with `rm -rf test-results` between runs:

| Run | Result |
|---|---|
| 1 | **70 passed** |
| 2 | **70 passed** |
| 3 | **70 passed** |
| 4 | **70 passed** |
| 5 | **70 passed** |

Reverted (`git stash`), re-ran once more on the same accumulated `e2e.db`: 69 passed / 1
failed, same signature as "before." Restored (`git stash pop`), re-ran once more: **70
passed** (run 6 of the fixed state). Type-check: `npm run type-check` → clean (`tsc -b`, no
output).

## What a stranger still cannot do

Nothing new is possible from outside this repository — this card repaired one test's own
setup, not product surface. A stranger driving the real app never encounters
`getOrCreateProject` at all; it is a test-only convenience. What changes is narrower: the
full E2E suite no longer fails this one test on a false premise, and the test itself now
proves what its own name claims (a freshly enrolled runner's declared model can be submitted
and the request appears in the Execution tab) rather than accidentally riding on an unrelated
test's leftover project state.

## Surface-map delta

None — test-only change, no product code touched.

## What is left

**`getOrCreateProject`'s "shared project" is not a stable identity under this suite's own
concurrency model, and this affects every spec file that calls it, not just this one.**
`crates/tack-db/src/repo/projects.rs::list_projects` orders results `ORDER BY updated_at
DESC` (line 82) — sensible product behavior for a project list a real operator would want
sorted by recency, and out of this card's ownership to change. `getOrCreateProject`
(`frontend/e2e/helpers.ts:43-55`) assumes `existing[0]` is a fixed identity every caller can
rely on; under `playwright.config.ts`'s `fullyParallel: true` and a persistent `e2e.db` many
spec files share, `existing[0]` is actually "whichever project any currently-running test
most recently created (`createFreshProject`) or patched (`PATCH /api/projects/:id`)." This
card fixed the one test whose own assertion depended on that assumption being true; it did
not, and structurally could not from within its ownership (`run-with-agent.spec.ts` and this
handoff only), fix the assumption itself in `helpers.ts`. A future test that calls
`getOrCreateProject` and then asserts anything sensitive to that project's `default_model`
(or any other mutable project field) being absent or at a known value is exposed to the same
class of failure this card diagnosed — worth its own card: either give `getOrCreateProject`
a stable selection (e.g. the earliest `created_at`, or a name-based lookup) or document the
hazard loudly at its own definition so the next author reaches for `createFreshProject`
instead, the way `scheduler-e2e.spec.ts`'s model-sensitive test already does.

## Context spent

- Read per this card's own text and read list: `docs/agent-handoffs/part-vi/VI-C9.md` (in
  full), `docs/agent-handoffs/part-vi/VI-C16.md` (in full),
  `frontend/src/shared/execution/capabilities.ts` (in full, read-only),
  `frontend/e2e/run-with-agent.spec.ts` (in full, owned).
- Also read, one hop beyond the card's list, because diagnosis required it:
  `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` (in full — to trace exactly which
  signals decide `modelMode`'s silent default and what `combinationGate()` is fed),
  `frontend/src/shared/runWithAgent/shared.ts` (in full, read-only — `resolveAutoModelPolicy`/
  `gateHarnessModelSelection`, to rule out an Auto-resolution bug before finding the real
  cause), `frontend/e2e/helpers.ts` (`getOrCreateProject`, `createFreshProject`,
  `setProjectDefaultModel`, `enrollRunner`, `createAgentProfile` — to find the exact shared-
  state mechanism), `frontend/e2e/scheduler-e2e.spec.ts` (in full — its own convention of
  explicit `Choose…` + index `"0"` selection on every `getOrCreateProject`-using test is what
  confirmed the fix direction, and its "unsupported model" test's fixture string is what the
  live-proof screenshots caught leaking), `frontend/playwright.config.ts` (`fullyParallel`,
  `testIgnore`), `crates/tack-api/src/handlers/projects.rs` and
  `crates/tack-db/src/repo/projects.rs` (to find `list_projects`'s exact ordering — the root
  cause, confirmed by reading the SQL directly, not guessed).
- A background research pass on runner enrollment/state/capability-snapshot mechanics
  (`crates/tack-api/src/handlers/runner_protocol.rs`,
  `crates/tack-db/src/repo/execution.rs`, `crates/tack-api/src/handlers/runner_admin.rs`) was
  run in parallel while the live-proof screenshots were already pointing at the real cause;
  it ruled out several other hypotheses (runner staleness filtering, `GET /runners`
  pagination) that turned out not to be needed once the screenshot evidence was in hand.
  Useful for ruling things out, not load-bearing for the final diagnosis.
- Files opened and not used for anything load-bearing: none beyond the above — the runner-
  enrollment research confirmed an absence (no staleness/pagination filter), which is itself
  a real, if secondary, finding, not wasted context.
- Read-list lines that were wrong: none identified as wrong; the card's own two "already
  ruled out" items (the execution-toggle lock, VI-C9's per-target capability scoping) held up
  exactly as stated — neither needed to be re-derived or re-argued.

## Proposed board row

*(Status board updates are the integrator's call — this is a draft for that row, not a
merge.)*

**VI-C18 integrated ‹date›.** The gate was right; the test's own setup was not — it left
`modelMode` on a silent default and let the Run-enabled assertion ride on whatever the
shared `getOrCreateProject` project's `default_model` happened to be, which is not a stable
identity: `list_projects` orders `updated_at DESC`, so under this suite's own
`fullyParallel: true`, any concurrently-running test's `createFreshProject`/`PATCH` makes
its project "first" for every other caller. Live-caught twice: the failing modal showed
`scheduler-e2e.spec.ts`'s own `opaque/model-not-declared-<timestamp>` fixture string leaking
in as this test's "Project default." Fixed by having the test select its own runner's
declared model explicitly (`Choose…` + index `"0"`), the same convention
`scheduler-e2e.spec.ts` already uses for every `getOrCreateProject`-touching test of its own.
Full chromium suite: 2 of 2 fresh-db baseline runs failed before, 6 of 6 passed after
(reverting reproduced the failure once more, confirming load-bearing). Escalates
`getOrCreateProject`'s broken stability assumption — live today for every spec file that
calls it, not just this one — as a card of its own.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

# VI-C13 handoff

**Base note: this worktree started on the wrong branch.** `git log --oneline -1` at dispatch
showed `worktree-agent-aeb6cef1f10488b16` at `e5206c7` (a stale, unrelated checkout — not
`develop`'s `891ae90`). Recreated explicitly: `git checkout -b agent/vi-c13-stranded-specs
891ae90`, confirmed `git merge-base --is-ancestor 891ae90 HEAD` succeeds and the tree was
clean before the first edit.

**Verdict: both drivers fixed; the flaky one needed three attempts before the right one held.**
`a11y.spec.ts`'s test drove VI-C2's retired "Fleet"/free-text picker and un-collapsed
repository fields — a mechanical re-drive, same shape VI-C9 already applied to
`scheduler-e2e.spec.ts`. `agents-page.spec.ts`'s step-5 test's cross-file flakiness took
longer: the shared state was `GET /api/runners`'s un-scoped, oldest-active-wins list, and two
plausible-looking fixes (revoke every competitor; use the embedded runner instead) were each
measured and rejected before a third — filtering this page's own view of that one response,
client-side, to this test's own runner — proved to hold under three consecutive full-parallel
runs.

- Base SHA / branch / final SHA: base `develop` at `891ae90` (recreated, see above), branch
  `agent/vi-c13-stranded-specs`, not committed (card rule: no commit/push/merge/rebase).
- Files changed (matches ownership — `a11y.spec.ts`, `agents-page.spec.ts`; no `helpers.ts`
  addition was needed, every fix uses helpers that already existed):
  - `frontend/e2e/a11y.spec.ts` — "item detail Execution tab (with a real request)" now
    enrolls a real runner and drives the current picker (`fillExactRunnerTarget`'s shape,
    inlined) instead of the retired "Fleet" combobox and always-visible Remote field.
  - `frontend/e2e/agents-page.spec.ts` — step-5 test now filters its own `GET /api/runners`
    response via `page.route` down to its own enrolled runner before `pickTestRunTarget`
    ever sees the list; the first test's own cleanup step is now conditional on the toggle
    still reading "Turn off" (a second, smaller cross-file race this same investigation
    turned up — see "What is left").
- Contract fixtures consumed: none — no runner-v1 wire shape changed.
- Behavior implemented: none (test-only card); no product code touched.
- Tests added and exact commands/results: no new tests — two pre-existing tests repaired.
  Commands and results below ("Claim → evidence" and "Three repeats").
- Failure/adversarial case proved: reverted `a11y.spec.ts`'s fix and re-ran alone — the
  original "Fleet" driver times out 100% of the time (`locator.selectOption: Test timeout of
  30000ms exceeded ... waiting for getByRole('combobox', { name: 'Fleet' })`), confirming the
  fix is load-bearing, not incidental. `agents-page.spec.ts`'s step-5 fix was proved the same
  way across the two rejected intermediate versions (below) — each was run three times under
  full parallel load and each failed at least once with the *same* assertion this card set out
  to fix (`"the claimed request is not this test's own item"`), before the adopted version
  held clean three times running.
- Schema/API/contract change requested from another owner: none. Not touched: `GET
  /api/executions` (another card owns its shape right now, per this card's own dispatch note)
  — neither spec depends on it.
- Known limitations or `not_measured` fields: webkit — see "What is left".
- Secrets/logging review: no secret introduced; the `page.route` interception reads and
  re-serves a public, non-secret operator list (`GET /api/runners`) already fetched by the
  real page.
- Safe merge order and likely conflicts: no conflicts expected — both files' recent history
  (VI-C9, VI-C1) touched different lines. Independent of every other in-flight VI-C card.
- Checklist: no unowned files touched (`git status --porcelain` shows only the two owned
  specs), no live secret, no panic stub, no blind retry (the adopted fix removes the
  dependency rather than retrying past it).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `a11y.spec.ts`'s Run-with-agent scan drives the current picker, not the retired one | `npx playwright test --project=chromium -g "item detail Execution tab \(with a real request\)"` → 1 passed; reverted, re-ran → 1 failed at `getByRole('combobox', { name: 'Fleet' })`, 30s timeout |
| `agents-page.spec.ts` step 5 no longer depends on which other spec files' runners are active | Full-parallel suite (`--project=chromium --project=firefox --workers=6`), 3 consecutive runs, zero failures in either `agents-page.spec.ts` test across both browsers — logs below |
| `agents-page.spec.ts` steps-1/2/4 no longer treats "someone else already turned it off" as a failure | Same 3 runs — that test passed in all 3 (its own, separate cross-file race, found and fixed as part of this investigation) |
| Fix removes the shared-state *dependency*, not just this test's own odds of winning it | The adopted mechanism (`page.route` filtering this page's own view) never reads or writes any row another spec file created — contrast with the two rejected versions below, which either mutated other files' rows (unsafe) or only removed rows already provably dead (safe, but measured to fix nothing within one run's ~40s duration, since nothing goes stale that fast) |

A row with no evidence is a claim to delete, not a row to leave blank.

## Three repeats — the acceptance bar

Full command each time (fresh `e2e.db`/`.tack-runner`/`storage-e2e` for run 1 only; runs 2
and 3 reuse whatever run 1 left, matching how this suite is meant to be run — see "What is
left" for why a fresh reset matters for the embedded runner specifically):

```
npx playwright test --project=chromium --project=firefox --workers=6
```

| Run | `agents-page.spec.ts` steps 1/2/4 | `agents-page.spec.ts` step 5 | `a11y.spec.ts` (chromium only — file `test.skip`s non-chromium) |
|---|---|---|---|
| 1 | ✓ chromium (2.7s), ✓ firefox (5.6s) | ✓ chromium (4.5s), ✓ firefox (6.4s) | ✓ chromium (1.9s) |
| 2 | ✓ chromium (6.1s), ✓ firefox (12.9s) | ✓ chromium (4.4s), ✓ firefox (5.6s) | ✓ chromium (1.7s) |
| 3 | ✓ chromium (4.2s), ✓ firefox (10.1s) | ✓ chromium (4.4s), ✓ firefox (5.7s) | ✓ chromium (1.8s) |

Every run also had 8–9 unrelated failures, all pre-existing and outside this card's two owned
files: `agent-assets.spec.ts` (needs its own dedicated release-build server on port 3311, per
its own header comment — never satisfied by this suite's default `cargo run`/`npm run dev`
webServer), one `provider-key-panel.spec.ts` timing flake, and once, `a11y.spec.ts:442`
("item detail drawer with the dispatch control visible") on a genuine, pre-existing
`color-contrast` finding on a `Task` badge chip (4.27:1 against a 4.5:1 threshold) — unrelated
UI, unrelated card, reproducible independent of parallelism. None of these touch either file
this card owns; confirmed by re-reading each failure's own stack trace.

Webkit: **not measured here.** `browserType.launch` fails on this machine —
`Host system is missing dependencies to run browsers ... sudo npx playwright install-deps`
(specifically `libavif16`) — a root-only fix, per this card's own instructions. Not a claim
about the product; a fact about this sandbox.

## What each spec was driving that no longer exists

**`a11y.spec.ts`'s "item detail Execution tab (with a real request)"** created an empty fleet
(`createFleet`) and selected it via `modal.getByRole('combobox', { name: 'Fleet' })`, then
filled `Remote` directly. VI-C2 replaced the separate Fleet/Runner-id pickers with one
"Machine or group" `<select>` (`RunWithAgentModal.tsx`'s `targetOptions`, values
`fleet:<id>`/`exact_runner:<id>`) and collapsed the repository fieldset into a read-only
summary behind a "Change for this run" button. Neither element the old test named exists any
more. Fixed the same way VI-C9 fixed `scheduler-e2e.spec.ts`: enroll a real runner (not a
fleet — `agent_fleet_members` has no write route on any surface, so an empty fleet can never
pass the live-capability gate VI-C2 added), select it via `exact_runner:<id>` when the picker
renders, click "Change for this run" before touching `Remote`, and pick the target's own
declared model combination (index `"0"`) since "Auto" now refuses to submit with no default
model configured anywhere.

**`agents-page.spec.ts`'s step 5** was not driving anything retired — its UI (`TestRunStep.tsx`)
matches the current build. Its bug was a design assumption: it treated "the runner I just
enrolled" as interchangeable with "the runner the page will auto-select," which is only true
when this test's own runner is the sole active, non-embedded, harness-installed one in the
whole database at the moment "Run test" is clicked.

## The shared state, and what didn't work before what did

**The state:** `GET /api/runners` returns every row in `agent_runners`, oldest first
(`list_runners`, `ORDER BY created_at`). `TestRunStep.tsx#pickTestRunTarget` (pure, unit-
tested, and correctly so) sorts this machine's own embedded runner first if active, then picks
the *first* remaining active runner with an installed harness — no operator-facing picker
exists for step 5, unlike the "Run with agent" modal's "Machine or group" select. Every other
spec file that enrolls a runner (`scheduler-e2e.spec.ts` ×5, `run-with-agent.spec.ts` ×3,
`execution-attempt-detail.spec.ts` ×2, and now `a11y.spec.ts` ×1) never revokes it — each one
stays `active` forever on the reused `e2e.db`. None of that is scoped by project, by test, or
by anything this test can address from its own side of the `GET /api/runners` response *as
served* — which is exactly why the fix had to live on the client side of that response
instead.

**Tried and rejected — revoke every other active runner** (the code this card inherited).
Unsafe, provably: `crates/tack-api/src/handlers/runner_protocol.rs`'s auth path hard-rejects
any further runner-protocol call from a revoked credential (`"revoked"` error, distinct from
`stale_lease`) — so revoking a sibling test's runner mid-flight (between its own `enroll` and
`claim`) breaks *that* test, not just reads its state. Also insufficiently effective for this
test's own reliability: reproduced the exact target assertion failure in a full-parallel run
during this investigation (chromium passed, firefox failed at the same
`"the claimed request is not this test's own item"` line the card names).

**Tried and rejected — turn on the embedded/local runner instead of enrolling one.** Its sort
priority beats every other active runner unconditionally, so in isolation this looked like the
clean fix. Two measurements killed it:
1. A genuine, separate, pre-existing environmental bug, found while testing this: the
   embedded runner's own local session (`tack-runner/src/config.rs`'s `DEFAULT_STATE_DIR =
   ".tack-runner"`, resolved relative to the `frontend/` cwd, i.e. `frontend/.tack-runner/`)
   is **not scoped to `e2e.db` at all** — deleting/recreating `e2e.db` without also deleting
   `.tack-runner/` desyncs a stored enrollment credential from a database that no longer
   recognizes it, and the embedded runner's self-enrollment then hard-fails
   (`enrollment token rejected: invalid, expired, or already used`, confirmed against a
   manually-run server with `TACK_LOG_LEVEL=debug`). Not this card's bug to fix (it's
   `tack-cli/src/local_runner.rs`, untouched by this card), but worth flagging: any local dev
   loop that resets `e2e.db` without also clearing `.tack-runner/` will see this.
2. Even with that state cleaned, a full-parallel run measured this approach as *worse*, not
   better: `execution-toggle.spec.ts`, `provider-key-panel.spec.ts`, and this same file's own
   first test all drive the identical server-wide "agent execution on/off" switch, and adding
   step 5 as a fourth concurrent driver of it caused both tests in this file to fail in the
   same run — the first test's "Installed v9.9.1" wait timed out (something else had already
   flipped the switch off mid-test) and step 5's own "Leased" wait timed out at 20s (a real
   harness-subprocess round trip through a heavily shared switch is slower and less
   predictable than an HTTP-only claim).

**Adopted — filter this page's own view of `GET /api/runners`.** `page.route('**/api/runners',
...)` fetches the real response and re-serves it with `data` filtered down to this test's own
enrolled runner id, before `pickTestRunTarget` (running in the browser) ever sees the list.
This is not a mock of the feature under test: `POST /api/executions` (the "Run test" button)
and the real runner-v1 `claim` call afterward are entirely unmocked, and the assertion this
card was told never to relax (`"the claimed request is not this test's own item"`) is
unchanged. It is also not a mutation of shared state in either direction — no other spec
file's row is read differently or written to at all; only this test's own browser tab's view
of one response is narrower than the database's real contents. Measured clean three times
running under full parallel load (table above).

**Bonus fix, found along the way:** `agents-page.spec.ts`'s *first* test ("steps 1, 2 and 4")
has the same shared-switch race description 2 above measures — its own final cleanup step
(`click "Turn off"`) assumed the switch was still on at that point, which failed once under
full load when `execution-toggle.spec.ts` turned it off first. Made that one click conditional
on the switch still reading "Turn off" — the goal ("leave it off") is already met either way,
so skipping the click when someone else already did it is correct, not a relaxed assertion.

## What a stranger still cannot do

Nothing new — this card repaired test infrastructure, not product surface. A stranger who
runs `npx playwright test` today gets the same product behavior as before this card; what
changes is that the suite itself no longer lies about two of its own specs being green.

## What is left

- **The structural fix this card could not make:** `TestRunStep.tsx` has no operator-facing
  target picker at all (unlike the "Run with agent" modal's "Machine or group" select) —
  `pickTestRunTarget`'s un-scoped, oldest-active-wins selection is a real, intentional gap
  (its own doc comment: "step 5's 'which runner runs the test' has no picker in the card, so
  this is the one auto-selection rule it needs"), not a bug this card introduced or could fix
  without touching `frontend/src/features/agents/steps/TestRunStep.tsx` — outside this card's
  ownership (`a11y.spec.ts`, `agents-page.spec.ts`, and additive `helpers.ts` only). The
  client-side route filter adopted here is a legitimate, unmocked-feature-preserving
  workaround, not a substitute for that picker; if `TestRunStep.tsx` is ever redesigned with an explicit target, this test's own filter becomes unnecessary and should
  be the first thing removed.
- **The `.tack-runner/` / `e2e.db` desync** described above is real and reproducible, but
  belongs to whoever owns `tack-cli/src/local_runner.rs` (`EmbeddedRunnerControl::new` calling
  `load_runner_config` with a state dir independent of `AppConfig.storage_dir`) — flagged here,
  not fixed here.
- **One server-wide "agent execution on/off" switch, driven by three spec files.**
  `execution-toggle.spec.ts`, `provider-key-panel.spec.ts`, and `agents-page.spec.ts`'s own
  first test all call `PUT /api/local-runner` against the same single row, with no per-test
  scoping possible (it is one switch, not one per test) — this card only made
  `agents-page.spec.ts`'s own cleanup step tolerate a sibling flipping it first (see "Bonus
  fix" above); it did not, and structurally could not from two owned spec files, remove the
  race between the three. The next file that starts driving this switch will hit the same
  class of failure this card's own measurements above already reproduced once.
- Webkit is not measured on this machine (missing `libavif16`, root-only fix) — chromium and
  firefox are the only real evidence in this handoff.

## Context spent

- Tokens read before the first edit (cold start): card + capsule extracts (~4k), plus reading
  `scheduler-e2e.spec.ts`, `helpers.ts`, both owned specs, `RunWithAgentModal.tsx`,
  `TestRunStep.tsx`, `runnerObservations.ts`, `AgentsPage.tsx`, `ExecutionToggle.tsx`,
  `select.rs`/`wiring.rs` (scheduler), and `runner_protocol.rs`/`execution.rs` (DB) to verify
  the staleness/revocation claims against real code rather than assumption — no fixed
  block estimate exists for this card; the reads were driven by what each hypothesis needed
  proving, not a pre-set budget.
- Files opened and not used in the final diff: none discarded outright, but two full
  implementation attempts (revoke-only-stale sweep, embedded-runner toggle) were written,
  measured under full parallel load, and reverted — each is described above rather than left
  as dead code in the spec file.
- Read-list lines that were wrong: none — no pre-supplied read list for this card beyond the
  card text and cold-start capsule.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

**2026-09-06 — correction to how `pickTestRunTarget` actually chooses, from a coordinator
review before merge.** Every place above that calls the selection "un-scoped, oldest-active-
wins" (the Verdict paragraph; "The state:" under "The shared state, and what didn't work
before what did"; the first bullet under "What is left") states the fallback correctly but
drops the clause that actually governs the function, and the matching comment in
`agents-page.spec.ts` (originally at lines 120–125) had the identical omission — both have
now been corrected in place (the code comment directly; this note covers the handoff prose,
left as originally written above per this repo's own rule that corrections are appended, not
rewritten).

What `pickTestRunTarget` (`runnerObservations.ts:100-109`) actually does, in order:
1. **This machine's own embedded runner wins whenever it is active with an installed
   harness — unconditionally, regardless of its age.** The function's own sort
   (`isThisMachineRunner` first) enforces this before anything else is considered; its own
   doc comment says so directly ("this machine's own preferred first").
2. **Only when no local runner qualifies** does it fall through to the rest of the list
   `GET /api/runners` returned, taking whichever runner is *first in that list* — the
   function itself never re-sorts that remainder by age. "Oldest active wins" is true of
   that remainder only because `list_runners` (`crates/tack-db/src/repo/execution.rs`)
   happens to return rows `ORDER BY created_at`; age is the server's ordering choice, not a
   rule `pickTestRunTarget` computes. Feed it the same rows in a different order and the
   fallback would follow that order instead.

Every claim and every measurement elsewhere in this handoff already held under the *correct*
reading (the un-scoped list, and this test's own runner never being the one such a list
picks by default, is the real reason step 5 needed a fix at all) — nothing about the fix, the
three rejected attempts, or the three-repeat evidence changes. Only the description of the
mechanism was imprecise; a reader chasing "oldest wins" as the rule to fix would not have
found it, since no such rule exists to find.

Also added directly to "What is left" in this same pass, not amended in (it is new content,
not a correction to existing prose): the shared server-wide "agent execution on/off" switch
that `execution-toggle.spec.ts`, `provider-key-panel.spec.ts`, and this file's own first test
all drive deserves its own line as a structural fact about this suite, not just a detail
inside the rejected embedded-runner approach — see the new bullet.

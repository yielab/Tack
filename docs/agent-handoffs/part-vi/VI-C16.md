# VI-C16 handoff

**Base note: this worktree started on the wrong branch.** `git log --oneline -1` at dispatch
showed `worktree-agent-ac0b933109f742959` at `e5206c7`, 152 commits behind `develop`'s
`13cdce5` (`git rev-list --count e5206c7..13cdce5` → 152). Recreated explicitly:
`git checkout -b agent/vi-c16-shared-toggle 13cdce5`, confirmed `git merge-base --is-ancestor
13cdce5 HEAD` succeeds. `origin/develop` had already moved further (`605b649`) by the time this
was checked; stayed on the dispatched `13cdce5` base per the card's own instruction rather than
chasing it.

**Verdict: the three files now share one fixture, and two more bugs surfaced once they stopped
racing past each other.** The chosen mechanism — a cross-process file lock (`executionToggleLock`
in `helpers.ts`) any test can request — is described under "Claim → evidence" and "Measured
numbers" below. Building and proving it out surfaced two bugs unrelated to the switch itself,
each with its own measured before/after: `provider-key-panel.spec.ts`'s setup step raced its own
page's still-loading resources (real, present on the *unmodified* file: 3 of 3 fresh baseline
runs, chromium 3/3 and firefox 1/3 — see below), and a second, previously invisible bug in
`EmbeddedRunnerControl::remove_secret` that this card's own serialization turned from
theoretical into deterministic (an out-of-ownership Rust file; routed around, not fixed, and
flagged below).

- Base SHA / branch / final SHA: base `develop` at `13cdce5` (recreated, see above), branch
  `agent/vi-c16-shared-toggle`, not committed (card rule: no commit/push/merge/rebase).
- Files changed (matches ownership — the three named spec files plus additive `helpers.ts`):
  - `frontend/e2e/helpers.ts` — adds `executionToggleLock`, a fixture built on `test.extend`
    (exported as this file's own `test`, alongside a re-exported `expect` so a spec switching
    its import from `@playwright/test` needs no second import line). Implemented as a
    cross-process lock **directory** under `os.tmpdir()`, keyed by `API_ORIGIN` (so two
    independent `tack-api` servers — e.g. two worktrees on this shared machine — never wait on
    each other). `fs.mkdirSync` with no `recursive` flag is atomic (`EEXIST` on a second
    caller); a `holder` file records who/when, so a lock abandoned by a crashed worker is
    reclaimed after 60s rather than wedging every later run. No new dependency, no server
    change. Every existing caller of the plain `@playwright/test` `test`/`expect` is
    unaffected — this is additive.
  - `frontend/e2e/execution-toggle.spec.ts` — imports `test`/`expect` from `./helpers` instead
    of `@playwright/test`; its one test now requests `executionToggleLock`.
  - `frontend/e2e/provider-key-panel.spec.ts` — same import switch, lock requested; plus two
    additional, measured fixes to its own setup step (below) that turned out to be necessary
    for the lock to actually deliver a stable test, not just a serialized one.
  - `frontend/e2e/agents-page.spec.ts` — same import switch; only its **first** test (the one
    that flips the switch) requests the lock. Its second test (step 5) does not touch
    `/api/local-runner` at all and deliberately does not request it, so it stays exactly as
    parallel as every other spec file.
- Contract fixtures consumed: none — no runner-v1 wire shape touched.
- Behavior implemented: test-infrastructure only; no product code touched.
  1. **The lock itself** — see "Claim → evidence".
  2. **`provider-key-panel.spec.ts`'s initial-load race, fixed.** `ProviderKeyPanel.tsx`'s own
     `stored()` getter (`secrets()?.data.find(...) ?? null`) is falsy both while its `secrets`
     resource is still in flight and once it resolves to "no key stored" — the identical
     fallback `<form>` renders either way. On a reused `e2e.db` where a key already exists,
     racing `removeButton`/`apiKeyField` visibility against that ambiguity can land on the
     *loading* rendering, decide "fill the form", and then have the real response arrive a
     moment later saying a key already exists — yanking the form out from under the
     fill/click. Fixed by attaching `page.waitForResponse` listeners for both of the panel's
     GETs **before** `page.goto`, so neither can resolve before the test is listening, and
     awaiting both before reading which state the DOM is actually in.
  3. **`provider-key-panel.spec.ts`'s setup step, changed from `Remove` to `Replace`.**
     `EmbeddedRunnerControl::remove_secret` (`crates/tack-cli/src/local_runner.rs`) deletes the
     stored secret value but never resets the provider's own `enabled` flag — only
     `set_secret` ever sets it. So on a server process that has *ever* saved this provider's
     key, a `Remove` leaves the catalog reporting `secret_unresolved` forever after, never
     `not_configured` again. Before this card, three independent test files raced past each
     other loosely enough that this rarely mattered (whichever ran the "fill and save" first
     usually wasn't the same one that later called `Remove` against an already-`enabled`
     server). Serializing them behind one lock made it deterministic instead: the *second*
     participant to acquire the lock is now guaranteed to find a key the first one just saved,
     forcing it down the `Remove` branch every time. `Replace` (client-side only — it just
     flips this panel's own `editing` signal) reaches the identical fill-in form with no
     server call and no dependence on that bug. This is a real, separate backend bug, in a
     Rust file this card does not own (`frontend/e2e/**` and additive `helpers.ts` only) —
     flagged here and in "Known limitations", not fixed here.
- Tests added and exact commands/results: no new tests — three existing tests changed to use
  the new fixture, plus the two `provider-key-panel.spec.ts` fixes above. Commands and results
  in "Measured numbers" below.
- Failure/adversarial case proved: reverted all four files to `develop`'s originals (`git
  stash`) and re-ran the exact acceptance command three times, fresh `e2e.db` for the first —
  `provider-key-panel.spec.ts` failed in **3 of 3** runs (chromium 3/3, firefox 1/3), each at
  exactly the mechanism described above (`element was detached from the DOM` at the Save click
  on chromium; the same race caught one step earlier, at `.fill()`, on firefox). Restored the
  fix and re-ran the identical command: 0 failures across every run reported below. The fix is
  load-bearing, not incidental — full transcript in "Measured numbers".
- Schema/API/contract change requested from another owner: none directly, but see "Known
  limitations" — `EmbeddedRunnerControl::remove_secret`'s asymmetric `enabled` flag
  (`crates/tack-cli/src/local_runner.rs`) is a real bug this card found and routed around
  rather than fixed (out of ownership: not one of the three spec files, not `helpers.ts`).
- Known limitations or `not_measured` fields:
  - Webkit: **not measured here.** `browserType.launch` fails on this machine — `Host system
    is missing dependencies to run browsers ... sudo npx playwright install-deps` (specifically
    `libavif16`), a root-only fix, matching VI-C13's own finding on this same sandbox. Not a
    claim about the product; a fact about this sandbox.
  - `EmbeddedRunnerControl::remove_secret` never resets `provider.enabled` — see above. Left
    as a finding, not fixed (Rust file, out of ownership).
  - `agent-assets.spec.ts` is a live, uncoordinated fourth actor on this exact switch **today**,
    contradicting the card's own framing ("there is no fourth file today") — see "What a
    stranger still cannot do" and "What is left".
  - One single-run anomaly attributable to this shared machine's load, not this mechanism —
    see "Measured numbers".
- Secrets/logging review: no secret introduced. The lock's `holder` file records a PID and a
  timestamp, nothing else. `provider-key-panel.spec.ts`'s existing write-only-contract
  assertion (the pasted key never reaches `page.content()`) is unchanged and still runs.
- Safe merge order and likely conflicts: independent of every other in-flight VI-C card — the
  only files touched are three spec files and an additive block appended to `helpers.ts`.
  `develop` has since taken VI-C15 (`crates/tack-cli/` + `docs/CONFIG.md`, nothing frontend);
  no overlap expected there either.
- Checklist: no unowned files touched (`git status --porcelain` shows exactly the four files
  above), no live secret, no panic stub, no blind retry (the lock is a bounded poll with a
  stale-reclaim ceiling, not an unbounded retry).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The three files no longer race the shared switch under full parallel load | "Measured numbers" — 3(+4) clean repeats after the fix, vs. 3/3 baseline failures on the unmodified files |
| The fix is load-bearing, not incidental | Reverted the whole fix (`git stash`) and re-ran the identical acceptance command: the original race reproduced 3/3 times; restored, reran clean |
| `executionToggleLock` serializes across worker processes and across browser projects, not just within one file | Every repeat below runs `--project=chromium --project=firefox --workers=6` — both browsers' copies of all three tests, six workers, one lock directory |
| The write-only secret contract still holds after the `Remove`→`Replace` change | `provider-key-panel.spec.ts`'s own assertion (`expect(await page.content()).not.toContain(secretValue)`) is untouched and passes in every repeat below |
| A fourth file already exists and does not cooperate with the lock (`agent-assets.spec.ts`) | `grep -n "local-runner" frontend/e2e/agent-assets.spec.ts` → line 167, `PUT /local-runner {enabled:true}` in its own `beforeAll`, confirmed reaching the *same* server as this card's three files under the default config (its own `BASE` falls back to `E2E_API_ORIGIN`, which `playwright.config.ts` sets before any worker spawns) |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

Every command below was run from `frontend/`, with `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C16`.

**Baseline — the three files as `develop` has them today, no lock, full parallel, fresh
`e2e.db` for the first of three:**

```
npx playwright test --project=chromium --project=firefox --workers=6
```

| Run | `execution-toggle` | `agents-page` (both tests) | `provider-key-panel` |
|---|---|---|---|
| 1 | ✓ chromium, ✓ firefox | ✓ chromium ×2, ✓ firefox ×2 | ✘ chromium (30.1s, detached at Save click), ✘ firefox (30.1s, detached at `.fill()`) |
| 2 | ✓ chromium, ✓ firefox | ✓ chromium ×2, ✓ firefox ×2 | ✘ chromium (30.1s, same signature), ✓ firefox |
| 3 | ✓ chromium, ✓ firefox | ✓ chromium ×2, ✓ firefox ×2 | ✘ chromium (30.1s, same signature), ✓ firefox |

`provider-key-panel.spec.ts` failed in **3 of 3** baseline runs — deterministic enough on
chromium (3/3) to call a real, reproducible bug rather than a rare flake, exactly the loading
vs. not-configured ambiguity this card's fix addresses.

**Fixed — the three files as this handoff leaves them, same command, same reused-`e2e.db`
methodology (fresh only for the first of each set of three):**

Set A (done right after finishing the fix, machine otherwise idle):

| Run | `execution-toggle` | `agents-page` (both tests) | `provider-key-panel` |
|---|---|---|---|
| 1 | ✓ chromium (1.7s), ✓ firefox (9.9s) | ✓ chromium ×2, ✓ firefox ×2 | ✓ chromium (3.2s), ✓ firefox (5.6s) |
| 2 | ✓ chromium (3.1s), ✓ firefox (10.4s) | ✓ chromium ×2, ✓ firefox ×2 | ✓ chromium (3.9s), ✓ firefox (11.6s) |
| 3 | ✓ chromium (6.6s), ✓ firefox (9.7s) | ✓ chromium ×2, ✓ firefox ×2 | ✓ chromium (3.3s), ✓ firefox (4.1s) |

Plus a fourth confirmation run, same command, after a final comment-only edit pass: 8/8 clean
again.

Set B (the official post-comment-cleanup re-run, done later — `uptime` at the time showed a
1-minute load average of 12–16 on this shared machine, consistent with the two other cards
this machine's own instructions say are running concurrently):

| Run | `execution-toggle` | `agents-page` (both tests) | `provider-key-panel` |
|---|---|---|---|
| 1 | ✓ chromium, ✓ firefox | ✓ chromium ×2, **✘ firefox** (step 1: `"Running"` never appeared within 15s, total test 30.1s) | ✓ chromium, ✓ firefox |
| 2 | ✓ chromium, ✓ firefox | ✓ chromium ×2, ✓ firefox ×2 | ✓ chromium, ✓ firefox |
| 3 | ✓ chromium, ✓ firefox | ✓ chromium ×2, ✓ firefox ×2 | ✓ chromium, ✓ firefox |

Set B's one failure is a timeout waiting for a state to become *true* ("Running" never
appeared), not an assertion catching the *wrong* state — the signature a coordination miss
would leave (VI-C13's own measured failure was exactly the latter: `"the claimed request is
not this test's own item"`). Set B's own run held no other lock participant active at the
same time (confirmed by re-reading its log), and `uptime`'s 12–16 load average against this
machine's own documented concurrent-card use is the more parsimonious explanation for a real
subprocess (the embedded runner) taking longer than 15s to report itself running. Reported
here rather than discarded, per this repo's own rule that a load-bearing number gets
re-measured rather than assumed — Set A's four clean runs, done when this same command showed
no other load, are the number this card stands behind; Set B is disclosed, not hidden.

Also measured, each with its own command in the diff's own comments:
- `provider-key-panel.spec.ts`'s initial-load race, solo, before the `waitForResponse` fix:
  `npx playwright test -g "saving a key re-probes" --project=chromium --workers=1`, **4 of 10**
  runs failed. After the fix: same command, **10 of 10** passed, and **6 of 6** on
  `--project=firefox`.
- Webkit: `npx playwright test -g "turning agent execution on and off" --project=webkit
  --workers=1` → `browserType.launch` fails, missing `libavif16`, root-only fix. Not measured
  further, per this card's own instructions.

## What a stranger still cannot do

Nothing new is possible from outside this repository — this card repaired test coordination,
not product surface. A stranger who runs `npx playwright test` today sees the same product
behavior as before this card. What changes is narrower: the suite itself no longer produces an
intermittent, un-reproducible-solo failure in `provider-key-panel.spec.ts` under full parallel
load, and the three files that share the execution switch now say, in their own comments,
which fixture keeps them from racing each other.

## Surface-map delta

None — this card touched no product surface. Nothing in §VI.0's surface map moved; every
change here is test-only.

## What is left

- **`EmbeddedRunnerControl::remove_secret` never resets the provider's own `enabled` flag**
  (`crates/tack-cli/src/local_runner.rs`) — only `set_secret` ever sets it. Once any process
  has saved this provider's key, every later `Remove` reports `secret_unresolved`, never
  `not_configured`, for the rest of that server's life. A genuine, reproducible product bug,
  found by this card, not fixed here (a Rust file, outside `frontend/e2e/**` and `helpers.ts`).
  The fix is small and symmetric with `set_secret`'s own logic — worth its own card.
- **`agent-assets.spec.ts` is a live fourth actor on this exact switch, today, not hypothetically.**
  Its own header comment says it wants a *separate* release-build server on port 3311, driven
  with its own `playwright.agent-assets.config.ts` — but nothing in `playwright.config.ts`'s
  `testIgnore` excludes it from the default suite, and `E2E_API_ORIGIN` is set as an
  environment variable before Playwright forks any worker, so its `BASE` fallback silently
  resolves to whatever the *default* config's own webServer is using, not port 3311. Its
  `beforeAll` really does `PUT /api/local-runner {enabled:true}` against the same server this
  card's three files share, before failing for its own, unrelated reason (a missing
  release-build precondition). It does not participate in `executionToggleLock` because it is
  outside this card's ownership. The straightforward fix — adding it to `testIgnore` alongside
  `screenshots.spec.ts`/`hero-gif.spec.ts`/`recovery-demo.spec.ts`, which it already resembles
  in needing a non-default config — is a one-line change to a file this card does not own
  (`playwright.config.ts`). Escalated here rather than fixed.
- Webkit not measured on this machine (missing `libavif16`, root-only fix) — chromium and
  firefox are the only real evidence in this handoff, matching VI-C13's own finding.

## What a fourth file's author sees, and what happens if they don't read this

Nothing forces cooperation — there is no second enforcement mechanism, the same way nothing
stops a spec from skipping any other helper already in `helpers.ts`. What a competent fourth
author actually encounters:

- **They open `helpers.ts` before writing a new spec, because everything else already does.**
  Every spec file in this suite that needs `waitForApp`, `getOrCreateProject`, `enrollRunner`,
  or any of the other dozen helpers imports them from this one file — it is already the place
  this suite's authors look first. `executionToggleLock` sits in it under a named,
  hard-to-miss section comment (search for "execution switch") that states, in the code itself
  rather than a card, exactly which route (`PUT /api/local-runner`) and which observable state
  (`enabled`/`state`/the "Turn on"/"Turn off" label) demand it, and shows the two-line usage.
- **If they skip it anyway** — import `test`/`expect` straight from `@playwright/test`, or
  import the extended `test` but never destructure `executionToggleLock` — their new spec
  becomes exactly what these three were before this card: a fourth uncoordinated driver of one
  server-wide switch, racing whichever of the *other* three (now-locked) files happens to run
  at the same moment for the WHOLE DURATION of ITS OWN critical section — the fourth file's own
  test, unprotected, can still observe or cause the same class of failure this handoff measured
  on the unmodified baseline (3/3 runs): a `Turn on` landing after a sibling's own `Turn off`,
  or a save that lands mid-probe. It will not reproduce by running that one new file alone —
  which is the specific property that made this bug take a full card to first diagnose, and
  will make the fourth file's own version just as hard, unless its author reads this far.
- **The floor under all of this:** even a fourth file that *does* request the lock is only
  coordinated against the three files (and any later ones) that also request it.
  `agent-assets.spec.ts` — a real, present fourth actor today, see above — proves that a file
  outside anyone's ownership boundary at dispatch time can still reach the same switch
  uncoordinated. No fixture, lock, or convention inside `frontend/e2e/**` can compel a file
  nobody is allowed to edit; that gap is closed only by fixing `playwright.config.ts`'s
  `testIgnore` (escalated above), not by anything this card could build.

## Context spent

- Tokens read before the first edit (cold start): card + capsule extracts (~4k), plus
  `docs/agent-handoffs/part-vi/VI-C13.md` in full (explicitly instructed — it named the
  problem), the three owned spec files, `helpers.ts`, `playwright.config.ts`,
  `crates/tack-api/src/handlers/local_runner.rs`, `crates/tack-cli/src/local_runner.rs`
  (including its own unit tests), `frontend/src/features/agents/{AgentsPage,ExecutionToggle,
  ProviderKeyPanel,steps/ProviderStep}.tsx` — read to verify the shared-mutex claim and the
  loading-ambiguity bug against real code before proposing either fix, not from assumption.
- Files opened and not used in the final diff: none discarded outright; two throwaway
  diagnostic spec files (`frontend/e2e/zzz-diag.spec.ts`, a scratchpad copy) were written and
  deleted in the course of reproducing the `provider-key-panel.spec.ts` race with a
  `MutationObserver`/request-log instrumented page — the finding they produced is described
  above; the files themselves are gone (`git status --porcelain` confirms).
- Read-list lines that were wrong: none — no pre-supplied read list for this card beyond the
  card text and cold-start capsule; the VI-C13 handoff's own description of the three files'
  shared switch matched what the code showed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

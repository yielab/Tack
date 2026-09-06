# VI-C20 handoff

- Base SHA / branch / final SHA: base `develop` at `e789435`, branch
  `agent/vi-c20-shared-project-identity`, single commit (not hardcoded here — the hash is
  only fixed once this handoff is itself part of the tree being hashed).
- Files changed:
  - `frontend/e2e/helpers.ts` — `getOrCreateProject` rewritten (see "The mechanism" below);
    new exported `SHARED_PROJECT_NAME` constant.
  - `frontend/e2e/global-setup.ts` (new) — creates/finds the one shared project once,
    before any worker starts, and publishes its id via
    `process.env.E2E_SHARED_PROJECT_ID`.
  - `frontend/playwright.config.ts` — wires `globalSetup: './e2e/global-setup.ts'`.
  - `frontend/e2e/shared-project-identity.spec.ts` (new) — the identity regression guard.
  - `frontend/e2e/run-with-agent.spec.ts` — two changes, both call sites:
    1. The VI-C18-fixed test's own doc comment updated: it used to state the
       ordering leak as an ongoing fact ("so under real concurrency, 'the shared
       project' is ... not a stable identity"); that's no longer true, so the
       comment now says why explicit model selection is still good practice
       without asserting an instability that no longer exists.
    2. **A second, previously unfixed exposure** in `'the run form submits the
       project's configured model default...'` — this test called
       `getOrCreateProject` and then `PATCH`ed `default_model` directly onto
       the result, violating `createFreshProject`'s own documented rule ("never
       the shared one `getOrCreateProject` returns"). VI-C18 only fixed the
       *other* test in this file; this one was untouched and still exposed.
       Now uses `createFreshProject`.
  - `frontend/e2e/agents-page.spec.ts` — removed two calls to `getOrCreateProject`
    that existed purely to prime `existing[0]` ordering ("keeps it ... as
    `existing[0]`" — one had that exact comment). Both are dead weight now that
    identity doesn't depend on ordering; removed the calls, the stale reasoning
    in the comments, and the now-unused import.
  - `docs/agent-handoffs/part-vi/VI-C20.md` (this file).
- Contract fixtures consumed: none. No API/wire shape changed.

## The mechanism, and why this one over the alternatives

The card offered three plausible shapes: a name-scoped lookup, a per-worker project, or a
fixture that creates one and hands it down. I used **global setup + name-scoped fallback**,
which is closest to the third option but resolves it at the Playwright-run level rather
than a per-test/per-worker fixture:

- **Why not per-worker:** the acceptance bar itself ("gets the same id ... while other spec
  files run") wants one identity across the *whole* run, not one-per-worker — dozens of
  spec files in different files (hence different workers under `fullyParallel: true`) all
  need to agree on the same project.
- **Why not a bare name-scoped lookup with no global setup:** two workers racing past "does
  a project with this name exist yet" at the same moment (very plausible at suite start
  under `fullyParallel: true`, when many spec files' first line is `getOrCreateProject`)
  could each decide "no" and each create one — the same bug in a smaller blast radius, now
  keyed on a duplicate name instead of ordering.
- **Why global setup removes the race, not just narrows it:** Playwright runs its
  `webServer` health checks, then any configured `globalSetup`, then forks worker
  processes — all *before* any test file starts (verified against the installed
  `playwright` package's own task ordering, `node_modules/playwright/lib/runner/index.js`).
  `global-setup.ts` runs once, single-threaded, in that window: it ensures the shared
  project exists (or, on a reused `e2e.db`, finds it) and writes its id to
  `process.env.E2E_SHARED_PROJECT_ID`. Every worker Playwright forks afterward inherits
  the main process's env at fork time (`node_modules/playwright/lib/runner/index.js`'s
  `child_process.fork(..., { env: { ...process.env, ...extraEnv } })`) — this is the same
  "pass data from global setup via `process.env`" pattern Playwright's own docs describe
  for auth state, applied here to an id instead of a token.
- `getOrCreateProject` reads that env var first. Its own fallback (env var absent — a spec
  run outside this config, or a future config that stops wiring `globalSetup`) still
  resolves **by the project's fixed name**, never by list position, so even the fallback
  path can't regress to "whichever project sorts first."

The reasoning above is more valuable than the mechanism because the actual defect was never
"there's no shared project" — there always was one, reliably created. The defect was that
*identity* was answered positionally (`GET /api/projects` ordered by `updated_at DESC`,
`existing[0]`), and position is exactly the one thing any concurrently-running test can
perturb by touching *any* project. Fixing identity means answering by something no other
test's activity can move: a name (checked once, centrally, before concurrency starts) plus
an id handed down through process env, never re-derived by position again.

## Two call sites fixed beyond `helpers.ts` itself

Owning "every call site" surfaced two pre-existing exposures that the identity fix alone
would not have caught, both described above under "Files changed":

1. `run-with-agent.spec.ts`'s `'the run form submits the project's configured model
   default...'` test wrote `default_model` straight onto whatever `getOrCreateProject`
   returned. Before this card, "whatever it returned" was itself unstable, so this write's
   damage was diffuse — it could land on any project in the table, not reliably the
   nominal "shared" one, which is part of why VI-C18's own reproduction pinned the leak on
   a *different* test (`scheduler-e2e.spec.ts`'s "unsupported model" test, which already
   used `createFreshProject` correctly). Once identity is stable, an unfixed version of
   *this* test would have reliably and permanently written `default_model` onto the one
   named shared project, on every run, forever (no route exists to clear it) — turning a
   diffuse, occasional problem into a deterministic one. Fixed by switching it to
   `createFreshProject`, matching the established convention.
2. `agents-page.spec.ts` called `getOrCreateProject` twice for a side effect only —
   priming which project would sort first as `existing[0]` for other specs' later calls,
   per its own comment. That reasoning is gone now that identity isn't positional; removed
   both calls as dead weight rather than leave a stale rationale in place.

I did not find a third: a grep across all eight spec files that call `getOrCreateProject`
(`table.spec.ts`, `scheduler-e2e.spec.ts`, `smoke.spec.ts`, `a11y.spec.ts`,
`execution-attempt-detail.spec.ts`, `agents-page.spec.ts`, `run-with-agent.spec.ts`,
`journey.spec.ts`) for any other `PATCH .../projects/:id` call against a
`getOrCreateProject`-sourced id found only the one above
(`grep -n "patch(\`\${API}/projects" frontend/e2e/*.spec.ts` against those eight files).

## How many spec files depended on the old behaviour without knowing it

**6 of the 8** spec files that call `getOrCreateProject` carried no acknowledgment that its
"stable identity" was actually fragile — they called it and trusted the result the way the
name suggests: `table.spec.ts`, `smoke.spec.ts`, `execution-attempt-detail.spec.ts`,
`journey.spec.ts` outright, plus `a11y.spec.ts` and `scheduler-e2e.spec.ts` at most of their
own call sites (`a11y.spec.ts`'s one comment on the subject — "`getOrCreateProject` reuses
one shared project across this whole spec file" — states the assumption as fact, not as
something that could break; `scheduler-e2e.spec.ts`'s "unsupported model" test is the one
exception in that file, already using `createFreshProject` for exactly this reason).

The other **2 of 8** already carried explicit, written awareness of the instability before
this card: `run-with-agent.spec.ts` (VI-C18's own fix, with a comment describing the
`updated_at DESC` mechanism directly) and `agents-page.spec.ts` (deliberately priming
`existing[0]` ordering). Even `run-with-agent.spec.ts`'s awareness was incomplete, though —
see "Two call sites fixed beyond `helpers.ts`" above: the awareness lived in one test in
that file, not the other.

## Tests added and exact commands/results

New test: `frontend/e2e/shared-project-identity.spec.ts` — re-derives the expected id by
querying the API directly and matching on the fixed name (independent of
`getOrCreateProject`'s own internals), and asserts exactly one project ever carries that
name. Chromium-only (`test.skip` on other browsers — identity is browser-independent).

All commands from `frontend/`, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C20`,
`nice -n 19`, port 3399 confirmed free (`ss -ltn | grep 3399`) before every run.

**Baseline — unmodified base branch (`e789435`), fresh `e2e.db` each time**,
`npx playwright test --project=chromium --workers=2`:

| Run | Result |
|---|---|
| 1 | 69 passed, 1 failed — `a11y.spec.ts:442` only |
| 2 | 68 passed, 2 failed — `a11y.spec.ts:192` and `a11y.spec.ts:442` |

**After the fix, same command, fresh `e2e.db` each time:**

| Run | Result |
|---|---|
| 1 | 70 passed, 1 failed — `a11y.spec.ts:442` only |
| 2 | 69 passed, 2 failed — `a11y.spec.ts:192` and `a11y.spec.ts:442` |
| 3 | 70 passed, 1 failed — `a11y.spec.ts:192` only |
| 4 | 69 passed, 2 failed — `a11y.spec.ts:192` and `a11y.spec.ts:442` |
| 5 | 69 passed, 2 failed — `a11y.spec.ts:192` and `a11y.spec.ts:442` |

**Zero identity-driven failures in any of the 7 runs above** (2 baseline + 5 fixed) — every
failure in every run is one or both of the two named `a11y.spec.ts` color-contrast checks.
See "The a11y flake I found and did not chase" below for why runs 3/192 is not this card's
bug either, and why "passes three times consecutively" needed re-measuring rather than
assuming.

**`--repeat-each=3` identity proof, full suite (other spec files running alongside),
`npx playwright test --project=chromium --repeat-each=3 --workers=2`, fresh `e2e.db`:**

```
✓   59 [chromium] › e2e/shared-project-identity.spec.ts:15:1 › the shared project keeps one stable identity while other specs run concurrently (4ms)
✓  130 [chromium] › e2e/shared-project-identity.spec.ts:15:1 › the shared project keeps one stable identity while other specs run concurrently (4ms)
✓  201 [chromium] › e2e/shared-project-identity.spec.ts:15:1 › the shared project keeps one stable identity while other specs run concurrently (18ms)
...
212 passed (1.8m)
1 failed — a11y.spec.ts:442 only
```

All 3 repeats of the identity test passed; the only failure in the 213-test run is the
known `a11y.spec.ts:442` contrast defect.

**Verbatim same-id proof**, run twice against one manually-started, persistent server
(`cargo run -j 4 -p tack-cli -- serve` / `npm run dev`, both left running so
`reuseExistingServer` reused them — no `e2e.db` reset between the two invocations):

- After invocation 1 (`--repeat-each=3`, 213 tests, 1 failed — `a11y.spec.ts:442`):
  `GET /api/projects` → 13 total projects, exactly 1 named `E2E Shared Project`, id
  `5310a698-2bdd-4629-bcea-6d48f11892d2`.
- After invocation 2 (`--repeat-each=3` again, same server/db, 213 tests, **0 failed** —
  both `a11y.spec.ts` checks happened to pass this time): `GET /api/projects` → 25 total
  projects (12 more created by the second invocation's own specs), still exactly 1 named
  `E2E Shared Project`, **same id** `5310a698-2bdd-4629-bcea-6d48f11892d2`, and its
  `updated_at` (`2026-09-06T17:01:39.929463884Z`) **unchanged** between the two — proving
  nothing wrote to it across either run, not just that its id held.

## Failure/adversarial case proved

Rather than chase a live reproduction of the old bug through the full suite's own
concurrency (VI-C18 already did that once, conclusively, and I could not re-trigger it in
2 baseline runs of my own — the specific interleaving it needs is not guaranteed every
run), I proved the mechanism directly against a real server:

```
POST /api/projects {name: "E2E Shared Project", ...}       -> id d27a25a8-...  (the "shared" one)
POST /api/projects {name: "some other concurrent project"} -> id c17d14d6-...  (created after)

GET /api/projects, position 0 (what the OLD `existing[0].id` logic returns):
  -> c17d14d6-...  ("some other concurrent project" — WRONG, this is the leak)

Name-scoped lookup (what the NEW code does, `find(p => p.name === SHARED_PROJECT_NAME)`):
  -> d27a25a8-...  (the actual shared project — correct, independent of creation order)
```

This is the exact mechanism VI-C18's handoff describes (`list_projects`'s
`ORDER BY updated_at DESC` making "the shared project" whichever row a concurrent test most
recently touched), demonstrated directly rather than inferred: the old logic provably picks
the wrong project the moment a second project is touched after the shared one, and the new
logic provably does not.

## The a11y flake I found and did not chase

The card's own framing expected exactly one known failure
(`a11y.spec.ts:442`, "item detail drawer with the dispatch control visible", carded as
VI-C24). Across my measurements, a **second** test — `a11y.spec.ts:192`, "item detail
drawer has no accessibility violations" — also failed intermittently, on **both** the
unmodified base branch (1 of 2 baseline runs) and after this fix (3 of 5 runs), always on
the same `color-contrast` rule family (`#5f736e`-ish muted text near the 4.5:1 threshold,
measured ratios clustering at 3.83–4.43 across the two tests — i.e. right at the boundary,
consistent with small rendering/anti-aliasing variance flipping the measured ratio run to
run rather than a data-dependent difference). Evidence this is not mine and not caused by
this card:

- It reproduces on `e789435` unmodified (baseline run 2 above), at the identical rate order
  of magnitude as after the fix.
- The flagged elements are generic drawer chrome (a "Description" heading, a tab button, a
  toolbar `<select>`) — not anything that depends on which project or item
  `getOrCreateProject`/`getOrCreateItem` happen to return, so it isn't a second identity
  leak in disguise.
- In one fixed run (run 3 above) `a11y.spec.ts:442` itself *passed* while `:192` failed —
  the two tests are trading which one crosses the threshold that run, consistent with one
  underlying contrast defect near the boundary rather than two independent bugs.

Per this same card's own instruction for the unrelated `scheduler-e2e.spec.ts` flake
("record what you saw — do not chase it into another card's territory"), I'm recording this
rather than fixing it: it's a rendering-level WCAG contrast question belonging with VI-C24
(or a sibling card), not an identity question, and no test here was weakened or skipped to
route around it.

- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - Firefox/webkit not run — matches the card's own acceptance bar (chromium only) and
    every other Part VI/VII handoff's finding on webkit in this sandbox.
  - The a11y flake above is reported, not diagnosed to a root line of frontend code — that
    diagnosis is VI-C24's (or a sibling card's) job, not this one's.
- Secrets/logging review: no secret introduced; no logging touched; `global-setup.ts` and
  `helpers.ts` only ever handle a project id and a fixed literal name, both non-secret.
- Safe merge order and likely conflicts: touches `frontend/e2e/helpers.ts`,
  `frontend/playwright.config.ts`, `frontend/e2e/run-with-agent.spec.ts`,
  `frontend/e2e/agents-page.spec.ts`, plus two new files. No other in-flight Part VI/VII
  card is known to touch `global-setup.ts` or `shared-project-identity.spec.ts` (new); a
  card touching `getOrCreateProject`'s signature or `run-with-agent.spec.ts`/
  `agents-page.spec.ts` bodies would be the ones to check first.
- Checklist: no unowned files touched outside the ownership line above (`git status
  --porcelain` shows exactly these files plus this handoff), no live secret, no panic stub,
  no blind retry/wait added to route around a timing issue, no assertion weakened or
  skipped to force a pass.

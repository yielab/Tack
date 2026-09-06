# VI-C31 handoff

**Base SHA / branch / final SHA:** base `develop` at `bd90093`, branch
`agent/vi-c31-refresh-keeps-data`, final SHA recorded at commit time (one commit, no
amend, no rebase).

**Files changed (must equal ownership list):** `frontend/src/shared/execution/store.ts`
(`loadAttempts`, `applyFetchedSummary`, `recordFor`, the `AttemptAvailability` vocabulary,
and three new local helpers: `deepEqual`, `reuseUnchangedAttempts`, `sameAttempts`),
`frontend/src/shared/execution/store.test.ts` (rewrote/added tests for the above),
`frontend/e2e/execution-attempt-detail.spec.ts` (one new E2E proof), `CHANGELOG.md`
(`[Unreleased] ### Fixed` bullet), this handoff. `ExecutionTimeline.tsx` was read but not
edited — the fix did not need it (see "Behavior implemented").

**Contract fixtures consumed:** none — this is a client-side reactivity fix; no wire
contract changed.

**Behavior implemented:** chose **"`loading` only when nothing is already held"** over a
`refreshing` flag on the `ready` variant — the card offered both. A `refreshing` flag was
tried first and does not actually work in this codebase: `RequestRow` (`ExecutionTimeline
.tsx`) reads `attemptsFor(id)` through a plain `createMemo`, not through a `<For>`. SolidJS
memos re-run every dependent computation whenever the tracked value's *reference* changes,
independent of what its `status` field says — so flipping a `refreshing` boolean still
requires constructing a *new* `AttemptAvailability` object, which is exactly as disruptive
as the `loading` transition it was meant to replace (see three rounds of E2E failures
below). The only version of this fix that actually holds is: **write nothing new to
`attemptsCache` for a refresh of already-held data unless the result differs**, so a
consumer's memo sees the exact same reference throughout and never re-renders anything
under it, decision inbox included. `loadAttempts` now:
- Sets `{status: 'loading'}` (and notifies) only when nothing is already `ready` for that
  request — matching the *other* place this same shape existed, `loadList`'s `listStatus`
  (see "What I found about `loadList`/`loadOne`" below).
- On a successful refresh, merges the fresh attempt rows against what's already held
  (`reuseUnchangedAttempts`, keyed by `attempt_id`, structural-equal via `deepEqual`) and
  writes/notifies only if the resulting array actually differs (`sameAttempts`) — a **real**
  content change (e.g. an attempt's `state` advancing) still produces a new object and
  still re-renders; a no-op poll produces nothing.

A second, harder-to-see churn source at the *request* level (not attempts) needed the same
treatment: `ExecutionTimeline`'s `RequestRow` list is rendered through a real `<For>`, whose
reference-based diffing (confirmed against `solid-js/dist/dev.js`'s `mapArray`) means a
brand-new `ExecutionRequestRecord` object on every read — which `recordFor` built
unconditionally, every single call, `fetchedAt: Date.now()` included — looked like "row
removed, different row added" to `<For>`, disposing and recreating the *whole* row
(including whatever `AttemptList`/`DecisionInbox` had mounted under it) on **any** store
mutation, not just an attempts-scoped one. `recordFor` now memoizes per request id and
reuses the prior record wholesale when its `summary`/`error`/`cancellation` are all
unchanged (`cancellationEqual`); `applyFetchedSummary` normalizes a fetched row to
`ExecutionSummary`'s five documented fields and reuses the previously-cached summary
object when the normalized result is structurally identical, via the same `deepEqual`.

**Failure/adversarial case proved:** the E2E proof (see below) failed three separate times
before the final design, each time for a different, real reason — kept here because each
one falsified something I initially believed:
1. First attempt (a `refreshing` flag on `ready`, no other changes): failed —
   `resolveHandle.isConnected` was `false` after the 6 s wait, `expanded` had reset. Cause:
   the `RequestRow`-level churn described above (unrelated to attempts at all — `loadOne`
   refreshes *every* known request on every tick per `executionContext.tsx`'s
   `watchedRequestIds: () => [...store.requests().keys()]`, and `recordFor` rebuilt every
   record every time).
2. Second attempt (added `recordFor` memoization + a `JSON.stringify` equality check for
   both attempts and summaries): still failed. Added temporary `console.log` instrumentation
   (removed before this handoff) and found `applyFetchedSummary`'s `JSON.stringify`
   comparison was **key-order-fragile**: `GET /executions/{id}` (`loadOne`, used by the
   realtime request-scope refresh) serializes an extra `protocol_version` field per object
   that `GET /executions` (`loadList`, used for the initial fetch) does not carry per row —
   comparing the raw payloads read the very first `loadOne` after a `loadList` as "the
   request changed" every time, even though nothing `ExecutionSummary`'s five documented
   fields describe had. Replaced with `deepEqual` (order-independent) and normalized
   `applyFetchedSummary`'s input to exactly those five fields first.
3. Third attempt (both fixes above, `refreshing` flag still in place): still failed.
   Instrumentation showed `recordFor`/`applyFetchedSummary` were now both stable (reused
   every tick), but `attemptsFor()`'s memo still re-ran, because entering the "refreshing"
   state itself wrote a *new* wrapper object (`{status:'ready', data, refreshing:true}`) —
   the same-reference guarantee has to cover the in-flight state too, not just the
   completed one. This is what forced dropping the `refreshing` flag entirely (see
   "Behavior implemented").

After all three fixes (record memoization, `deepEqual` summary/attempt comparison,
dropping `refreshing`), the E2E proof passed 2/2 consecutive full runs; reverting
`store.ts` alone (test files untouched) reproduces both failures below.

**What I found about `loadList`/`loadOne`:** they do **not** have the same failure shape,
but for a reason worth recording rather than assuming. `loadList`'s `setListStatus
('loading')` at its top is unconditional, exactly like the original `loadAttempts` bug —
but nothing in this codebase calls `loadList` on a timer: `ExecutionTimeline`'s own
`createEffect` calls it once per `props.itemId` change, and the realtime `'list'`-scope
tick goes through the *separate* `loadForItems`/`watchItem` path, which — per its own doc
comment — "never touches `listStatus`/`listError`." So `listStatus` never cycles at
runtime today. `loadOne`, however, **is** on the realtime tick (every watched request id,
every 4 s, via `connectRealtime`'s `'request'`-scope handler) and is exactly the mechanism
that exposed the `applyFetchedSummary`/`recordFor` churn above — that part of the finding
was real and is fixed. If `loadList` is ever wired to a timer later, its `setListStatus`
call needs the same "only when nothing is held" treatment `loadAttempts` now has, or the
list-loading flicker returns at the request-history level instead of the attempts level.

**Stop condition — answered, not triggered:** the card asked whether keeping stale data
rendered during a refresh could let a terminal attempt keep showing a live Resolve control
after the server already resolved or expired the decision. It does not, for two
independent reasons: (1) `DecisionInbox` fetches decisions through its own `createResource`
against `GET .../attempts/{n}/decisions`, entirely independent of `attemptsCache` — a
decision's own `pending`/`resolved`/`expired` state is refreshed only by `DecisionInbox`'s
`refetch()` after a resolve, or by that component actually remounting; this fix does not
touch when that refetch happens. (2) Even for the attempt-level data this card *does* own:
the refreshed data always **replaces** the held data once the fetch resolves and content
differs (`sameAttempts` returns `false`, `attemptsCache` is written, `touch()` fires) — the
question reduces to the in-flight window only, and that window is bounded by one HTTP
round trip (bounded further by `realtime.ts`'s 4 s poll cadence), not by any deliberate
holding. Nothing in this change makes stale data persist longer than it already would have
before a decision UI had ever been touched.

**Schema/API/contract change requested from another owner:** none. Noting as a finding,
not a request: `GET /executions/{id}` (`get_execution`) serializes `protocol_version` at
the object level while `GET /executions`'s per-row shape does not — an inconsistency
between the two handlers (`crates/tack-api/src/handlers/executions.rs`), not a documented
part of `ExecutionSummary`. Worked around defensively on the frontend (normalizing to the
five documented fields in `applyFetchedSummary`) rather than changing either handler,
since `ExecutionSummary`'s TS type already describes the contract frontend code should
rely on.

**Known limitations or `not_measured` fields:** none introduced. `deepEqual` is a plain
recursive structural comparison with no cycle detection — safe here because every compared
value is JSON-shaped data from an HTTP response, never a value with cycles.

**Secrets/logging review:** not applicable — no logging paths touched, no secret-shaped
data compared or stored differently than before (`deepEqual`/`JSON.stringify`-adjacent
code only ever sees `ExecutionSummary`/`AttemptSummary` fields, none of which are secrets).

**Safe merge order and likely conflicts:** low conflict risk. `store.ts`/`store.test.ts`
are this card's alone per the board's ownership table; `execution-attempt-detail.spec.ts`
gained one new test appended at the end of the existing `describe` block (no edits to the
other two tests). **This card unblocks VI-C28's merge** (per the board: "Blocks VI-C28's
merge") — VI-C28 should land after this branch.

**Checklist:** no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A pending decision (chosen option, typed-but-unsaved token, expanded detail) survives a realtime poll tick indefinitely | `npx playwright test --project=chromium execution-attempt-detail.spec.ts -g "poll tick during a pending decision"` — 1 passed, ~10s; same command against `store.ts` reverted (test files kept) — 1 failed, `resolveHandle.isConnected` false |
| A refresh of already-`ready` attempt data never transitions through `loading`, and stays the exact same object reference | `npx vitest run src/shared/execution/store.test.ts` — 37 passed; same command against `store.ts` reverted — 2 failed (`a refresh of a ready entry never transitions it through loading`, `a refresh whose content is unchanged leaves attemptsFor() returning the exact same object`) |
| A real state change (attempt state advancing) still re-renders | `store.test.ts`'s `'a refresh with a changed row returns a new ready object, so a real state transition still renders'` — passes with the fix |
| Whole frontend test suite unaffected | `npx vitest run` — 851 passed |
| Whole chromium E2E suite unaffected | `npx playwright test --project=chromium --workers=2` — 77 passed |
| `.githooks/pre-push` still exits 0 | `./.githooks/pre-push` — `✓ pre-push checks passed` (comments, test hygiene, `cargo fmt` ×2, `cargo clippy -D warnings`, generated-file freshness) |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `cd frontend && npx vitest run src/shared/execution/store.test.ts` → 37 passed, ~0.55s
- `cd frontend && npx vitest run` (whole suite) → 851 passed, ~8.3s
- `cd frontend && npm run type-check` → clean (`tsc -b`, no output)
- `cd frontend && npx playwright test --project=chromium --workers=2 --reporter=line execution-attempt-detail.spec.ts` → 3 passed, ~10s (run twice consecutively, both green)
- `cd frontend && npx playwright test --project=chromium --workers=2 --reporter=line` (full suite) → 77 passed, ~37.1s
- Revert proof: `git stash push -- frontend/src/shared/execution/store.ts` (diff confirmed empty via `git diff frontend/src/shared/execution/store.ts`), then:
  - `npx vitest run src/shared/execution/store.test.ts` → 2 failed, 35 passed
  - `npx playwright test --project=chromium --workers=2 execution-attempt-detail.spec.ts` → 1 failed, 2 passed
  - `git stash pop` restored the fix; both suites re-confirmed green afterward.
- `./.githooks/pre-push` (from repo root) → exits 0, ~3s wall (nothing needed rebuilding)

## What a stranger still cannot do

Nothing new is blocked by this card specifically. A stranger who opens the Execution tab
on a real pending decision, picks an option, and starts typing a token can now safely take
longer than 4 seconds to finish and click Resolve — before this fix, waiting past the first
poll tick silently threw away the selection and the typed text and could leave Playwright
(or a real slow human) clicking into a detached radio button.

## Context spent

- Tokens read before the first edit (cold start): the card text (~1.4k tokens from
  `TODO.md`), `store.ts`/`realtime.ts`/`ExecutionTimeline.tsx`/`AttemptList.tsx`/
  `DecisionInbox.tsx`/`store.test.ts`/`execution-attempt-detail.spec.ts`/`helpers.ts` in
  full or near-full — no dispatch-plan block existed for this card (VI-C31 is not listed
  in `docs/agent-handoffs/part-vi/README.md`), so the generic per-card read list from
  `.claude/skills/card/SKILL.md` was followed instead.
- Context size at handoff: moderate-to-heavy — three full rounds of E2E failure diagnosis
  (each requiring re-reading Playwright output, adding/removing temporary `console.log`
  instrumentation, and one direct read of `solid-js/dist/dev.js`'s `mapArray`/`Show`
  source to settle exactly how SolidJS's `<For>`/`createMemo` decide whether to
  re-render) were the dominant cost, not the initial reading.
- Files opened and not used: `AttemptList.tsx`, `DecisionInbox.tsx` were read in full to
  understand what gets destroyed, but neither was edited — the fix lives entirely in
  `store.ts`, one layer below where I initially expected to need to touch.
- Read-list lines that were wrong: the card's own diagnosis ("`ExecutionTimeline` renders
  `AttemptList` only under `<Show when={attempts().status === 'ready'}>`. So every four
  seconds the attempt panel ... is destroyed and rebuilt") is correct about the *symptom*
  but incomplete about the *mechanism* two levels deep — it does not mention that (a) the
  same destruction independently happens at the `RequestRow` level via `<For>`'s
  reference-based diffing regardless of the attempts fix, and (b) a `refreshing` flag
  (the card's own first-listed option) does not work at all given how `attemptsFor()` is
  consumed in this codebase (a plain `createMemo`, not a `<For>`). Both are documented
  above under "Behavior implemented" and "Failure/adversarial case proved" so the next
  reader does not have to re-derive them.

## Proposed board row text

> **VI-C31 — done.** A refresh of an already-`ready` attempt list no longer transitions
> through `loading` — `loadAttempts` writes nothing new to the cache unless the fetched
> result actually differs from what is held (`deepEqual`, keyed by `attempt_id`), and the
> `RequestRow` level needed the identical treatment for the same reason: `applyFetchedSummary`
> normalizes away an undocumented `protocol_version` field `GET /executions/{id}` leaks
> that `GET /executions`'s row shape doesn't, and `recordFor` now memoizes so an unrelated
> mutation elsewhere in the store doesn't rebuild every request row's object and detach
> whatever `<For>` had mounted under it. A `refreshing` flag (the card's other offered
> option) does not work in this codebase — `attemptsFor()` is read through a plain
> `createMemo`, not a `<For>`, so any new wrapper object re-renders its consumer exactly
> like `loading` did, flag or no flag; three rounds of E2E failure before landing on
> "write nothing when nothing changed" are in the handoff's evidence trail. New E2E proof
> passes with the fix, fails 1-for-1 with `store.ts` reverted; `store.test.ts` pins the
> same both ways. Full chromium suite 77/77, `pre-push` green. **VI-C28 can now merge.**

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

**2026-09-06, integrator, at merge.** The projection of `GET /executions/{id}`'s response to
`ExecutionSummary`'s five fields moved out of `applyFetchedSummary` into `executionsApi.get`:
the detail response is its own envelope (it carries `protocol_version` beside the fields,
where the list carries it once around `data`), so which fields a summary has is the API
layer's decision, and the store now compares values that already agree by construction.
Behaviour unchanged; `npx vitest run src/shared/execution` 125 passed, `npm run type-check`
clean after the move. The `refreshing`-flag finding stands as written.

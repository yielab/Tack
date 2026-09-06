# VI-C14 handoff

- Base SHA / branch / final SHA: the dispatch note's stated base (`ff7b786`) was stale by
  one commit by the time this card started — `git merge-base --is-ancestor ff7b786 HEAD`
  on the worktree's pre-existing checkout (`e5206c7`) reported `BASE-STALE` (`e5206c7` was
  actually an *ancestor* of `ff7b786`, several commits behind, not ahead of it). Per
  instruction, the branch was recreated explicitly: `git checkout -b
  agent/vi-c14-board-badge-bound develop`, where `develop` resolved to `25819e0`
  ("docs(board): VI-C15 is in..." — one docs-only commit past `ff7b786`, confirmed via
  `git merge-base --is-ancestor ff7b786 develop` → true). Branch
  `agent/vi-c14-board-badge-bound`, based on `25819e0`; tree was clean before the first
  edit; not committed — working tree only, per instruction.
- Files changed (must equal ownership list): the Owns line names `RunWithAgentButton.tsx`,
  `TestRunStep.tsx`, "the store call behind them," and the handoff. Files actually touched
  beyond the literal sentence: `frontend/src/shared/execution/store.ts` (the store call
  itself — `loadList`), `frontend/src/shared/execution/api.ts` (the client wrapper
  `loadList` calls), and the matching `*.test.ts(x)` files for all three, per the standing
  rule "changing an API response shape [or a client call's shape] updates the matching
  frontend unit/E2E mocks in the same change." One additional file was touched for a
  reason outside that rule: `frontend/src/features/agents/AgentsPage.test.tsx` — its
  fetch mock matched `/api/executions` with `url.endsWith(path)`, which broke once the
  preload started appending `?limit=2000` (the URL no longer ends with the bare path);
  fixed to match on the URL's path alone. `TestRunStep.tsx` itself is unchanged — see
  "Known limitations" for why it was verified, not silently skipped.
- Contract fixtures consumed: none. This card touches `/api/executions` (a plain operator
  read) and its frontend client only; `docs/contracts/runner-v1/**` governs
  `/api/runner/v1`, untouched.
- Behavior implemented: two independent, additive changes to the badge's data path,
  neither of which required a schema or route change:
  1. **`executionsApi.list(itemId?, limit?)`** (`api.ts`) now accepts an optional
     `limit`, forwarded as `?limit=` alongside the existing `?item_id=`. The app-wide
     preload (`store.ts`'s `loadList()` with no `itemId`) now calls
     `executionsApi.list(undefined, EXECUTION_LIST_PRELOAD_LIMIT)` — a new exported
     constant set to `2000`, mirroring `crates/tack-api/src/handlers/executions.rs`'s
     `ListExecutionsQuery::MAX_LIMIT` (re-read at the current SHA to confirm it is still
     200/2000 before relying on it, per the standing rule on inherited numbers) — instead
     of leaving `limit` unset, which the handler defaults to 200
     (`ListExecutionsQuery::DEFAULT_LIMIT`). This raises the row window the shared
     preload covers by 10x, and stays exactly one HTTP request either way (see "Measured
     numbers").
  2. **`store.listMayBeIncomplete(): () => boolean`** — true once the unscoped preload's
     response has come back at exactly `EXECUTION_LIST_PRELOAD_LIMIT` rows (meaning older
     execution requests may still exist beyond what was fetched), false whenever the
     response came back short of that cap (the table is provably covered in full) or no
     unscoped fetch has resolved yet. Never set by an item-scoped `loadList(itemId)`,
     which is already exhaustive for its one item regardless of row count.
     `RunWithAgentButton`'s `latestState` memo now renders an explicit `"Unknown"` chip
     (neutral tone, matching the existing convention for an unrecognized execution state
     in `describeExecutionState`) instead of silently rendering nothing, whenever the
     item is absent from the cache AND `listMayBeIncomplete()` is true — turning a
     misleading false "no activity" into an honest "don't know." Clicking it still opens
     the item's Execution tab, which (per VI-C12) issues its own item-scoped fetch and
     reveals the real state.
  `TestRunStep.tsx` needed no change: it never renders `RunWithAgentButton`'s badge; its
  own `store.requestsForItem(id)` read is for the item it *just created in this session*,
  and `store.create()` already hydrates that exact request into the cache immediately
  (via `loadOne`, keyed by `request_id`, independent of the preload's row cap) before this
  effect ever runs — verified by tracing `create()` in `store.ts`, not assumed.
- Tests added and exact commands/results:
  - `frontend/src/shared/execution/api.test.ts` — 2 new: `list(undefined, limit) calls GET
    /api/executions?limit={n} with no item_id`; `list(itemId, limit) combines both query
    parameters`.
  - `frontend/src/shared/execution/store.test.ts` — 5 new: the preload requests
    `EXECUTION_LIST_PRELOAD_LIMIT` explicitly; a scoped `loadList(itemId)` still passes
    exactly one argument; **the core acceptance proof** — "an item touched before the
    server's DEFAULT_LIMIT (200) of newer requests still shows its real state" (205 rows
    seeded, oldest belongs to `stale-item`, a fake `executionsApi.list` implementation
    that actually applies the requested `limit` the way the real handler's `ORDER BY
    created_at DESC LIMIT ?` does — proven against the *data path*, not a rendered card);
    `listMayBeIncomplete()` true at the cap / false short of it; `listMayBeIncomplete()`
    never set by a scoped call.
  - `frontend/src/shared/runWithAgent/RunWithAgentButton.test.tsx` — 3 new: the "Unknown"
    chip appears when the item is absent from a capped preload; it does NOT appear when
    the preload came back short of its cap and the item genuinely has no executions
    (regression guard distinguishing "unknown" from "none"); **the request-count
    measurement** — 12 `RunWithAgentButton`s (`showStateChip`) sharing one
    `ExecutionStoreProvider` issue exactly one `/executions` fetch, and that one call's
    URL is `/api/executions?limit=2000`.
  - `frontend/src/features/agents/AgentsPage.test.tsx` — no new test; existing mock fixed
    (see above), all its existing tests unchanged and still passing.
  - Full gate: `cd frontend && npm run type-check` — clean. `cd frontend && npx vitest
    run` — **847 passed, 0 failed, 91 files** (baseline before this card, per VI-C12's own
    handoff: 809 passed/90 files; the difference beyond this card's own +10 net new tests
    reflects other Wave 17 cards merged to `develop` since VI-C12, not a discrepancy in
    this card's own count).
- Failure/adversarial case proved: reverted `store.ts`'s `loadList` to the pre-card
  behavior once (`const { data } = await executionsApi.list(itemId);` unconditionally, no
  `EXECUTION_LIST_PRELOAD_LIMIT`, no `listMayBeIncomplete` write) and re-ran the affected
  suites. Exactly three tests failed, each exactly as expected:
  ```
  FAIL  store.test.ts > loadList() (unscoped) asks the server for EXECUTION_LIST_PRELOAD_LIMIT rows instead of leaving limit unset
  AssertionError: expected "vi.fn()" to be called with arguments: [ undefined, 2000 ]
  Received: 1st vi.fn() call: [ undefined ]   (2000 missing)

  FAIL  store.test.ts > an item touched before the server's DEFAULT_LIMIT (200) of newer requests still shows its real state, because the preload asks past that bound
  AssertionError: expected undefined to be 'succeeded'

  FAIL  store.test.ts > listMayBeIncomplete() is true once the unscoped preload comes back at the row cap, false once it comes back short of it
  AssertionError: expected false to be true
  ```
  and, separately, mounting the 12-card screen against the reverted code:
  ```
  FAIL  RunWithAgentButton.test.tsx > a screen of many cards...
  AssertionError: expected '/api/executions' to be '/api/executions?limit=2000'
  ```
  (this one's own `toHaveLength(1)` request-count assertion still passed under the
  revert — proving the *count* claim was never what the fix changed; only the row-cap
  in the URL was). Every other test (844 of 847) stayed green throughout, confirming
  these are the only tests this specific code path drives. Restored the fix and re-ran
  the full suite — 847/847 again.
- Schema/API/contract change requested from another owner: **yes — see "What a stranger
  still cannot do" below for the plain-language version and "Priced: a new route vs. an
  extension" for the full spec.** In short: the badges' full, architecturally-complete
  fix needs a way to ask the server for "the latest execution per item, for a batch of
  item ids" in one request — a capability `GET /api/executions` does not have today (its
  `item_id` field is `Option<Uuid>`, one item only). `crates/tack-api/` is outside this
  card's Owns, so it was not implemented here.
- Known limitations or `not_measured` fields:
  - The shipped fix **raises the practical threshold at which a badge can go stale from
    200 to 2000 execution requests install-wide; it does not eliminate the class of
    bug.** An install that ever accumulates more than `EXECUTION_LIST_PRELOAD_LIMIT`
    (2000) execution requests hits the same gap again, one order of magnitude later —
    at which point `listMayBeIncomplete()` correctly flags it and the badge shows
    `"Unknown"` rather than silently claiming "no activity." The real, size-independent
    fix is the server capability requested above.
  - `listMayBeIncomplete()`'s heuristic ("the response came back at exactly the
    requested cap") has one imprecise edge: if the install has *exactly* 2000 execution
    requests total (no more), the preload legitimately returns all 2000, the flag still
    reports "possibly incomplete," and an item with genuinely no executions would show
    `"Unknown"` rather than nothing — a conservative false positive, never a false
    negative (it never reports "complete" when rows were actually left off). No API
    exists today to disambiguate this boundary case (no total-count header on the
    response) short of the same batch capability requested above.
  - `EXECUTION_LIST_PRELOAD_LIMIT` (2000) is a hand-duplicated mirror of the server's
    `ListExecutionsQuery::MAX_LIMIT`, not derived from any generated contract (it is a
    plain query parameter value, not a schema field) — the two can drift if the server
    constant changes without this one following. Flagged in both the constant's own doc
    comment and here.
- Secrets/logging review: no new secret surface. `limit` is a plain, non-sensitive
  numeric query parameter forwarded exactly like the existing `item_id`; no new log line
  was added.
- Safe merge order and likely conflicts: `RunWithAgentButton.tsx` and `store.ts` are also
  touched by VI-C10 (in-flight, comments only) — every edit in both files here is either
  new code or an entirely new, separately-inserted comment block; no pre-existing comment
  line in either file was reworded, reflowed, or moved (verified via `git diff` on both
  files before finishing: every hunk either changes a code line or inserts new lines
  ahead of an untouched existing comment block, never mid-block). This should keep the
  two diffs textually disjoint. No other Wave 17 card (C5, C6, C7, C8, C9, D2, D1) touches
  any file this card changed, per `docs/agent-handoffs/part-vi/README.md`'s Wave 17
  section.
- Checklist: no unowned files beyond the justified `AgentsPage.test.tsx` mock fix and the
  store/api pair "the store call behind them" already covers; no live secret; no panic
  stub; no blind retry.

## Priced: a new route vs. an extension

The card's brief calls a projection — "latest execution per item, for the items on
screen" — the shape worth pricing first, and asks for both a new-route price and an
extension price before choosing. Both were designed to the same level of detail; neither
was implemented (`crates/tack-api/` is outside this card's Owns).

**Shape A — a new, dedicated route.**
`GET /api/executions/latest?item_id=<uuid>&item_id=<uuid>&...` (axum's `Query` extractor
deserializes a repeated same-named query key into a `Vec<Uuid>` field). Response reuses
the existing `ExecutionListResponse`/`ExecutionSummary` shapes verbatim — one row per
requested item id that has at least one execution (an item with none is simply absent,
matching how `requestsForItem` already treats absence as "no data yet," not padded with
nulls). Query, conceptually: `SELECT id, item_id, state, cancellation_requested_at,
created_at FROM (SELECT *, ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY created_at
DESC, id DESC) AS rn FROM execution_requests WHERE item_id IN (<bound list>)) WHERE rn =
1` — a window function, not `GROUP BY item_id HAVING created_at = MAX(created_at)`,
because two executions for the same item could share a `created_at` value and only the
window function's `id DESC` tiebreak stays deterministic. New axum route, new
`#[utoipa::path]` block, new request/response pair in the generated spec.

**Shape B — an extension of `GET /api/executions` (recommended).**
Add one new optional field to the existing `ListExecutionsQuery`
(`crates/tack-api/src/handlers/executions.rs:351`): `item_ids: Option<Vec<Uuid>>`,
alongside the current singular `item_id: Option<Uuid>` (kept as-is, for
`ExecutionTimeline.tsx`'s existing one-item scoped fetch — no behavior change for that
caller). `list_executions` branches three ways instead of two: `item_ids` present → the
same windowed "latest per item" query as Shape A, ignoring `limit` (the result is already
bounded by `item_ids.len()`, itself bounded by how many items a screen can show —
`ITEMS_MAX_LIMIT`, `economics.rs`); else `item_id` present → today's single-item path,
unchanged; else → today's unscoped path, unchanged. Same URL, same documented route, same
`utoipa::path` block with one more parameter — smaller API surface than Shape A, and
matches the precedent VI-C12 itself set (this same route already grew from
zero-parameters to `item_id` + `limit` in one prior card). **Recommended over Shape A**
for that reason. Both shapes need the same new piece of SQL (dynamic `IN (...)` binding
for a runtime-sized id list, which `sqlx` does not support as a single bound `Vec` for
SQLite — the placeholder string has to be built at the call site); that cost is identical
either way, so it does not favor one shape over the other.

**What this card could and could not prove without it.** Could prove, against the real
constants and the client's actual data path: the preload now asks for 10x more rows in
the same single request (`EXECUTION_LIST_PRELOAD_LIMIT` test); an item pushed past the
server's *default* bound (200) but under the client's new ask (2000) now shows its real
state (the core acceptance test, using a fake that applies the requested limit the way
VI-C12's own backend test proved the real handler does); an item pushed past the *client's*
new ask still gets an honest `"Unknown"` signal instead of false silence; a screen of many
items still issues exactly one request, both before and after. Could **not** prove:
that any item, at any install size, always shows its correct state via exactly one
request — that specific, size-independent guarantee is what Shape A or B would provide,
and proving it needs a real seeded-database test against that capability, which does not
exist yet.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The app-wide badge preload now requests up to 2000 rows (not 200) in one HTTP call | `loadList() (unscoped) asks the server for EXECUTION_LIST_PRELOAD_LIMIT rows instead of leaving limit unset` (store.test.ts) |
| An item-scoped fetch (Execution tab) is unaffected — still one argument, no limit added | `loadList(itemId) (scoped) still asks for exactly one argument, unaffected by the preload limit` (store.test.ts) |
| An item touched before the server's old 200-row default now shows its real state (given the install has under 2000 execution requests total) | `an item touched before the server's DEFAULT_LIMIT (200) of newer requests still shows its real state...` (store.test.ts) — fails on revert, see above |
| An item still missing from the cache once the preload hits its (new, larger) cap shows an honest "Unknown" chip, not silence | `showStateChip renders an explicit "Unknown" chip, not silence, when the item is absent from a preload that hit its row cap` (RunWithAgentButton.test.tsx) |
| An item with genuinely no executions, and a preload that was NOT truncated, still shows nothing (not "Unknown") | `showStateChip still renders nothing (not "Unknown") when the item has no executions and the preload came back short of its cap` (RunWithAgentButton.test.tsx) |
| Pre-existing behavior unchanged: an item with no executions and an untruncated preload renders nothing | `showStateChip renders nothing when the item has no execution requests` (RunWithAgentButton.test.tsx, unmodified, still passing) |
| A screen of many cards still issues exactly one `/executions` request, unchanged from before this card | `a screen of many cards, each with showStateChip, still issues exactly one /executions fetch...` (RunWithAgentButton.test.tsx) — 12 cards, 1 fetch, both pre- and post-fix (measured by reverting once, see "Measured numbers") |
| The runner-v1 wire contract is untouched | No file under `docs/contracts/runner-v1/` or `crates/tack-orch` touched; `cargo nextest -E 'binary(runner_contract)'` not re-run because nothing in its scope changed |
| Generated files (`docs/openapi.json`, `schema.gen.ts`) correctly left untouched | No server-side shape changed; `git diff --stat` shows neither file in this card's diff |

## Measured numbers

- Request count, a screen of items, **before**: `/api/executions` — exactly 1 request,
  for any number of on-screen cards sharing one `ExecutionStoreProvider` (measured by
  reverting `store.ts`'s `loadList` once and re-running `a screen of many cards...` —
  `expect(String(executionCalls[0][0])).toBe(...)` failed with `Received:
  "/api/executions"`; the request-count assertion, `toHaveLength(1)`, still passed).
- Request count, a screen of items, **after**: `/api/executions?limit=2000` — still
  exactly 1 request (same test, current code): `npx vitest run
  src/shared/runWithAgent/RunWithAgentButton.test.tsx -t "a screen of many cards"` — 1
  passed.
- `cd frontend && npm run type-check`: clean, 0 errors.
- `cd frontend && npx vitest run`: **847 passed, 0 failed, 91 files** (10 net new tests
  over VI-C12's own baseline of 809/90 files; the remaining gap between 809+10=819 and 847
  is other Wave 17 cards' own tests merged to `develop` since VI-C12, not this card's).
- `git diff --stat`: 7 files changed, 208 insertions(+), 11 deletions(-).
- `ListExecutionsQuery::DEFAULT_LIMIT` / `MAX_LIMIT`: re-read directly from
  `crates/tack-api/src/handlers/executions.rs` lines 358/360 at this card's base SHA —
  still 200 / 2000, matching VI-C12's own figures (re-verified per the standing rule on
  inherited numbers, not assumed).

## What a stranger still cannot do

A stranger still cannot get a Board/Sprint badge to reliably reflect an item's real
execution state once the install has recorded more than 2000 execution requests overall
without that item having run recently itself — this is the same class of bug VI-C12
flagged at 200, now needing an order of magnitude more volume to reach, and honestly
signaled (an `"Unknown"` chip, not a falsely-empty one) rather than fixed at its root. The
root fix needs a "latest execution per item, for a batch of ids" server capability that
does not exist yet (priced above, requested from whoever owns `crates/tack-api` next). A
stranger also still cannot distinguish, from the badge alone, "this item has 2000 other
requests ahead of it in a bounded preload" from "this item has exactly 2000 other requests
and none of them is its own" — both render `"Unknown"`; only opening the Execution tab
(already scoped, per VI-C12) resolves the ambiguity.

## Surface-map delta

Not applicable — this card is a data-path correctness fix for an existing badge, not a
step in §VI.0's console-to-UI surface map. It touches no console step, no
provider/model onboarding path, and no default-screen vocabulary.

## Context spent

- Tokens read before the first edit (cold start): the card text (given verbatim in the
  dispatch prompt, no re-read needed), `docs/agent-handoffs/part-vi/VI-C12.md` in full
  (169 lines), a `grep` over `docs/agent-handoffs/part-vi/README.md`'s headings (confirmed
  no VI-C14 dispatch block exists there — it predates this card), then the full contents
  of `store.ts`, `api.ts`, `RunWithAgentButton.tsx`, `TestRunStep.tsx`,
  `executionContext.tsx`, `Badge.tsx`, and the relevant slice of
  `crates/tack-api/src/handlers/executions.rs` (query struct + handler body, ~120 lines,
  not the whole file).
- Context size at handoff: moderate — comparable to VI-C12's own "well under any stated
  ceiling" (this card has no dispatch-README block with a numeric ceiling either).
- Files opened and not used: `frontend/src/shared/execution/index.ts` (the barrel) — read
  to check whether `EXECUTION_LIST_PRELOAD_LIMIT` needed re-exporting; decided against it,
  since only `store.test.ts` and `RunWithAgentButton.test.tsx` import it, both directly
  from `./store`/`../execution/store`, matching this file's own existing pattern of
  importing test-only internals directly rather than through the barrel.
- Read-list lines that were wrong: none — there was no dispatch-README block for this
  card to check against; the card text itself (given verbatim) matched what the code and
  VI-C12's handoff described.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

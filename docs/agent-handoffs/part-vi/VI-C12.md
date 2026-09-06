# VI-C12 handoff

- Base SHA / branch / final SHA: base `891ae90` (`develop` tip, confirmed identical —
  `git rev-parse develop` == `891ae90`); branch `agent/vi-c12-executions-filter`; not
  committed — working tree only, per instruction. Note: this worktree's checked-out ref
  was several commits *behind* `891ae90` (an ancestor of it, on an old branch) when this
  card started, not ahead of it like the four stale-base cards the dispatch prompt warned
  about — the branch was recreated explicitly from `891ae90` (`git switch -c
  agent/vi-c12-executions-filter 891ae90`) rather than from whatever the worktree
  happened to have checked out.
- Files changed (must equal ownership list): the literal Owns line names `list_executions`,
  its mount, the regenerated spec, "the matching client call and its mocks", and this
  handoff. Two files beyond that literal sentence were touched — `store.ts` and
  `ExecutionTimeline.tsx` — both required to satisfy the hard rule "take the client past
  the new filter in the same change"; justified in "Files changed, against ownership"
  below.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/**` governs the
  runner↔board wire protocol under `/api/runner/v1`; `GET /api/executions` is a plain
  operator read under `/api`, outside that contract's scope. Confirmed by
  `runner_contract` staying byte-identical: `cargo nextest run --workspace -E
  'binary(runner_contract)'` — 18/18 passed.
- Behavior implemented: `GET /api/executions` now accepts two optional query
  parameters — `item_id` (scopes the result to one item's requests) and `limit`
  (defaults to 200, hard-capped at 2000 even when a caller asks for more; also applies
  to the unscoped case, which previously had no bound at all). The frontend's
  `executionsApi.list(itemId?)` and `store.loadList(itemId?)` both take an optional item
  id and forward it as `?item_id=`; `ExecutionTimeline.tsx` (the item's Execution tab)
  now calls `store.loadList(props.itemId)` in a `createEffect` keyed on `props.itemId`,
  so opening (or switching) that tab issues a real, item-scoped network fetch instead of
  relying solely on the app-wide preload.
- Tests added and exact commands/results:
  - Backend, `crates/tack-api/tests/handlers/executions_runner_admin.rs`: two new tests.
    `list_executions_scoped_to_item_id_excludes_other_items_rows` seeds two items (one via
    `repo.create_item` against the same project `setup()` already creates), two
    executions on the first and one on the second, then asserts `?item_id=<first>`
    returns exactly 2 rows, all with that item's id, and that the unscoped call still
    returns all 3. `list_executions_limit_bounds_the_unscoped_response` seeds 3
    executions on one item and asserts `?limit=2` returns exactly 2 rows.
    `cargo nextest run --workspace -E 'test(list_executions_scoped_to_item_id_excludes_other_items_rows) + test(list_executions_limit_bounds_the_unscoped_response)'`
    — 2/2 passed.
  - Frontend, `frontend/src/shared/execution/api.test.ts`: `list(itemId) calls GET
    /api/executions?item_id={id}, URL-encoded` — asserts the built URL.
  - Frontend, `frontend/src/shared/execution/store.test.ts`:
    `loadList(itemId) forwards the item id to executionsApi.list and still merges into
    the shared cache` — asserts the mock is called with the id and the row lands in
    `getRequest`.
  - Full gate, run after the fix (not during the revert-once check below):
    - `cargo nextest run --workspace` — 1430 passed, 7 skipped (unchanged skip count).
    - `cargo clippy --workspace --all-targets -- -D warnings` — clean.
    - `./scripts/check-comments.sh` — clean.
    - `./scripts/check-test-hygiene.sh` — clean.
    - `cd frontend && npm run type-check` — clean.
    - `cd frontend && npx vitest run` — 809 passed (90 files).
- Failure/adversarial case proved: reverted the `WHERE item_id = ?` clause once (kept the
  `limit` bind, dropped only the item filter) and re-ran the two new backend tests —
  `list_executions_scoped_to_item_id_excludes_other_items_rows` failed exactly as
  expected (`left: 3, right: 2`, with the other item's row named in the panic message as
  having leaked into the scoped response); `list_executions_limit_bounds_the_unscoped_response`
  stayed green throughout, since it never depends on the item filter. Restored the fix
  and re-ran both — 2/2 passed again. This is the same file
  (`crates/tack-api/src/handlers/executions.rs`), never committed mid-experiment.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - `RunWithAgentButton.tsx`'s and `TestRunStep.tsx`'s Board/Sprint-card "most recent
    execution" badges still read only from the app-wide, now-bounded preload
    (`ExecutionStoreProvider`'s unscoped `store.loadList()` on mount) — they were not
    switched to an item-scoped fetch. This is deliberate, not an oversight: that preload
    exists specifically so many Board cards share one fetch instead of issuing one
    network call per visible card (`RunWithAgentButton.tsx`'s own comment: "never a
    second fetch"), and per-card item-scoped fetches would reintroduce exactly the
    N-requests-per-page-load problem this architecture was built to avoid. The
    consequence: if an install ever accumulates more than 200 execution requests
    (`ListExecutionsQuery::DEFAULT_LIMIT`) without touching a given item, that item's
    Board badge could stop reflecting its true most-recent state, while its own
    Execution tab (which now always fetches item-scoped) stays correct. Flagged for
    whoever owns Board-badge correctness next, not fixed here — it is a pre-existing
    correctness gap this card's bound makes newly reachable, not one it introduces from
    scratch (before this card, the same badge read from an *unbounded* preload, so this
    edge case literally could not occur; 200 is a real, if generous, number for a
    pre-1.0 single-operator install).
  - `ListExecutionsQuery::{DEFAULT_LIMIT,MAX_LIMIT}` (200 / 2000) are not configurable via
    any `TACK_*` variable — matches `docs/CONFIG.md`'s existing "every list route is
    bounded, not every bound is user-configurable" pattern (`economics.rs`'s
    `ITEMS_DEFAULT_LIMIT`/`ITEMS_MAX_LIMIT` are the same shape). Not treated as a gap;
    flagging only so a future reader doesn't go looking for a config row that doesn't
    exist.
- Secrets/logging review: no new secret surface. `item_id` and `limit` are plain,
  non-sensitive query parameters; the handler's one error path
  (`"Could not list executions"`) is unchanged and still carries no query string, id, or
  credential beyond what it already logged.
- Safe merge order and likely conflicts: independent of every other Wave 17 card named in
  `docs/agent-handoffs/part-vi/README.md`'s Wave 17 section (D2, D1) — this card touches
  no file either of them owns. `docs/openapi.json` / `schema.gen.ts` are regenerated, not
  hand-merged; whichever card lands last after this one regenerates both from source per
  the standing rule.
- Checklist: no unowned files (see "Files changed, against ownership" below); no live
  secret; no panic stub; no blind retry.

## Files changed, against ownership

| File | Covered by Owns as | Note |
|---|---|---|
| `crates/tack-api/src/handlers/executions.rs` | "`list_executions`... its mount" | `ListExecutionsQuery` (new), its `DEFAULT_LIMIT`/`MAX_LIMIT`, and the handler body; the route's mount line in this same file's `routes()` is unchanged — no `router.rs` edit was needed, since `list_executions`'s signature change (adding a `Query` extractor) doesn't change how or where it's mounted |
| `crates/tack-api/tests/handlers/executions_runner_admin.rs` | implied by owning the handler | Two new tests, described above |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | "the generated spec regeneration" | Regenerated via `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'` then `cd frontend && npm run gen:api`; never hand-edited |
| `frontend/src/shared/execution/api.ts`, `api.test.ts` | "the matching client call and its mocks" | `executionsApi.list(itemId?)`, mirroring `runnersApi.list(fleetId?)`'s existing optional-filter shape in the same file |
| `frontend/src/shared/execution/store.ts`, `store.test.ts` | **not named literally** | `loadList(itemId?)` — the one function that calls `executionsApi.list()` store-side. Without this, `executionsApi.list(itemId)` would exist with no caller able to reach it, which is exactly the "mechanism with no caller" defect the card's hard rule calls out by name |
| `frontend/src/shared/runWithAgent/ExecutionTimeline.tsx` | **not named literally** | The real caller: the item's Execution tab, which is the exact scenario the card's own brief describes ("opening one item's Execution tab pulls the whole table over the wire"). One `createEffect` added; nothing else in the file changed |

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `GET /api/executions?item_id=<id>` returns only that item's rows | `list_executions_scoped_to_item_id_excludes_other_items_rows` — 3 total rows across two items, scoped call returns exactly 2, all matching the id |
| The unscoped call still returns every item's rows (existing callers unaffected by the filter) | Same test — the unscoped call in it returns 3 |
| The response is bounded even when unscoped | `list_executions_limit_bounds_the_unscoped_response` — 3 rows seeded, `?limit=2` returns exactly 2 |
| The filter is load-bearing, not a decoration | Revert-once proof above: removing `WHERE item_id = ?` makes the scoped test fail with the leaked row named in the panic message |
| Opening an item's Execution tab now issues a real, item-scoped fetch | `ExecutionTimeline.tsx`'s `createEffect(() => void store.loadList(props.itemId))`; `store.test.ts`'s `loadList(itemId) forwards the item id to executionsApi.list...` |
| The runner-v1 wire contract is untouched | `cargo nextest run --workspace -E 'binary(runner_contract)'` — 18/18, byte-identical |
| Generated files reflect the change and were not hand-edited | `docs/openapi.json` diff (+22/-1): new `parameters` array (`item_id`, `limit`) and updated `description`; `schema.gen.ts` diff (+5/-2) mirrors it |

## Measured numbers

- `cargo nextest run --workspace`: 1430 passed, 0 failed, 7 skipped.
- `cargo nextest run --workspace -E 'binary(runner_contract)'`: 18/18.
- `cargo nextest run --workspace -E 'binary(openapi_contract)'` (with `UPDATE_OPENAPI=1`): 5/5.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings.
- `cd frontend && npx vitest run`: 809 passed, 90 files.
- `docs/openapi.json` diff: +22/-1 (one new `parameters` array, one updated `description`).
- `frontend/src/shared/api/schema.gen.ts` diff: +5/-2.
- Total diff: 9 files changed, 231 insertions(+), 19 deletions(-) (`git diff --stat`).

## What a stranger still cannot do

A stranger still cannot configure the 200/2000 default and hard-cap bounds — they are
Rust constants, not a `TACK_*` variable, matching the rest of this tree's list routes.
A stranger also still cannot get an item's Board-card "most recent execution" badge to
reflect an execution older than the install's most recent 200 requests without that item
having run recently itself (see "Known limitations" above) — this was already true in a
different, unbounded-but-still-incomplete-for-other-reasons way before this card, and is
unrelated to the item's own Execution tab, which is now always complete.

## Surface-map delta

Not applicable — this card is a data-volume/API-shape fix (an unbounded, unfiltered list
route), not a step in §VI.0's console-to-UI surface map. It touches no console step, no
provider/model onboarding path, and no default-screen vocabulary.

## Context spent

- Tokens read before the first edit (cold start): board extraction (`VI-C12`'s ~26-line
  section, the §VI.0 capsule, §VI.1 rules, §VI.2 ownership table) plus the handler file,
  the frontend store/api/component chain, and the two precedent files
  (`economics.rs`'s `EconomicsItemsQuery`, `tack-core/src/models.rs`'s `ItemFilter`) used
  to match this tree's existing bounded-list-query convention.
- Context size at handoff: moderate — well under any stated ceiling for this card (it
  has no dispatch-README block with a numeric ceiling; Part VI's other numbered ceilings
  apply to dispatch-block cards).
- Files opened and not used: none of substance — every file read (economics.rs,
  models.rs's `ItemFilter`, runner_admin.rs's `ListRunnersQuery`/`runnersApi.list`) fed
  directly into either the query-param naming convention or the frontend optional-filter
  call shape actually shipped.
- Read-list lines that were wrong: none — the card's own `TODO.md` section matched the
  dispatch prompt's paraphrase closely; no correction needed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

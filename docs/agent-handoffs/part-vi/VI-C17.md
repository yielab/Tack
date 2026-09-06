# VI-C17 handoff

- Base SHA / branch / final SHA: the worktree's default checkout
  (`worktree-agent-a22bf9c5e542cb1d1`) was at `e5206c7`, an *ancestor* of the
  card's stated base `495b00e` on a different lineage (`git merge-base
  --is-ancestor 495b00e HEAD` reported `BASE-STALE`). Per instruction,
  recreated explicitly: `git switch -c agent/vi-c17-executions-batch
  develop`, where `develop` resolved to exactly `495b00e` (confirmed via
  `git log --oneline -1 develop`) — the card's stated base, not stale
  relative to it. Tree was clean before the first edit. Not committed —
  working tree only, per instruction.
- Files changed (must equal ownership list): the Owns line names
  `ListExecutionsQuery`/`list_executions` (executions.rs), "the repository
  call behind it" (execution.rs), `EXECUTION_LIST_PRELOAD_LIMIT`/`loadList`
  (store.ts), `executionsApi.list` (api.ts), the generated spec/client
  types, and the handoff. Files touched beyond the literal sentence, each
  required because the new mechanism needs a caller (the same
  justification VI-C12's own handoff used for the identical situation):
  `crates/tack-api/tests/handlers/executions_runner_admin.rs` (new tests
  for the owned handler), `frontend/src/shared/execution/api.test.ts`
  (tests for the owned client change),
  `frontend/src/shared/execution/store.test.ts` (tests for the owned
  store change), `frontend/src/shared/state/executionContext.tsx` +
  `.test.tsx` (the mount-time preload call this card deletes lived here),
  `frontend/src/shared/runWithAgent/RunWithAgentButton.tsx` + `.test.tsx`
  (the actual badge caller — `watchItem`'s only real consumer today),
  `frontend/src/shared/runWithAgent/ExecutionTimeline.tsx` (one stale
  comment referencing the deleted preload, no behavior change — its own
  `loadList(itemId)` call and effect are untouched).
- Contract fixtures consumed: none. `GET /api/executions` is a plain
  operator read outside `docs/contracts/runner-v1/`, confirmed unaffected:
  `cargo nextest run --workspace -E 'binary(runner_contract)'` — 18/18,
  byte-identical.
- Behavior implemented: the badges now ask the server the question they
  mean, and the client's row-cap constant is gone rather than re-derived.
  1. **`GET /api/executions?item_ids=<uuid>,<uuid>,...`** — a new,
     comma-separated batch parameter on the same route
     (`ListExecutionsQuery.item_ids: Option<String>`, not `Vec<Uuid>` — see
     "Escalations" for why), parsed by
     `ListExecutionsQuery::parsed_item_ids` into a `Vec<Uuid>`. When
     present it wins over `item_id`/`limit` and returns **exactly one row
     per id that has at least one execution** — that id's own most recent
     request, never padded with a null row for an id with none. A new
     repository function, `Repository::list_latest_executions_for_items`
     (`crates/tack-db/src/repo/execution.rs`), runs one windowed query
     (`ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY created_at DESC,
     id DESC)`, `id DESC` as the deterministic tiebreak for a shared
     `created_at`) — the exact shape VI-C14's handoff priced as "Shape B"
     and recommended. The existing unscoped/one-item path was extracted
     into a sibling repository function, `Repository::list_executions`,
     matching this crate's existing repository pattern (every other
     `list_*` route already delegates to `tack-db`; this one previously
     built its SQL inline in the handler). A malformed id, or more ids
     than `MAX_LIMIT` (2000, reused — not a new constant), is rejected
     with this route's own contract-shaped `invalid_request` (`{"field":
     "item_ids", "value": "<bad fragment>"}`), never axum's generic
     query-rejection body.
  2. **`store.ts#watchItem(itemId): () => void`** replaces the mount-time,
     install-wide preload entirely. `RunWithAgentButton` calls it in
     `onMount`/`onCleanup` instead of relying on `ExecutionStoreProvider`
     eagerly fetching anything. Every `watchItem` call within the same
     microtask (a Board render mounting a dozen badges at once) coalesces
     into exactly one `?item_ids=` request for the whole set —
     `queueMicrotask`-scheduled, so the batch always reflects every
     registration made in that render pass before the network round-trip
     starts. Reference-counted (two badges watching the same id both need
     their own unwatch before it drops out of the batch). The realtime
     `'list'`-scope tick (`connectRealtime`) now re-asks for exactly the
     currently-watched set on each poll, instead of re-running the deleted
     unscoped call.
  3. **`EXECUTION_LIST_PRELOAD_LIMIT` deleted, not bound to the server
     constant.** The stop-and-check the card asked for came back
     affirmative: once badges ask by id, there is no row cap left to
     mirror — the constant's only reason to exist was the preload it no
     longer powers. `store.ts#loadList` now requires `itemId` (its
     unscoped shape had zero remaining callers once the mount-time preload
     and the realtime tick were both migrated to `watchItem`/
     `loadForItems` — an unscoped `loadList()` would have been exactly the
     "mechanism with no caller" defect this repo's scope-discipline rule
     names). `listMayBeIncomplete()` is gone with it: absence from the
     cache is now conclusive (the batch fetch asked by exact id), so
     `RunWithAgentButton`'s `"Unknown"` chip fallback is gone too, reverted
     to plain silence for "no executions," the pre-VI-C14 behavior — now
     correct rather than a stopgap.
- Tests added and exact commands/results:
  - Backend, `crates/tack-api/tests/handlers/executions_runner_admin.rs` —
    4 new: `list_executions_item_ids_finds_an_item_whose_only_execution_predates_every_other_row`
    (the card's own acceptance scenario — table seeded to `MAX_LIMIT + 5`
    noise rows against a different item, all newer; the target item's
    single, oldest-in-the-table row still comes back correctly by state);
    `list_executions_item_ids_returns_exactly_the_latest_row_per_item`
    (two items, two rows each — batch returns exactly one row per id, the
    newer one, never both); `list_executions_item_ids_takes_precedence_over_item_id_and_limit`;
    `list_executions_item_ids_rejects_a_malformed_id`. `cargo nextest run
    --workspace -E 'test(list_executions_item_ids)'` — 4/4 passed.
  - Frontend, `frontend/src/shared/execution/api.test.ts` — 2 new:
    `list(undefined, undefined, itemIds)` builds `?item_ids=` comma-joined
    and URL-encoded; an empty `itemIds` array omits the parameter entirely
    rather than sending an empty one.
  - Frontend, `frontend/src/shared/execution/store.test.ts` — a new
    `watchItem() / loadForItems()` describe block, 6 tests: coalescing
    (three `watchItem` calls in one tick → one request carrying all three
    ids); the batched response populates the cache keyed by each row's own
    item; an id with no execution stays absent (never conflated with "not
    yet fetched"); unwatch removes an id from the next batch without
    affecting one watched in a later, separate tick; two callers watching
    the same id both need to unwatch before it drops; and the frontend
    mirror of the backend's own acceptance proof — a fake that answers
    per requested id (not a fixed list) proves an item whose only
    execution predates ~2000 noise rows elsewhere still shows its real
    state. `connectRealtime`'s two list-scope tests were rewritten
    (refreshes exactly the watched set; calls the API zero times, not with
    an empty `item_ids`, when nothing is watched) and the unsubscribe test
    was made meaningful again (watches an item first, so detaching is
    actually observable). Five obsolete tests were removed (the unscoped
    `EXECUTION_LIST_PRELOAD_LIMIT` request-shape test, the now-redundant
    "scoped, one argument" test, `listMayBeIncomplete()`'s two tests, and
    the old preload-based acceptance test — superseded by the `watchItem`
    equivalent above).
  - Frontend, `frontend/src/shared/runWithAgent/RunWithAgentButton.test.tsx` —
    the two `"Unknown"`-chip tests were removed (the concept no longer
    exists); the "screen of many cards" test was rewritten to assert the
    one request's URL carries exactly the 12 rendered items' ids via
    `?item_ids=`, and that no `limit` parameter is sent at all.
  - Frontend, `frontend/src/shared/state/executionContext.test.tsx` — the
    "loads the execution list once on mount" test was rewritten: proves
    the Provider fetches nothing on mount, and that a fetch requested
    through one consumer's store handle (`watchItem`) is visible through
    another consumer sharing the same instance — a stronger proof of "one
    shared store" than object identity alone.
  - Full gate:
    - `cargo nextest run --workspace` — 1440 passed, 7 skipped (twice, to
      rule out one observed flake — see "Known limitations").
    - `cargo clippy --workspace --all-targets -- -D warnings` — clean.
    - `./scripts/check-comments.sh` — clean (`crates/` and `frontend/src`).
    - `./scripts/check-test-hygiene.sh` — clean.
    - `cargo nextest run --workspace -E 'binary(runner_contract)'` — 18/18.
    - `cargo nextest run --workspace -E 'binary(openapi_contract)'` — 5/5
      (after regenerating via `UPDATE_OPENAPI=1 cargo nextest run
      --workspace -E 'binary(openapi_contract)'` then `cd frontend && npm
      run gen:api`).
    - `cd frontend && npm run type-check` — clean.
    - `cd frontend && npx vitest run` — 848 passed, 0 failed, 91 files.
- Failure/adversarial case proved: two, both reverted afterward.
  1. **Backend.** Reverted the handler's `item_ids` handling to
     `let item_ids: Option<Vec<Uuid>> = None;` (simulating the parameter
     being silently ignored) and re-ran the 4 new backend tests — all 4
     failed. The acceptance test's failure is the load-bearing one:
     ```
     thread '...list_executions_item_ids_finds_an_item_whose_only_execution_predates_every_other_row'
     panicked at .../executions_runner_admin.rs:718:5:
     assertion `left == right` failed: expected exactly the target item's own row: {"data":[... 200 "noise-*" rows, all newer, the target row absent ...],"protocol_version":1}
       left: 200
      right: 1
     ```
     — with `item_ids` ignored, the handler falls through to the unscoped
     path (its own default `limit`, 200), and the exact bug this card
     exists to fix reappears: the target item's row, genuinely the oldest
     in a table pushed past `MAX_LIMIT`, is truncated away. Restored the
     fix, re-ran — 4/4 passed again (and the full `list_executions` test
     group, 7/7).
  2. **Frontend.** Reverted `loadForItems` to call
     `executionsApi.list()` (unscoped, forwarding no `itemIds`) and re-ran
     the `watchItem`/`loadForItems` block plus the button's "screen of
     many cards" test — 6 failed, each exactly as expected (e.g. the
     coalescing test expected the call to carry `['item_1', 'item_2',
     'item_3']` and received `[]`; the acceptance mirror expected
     `'succeeded'` and received `undefined`). Restored the fix, re-ran —
     34/34 in `store.test.ts`, 8/8 in `RunWithAgentButton.test.tsx`, full
     suite 848/848 again.
- Schema/API/contract change requested from another owner: none — this
  card is the one that implements the change VI-C14 requested from
  whoever owned `crates/tack-api` next. Nothing further requested.
- Known limitations or `not_measured` fields:
  - **A brand-new execution created for an item whose badge is already
    cached, between one `watchItem`-driven batch and the next, is not
    picked up until something re-triggers a batch for that id** (a Board
    item-set change, or the periodic realtime tick, which now refreshes
    exactly the watched set rather than an install-wide guess). This is
    the same class of staleness the pre-VI-C14 unbounded preload always
    had in a different shape (a badge was live-refreshed only by the
    periodic tick's own cadence, ~4s, never instantly); it is not
    introduced by this card, and the realtime tick closes it at the same
    cadence as before for every currently-watched id.
  - `ExecutionTimeline`'s own item-scoped `loadList(itemId)` (the
    Execution tab) is untouched — it still asks for one item's *entire*
    request history, a different question from the batch's "just the
    latest," and was never bounded by `EXECUTION_LIST_PRELOAD_LIMIT` in
    the first place (VI-C12 built it item-scoped-and-unbounded already).
  - `item_ids`, on the wire, is `Option<String>` (comma-separated), not
    `Vec<Uuid>` — a deliberate, measured choice, not a shortcut; see
    "Escalations" below. `ListExecutionsQuery::MAX_LIMIT` doubles as the
    cap on how many ids `item_ids` may carry (rejected above that count),
    reusing the existing constant rather than adding a second one.
  - `tack-cli::embedded_runner_state_scoping::two_servers_on_two_databases_each_see_only_their_own_runner_enrollment`
    failed in 3 of ~5 full-workspace `cargo nextest run --workspace` runs
    during this card's gating, always with `tack serve --with-runner
    exited early during startup: exit status: 1`, and passed 3/3 times
    run standalone (`cargo nextest run --workspace -E
    'test(two_servers_on_two_databases_each_see_only_their_own_runner_enrollment)'`).
    It touches no file this card changed (`grep` for
    `list_executions|ListExecutionsQuery|item_ids` over its test file: no
    match) — a real subprocess-startup flake under parallel full-suite
    load, pre-existing and not introduced here.
- Secrets/logging review: no new secret surface. `item_ids` is a plain,
  non-sensitive list of item UUIDs; the one new error path
  ("item_ids must be a comma-separated list of UUIDs") echoes the
  malformed fragment and the field name only, matching this handler's
  existing `invalid_request` shape for `selector_kind` — no query string,
  credential, or full request body is logged or echoed beyond that one
  field.
- Safe merge order and likely conflicts: this card is the only one in
  Wave 17 that owns `docs/openapi.json`/`schema.gen.ts` (per the card's
  own instruction) — nothing else in this wave should touch either.
  `RunWithAgentButton.tsx`/`store.ts` were also touched by VI-C10/VI-C14
  (both already integrated per the board); every hunk here is either new
  code or a full-block comment rewrite, not a mid-block reword, so a
  textual conflict is unlikely but worth a second look if VI-C18
  (E2E-only, `run-with-agent.spec.ts`) turns out to touch either file —
  it does not, per its own Owns line.
- Checklist: no unowned files beyond the justified caller-side touches
  (table above); no live secret; no panic stub (`.expect()` calls in the
  new repo functions are on this same crate's existing `sqlx::Row::get`
  convention, not a new fallible path); no blind retry.

## Escalations

**Why `item_ids` is a comma-separated string, not `Vec<Uuid>`, contradicting
VI-C14's own Shape B sketch.** VI-C14's handoff assumed "axum's `Query`
extractor deserializes a repeated same-named query key into a `Vec<Uuid>`
field" and priced Shape B on that assumption. It is wrong for this
workspace's exact dependency versions — checked empirically, not assumed,
per the standing rule on inherited claims: `axum 0.8.9`'s `Query` extractor
(`axum-0.8.9/src/extract/query.rs`) deserializes via
`serde_urlencoded::Deserializer`, and `serde_urlencoded::from_str` on a
struct with a `Vec<String>` field fed a repeated key
(`item_ids=a&item_ids=b`) returns `Err("invalid type: string \"a\",
expected a sequence")` — reproduced in an isolated scratch crate against
this exact `Cargo.lock`-pinned `serde_urlencoded` before writing a line of
the real fix. `axum-extra::extract::Query` (backed by `serde_html_form`,
which does support this) would have worked but is not a workspace
dependency, and adding one for a single field is a heavier, less
back-compatible change than encoding the batch as one comma-separated
string and parsing it by hand in the handler — which also, as a side
effect, lets a malformed id return this route's own contract-shaped
`invalid_request` (`{"field": "item_ids", "value": "<bad fragment>"}`)
instead of axum's generic query-rejection body, which the existing
singular `item_id: Option<Uuid>` field does NOT get today (a pre-existing,
unrelated inconsistency, not fixed here). Flagging this for whoever reads
VI-C14's "Shape B" pricing next: the SQL and the precedence rule it
describes are accurate and were reused verbatim; the wire encoding it
sketches is not what shipped.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `GET /api/executions?item_ids=<a>,<b>` returns exactly one row per id, that id's own most recent execution | `list_executions_item_ids_returns_exactly_the_latest_row_per_item` |
| An item whose only execution is the single oldest row in a table pushed past `MAX_LIMIT` (2000+ newer rows elsewhere) still surfaces correctly when asked for by id | `list_executions_item_ids_finds_an_item_whose_only_execution_predates_every_other_row` — fails on revert (see above) |
| `item_ids` wins over `item_id`/`limit` when both are given | `list_executions_item_ids_takes_precedence_over_item_id_and_limit` |
| A malformed `item_ids` entry is rejected with this route's own `invalid_request` shape, naming the bad fragment | `list_executions_item_ids_rejects_a_malformed_id` |
| The existing `item_id`/unscoped/`limit` behavior is unchanged | `list_executions_scoped_to_item_id_excludes_other_items_rows`, `list_executions_limit_bounds_the_unscoped_response` (both pre-existing, still passing) |
| A Board screen with many badges still issues exactly one `/executions` request, now carrying exactly those items' ids instead of an install-wide `limit` | `a screen of many cards, each with showStateChip, still issues exactly one /executions fetch...` (RunWithAgentButton.test.tsx) |
| Watching many items in one render pass coalesces into one batched request | `coalesces every watchItem call in the same tick into exactly one batched request` (store.test.ts) — fails on revert (see above) |
| An item absent from the batch response is genuinely absent (no execution), never rendered as "Unknown" | `an id with no execution stays absent from the cache` (store.test.ts); `RunWithAgentButton.test.tsx`'s unmodified "renders nothing when the item has no execution requests" |
| `EXECUTION_LIST_PRELOAD_LIMIT` no longer exists anywhere in the frontend | `grep -rl EXECUTION_LIST_PRELOAD_LIMIT frontend/src` — no matches |
| The runner-v1 wire contract is untouched | `cargo nextest run --workspace -E 'binary(runner_contract)'` — 18/18, byte-identical |
| Generated files (`docs/openapi.json`, `schema.gen.ts`) reflect the change and were regenerated, never hand-edited | Both diffs shown in "Measured numbers"; regenerated via the documented commands |

## Measured numbers

- Request count, a screen of items, **before** (VI-C14's own figure,
  re-verified — VI-C14.md's "Measured numbers"): exactly 1 request,
  `/api/executions?limit=2000`, for any number of on-screen cards.
- Request count, a screen of items, **after**: still exactly 1 request —
  `a screen of many cards...` (RunWithAgentButton.test.tsx), 12 cards, one
  `/api/executions?item_ids=item-0,item-1,...,item-11` call, no `limit`
  parameter at all.
- Response row count, **before**: up to `MAX_LIMIT` (2000) rows,
  independent of how many items are actually on screen — VI-C14's own
  figure, re-verified directly from `ListExecutionsQuery::MAX_LIMIT` at
  this card's base SHA (still 2000).
- Response row count, **after**: bounded by the number of on-screen items
  requesting a badge — at most 12 for the measured test's screen, never
  more regardless of install size, proven server-side by
  `list_executions_item_ids_returns_exactly_the_latest_row_per_item`
  (2 ids requested, 2 rows returned, never 3+ even though 4 execution
  requests exist in the table).
- Per-row serialized size, re-measured directly (not inherited from
  VI-C14's ~195 B/row estimate): a real `ExecutionSummary` row —
  `printf '%s' '{"request_id":"exec_678aed...(64 hex)","item_id":"<uuid>","state":"queued","cancellation_requested_at":null,"created_at":"2026-09-06T13:12:42.059114405+00:00"}' | wc -c`
  → **236 bytes** (uncompressed, one row, no array braces/commas).
- Worst-case response size, **before**: 2000 rows × 236 B ≈ 460,900 B ≈
  **≈450 KiB**.
- Worst-case response size, **after**, for the measured 12-item screen:
  12 rows × 236 B ≈ 2,832 B ≈ **≈2.8 KiB** — beats VI-C14's own 200-row
  baseline (200 × 236 B ≈ 46 KiB), not just its 2000-row one, and scales
  with items on screen rather than with install size.
- `cargo nextest run --workspace`: 1440 passed, 7 skipped (run twice to
  confirm).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cd frontend && npx vitest run`: 848 passed, 0 failed, 91 files.
- `git diff --stat`: 14 files changed, 745 insertions(+), 204 deletions(-).
- `docs/openapi.json` diff: +21/-1 (one new `item_ids` parameter, one
  updated 200-description, one new 400 response).
- `frontend/src/shared/api/schema.gen.ts` diff: +17/-1.

## What a stranger still cannot do

A stranger still cannot get a Board badge to reflect an execution created
for an already-cached item *between* one batch and the next without
waiting for the next realtime tick (~4s) or an item-set change on the
board — the same staleness window every live-refreshed UI in this app
already has, now scoped to the watched set instead of an install-wide
guess. A stranger also still cannot send a bare, repeated `item_id=` query
key to ask for a batch (`?item_id=a&item_id=b`) the way a strict OpenAPI
"array, form style, exploded" parameter would suggest — this route's
`item_ids` is comma-separated, documented as a plain string in the
generated spec, and a client following the array convention instead would
get a single, malformed id back (`invalid_request`) rather than silent
misinterpretation.

## Surface-map delta

Not applicable — this card is a data-path correctness and efficiency fix
for an existing badge, not a step in §VI.0's console-to-UI surface map.

## Context spent

- Tokens read before the first edit (cold start): the card text (given
  verbatim), `VI-C14.md` and `VI-C11.md` in full, `VI-C12.md` in full
  (all three named in the card's "Read these first"), the full contents
  of `executions.rs` (handler), the relevant slice of
  `crates/tack-db/src/repo/execution.rs` (~200 lines around the listing
  row structs and sibling `list_*` functions), `store.ts`, `api.ts`,
  `RunWithAgentButton.tsx`, `ExecutionTimeline.tsx`,
  `executionContext.tsx`, `dependencies.rs`'s `list_dependencies_for_items`
  (the dynamic-`IN`-clause precedent), and `economics.rs`'s
  `ITEMS_DEFAULT_LIMIT`/`ITEMS_MAX_LIMIT` (precedent for reusing an
  existing bound rather than inventing one). No dispatch-README block
  exists for this card (it postdates Part VI's README dispatch plan,
  which stops at Wave 17's D2/D1) — confirmed via `grep -n "VI-C17"
  docs/agent-handoffs/part-vi/README.md`, no match, so the generic §1
  recipe was followed instead.
- Context size at handoff: moderate-to-large for a single-card diff (14
  files), driven by the frontend architecture change (`watchItem`
  replacing the mount-time preload) needing its own new test block rather
  than a small patch to an existing one — not by re-reading anything
  large repeatedly.
- Files opened and not used: `crates/tack-db/tests/repository/execution_repo.rs`
  (read in full to check whether sibling `list_*` repository functions
  carry dedicated repo-level tests; found they do not — `list_attempts_for_request`/
  `list_events_for_attempt_number` are tested only through the HTTP layer
  in `executions_runner_admin.rs` — so this card's new repo functions
  were tested the same way, matching the existing convention, and no repo-level
  test file was touched).
- Read-list lines that were wrong: VI-C14.md's foundational technical
  claim about axum's `Query` extractor supporting `Vec<Uuid>` via a
  repeated key was wrong (see "Escalations") — caught before implementing
  Shape B literally as sketched, by testing the claim in an isolated
  scratch crate rather than trusting it, per the standing rule on
  inherited claims.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

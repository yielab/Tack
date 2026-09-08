# VIII-A3 handoff

- Base SHA / branch / final SHA: `develop` at `0002136` (the docs-only commit that carded
  VIII-A3 itself; a descendant of the board's recorded `b66ae27`) / `agent/viii-a3-run-readback`
  / not committed — see *Amendments*, this handoff was written against the uncommitted
  working tree by explicit instruction not to commit without the user's own request.
- Files changed (must equal ownership list):
  - `crates/tack-api/src/handlers/orch.rs` — `OrchRunReadbackResponse` + `get_orch_run` handler
  - `crates/tack-api/src/router.rs` — `GET /orch-runs/{run_id}` route registration
  - `crates/tack-api/src/openapi.rs` — path + schema registration for the new handler/type
  - `crates/tack-cli/src/main.rs` — `OrchAction::Run`, `cmd_orch_run`
  - `crates/tack-api/tests/orchestration/reporting.rs` — registers the new test module
  - `crates/tack-api/tests/orchestration/reporting/run_readback.rs` — new, 5 tests
  - `docs/API-REFERENCE.md` — route added to the Runner/Fleet/Execution surface list + a
    worked-description paragraph
  - `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` — regenerated, not hand-edited
- Contract fixtures consumed: none. This route touches no `docs/contracts/runner-v1/` type —
  it reads `orch_runs`, a legacy-bridge table, not the neutral runner-v1 execution domain.
- Behavior implemented: `GET /api/orch-runs/{run_id}` reuses `Repository::get_orch_run` (added
  no second query), gated by `orch_routes`' existing `require_orch_enabled` layer (so
  `TACK_ORCH_ENABLE` off still 409s it) and by the ordinary `require_token` Bearer gate — no
  new privileged token, since this is read-only. `mirrored: false` with every other field
  `null` is a `200`, never a `404`, when no `orch_runs` row exists yet for that id.
  `tack orch run <run_id>` is the CLI caller, paired with `tack orch dispatch`.
- Tests added and exact commands/results:
  `cargo nextest run --workspace -E 'binary(orchestration) and test(run_readback)'` → 5 tests
  run, 5 passed. `cargo nextest run --workspace -E 'binary(orchestration)'` → 147 tests run,
  147 passed (no regression in the surrounding suite). `cargo nextest run --workspace` → 1502
  tests run, 1502 passed, 7 skipped.
- Failure/adversarial case proved: see *Revert proof* below.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced. Every field on
  `OrchRunReadbackResponse` is either always populated (`run_id`, `mirrored`) or `Option`-typed
  and genuinely nullable (unmeasured-is-nullable, not a placeholder zero/success).
- Secrets/logging review: the route logs nothing beyond `#[instrument(skip(state))]`'s own
  span fields (no request body, no headers to skip — it's a bare `GET`). `run_id` is an id,
  not a secret, consistent with the "logs carry ids only" rule.
- Safe merge order and likely conflicts: touches `handlers/orch.rs`, `router.rs`,
  `openapi.rs`, `tack-cli/src/main.rs`, `docs/API-REFERENCE.md`, and the generated files.
  VIII-C2 owns a disjoint file set per the wave README, so no expected conflict. If another
  Wave-26 branch also regenerated `docs/openapi.json`/`schema.gen.ts`, regenerate once more
  at the end per the workspace's generated-file rule rather than trusting either branch's copy.
- Checklist: no unowned files touched, no live secret, no panic stub (`unwrap()`/`expect()`
  used only in test code, matching the surrounding files' own convention), no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An operator can read a dispatched pipeline run's mirrored state by its run id alone, without an item, over Tack's own API | `run_readback_round_trips_a_mirrored_run_with_no_item`, `run_readback_carries_an_attributed_item_id_when_one_exists` |
| The route stays behind `TACK_ORCH_ENABLE` | `run_readback_409s_when_orch_disabled` — `409` with `error.code: "orchestration_disabled"` |
| An un-polled run reports as a legitimate, non-error answer (`200`, `mirrored: false`, every other field `null`) rather than a fabricated `404` | `run_readback_reports_unmirrored_run_as_200_not_404` — asserts `200` plus every field `null` explicitly, not just the status code |
| A guardrail-blocked run's `failed` state surfaces here once mirrored, and the response never claims the run was permitted/allowed/approved | `run_readback_reports_a_failed_run_state_without_calling_it_a_permission_verdict` — asserts the exact field-name set and greps it for those three words |
| `tack orch run <run_id>` is the in-tree CLI caller | `cmd_orch_run` in `crates/tack-cli/src/main.rs`, wired from `OrchAction::Run`; built clean with `cargo build -p tack-cli` |
| The route reuses `Repository::get_orch_run`, no second `orch_runs` query | *Revert proof* below — forcing the handler to ignore the repo call and always answer `mirrored: false` turns 3 of the 5 new tests red |

## Measured numbers

- `cargo nextest run --workspace -E 'binary(orchestration) and test(run_readback)'`: 5 tests
  run, 5 passed, 142 filtered out.
- `cargo nextest run --workspace -E 'binary(orchestration)'`: 147 tests run, 147 passed — the
  pre-existing 142 plus this card's 5, no regression.
- `cargo nextest run --workspace`: 1502 tests run, 1502 passed, 7 skipped (unchanged skip
  count from before this card — the 7 are the pre-existing `#[ignore]`d live-harness tests).
- `docs/openapi.json` diff after `UPDATE_OPENAPI=1 cargo nextest run --workspace -E
  'binary(openapi_contract)'`: 116 insertions, 0 deletions (one new path + one new schema).
- `frontend/src/shared/api/schema.gen.ts` diff after `npm run gen:api`: 112 insertions, 0
  deletions. Ran `gen:api` twice in a row and `md5sum`'d the file both times — identical
  hash (`dc6cca45cd96fa8979f9b3754b451e4e`), proving the regenerated file is stable, not
  still drifting.

## What a stranger still cannot do

Someone arriving from outside this repo still cannot ask Tack for a *list* of recent
pipeline runs — this route answers for exactly one id, by design (the card's "must not"
list rules out a runs browser as a separate decision). They also still cannot get docket's
live verdict faster than Tack's own reconciler poll cadence — this route is a mirror read,
never a live docket call, so a run dispatched seconds ago legitimately reports
`mirrored: false` until the next poll lands.

## rack-cli read

Nothing. This card never touches docket's wire contract — `Repository::get_orch_run`
already exists and reads Tack's own `orch_runs` table, populated out-of-band by the
reconciler (a different card's concern). There was no reason to open `../rack-cli`.

## Revert proof

Command: forced `get_orch_run` in `crates/tack-api/src/handlers/orch.rs` to discard the
repository call and always take the `None` branch —

```rust
let run: Option<OrchRun> = None; // revert-proof: force "never mirrored"
```

Test run: `cargo nextest run --workspace -E 'binary(orchestration) and test(run_readback)'`
went from 5/5 passed to 2 passed / 3 failed. The three that went red:
`run_readback_round_trips_a_mirrored_run_with_no_item`,
`run_readback_carries_an_attributed_item_id_when_one_exists`, and
`run_readback_reports_a_failed_run_state_without_calling_it_a_permission_verdict` — each
failed on an `assert_eq!` expecting the seeded row's real fields (`mirrored: true`, the
attributed `item_id`, `state: "failed"`) and got the forced-null answer instead. The two
that stayed green (`_409s_when_orch_disabled`, `_reports_unmirrored_run_as_200_not_404`)
are exactly the ones that don't depend on a seeded row existing, which is the expected
shape of the proof. Reverted the edit immediately after (`git diff --stat` on the file
showed only additions afterward, confirming a clean restore), then re-ran the same
command: 5/5 passed again.

## Unanswered question

None — §VIII.1 rule 1 did not stop this card; see *rack-cli read* above.

## Context spent

- Tokens read before the first edit (cold start): read the card block (~40 lines), the
  Part VIII README header + VIII-A3 dispatch block (~50 lines), ADR 0065's decisions table
  and "A block is not synchronously observable" section (~50 lines), plus targeted greps and
  line-ranged reads of `handlers/orch.rs`, `repo/orch.rs`, `orch_store.rs`,
  `router.rs`, and one existing test file (`agent_activity.rs`) used as a template. No file
  was read in full; the largest single read was ~200 lines.
- Context size at handoff: well under the dispatch plan's 120k ceiling — this card's read
  list was small and the implementation is one additive handler plus one CLI command.
- Files opened and not used: none — every file read fed directly into either the design
  decision or the test harness.
- Read-list lines that were wrong: none noticed. The dispatch block's read list (board card,
  ADR 0065 decisions 5/7, `list_orch_runs_for_item`, `get_orch_run` + its caller) matched
  what was actually needed; `dispatch_project_pipeline` and `cmd_orch_dispatch` (named in the
  outer task prompt, not the dispatch block) were also read and were directly useful as the
  route/CLI shape to mirror.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

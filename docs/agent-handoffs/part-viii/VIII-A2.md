# VIII-A2 handoff

- Base SHA / branch / final SHA: `884b217` / `agent/viii-a2-dispatch-route` / _(recorded
  at commit time — see the commit this handoff ships in)_.
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/orch.rs` (the
  new `dispatch_project_pipeline` route, `require_dispatch_token`,
  `DispatchProjectPipelineRequest`/`Response`, `DISPATCH_TOKEN_HEADER`),
  `crates/tack-api/src/router.rs` (route registration inside `orch_routes`),
  `crates/tack-api/src/config.rs` (`TACK_ORCH_DISPATCH_TOKEN` → `orch_dispatch_token`),
  `crates/tack-cli/src/main.rs` (`tack orch dispatch`, `OrchAction`),
  `crates/tack-cli/src/client.rs` (`TackClient::post_with_headers` — needed so the CLI can
  send `X-Tack-Dispatch-Token` alongside the ordinary Bearer token; see the note below),
  `docs/CONFIG.md`, `docs/API-REFERENCE.md`, `docs/openapi.json` (regenerated),
  `frontend/src/shared/api/schema.gen.ts` (regenerated),
  `crates/tack-api/tests/orchestration/dispatch/pipeline.rs` (new file),
  `crates/tack-api/tests/orchestration/dispatch.rs` (added the one `mod pipeline;` line
  registering it — additive only), this handoff. One file outside the literal ownership
  list: `crates/tack-api/src/openapi.rs` — see "One file outside the strict ownership
  list" below.
- Contract fixtures consumed: none — `docs/contracts/runner-v1/` is untouched, matching
  ADR 0065's "Wire contract: unchanged" line. This route lives wholly on the legacy
  Docket bridge.
- Behavior implemented: `POST /api/projects/{id}/orch-dispatch` resolves the project's
  existing `orch_links` row (control plane + `remote_project`; `404` if the project
  doesn't exist or has no link — no second way to name a docket project), requires
  `X-Tack-Dispatch-Token` to match `TACK_ORCH_DISPATCH_TOKEN` (fail-closed when unset,
  mirroring `require_approval_token` exactly), then calls
  `DocketAdapter::dispatch(remote_project, variables)` and returns `{remote_project,
  run_id}`. It sits inside `orch_routes`, so `TACK_ORCH_ENABLE` off makes the whole route
  unreachable (`409 orchestration_disabled`). `variables` is an opaque JSON object,
  defaulted to `{}` when the request body omits the field, forwarded to docket verbatim
  and never logged. No `orch_tasks`/`execution_requests` row is written and
  `decide_scheduling_owner` is never consulted. `tack orch dispatch <project>` is the
  CLI caller — it reads `TACK_ORCH_DISPATCH_TOKEN` from the environment (or
  `--dispatch-token`) and refuses locally with a plain message if neither is set, rather
  than sending a request that could only come back `403`.
- Tests added and exact commands/results:
  `cargo nextest run --workspace -E 'test(dispatch::pipeline)'` →
  `10 tests run: 10 passed, 1492 skipped`. New cases in
  `crates/tack-api/tests/orchestration/dispatch/pipeline.rs`:
  `dispatch_409s_when_orch_disabled`, `dispatch_403s_when_dispatch_token_unset`,
  `dispatch_403s_when_dispatch_token_wrong`,
  `dispatch_403s_when_dispatch_token_header_missing`,
  `dispatch_404s_for_unknown_project`, `dispatch_404s_when_project_not_linked`,
  `dispatch_success_returns_run_id_and_writes_no_item_scoped_row` (asserts
  `orch_tasks`/`execution_requests` row counts unchanged, not just a `200`),
  `dispatch_sends_variables_to_docket_verbatim`,
  `dispatch_omitted_variables_default_to_empty_object`,
  `dispatch_variables_never_reach_the_logs` (a self-contained global-subscriber capture
  rig, copied from `runner_protocol/log_capture.rs`'s pattern rather than shared — see
  that module's own doc comment for why a *global* default subscriber, not a
  thread-local one, is what closes the cross-test `tracing` interest-cache race; not
  importable across binaries, and this binary's tests are all plain `#[tokio::test]`,
  the same precondition that makes the pattern safe here too).
  Full workspace: `cargo nextest run --workspace` → `1495 tests run: 1495 passed, 7
  skipped` (one run mid-session showed an unrelated flake —
  `tack-cli::embedded_runner_orphaned_credential::embedded_runner_recovers_a_credential_orphaned_by_a_recreated_database`
  failed once with `"tack serve --with-runner exited early during startup: exit status:
  1"`, then passed both alone and on a full clean rerun; nothing in this card touches the
  embedded runner or its credential store).
- Failure/adversarial case proved: the fail-closed dispatch-token gate — see "Revert
  proof" below. `OrchError::PolicyBlocked` is mapped defensively (mirrors
  `DocketAdapter::dispatch`'s own 400 handling) but not covered by a new test here — see
  "Known limitations" below, same disclosed gap VIII-A1's handoff already recorded for
  that branch.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - `OrchError::PolicyBlocked` from this route is untested (see above) — defensive code
    for a path VIII-A1's own reading of `../rack-cli` found unreachable against a real
    docket `/dispatch/` route today.
  - The response text points an operator at "the reconciler's mirrored run state" for
    the eventual verdict, but as of this card there is **no dedicated operator-facing
    route** that reads an `orch_runs` row by run id when it isn't correlated to a Tack
    item (mine never is — decision 5). See "Question I did not answer" below; this is a
    real, disclosed gap, not something this card's text papers over.
- Secrets/logging review: `TACK_ORCH_DISPATCH_TOKEN` is parsed the same way
  `TACK_ORCH_APPROVAL_TOKEN`/`TACK_EXECUTION_DECISION_TOKEN` already are (skipped in
  every `debug!`/`tracing` call, never included in a response) — no test asserts *that*
  token's absence from logs specifically (none of the three sibling tokens has one
  either; this mirrors existing practice, not a new gap). `variables` **is** covered:
  `dispatch_variables_never_reach_the_logs` proves a secret-marked value never appears in
  captured `tracing` output across the whole request, non-vacuously (the captured text is
  asserted non-empty and to contain the project id, proving the capture rig actually
  observed real production log lines from this request rather than an unreached
  subscriber).
- Safe merge order and likely conflicts: no overlap with VIII-B3 (disjoint files per
  §VIII.2 — it owns `handlers/executions.rs` and
  `tests/orchestration/dispatch/dual_scheduling.rs`, neither touched here). Depends on
  VIII-A1 and VIII-B2, both already merged into `develop` at this card's base SHA
  (`884b217` — verified via `git log --oneline develop | grep -i "viii-a1\|viii-b2"`
  before branching).
- Checklist: no unowned files touched beyond the one noted exception below (which is
  additive registration, not a redesign), no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `POST /api/projects/{id}/orch-dispatch` resolves the docket project from the existing `orch_links` row only | `dispatch_404s_for_unknown_project`, `dispatch_404s_when_project_not_linked` |
| `TACK_ORCH_DISPATCH_TOKEN` is fail-closed when unset, and checked against the request header, not the ordinary API token | `dispatch_403s_when_dispatch_token_unset`, `dispatch_403s_when_dispatch_token_wrong`, `dispatch_403s_when_dispatch_token_header_missing` |
| The route is unreachable with `TACK_ORCH_ENABLE` off | `dispatch_409s_when_orch_disabled` |
| No `orch_tasks`/`execution_requests` row is written, and `decide_scheduling_owner` is never consulted | `dispatch_success_returns_run_id_and_writes_no_item_scoped_row` (row-count assertions, not a status code) |
| `variables` reaches docket verbatim, defaulting to `{}` when omitted | `dispatch_sends_variables_to_docket_verbatim`, `dispatch_omitted_variables_default_to_empty_object` |
| `variables` never reaches the logs | `dispatch_variables_never_reach_the_logs` |
| The response never claims the run was permitted, only that it started | see the handler's own doc comment and response shape (no `status`/`outcome` field) — `dispatch_success_returns_run_id_and_writes_no_item_scoped_row` asserts those keys are absent from the body |
| Reverting the token check fails exactly the three token-gate tests | see "Revert proof" below |

## Measured numbers

- `cargo nextest run --workspace -E 'test(dispatch::pipeline)'` →
  `10 tests run: 10 passed, 1492 skipped`.
- `cargo nextest run --workspace -E 'binary(orchestration)'` →
  `140 tests run: 140 passed, 0 skipped`.
- `cargo nextest run --workspace -E 'binary(openapi_contract)'` (no `UPDATE_OPENAPI`, run
  after regeneration) → `5 tests run: 5 passed, 0 skipped` — proves the committed
  `docs/openapi.json` matches the live spec, not just that regeneration ran once.
- Full workspace: `cargo nextest run --workspace` → `1495 tests run: 1495 passed, 7
  skipped` (on the clean rerun; see the flake note above).
- `.githooks/pre-push` → `✓ pre-push checks passed` (run with everything staged — the
  hook's own "generated files are current" step compares the working tree to the index,
  so this must be run staged or after a commit, not against an unstaged working tree, or
  it reports every new file as "stale" even when it is not).

## What a stranger still cannot do

Trigger a docket pipeline run from the UI, or attach one to a Tack item — this card is a
CLI-only, item-free trigger by design (ADR 0065 decisions 2, 5; "Must not: build a UI,
attach the run to a Tack item"). More concretely: a stranger who runs `tack orch dispatch
<project>` still cannot ask Tack "did that run succeed?" through any route — the only
place the outcome becomes visible is `orch_runs`, mirrored by the reconciler on its own
schedule, and nothing in this tree exposes that table for a run with no correlated item.
See "Question I did not answer."

## Context spent

- Tokens read before the first edit (cold start): `docs/agent-handoffs/part-viii/README.md`
  header + the VIII-A2 block; `TODO.md` §VIII.1, §VIII.2, and the VIII-A2 card body (via
  `grep -n` + `sed -n`, not the whole file); the full text of
  `docs/adr/0065-docket-pipeline-dispatch-trigger.md`; `require_approval_token` and
  `orch_routes` with their doc comments; `DocketAdapter::dispatch`'s trait doc and
  implementation; `dispatcher.rs`'s `dispatch_item`/`build_control_plane` for the
  project→link→control-plane resolution pattern; `handlers/orch.rs`'s
  `get_orch_link`/`get_orch_budget`/`decide_approval` for response and error-mapping
  conventions; `crates/tack-cli/src/main.rs` and `client.rs` for the CLI command/HTTP-client
  shape; `crates/tack-api/tests/orchestration/dispatch/item.rs` and `dual_scheduling.rs`
  and `reporting/approvals.rs` for the test harness pattern;
  `runner_protocol/log_capture.rs` for the redaction-test rig. No token estimate was given
  in the dispatch prompt to compare against.
- Context size at handoff: mid-size single-card session, no compaction needed.
- Files opened and not used: `crates/tack-orch/src/reconciler.rs`'s `poll_runs`/
  `persist_runs`/`RunSource` were read to answer "what does 'the reconciler /runs poll'
  actually mean today" (see "Question I did not answer") but nothing there was edited —
  reading it, not using it as a caller, is what surfaced the gap.
- Read-list lines that were wrong: none — the dispatch prompt's read order
  (README block → TODO.md extract → ADR 0065 whole → named code) matched what was
  actually needed.

## What I read in `../rack-cli`

Nothing new beyond what VIII-A1's handoff already recorded and ADR 0065 already quotes
verbatim (the `/dispatch/` branch of `serve.py`'s `do_POST`, and the "answers before the
pipeline runs" fact). This card's own read there was a spot check, not new research:
confirmed `_check_auth()` is docket's own auth gate on `/dispatch/{project}` (unrelated to
and layered independently under Tack's `TACK_ORCH_DISPATCH_TOKEN` — the two tokens never
interact), and that the response body's `run` field is the only field this adapter
consumes (`project`/`status` are present on the wire but unused, matching
`DispatchResponse { run: String }` in `docket.rs`). Nothing was changed in this tree
because of this reading — it confirmed the existing adapter's contract rather than adding
to it. Repository: `/home/ox/Sites/rack-cli`, read-only, no writes or commits made there.

## Revert proof

```
$ sed -n '2939p' crates/tack-api/src/handlers/orch.rs
    require_dispatch_token(&state, &headers)?;
$ sed -i '2939s/.*/    \/\/ TEMP REVERT PROOF: require_dispatch_token(\&state, \&headers)?;/' \
    crates/tack-api/src/handlers/orch.rs
$ cargo nextest run --workspace -E 'test(dispatch::pipeline)'
```

Result: `10 tests run: 7 passed, 3 failed` — exactly the three token-gate tests, nothing
else:

```
FAIL tack-api::orchestration dispatch::pipeline::dispatch_403s_when_dispatch_token_header_missing
FAIL tack-api::orchestration dispatch::pipeline::dispatch_403s_when_dispatch_token_wrong
FAIL tack-api::orchestration dispatch::pipeline::dispatch_403s_when_dispatch_token_unset
```

Each failed on the same assertion — the response came back `404` (the commented-out
guard skipped straight to project resolution, whose test fixtures leave the project
unlinked in two of the three cases and the token check no longer intercepts the third
before it) instead of the expected `403`. The call site
(`require_dispatch_token(&state, &headers)?;`) was then restored verbatim and the same
command re-verified `10 tests run: 10 passed, 1492 skipped`.

## One file outside the strict ownership list

The dispatch prompt's owns list names `handlers/orch.rs` (route), `router.rs`
(registration), `config.rs` (token), `tack-cli/` (CLI), the two docs, the two generated
files, and one new test case. It does not name `crates/tack-api/src/openapi.rs` — but
every existing orchestration route (`dispatch_item`, `decide_approval`, etc.) is
registered by hand in that file's `ApiDoc::paths(...)`/`components(schemas(...))` macros,
and `docs/openapi.json` only picks up a new route if it's listed there — the regeneration
command reads the annotations `openapi.rs` assembles, it doesn't discover routes from
`router.rs` on its own. I added `handlers::orch::dispatch_project_pipeline` to `paths(...)`
and `DispatchProjectPipelineRequest`/`Response` to `components(schemas(...))` — the same
two-line addition every prior route in this module has required, immediately followed by
regenerating both generated files with the documented commands (never hand-edited). No
other card in Wave 25 or the closed Wave 24 touches this file's orch-route entries, so
this is treated as necessary machinery for the owned route, not a boundary violation — but
it is flagged here explicitly per the "if you believe you need a file outside your list,
write the request in your handoff" instruction, since the list did not name it.

## Question I did not answer

Whether Tack should gain an operator-facing route to read a mirrored `orch_runs` row by
run id — today `state.repo.get_orch_run(run_id)` exists at the repository layer but is
only ever called internally by the reconciler's own store (`orch_store.rs`), never
through any handler. `get_project_agent_activity`/`get_item_agent_activity` both read
`orch_tasks`/item-correlated `orch_runs`, neither of which this route's run ever
populates (decision 5: no item id, so `correlate_remote_task` in
`reconciler::persist_runs` has nothing to match against). ADR 0065's "the reconciler
`/runs` poll is... the only place the verdict ever becomes visible to Tack" is true of
the *ingestion* mechanism (docket → `orch_runs`, which already runs today, unconditionally,
for every linked project) — it does not by itself mean an operator can *read* that verdict
anywhere yet. I did not build that read route: it is not in this card's owns list, ADR
0065 does not decide it, and "no new ingestion path" (decision 7, "Must not: add a second
ingestion path") reads adjacent enough to a new *read* surface that deciding "this is
fine, it's just a read" felt like exactly the kind of surface-widening §VIII.1 rule 1 says
to stop on rather than improvise. The response text and CLI output are worded to be
honest about this rather than pointing at a route that doesn't exist — "becomes visible in
Tack once the reconciler's next poll mirrors it" is true of the database row, not of any
API surface today. Whoever picks this up next (plausibly VIII-C2, which already owns "the
Verified live" section of `docket.rs`, or a future card) should treat this as an open
question, not a settled "no."

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

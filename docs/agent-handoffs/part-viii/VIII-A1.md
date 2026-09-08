# VIII-A1 handoff

- Base SHA / branch / final SHA: `abfd24b` / `agent/viii-a1-dispatch` / `cc2a2ae`
- Files changed (must equal ownership list): `crates/tack-orch/src/adapters/docket.rs`
  (the `dispatch` method, the "Write methods" module-doc paragraph, and the `dispatch: true`
  comment in `capabilities()` — see the note on that last one below), `crates/tack-orch/src/lib.rs`
  (the `dispatch` trait doc), `crates/tack-orch/tests/docket_adapter_test.rs`,
  `crates/tack-orch/tests/docket_wire_contract_test.rs` (new cases, plus its module doc's
  now-stale "dispatch — no request" section), `crates/tack-orch/tests/golden/wire/dispatch.json`
  (regenerated). No file outside this list.
- Contract fixtures consumed: none from `docs/contracts/runner-v1/` — docket is a control
  plane adapter, not a runner-v1 wire type.
- Behavior implemented: `DocketAdapter::dispatch(project, vars)` POSTs `vars` verbatim as
  the JSON body to `/dispatch/{project}`, Bearer-authed, and returns docket's `run` id as
  `Ok(String)`. 401/403 → `Auth`, 404 → `NotFound`, a 400 naming a guardrail policy →
  `PolicyBlocked` (via `parse_policy_block`), any other 400 or non-2xx → `Http`.
- Tests added and exact commands/results:
  `cargo nextest run --workspace -E 'binary(docket_adapter_test) or binary(docket_wire_contract_test)'`
  → `55 tests run: 55 passed, 0 skipped`. Six new cases in `docket_adapter_test.rs`
  (`dispatch_happy_path_returns_the_run_id`, `dispatch_sends_vars_as_the_request_body_verbatim`,
  `dispatch_unauthorized_maps_to_auth_error`, `dispatch_404_maps_to_not_found`,
  `dispatch_bad_request_without_guardrail_wording_maps_to_http_not_policy_blocked`,
  `dispatch_bad_request_with_guardrail_wording_maps_to_policy_blocked`) replace the old
  `dispatch_is_still_disabled`; one rewritten case in `docket_wire_contract_test.rs`
  (`dispatch_wire_contract`, now mounting a mock and asserting a real request/response
  golden instead of an empty one). Full workspace:
  `cargo nextest run --workspace` → `1481 tests run: 1481 passed, 7 skipped`.
- Failure/adversarial case proved: `dispatch_bad_request_without_guardrail_wording_maps_to_http_not_policy_blocked`
  — a 400 whose message doesn't name a guardrail policy (docket's real `VariableError` shape)
  must map to `Http`, not be swept into `PolicyBlocked` just because the status code matches
  `enqueue_task`'s block case.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the "guardrail wording" 400 branch
  (`dispatch_bad_request_with_guardrail_wording_maps_to_policy_blocked`) is defensive —
  reading `../rack-cli`'s source shows this path is currently unreachable against a real
  docket server (see "What I read in `../rack-cli`" below). It is kept because Acceptance 3
  requires `parse_policy_block` to be reused for this mapping, and because a future docket
  build could start reporting a block this way without this adapter's classification
  silently breaking.
- Secrets/logging review: no new log line. The Bearer token is attached the same way every
  other write method already does (`bearer_auth`, never logged); `vars` (arbitrary
  caller-supplied JSON) is never logged either — only forwarded as the request body.
- Safe merge order and likely conflicts: no overlap with VIII-B1/B2/C1 (disjoint files per
  §VIII.2). VIII-A2 depends on this branch merging first — it calls `ControlPlane::dispatch`
  from a new route this card does not add.
- Checklist: no unowned files touched, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `ControlPlane::dispatch` triggers a real docket pipeline dispatch and returns a run id | `dispatch_happy_path_returns_the_run_id`, `dispatch_wire_contract` (golden: `tests/golden/wire/dispatch.json`) |
| `vars` reaches docket as the literal request body, not wrapped or reshaped | `dispatch_sends_vars_as_the_request_body_verbatim` |
| Docket auth/not-found failures map the same way every other write method's do | `dispatch_unauthorized_maps_to_auth_error`, `dispatch_404_maps_to_not_found` |
| A dispatch-route 400 is *not* assumed to be a guardrail block the way `enqueue_task`'s is | `dispatch_bad_request_without_guardrail_wording_maps_to_http_not_policy_blocked` |
| `parse_policy_block` is reused, not reimplemented, for the one shape that would be a block | `dispatch_bad_request_with_guardrail_wording_maps_to_policy_blocked`; no second parser function exists in `docket.rs` |
| Reverting the method body to `Err(OrchError::Disabled)` fails exactly the new coverage | see "Revert proof" below |

## Measured numbers

- `cargo nextest run --workspace -E 'binary(docket_adapter_test) or binary(docket_wire_contract_test)'`
  → `55 tests run: 55 passed, 0 skipped`.
- `cargo nextest run --workspace` (full suite) → `1481 tests run: 1481 passed, 7 skipped`.
- `.githooks/pre-push` → exits 0 (`✓ pre-push checks passed`), covering `check-comments.sh`,
  `check-test-hygiene.sh`, `cargo fmt --all --check` (workspace and `tack-desktop`
  separately), `cargo clippy --workspace --all-targets -- -D warnings`, and the lockfile/
  generated-file freshness check.

## What a stranger still cannot do

Attach a docket pipeline run to a Tack item, or trigger one from the UI or CLI — this card
only implements the adapter method itself; nothing in this tree calls it yet (`ControlPlane::dispatch`
still has zero callers after this change, same as before). That caller is VIII-A2, gated on
ADR 0065's acceptance, and is explicitly out of this card's scope (§VIII.4, "Must not").

## Context spent

- Tokens read before the first edit (cold start): read `docs/agent-handoffs/part-viii/README.md`
  header + VIII-A1 block, `TODO.md` §VIII.0–§VIII.4 (~230 lines via `sed`, not the whole
  file), root `CLAUDE.md`, the full module doc and trait section of `docket.rs`/`lib.rs`,
  and `../rack-cli/src/docket/serve.py`'s `do_POST` (all three write branches, for shape
  comparison) plus the relevant slice of `core/dispatch.py`/`core/pipeline.py`. No estimate
  was given in the dispatch prompt to compare against.
- Context size at handoff: mid-size single-card session, no compaction needed.
- Files opened and not used: none outside the read list — `core/dispatch.py`'s
  `dispatch_pod` (line ~2319) was located by `grep` but not read line-by-line; its behavior
  needed for this card (that it runs off the request thread) was already established by
  `serve.py`'s `do_POST` calling it inside `threading.Thread(target=_run, daemon=True)`.
- Read-list lines that were wrong: none — the dispatch prompt's read order matched the
  actual file layout.

## What I read in `../rack-cli`

Repository: `/home/ox/Sites/rack-cli`, `main` branch, `cbea12a` at the time of reading
(read-only; nothing written or committed there).

- **`src/docket/serve.py`, `do_POST`, the `/dispatch/` branch (~line 1047–1104).** This is
  the whole contract. Confirmed: `_check_auth()` gate (401, same as `/tasks/` and
  `/approvals/`); the request body is read as raw JSON, rejected with 400 if invalid or not
  an object; it is resolved against the pod's pipeline via
  `core.pipeline.resolve_variables`, which raises `VariableError` → 400 on a bad variable;
  a run record is created (`core.runs.create_run("webhook", project, variables=variables)`)
  and its id returned as `{"ok": true, "run": "<id>", "project": ..., "status": "dispatched"}`
  — **before** the pipeline is dispatched. The actual dispatch
  (`core.dispatch.dispatch_pod`) runs inside `threading.Thread(target=_run,
  daemon=True).start()`, called after the response is already queued to send.
  **This is what the adapter and both trait/module docs now say directly**: the run id can
  come back before the dispatched work runs, and any `pre_input` guardrail evaluation inside
  `dispatch_pod` cannot reach this HTTP response — it happens later, off the thread the
  handler returns from.
- **`src/docket/core/dispatch.py`, `_enqueue_pre_input_gate` and `effective_pipeline`
  (~line 303–390, ~478–517).** Confirmed the `pre_input` gate
  (`_enqueue_pre_input_gate`) is called only from `enqueue_task` — nothing in the
  `/dispatch/` code path (`effective_pipeline`, `resolve_variables`, `create_run`,
  `dispatch_pod`'s entry point) calls it before the HTTP response is sent. This is the
  finding Acceptance 3 asked for: **docket's dispatch route does not report a `pre_input`
  block the way its task route does — it doesn't report one synchronously at all.** The
  200-response shape (`{"ok": true, "run": ..., "status": "dispatched"}`) is unconditional
  once the body/variables validate; a block can only ever surface later, as the dispatched
  run's own failed state via `GET /runs/{id}` or `/traces/{project}`, neither of which this
  method touches.
- **What changed because of this reading:** the 400 branch of `dispatch` does not
  unconditionally call `parse_policy_block` the way `enqueue_task`'s does (which would be
  correct for `enqueue_task`, where `DispatchError`'s two causes really are only "no pod" —
  handled by 404 — or a block). Instead it checks the extracted message for the same
  `"guardrail policy"` wording before calling `parse_policy_block`, and otherwise maps to
  `Http`. Without this reading I would have mirrored `enqueue_task`'s 400 handling
  unconditionally and misclassified every real dispatch-route 400 (bad JSON, a non-object
  body, an unresolved variable) as a guardrail block that never happened.
- **`src/docket/serve.py`, `_check_auth`/`_send_json_error`.** Confirmed the auth check and
  the `{"ok": false, "error": "..."}` error-body shape are identical to every other
  authenticated route this adapter already calls — no new error-body parsing needed;
  `ErrorBody`/the inline extraction pattern from `enqueue_task` is reused as-is.

## Revert proof

```
$ git diff crates/tack-orch/src/adapters/docket.rs   # saved the full method+doc diff first
$ # reverted only the `dispatch` method body to:
    async fn dispatch(
        &self,
        _project: &str,
        _vars: serde_json::Value,
    ) -> Result<String, OrchError> {
        Err(OrchError::Disabled)
    }
$ cargo nextest run --workspace -E 'binary(docket_adapter_test) or binary(docket_wire_contract_test)'
```

Result: `55 tests run: 48 passed, 7 failed`. The seven failures were exactly the new/changed
coverage and nothing else:

```
FAIL tack-orch::docket_adapter_test dispatch_happy_path_returns_the_run_id
FAIL tack-orch::docket_adapter_test dispatch_sends_vars_as_the_request_body_verbatim
FAIL tack-orch::docket_adapter_test dispatch_bad_request_with_guardrail_wording_maps_to_policy_blocked
FAIL tack-orch::docket_adapter_test dispatch_unauthorized_maps_to_auth_error
FAIL tack-orch::docket_adapter_test dispatch_404_maps_to_not_found
FAIL tack-orch::docket_adapter_test dispatch_bad_request_without_guardrail_wording_maps_to_http_not_policy_blocked
FAIL tack-orch::docket_wire_contract_test dispatch_wire_contract
```

`dispatch_wire_contract`'s failure was a golden mismatch: the reverted adapter sent zero
requests and returned the old `Disabled` error, which is exactly the previous golden's
shape — proving the golden itself is now load-bearing, not just descriptive.

The method body (and only the method body — the docs and struct additions were never
touched) was then restored from the saved diff and the full suite re-verified green
(`cargo nextest run --workspace` → `1481 tests run: 1481 passed, 7 skipped`;
`.githooks/pre-push` → `✓ pre-push checks passed`).

## The `capabilities()` comment — one line outside the strict ownership list

`capabilities()`'s `dispatch: true` field carried a comment that literally said
`ControlPlane::dispatch` (the trait method) "always returns `OrchError::Disabled`" — true
before this card, false after it. Leaving it would have put a demonstrably wrong statement
three lines below code this same card changed. I corrected that one comment (still inside
`docket.rs`, still about the same `dispatch: true` line) to describe both routes docket
answers "can this plane accept new work" with, rather than widening scope to the rest of
`capabilities()`. Flagging it explicitly since it is not in the literal ownership list
("the `dispatch` method and the 'Write methods' paragraph").

## Question I did not answer

None — §VIII.1 rule 1 did not stop this card. The one place the card anticipated a decision
(Acceptance 3's "if docket's dispatch route reports blocks differently... that is a
finding") was answered by reading, not by a design choice: it doesn't report one
synchronously at all, and that's stated as fact above, not decided.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

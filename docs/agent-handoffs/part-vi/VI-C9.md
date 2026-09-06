# VI-C9 handoff

**Base note: this worktree started on the wrong branch.** `git log --oneline -1` at dispatch
showed `worktree-agent-a1aa43746c9cde641` at `e5206c7` (414 files diverged from `develop`,
completely unrelated content — a leftover from a different task), not `0bde141`. Recreated
explicitly: `git checkout -b agent/vi-c9-auto-truth 0bde141`, confirmed clean
(`git status --porcelain` empty before the first edit) and `git merge-base --is-ancestor
0bde141 HEAD` succeeds. Every claim below is against that recreated branch.

**Verdict: both directions closed, and the honest fix needed no new server route.** The
explicit-model gate (`isCombinationSupported`) never consulted `model_passthrough`, so it
disabled the only combination that actually works on either bundled harness. The Auto gate
had no way to tell "will schedule" from "will queue forever" because it never looked at the
one thing that decides it — whether any tier (agent profile, project, fleet) resolves a
default model — even though the modal already fetches all three tiers' raw data for other
fields. Both fixes are pure, additive, and stay entirely inside
`frontend/src/shared/runWithAgent/**` and `frontend/src/shared/execution/capabilities.ts`
(the file the dispatch block itself named as the fix target). No server route was needed;
"Stop if" does not apply.

- Base SHA / branch / final SHA: base `develop` at `0bde141` (recreated, see above), branch
  `agent/vi-c9-auto-truth`, final SHA not committed by this agent (card rule: no commit/push
  /merge/rebase).
- Files changed (matches ownership: `runWithAgent/**` plus the one `execution/` file the
  dispatch block itself named, plus the e2e spec/helper this card's own acceptance bar
  names):
  - `frontend/src/shared/execution/capabilities.ts` — `isCombinationSupported` now mirrors
    `crates/tack-orch/src/scheduler/select.rs`'s `!declared && !passthrough` arm exactly.
  - `frontend/src/shared/execution/capabilities.test.ts` — four new tests for the
    passthrough branch (unlocks an undeclared pairing, `advisory` doesn't count, a probe
    error doesn't count, no double-counting when a runner both declares and attests).
  - `frontend/src/shared/runWithAgent/shared.ts` — `parseModelDefaultConvention` and
    `resolveAutoModelPolicy` (a client-side mirror of `crates/tack-orch/src/model_policy/
    mod.rs`'s precedence walk over data the modal already fetches); `gateHarnessModelSelection`
    rewritten to consult it; `CombinationGate` gained an optional `fix` action.
  - `frontend/src/shared/runWithAgent/shared.test.ts` — the two contradicted tests VI-C7
    flagged are gone; new tests for the passthrough branch, both Auto outcomes
    (`unresolved`/`pinned_auto`), the `fix` field's tier-dependent presence, and the
    `resolveAutoModelPolicy` precedence walk (including the "a tier pinned to literal auto
    stops the walk" nuance).
  - `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` — a `autoModelResolution` memo
    feeding the gate; the gate's own capability input switched from the whole runner
    population to `targetCapabilities()` (see "A second bug found live" below); the model
    fieldset renders the `fix` link when present.
  - `frontend/src/shared/runWithAgent/RunWithAgentModal.test.tsx` — `PROFILE` now carries a
    matching default so existing Auto-submission tests keep proving what they always meant
    to; two new tests for the unresolved and pinned-auto blocks, including the fix link's
    exact href.
  - `frontend/e2e/scheduler-e2e.spec.ts` — see "A second, larger finding" below: this file
    was already broken on the base commit, for reasons unrelated to this card, before any of
    my edits.
  - `frontend/e2e/helpers.ts` — one addition, `setProjectDefaultModel`, needed by the
    rewritten "unsupported model" scenario.
  - `docs/agent-handoffs/part-vi/VI-C9.md` (this file).
- Contract fixtures consumed: none — no `docs/contracts/runner-v1/` fixture touched, no
  scheduler code touched.
- Behavior implemented: see "Verdict" above and "Claim → evidence" below.
- Tests added and exact commands/results:
  - `cd frontend && npx tsc -b` → clean.
  - `cd frontend && npx vitest run` → **807 passed** across 90 files (re-run three times
    across this session, identical result each time).
  - `cd frontend && CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C9 npx playwright test
    e2e/scheduler-e2e.spec.ts --project=chromium --project=firefox` → **8/8 passed** (4
    tests × 2 browsers), ~10.3s combined. Re-run three additional times on chromium alone
    (isolating from cross-file flakes) — 4/4 every time.
  - `cargo nextest run --workspace` (CARGO_TARGET_DIR as above) → **1428 passed, 7 skipped**
    (the skipped are the pre-existing `#[ignore]` live-harness tests), 14.837s.
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean, zero warnings.
  - webkit: attempted, fails identically to V-C4's own documented finding (`ldd`-confirmed
    missing system libraries this sandbox cannot install without root) — not this card's
    regression, not measurable here either.
- Failure/adversarial case proved: yes, three ways, all live against a real server + a real
  `tack-runner` process (not the fake-client `enrollRunner`/`claimOnce` HTTP simulation) —
  see "Live proof" below.
- Schema/API/contract change requested from another owner: none. The three tiers'
  raw data (`AgentProfileSummary.limits`, `Project.default_model`, `FleetSummary.
  default_policy`) were already fetched by this modal for other fields; `resolveAutoModelPolicy`
  is a pure client-side mirror of the server's own precedence walk over that same data, so no
  resolution-preview endpoint was needed (VI-C7's amendment named this as the harder of two
  honest paths — turned out to be reachable without a schema change after all, because the
  modal's own existing fetches already carry every tier's raw value).
- Known limitations or `not_measured` fields:
  - The `fix` link only ever points at Project Settings → Agents, because that is the *only*
    tier with a settings UI today (§VI.0's own evidence table: "Agent-profile default model
    … no UI field"). When `pinned_auto`'s culprit tier is `agent_profile` or `fleet`, the
    gate still names the correct tier in its reason text but carries no `fix` link — there is
    nowhere in the UI to send the operator. Confirmed in `shared.test.ts`'s "pinned_auto at
    the agent-profile or fleet tier carries no fix" test.
  - `harnessProbeStatus` (`shared/execution/capabilities.ts`) lost its only caller in this
    change (it drove the Auto gate's old, now-retired advisory) and is unused in production
    code as of this branch. Not deleted — its own file and tests are outside this card's
    named ownership, and removing an orphaned pure function is a smaller decision than fixing
    the two live bugs this card exists for; flagged here rather than acted on
    unilaterally (scope discipline: escalate, don't widen).
  - `GET /api/executions` ignores its own `item_id` query parameter entirely (confirmed by
    reading `crates/tack-api/src/handlers/executions.rs::list_executions`: no `Query<>`
    extractor, no `WHERE item_id = ?`, unconditionally returns every request in the system).
    Found by accident while writing this card's own live-proof script, not touched — a
    different owner's endpoint, unrelated to the gate.
  - Two **pre-existing** E2E breakages found while making `scheduler-e2e.spec.ts` pass, not
    caused by this card and not fixed by it (out of ownership): `a11y.spec.ts:1106` ("item
    detail Execution tab (with a real request)") still drives the old, pre-VI-C2
    "Fleet"/free-text-"Runner id" target picker and the un-collapsed Repository fields;
    reproduces in isolation, 100% of the time, unrelated to this card's changes (confirmed:
    `git diff` never touches the target-picker markup). `agents-page.spec.ts`'s step-5 test
    is flaky only when run in the same parallel batch as many other spec files (passes 100%
    of the time alone) — a pre-existing cross-file race the test's own comment already
    half-anticipates, not this card's to fix either.
- Secrets/logging review: no secret introduced or logged. The live-proof runner's enrollment
  token was generated fresh against a disposable scratch SQLite DB/state dir under this
  session's own scratchpad (never the repository's `tack.db`), and every process that touched
  it (the scratch `tack serve`, `tack-runner`, and Vite dev server) was killed at the end of
  this session — confirmed via `ps aux` showing none running afterward.
- Safe merge order and likely conflicts: touches `RunWithAgentModal.tsx`/`shared.ts` and
  `capabilities.ts` — VI-C10 (frontend comment cleanup, next on the board) will very likely
  touch the same files for comment citations; **this card should merge first** so VI-C10's
  sweep runs over the final text once, not twice. No other in-flight Part VI/VII card is
  known to touch these files per `TODO.md`'s own conflict notes.
- Checklist: no unowned files beyond what's justified above; no live secret (confirmed); no
  panic stub (no new `unwrap`/`unimplemented!`); no blind retry (no retry logic added or
  changed).

## A second bug found live, not by reading

`combinationGate`'s own capability input was `capabilities()` — every active runner in the
whole system, not `targetCapabilities()` (the selected runner, or fleet members) that every
sibling memo in this same file (`modelCombos`, `targetHarnessCapability`, `passthroughAttested`)
already correctly scopes to. This was harmless before this card's fix, because a false match
required two *different* runners to declare the identical provider/model pair. It became a
live, reproducible flake the moment `isCombinationSupported` started honoring
`model_passthrough` (this card's own fix): **any** active runner anywhere in the system
attesting `codex`/`model_passthrough: supported` — including this repo's own embedded runner,
left active from an earlier local `agents-page.spec.ts` run against the same persistent
`e2e.db` — made every unrelated explicit `codex` request everywhere read `Supported`,
regardless of what the *actually selected* target declared. Caught by `scheduler-e2e.spec.ts`'s
own "unsupported model" test failing nondeterministically across repeated local runs (green
the first two times, red the third, for a reason that had nothing to do with the test's own
logic — the log showed `Supported … forwards an operator-chosen model verbatim`, not
`Unsupported`). Root-caused via the request's own accumulated `GET /runners` state, fixed by
scoping the gate's input to `targetCapabilities()` (falling back to `props.capabilities()`
first, unchanged, so the test-injection override this file's own tests rely on keeps working
exactly as documented). Re-ran `scheduler-e2e.spec.ts` three more times after the fix with zero
further flakes. This is a real, independent correctness bug this card's own acceptance bar
exposed and that this card's own ownership (`RunWithAgentModal.tsx`) covers fixing.

## A second, larger finding: `scheduler-e2e.spec.ts` was already broken, for reasons unrelated to Auto

Before any of this card's own edits, all four of this file's tests failed at
`fillExactRunnerTarget`'s very first line: `modal.getByLabel('Exact runner').check()` — that
radio, the free-text "Runner id" field, and the "Choose a model" label do not exist in the
current UI. `git log -S"combined machine/group picker"` traces this to `35f9ab8` ("feat
(frontend): stop the run-with-agent modal asking for five typed fields", VI-C2) — already an
ancestor of this card's own base commit (`git merge-base --is-ancestor 35f9ab8 0bde141`
succeeds). VI-C2 replaced the separate "Exact runner"/"Fleet" radios and typed "Runner id"
field with one combined "Machine or group" picker, collapsed the Repository fields behind a
"Change for this run" toggle, and renamed the "Choose a model" radio to "Choose…" — and this
spec file (written earlier, for the pre-VI-C2 shape) was never updated to match. This card's
own acceptance bar names this exact file as something that "must pass, unweakened" — a broken
helper is not a weakened assertion, so bringing the driving mechanics current is squarely
required, not scope creep. Fixed:
- `fillExactRunnerTarget` now drives the combined picker (`selectOption('exact_runner:' +
  runnerId)`, skipped when the picker is hidden because exactly one runner is active — the
  same auto-select case `RunWithAgentModal.test.tsx` already unit-tests) and clicks "Change
  for this run" before touching the now-conditionally-rendered Remote/Base revision fields.
- Every `getByLabel('Choose a model')` → `getByLabel('Choose…')` (the real current label,
  verified byte-for-byte against `RunWithAgentModal.tsx`'s own JSX).
- The "Model" dropdown's options are keyed by array index into the *target's own declared*
  `model_combinations` (`RunWithAgentModal.tsx`'s `modelCombos()`), not by a `model_profiles`
  id — that mechanism (`POST /api/model-profiles`) is the same one `.claude/scope-discipline.md`
  already names as consulted by nothing; this modal never read it even before this card. The
  three tests that pick the target's own single declared model now select index `'0'`
  directly, and no longer call `createModelProfile` at all.
- The "unsupported model" test's entire premise — select a model the target genuinely
  doesn't support — has no dropdown path any more (the list only ever shows what the target
  itself declares, or a free-text override unlocked only by `model_passthrough`, which this
  fixture never sets). Rewrote it to use the one tier the current UI *can* still reach an
  undeclared explicit pair through: a fresh project's own default model
  (`setProjectDefaultModel`, new in `helpers.ts`, using `createFreshProject` per that
  function's own doc comment — never the shared `getOrCreateProject` project, since
  `default_model` has no clear-route-back). The test still proves exactly what it always
  proved: a genuinely unsupported combination is named `Unsupported` and blocks submission —
  just reached through the UI surface that still exists for it.

## Live proof (real server, real `tack-runner`, real button clicks)

Machine: this worktree's own sandbox, Linux 6.14.0-37-generic x86_64. Build:
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C9 cargo build -p tack-cli -p tack-runner`
(debug profile — functional proof, not an asset-quality recording, so release/embed-spa was
not needed). Own scratch DB/storage/state, own port (`TACK_PORT=3316`), never the
repository's `tack.db`. Fake `claude`/`codex` shim binaries (same technique as
`scripts/smoke.sh` and `frontend/e2e/fixtures/harness-shims/`) so the harness subprocess
completes deterministically; the *adapter* code that reports capabilities and drives the
subprocess is 100% real, unmodified production code — only the vendor binary is a shim. UI
driven by a throwaway Playwright script (deleted after this run; not part of this diff) against
a real Vite dev server proxying to the scratch API; every dispatch below went through the
actual "Run" button, never a direct `POST /executions`.

The real, embedded-adapter capability report (`GET /api/runners`, live, both harnesses):
```
"harness_kind": "codex",        "model_combinations": [], "model_passthrough": {"support": "supported", ...}
"harness_kind": "claude-code",  "model_combinations": [], "model_passthrough": {"support": "supported", ...}
```
— confirming both bundled harnesses ship exactly the empty-`model_combinations`-plus-attested
-passthrough shape this card's whole fix is about, from the real adapters, not a fixture.

**Case 1 — explicit model, harness attests `model_passthrough: Supported`, submits through
the button, reaches a completed attempt.** Harness Codex (default), mode "Choose…" → "Other
(type a model id)" (unlocked because the target attests passthrough) → provider `openai`,
model id `gpt-5-codex`. Badge read **Supported** before the click. Clicked **Run**.
- Request `exec_18e9b0db6b7d6cf1330c232d4ad05f75edb3906821c3be67c63421b9874511fc`
- `created_at` `2026-09-06T05:42:59.379794309+00:00` → attempt leased
  `05:42:59.388733904+00:00` (9ms) → attempt `state: succeeded`,
  `ended_at 05:42:59.445870580+00:00`.
- `actual_execution`: `harness_kind: codex`, `model_provider: openai`, `model_id:
  gpt-5-codex`, `model_observation_source: requested_not_confirmed` (passthrough — codex
  doesn't echo the model back); `model_provenance.kind: matched`.
- `terminal_reason.stdout.text_preview`: `"vi-c9-live-proof-ok\n"` — the shim's own marker,
  proving the real adapter actually spawned the real (fake) binary through the real
  `tack-runner` process this dispatch created.

**Case 2 — no default model at any tier, Auto cannot be submitted, and the dialog names the
fix.** Fresh project, fresh agent profile, neither with any `default_model`/`limits`
convention; fleet not involved (exact-runner auto-selected, the only active runner). Left on
"Auto (let the runner decide)" — never touched. Result, read directly off the live DOM:
- Badge: **Unsupported**.
- Reason (verbatim): *"No agent profile, project, or fleet default model is configured for
  this target, and no runner can attest it safely accepts an unspecified model — this
  request would queue forever and never run. Choose an explicit model below, or set a
  default model."*
- Fix link present, text *"Set a default model for this project"*, `href`
  `/projects/1dd18e26-31e7-44dd-9f3b-be0c80b7f379/settings?tab=agents` (that project's own
  id — confirmed exact match).
- **Run** button: `disabled` — confirmed via `expect(...).toBeDisabled()`, not inferred.

**Case 3 — a tier names an explicit model, the same (Auto) dispatch still submits and still
runs.** A different fresh project, its own `default_model` set to `{kind: explicit,
provider: openai, model_id: gpt-5-codex}` via `PATCH /api/projects/{id}` (the same route
`AgentsPanel.tsx`/`ModelDefaultStep.tsx` write). Opened the modal, selected the agent profile
— **never touched the model-mode radios**; "Project default — openai / gpt-5-codex" was
already checked by the pre-existing auto-select effect. Badge read **Supported**. Expanded
Repository, filled a real disposable one-commit git fixture
(`/…/scratchpad/vi-c9-live/repo`, rev `42eea8ecbf812b2b4fcecb79224bff18d6020473`). Clicked
**Run**.
- Request `exec_079fc8303cf523135f0aa52e4cbefc73e980d089ee826177327eb543532de791`,
  `created_at 2026-09-06T05:49:57.417214052+00:00` → `leased` → **`succeeded`**.
- Confirms both halves of this card's acceptance line at once: the wire body this "Project
  default" mode sends is still `requested_model_provider: null, requested_model_id: null`
  (III.1.2's "Auto" shape — the *client* never needs to resolve it, only decide whether
  *to allow submitting it*), and the *server's own* resolution (unmodified,
  `crates/tack-orch/src/model_policy/mod.rs`) independently reaches the same explicit pair
  and schedules it — this card's frontend-side `resolveAutoModelPolicy` and the server's
  `resolve_model_policy` agreeing is exactly the property the whole fix depends on.

## Claim → evidence

| Claim | Evidence |
|---|---|
| `isCombinationSupported` never consulted `model_passthrough` before this card | `git diff` on `capabilities.ts`; `capabilities.test.ts`'s new "an undeclared pairing is supported when the harness attests model_passthrough: supported" test failed against the pre-change function (verified by reverting the fix locally before committing to it) |
| The fix mirrors the scheduler's own arm exactly, not a looser approximation | `crates/tack-orch/src/scheduler/select.rs:149-174`'s `declared`/`passthrough` logic vs. `capabilities.ts`'s `declaredHere`/`harness.model_passthrough?.support === 'supported'` — read side by side |
| An explicit model on a passthrough-attesting harness submits through the button and reaches succeeded | Live proof, case 1: request `exec_18e9b0db6b7d6cf1330c232d4ad05f75edb3906821c3be67c63421b9874511fc`, `succeeded`, stdout `vi-c9-live-proof-ok` |
| An install with no default model anywhere cannot submit Auto, and is told why + how to fix it | Live proof, case 2: `Run` button `disabled`, exact reason text and fix `href` captured above |
| A tier naming an explicit model still lets the identical Auto dispatch submit and run | Live proof, case 3: request `exec_079fc8303cf523135f0aa52e4cbefc73e980d089ee826177327eb543532de791`, `succeeded`, model mode never touched by hand |
| A tier explicitly pinned to literal `"auto"` is a distinct, correctly-named block (not silently treated as unresolved) | `shared.test.ts`'s `resolveAutoModelPolicy` precedence tests + `gateHarnessModelSelection`'s `pinned_auto` tests |
| The false "the scheduler will still validate at claim time" string is gone from the tree | `grep -rn "scheduler will still validate at claim time" frontend/` → only the regression test's own negative assertion (`shared.test.ts:240`, `.not.toMatch(...)`) |
| `scheduler-e2e.spec.ts`'s four tests pass, unweakened | `npx playwright test e2e/scheduler-e2e.spec.ts --project=chromium --project=firefox` → 8/8; re-run 3× chromium-only with zero flakes after the `targetCapabilities()` fix |
| The gate's own capability scope was a second, independent bug | Reproduced the flake three separate times against the accumulating local `e2e.db` (an active embedded runner's real passthrough attestation leaking into an unrelated request's gate check); fixed by scoping to `targetCapabilities()`; zero further flakes across 4 additional full test-file runs (3× chromium-only, 1× chromium+firefox combined) |
| `cargo nextest run --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are unaffected | 1428 passed / 7 skipped; clippy clean — no Rust file was touched by this card |

## Measured numbers

- `cargo build -p tack-cli -p tack-runner` (debug, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C9`, cold): 43.34s (from the e2e webServer's own first compile, same command).
- `cargo nextest run --workspace`: 1428 tests, 14.837s, 7 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean, ~26.45s incremental.
- `npx vitest run` (frontend): 807 tests / 90 files, ~7-8.6s per run (3 runs, identical pass count each time).
- `npx playwright test e2e/scheduler-e2e.spec.ts --project=chromium --project=firefox`: 8/8 passed, 10.3s combined; chromium-only re-runs: 7.8-8.0s each (3 runs).
- Live proof case 1: request-to-lease 9ms (`created_at` → `lease_issued_at`), attempt wall-clock `duration_ms` (measured) = 9ms, full request-to-succeeded well under 100ms wall clock.
- Live proof case 3: request-to-succeeded, same order of magnitude (sub-second; not independently isolated to the millisecond since case 3's script polled at 1s granularity — the request was already `succeeded` on the very first poll after `leased`).

## What a stranger still cannot do

A stranger who leaves every model field on its own default ("Auto") on a bare install (no
agent-profile, project, or fleet default model configured anywhere — the state a fresh
install is actually in until someone completes the Agents page's model-default step) can no
longer submit a request that silently queues forever: the dialog now blocks the button and
names the exact fix, with a working link straight to Project Settings → Agents, for the one
tier that has a settings page at all. What a stranger still cannot do: make Auto itself
*work* without configuring a default somewhere first, or type a model on every single run —
that is a genuine, structural limit of the runner-v1 contract this card was explicitly told
not to touch (`crates/tack-orch/src/scheduler/select.rs`'s `AutoSelectNotVerified`, correct
and unchanged). A stranger whose install has a default configured on the *agent profile* or
*fleet* tier (API-only, no settings UI for either) gets an accurate, tier-named reason but no
clickable fix — there is nowhere in the product to send them yet. And a stranger who runs
this repository's own full E2E suite, not just `scheduler-e2e.spec.ts`, will still see
`a11y.spec.ts`'s one pre-existing, VI-C2-era failure and `agents-page.spec.ts`'s
parallelism-only flake — both predate this card, both are named above, neither is this
card's to fix.

## Surface-map delta

No §VI.0 row moves from console to UI in this change — the project-level default-model
*setting* surface (`ModelDefaultStep.tsx`/`AgentsPanel.tsx`) already existed before this card
(it is what case 3's live proof used, unmodified). What this card changes is whether the
*submit gate* is honest about, and points at, that already-existing surface — before this
card, the gate's advisory never mentioned it existed at all. The row that's arguably now
*more* true than the table states it: "Run an item … zero hand-typed identifiers" — that was
already false for exactly one path (Auto with nothing configured, per VI-C7's finding); this
card doesn't add a new field, it makes that one path fail loudly with a fix instead of
silently forever.

## Context spent

- Read per the dispatch block: `CLAUDE.md`, this card's `TODO.md` section (`VI-C9` through
  the start of `VI-C10`), `§VI.0`'s cold-start capsule, `VI-C7.md` in full including its
  amendment, `VI-D2.md` in full including its amendments (first ~784 lines directly, the
  remainder — the re-recording recipe — via a follow-up read), `V-C4.md`'s relevant
  `scheduler-e2e.spec.ts` section.
- Also read, one hop beyond the dispatch list, because the fix required it: the whole of
  `RunWithAgentModal.tsx`, `shared.ts`, `capabilities.ts`, `capabilities.test.ts`,
  `RunWithAgentModal.test.tsx`, `shared.test.ts`; `crates/tack-orch/src/scheduler/select.rs`,
  `crates/tack-orch/src/model_policy/{mod.rs,wiring.rs}` (to mirror the precedence walk
  correctly, including the `pinned_auto` nuance); `crates/tack-api/src/handlers/
  executions.rs` (to confirm exactly what the operator route resolves and where
  `fleet_id`/`project_id` come from); `frontend/src/features/settings/panels/AgentsPanel.tsx`
  and `frontend/src/features/agents/steps/ModelDefaultStep.tsx` (to find the one real UI
  surface to link the `fix` action at, and confirm the route/tab); `frontend/e2e/
  scheduler-e2e.spec.ts` and `helpers.ts` in full (to diagnose and fix the pre-existing
  breakage); `scripts/smoke.sh` (the shim-harness/live-proof technique, reused for the live
  proof above).
- Files opened and not used for anything load-bearing: `docs/agent-handoffs/part-vi/VI-D2.md`'s
  asset-recording sections (hero.gif/agents.png/attempt.png/two-machines.png reasoning) —
  read for context on the passthrough-gate discovery, not otherwise used.
- Context size at handoff: substantial — this card touched eight files across three
  concerns (the passthrough fix, the Auto-resolution fix, and the pre-existing E2E
  breakage), plus a from-scratch live-proof rig (server + runner + Vite + Playwright) built
  and torn down twice.
- Read-list lines that were wrong: none identified as wrong per se, but the dispatch block's
  framing of this as a two-file fix (`capabilities.ts` + the submit gate) undersold the
  actual footprint once `scheduler-e2e.spec.ts` turned out to be broken for an unrelated,
  larger reason — that cost roughly as much time as the two intended fixes combined.

## Amendments

### Amendment (2026-09-06, coordinator review — the mirror's own cost)

The coordinator verified the diff directly (the passthrough arm, the four `scheduler-e2e`
tests — confirmed exactly one changed assertion line in that spec, an addition — 1428 Rust
tests, 807 frontend tests, clippy clean, the fleet `default_policy` only read when the
target is a fleet) and is merging this, with one omission named: this handoff never said
plainly that `resolveAutoModelPolicy` is a **mirror**, not a shared definition, and never
said what that costs. Recorded here, appended rather than folded into the text above.

`resolveAutoModelPolicy` (`frontend/src/shared/runWithAgent/shared.ts`) is a hand-written
copy of three specific Rust decisions: `crates/tack-orch/src/model_policy/mod.rs`'s
`ModelPolicyTier::ORDER` (the four-tier precedence) and its `resolve_model_policy` (the walk
over that order), and `crates/tack-orch/src/model_policy/wiring.rs`'s
`parse_model_default_convention` (how an agent profile's `limits` or a fleet's
`default_policy` blob is read as a default-model opinion). **Nothing in this tree binds the
two together** — no shared fixture, no test that runs against both implementations, no CI
check that fails when one changes without the other. Add a tier server-side, reorder
`ModelPolicyTier::ORDER`, or change what `parse_model_default_convention` accepts, and the
Rust side moves while this TypeScript copy does not, silently.

Before this card, that drift was cosmetically bad at worst — the old advisory text was never
load-bearing, so a stale belief just produced a misleading hint next to a button the server
would validate for real regardless. **After this card, the copy is load-bearing**: the gate
now blocks submission outright on its own answer. The direction that failure runs in is not
symmetric. `parse_model_default_convention`'s own doc comment states its posture as
permissive by design — a missing key, malformed JSON, wrong type, or unrecognised shape all
read as "no opinion," never an error. A Rust-side change that resolves *more* requests than
today (a tier added, a reordered precedence, a convention parsed more leniently) is the
change consistent with that posture continuing to hold, and it is exactly the change this
copy cannot see: it would keep answering `'unresolved'`/`'pinned_auto'` for a request the
server would now schedule without complaint, so the dialog confidently refuses a dispatch
that would have worked. The opposite failure — this copy believing a tier resolves something
the server no longer does, letting a doomed dispatch through — would need
`parse_model_default_convention` (or the precedence it feeds) to become *stricter* than it
is today, which its own stated posture argues against happening quietly. Not provably
impossible, just the far less likely evolution path, and not one this file can rule out on
its own either way. This is the same class of defect this card exists to fix — a dialog
promising something the scheduler won't honor — now pointed the other way: a dialog refusing
something the scheduler would have honored.

The coordinator is carding the real fix separately: a shared fixture in the manner of
`docs/contracts/runner-v1/`, not a hand-copied table re-derived in a `.test.ts`. A matching,
shorter note was added directly above `resolveAutoModelPolicy`'s own doc comment in
`shared.ts` for whoever changes the Rust side next and has no reason to read this handoff at
all.

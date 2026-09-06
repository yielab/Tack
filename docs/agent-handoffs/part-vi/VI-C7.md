# VI-C7 handoff

**Verdict: it reproduces, and it is not a timing issue — dispatching through the "Run with
agent" dialog with harness = Codex and model mode = "Auto" can never produce a claimable
request on `develop`, for any harness, on any runner, no matter how long an operator waits.
The cause is `crates/tack-orch/src/scheduler/select.rs:144-147`: `evaluate_candidate`
unconditionally rejects every `ModelSelector::AutoSelect` request with
`IneligibleReason::AutoSelectNotVerified`, before any harness- or runner-specific check runs.
This is deliberate, documented, and has its own passing unit test (`select.rs:628-650,
auto_select_is_rejected_with_a_named_reason_not_an_empty_list`) — it is not a regression to
patch in the scheduler. The defect is one layer up: the dialog defaults every harness to
"Auto" and the submit gate reports it as an allowed, merely-advisory choice
(`frontend/src/shared/runWithAgent/shared.ts:241-259`), so an operator following the UI's own
default gets a request that silently sits `queued` forever with no error, no toast, and no
attempts — ever.**

- Base SHA / branch / final SHA: base `develop` at `a84a089`, branch
  `agent/vi-c7-codex-dispatch`, final SHA: not committed by this agent (card rule: no
  commit/push/merge/rebase).
- Files changed (matches this card's ownership — nothing else): only this handoff,
  `docs/agent-handoffs/part-vi/VI-C7.md`. No production file touched, per the card's own
  rule and its "Needs nothing" board line.
- Contract fixtures consumed: none.
- Behavior implemented: none — this card measures, it does not change anything.
- Tests added and exact commands/results: none added; the reproduction below is a live,
  manual run against a real build, not a new automated test (out of scope — "changes no
  production file").
- Failure/adversarial case proved: yes — this *is* the adversarial case. See "Evidence" below.
- Schema/API/contract change requested from another owner: none required to fix this — see
  "The smallest fix" below; it is UI-only.
- Known limitations or `not_measured` fields: the real `codex`/`claude` vendor CLIs were not
  used — a POSIX-shell shim stands in for both (`scripts/smoke.sh`'s own precedent, same
  technique). This does not weaken the finding: the block happens entirely server-side, in
  the pure scheduler, before any harness process is ever spawned — confirmed directly (see
  the DB/log evidence below: 0 rows in `execution_attempts`, meaning the runner's harness
  adapter code was never reached for the Auto request at all). macOS/Windows not attempted
  (Linux only, matching every other card's constraint here).
- Secrets/logging review: no secret was introduced or logged. The one credential in play
  (a runner enrollment token) was generated fresh by the disposable local server for this
  card's own throwaway SQLite DB and runner state dir, never committed, and is gone with the
  killed processes.
- Safe merge order and likely conflicts: none — no production file touched.
- Checklist: no unowned files (only this handoff); no live secret (confirmed above); no
  panic stub (no code changed); no blind retry (N/A, no code changed).

## Evidence

Environment: own build (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C7`), own port
(`TACK_PORT=3312`), own SQLite DB and runner state dir under this session's scratch
directory — never the repository's `tack.db`. One real `tack serve` process and one real
`tack-runner` process, both built from this branch's checkout of `develop`.

**1. The dialog's own default state, traced to the wire.** `modelMode` initializes to
`'auto'` and resets to `'auto'` on every open
(`frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:163,188`) — Auto is not a corner
case, it is what the dialog shows before an operator touches anything. With `modelMode() ===
'auto'`, both `modelProvider()` and `modelId()` fall through to `null`
(`RunWithAgentModal.tsx:307-324`, no `'auto'` branch in either function). `shared.ts`'s
`buildCreateExecutionInput` copies those verbatim into the wire body
(`requested_model_provider: values.modelProvider`, `requested_model_id: values.modelId` —
`shared.ts:122-123`).

**2. The server accepts and resolves it as Auto.** `POST /api/executions`
(`crates/tack-api/src/handlers/executions.rs:568-600`) resolves a null/null pair through
`resolve_request_model_policy` (agent-profile → project → fleet default); with none of those
configured (this test's project/profile/fleet all bare), the resolution stays
`ModelSelector::AutoSelect` and is stored as literal `NULL`/`NULL`. Confirmed directly against
the request row's own database columns:

```
$ sqlite3 vic7.db "SELECT id, state, requested_harness_kind, requested_model_provider, \
    requested_model_id, created_at FROM execution_requests \
    WHERE id='exec_52b0ea621f2d7712286d7dedc690c17143f15bda31bfc7f16606d912684038fe';"
exec_52b0ea6...038fe|queued|codex||2026-09-06T03:29:57.445556121+00:00
```
(the two empty fields between `codex` and the timestamp are `requested_model_provider` and
`requested_model_id`, both `NULL`).

**3. The scheduler is where it stops, and it is unconditional.**
`crates/tack-orch/src/scheduler/wiring.rs::build_scheduling_request` turns the stored
`NULL`/`NULL` into `ModelSelector::from_parts(None, None)` → `Ok(AutoSelect)`
(`crates/tack-orch/src/scheduler/types.rs:82-93`). `select.rs::evaluate_candidate` then hits:

```rust
// crates/tack-orch/src/scheduler/select.rs:139-148
match &request.requested_model {
    ModelSelector::AutoSelect => {
        return Err(IneligibleReason::AutoSelectNotVerified {
            harness: request.requested_harness_kind.clone(),
        });
    }
    ...
```

This runs **before** the harness-declared-model-combinations check and before the
`model_passthrough` check — every candidate is rejected identically, regardless of what any
runner declares. `IneligibleReason::AutoSelectNotVerified`'s own doc comment
(`types.rs:194-202`) states why: "No capability field in runner-v1 v1 records whether a
harness safely accepts an unspecified model... every candidate is reported ineligible for an
auto-select request... rather than silently narrowing to whichever harness the scheduler
happens to guess is safe." This is proved deliberate, not incidental, by its own test:
`select.rs:628-650`, `auto_select_is_rejected_with_a_named_reason_not_an_empty_list`, which
asserts exactly this outcome and passes today. `scripts/smoke.sh` (this repo's own end-to-end
smoke test) already knows this and works around it by always supplying an explicit
model/provider pair — its step-8 failure-diagnosis branch says so directly: *"AutoSelect is
likewise always rejected"* (`scripts/smoke.sh`, step 8 comment block).

`choose_request_for_runner` (`wiring.rs:172-273`) therefore returns `Ok(None)`, and
`crates/tack-db/src/repo/execution.rs:2195-2200` treats `RequestSelection::Scheduled(None)` as
"no work" outright — it never falls through to the naive query, so there is no path by which
this request becomes claimable later, on this same runner or any other, without a code
change.

**4. Live proof, not just a code read — ids and timestamps.**

- Request created: `exec_52b0ea621f2d7712286d7dedc690c17143f15bda31bfc7f16606d912684038fe`,
  `created_at: 2026-09-06T03:29:57.445556121+00:00`, harness `codex`,
  `requested_model_provider`/`requested_model_id` both `null` — the exact payload the dialog
  sends with Codex + Auto (`POST /api/executions`, body in
  `req_auto.json`, mirrored field-for-field from `buildCreateExecutionInput`).
- Runner `runr_145c25ec-95ca-4311-bbbb-fb0dc105a8d9` was `active`, heartbeating, and had
  `available_capacity: 1` throughout — confirmed via `GET /api/runners`, and it was
  observably polling `POST /api/runner/v1/claim` continuously the whole time (server debug
  log shows repeated 200-status claim calls at sub-2ms intervals, e.g.
  `2026-09-06T03:35:29.427065Z ... path=/api/runner/v1/claim ... status=200`).
- Checked at `2026-09-06T03:35:29.205Z` — **5 minutes 32 seconds** after creation, well past
  V-C2's ">3 minutes" observation window:
  ```
  $ curl -sf .../api/executions/exec_52b0ea6...038fe -H 'x-tack-principal: vic7-operator'
  {"request_id":"exec_52b0ea6...038fe","state":"queued","cancellation_requested_at":null,
   "created_at":"2026-09-06T03:29:57.445556121+00:00"}
  $ sqlite3 vic7.db "SELECT COUNT(*) FROM execution_attempts WHERE request_id='exec_52b0ea6...038fe';"
  0
  ```
  Still `queued`, still zero attempts, runner still active and still polling.
- **Contrast, same runner, same harness, one field different**: a second request
  (`exec_026a10b32c34aee4e4cc5bf3279c0eb4829c9ffa2377a1a0fe1a3c3391a275eb`, `requested_model_provider:
  "openai"`, `requested_model_id: "gpt-5-codex"` — codex's own `model_passthrough: supported`
  attestation covers this pairing) created at `2026-09-06T03:30:35.999Z` was leased at
  `2026-09-06T03:30:36.032168247+00:00` — **33 milliseconds later** — and reached `succeeded`
  by `03:30:36.087340446+00:00`:
  ```
  $ sqlite3 vic7.db "SELECT id, request_id, state FROM execution_attempts;"
  att_edf9c490-bafe-43f0-a9b0-af7e81153afe|exec_026a10b3...275eb|succeeded
  ```
  This isolates the cause to the Auto/null-model request shape specifically — the runner,
  the harness probe, capacity, and the git repository fixture are all identical between the
  two requests; only `requested_model_provider`/`requested_model_id` differ.

**5. The failure is silent by design, not just by accident.** The `claim` handler's `None`
branch (`crates/tack-api/src/handlers/runner_protocol.rs:998-1004`) returns the same body —
`{"lease": null, "reason": "no_eligible_work"}` — regardless of *why* the pure scheduler found
nothing: a genuinely empty queue, a stale heartbeat, a harness the runner never declared, and
an unconditionally-rejected Auto request are all reported identically to the runner. Nothing
here distinguishes "will become claimable once the queue clears" from "can never become
claimable no matter what." The dialog has no polling/timeout UI at all for this state, so an
operator sees only a request that stays "Queued" with no explanation.

## The smallest fix (diff only — not applied)

The scheduler's rejection is correct given the actual runner-v1 contract (no runner can today
attest "I accept an unspecified model" — adding that is a runner/contract change, explicitly
out of this card's scope per its own "Stop if" line). The bug worth fixing is the UI
presenting a request shape that can *never* succeed as a normal, merely-unverified,
advisory-only choice. Smallest fix: make the frontend submit gate refuse Auto outright
(structural block + reason, matching this repo's own rule, `TODO.md` III.2 rule 7 — "an
unsupported combination cannot be submitted, disabled + reasoned, not merely rejected
server-side") instead of waving it through with a soft advisory that a real backend claim can
never honor:

```diff
--- a/frontend/src/shared/runWithAgent/shared.ts
+++ b/frontend/src/shared/runWithAgent/shared.ts
@@ export function gateHarnessModelSelection(
   if (modelProvider == null || modelId == null) {
-    const probe = harnessProbeStatus(capabilities, harnessKind);
-    if (probe.probed) {
-      return { allowed: true, advisory: false, reason: 'At least one runner reports this harness cleanly.' };
-    }
-    return {
-      allowed: true,
-      advisory: true,
-      reason: probe.lastError
-        ? `No runner currently reports this harness cleanly (last probe error: "${probe.lastError}"). ` +
-          'The scheduler will still validate at claim time.'
-        : 'No runner capability data is available yet to confirm this harness is installed anywhere ' +
-          '(see docs/agent-handoffs/part-iii/III-E2.md, Gap 1: no GET /runners endpoint exists). ' +
-          'The scheduler will still validate at claim time.',
-    };
+    // crates/tack-orch/src/scheduler/select.rs's evaluate_candidate
+    // unconditionally rejects ModelSelector::AutoSelect with
+    // IneligibleReason::AutoSelectNotVerified for every runner and every
+    // harness -- runner-v1 has no capability field that lets a runner
+    // attest it safely accepts an unspecified model, so "the scheduler
+    // will still validate at claim time" is not true today: it will
+    // always refuse (VI-C7, docs/agent-handoffs/part-vi/VI-C7.md). Block
+    // submission instead of advertising a request shape that can never be
+    // claimed.
+    return {
+      allowed: false,
+      advisory: false,
+      reason: 'Auto mode cannot be scheduled today -- no runner can attest it safely accepts ' +
+        'an unspecified model, so every Auto request is rejected at claim time ' +
+        '(crates/tack-orch/src/scheduler/select.rs, AutoSelectNotVerified). Choose an explicit ' +
+        'model, or "Other" if this harness supports typing one in.',
+    };
   }
```

This also requires updating the two now-contradicted unit tests in
`frontend/src/shared/runWithAgent/shared.test.ts:178-183,219-232` (which currently assert
Auto is `allowed`/advisory) to assert the new blocked state instead, and probably retiring
`harnessProbeStatus`'s only remaining call site if this is its last one (not checked — outside
this card's read scope). Not applied, per this card's rule.

## Claim → evidence

| Claim | Evidence |
|---|---|
| The dialog defaults to, and resets to, Auto | `RunWithAgentModal.tsx:163,188` |
| Auto mode sends `requested_model_provider`/`requested_model_id` as `null`/`null` | `RunWithAgentModal.tsx:307-324`, `shared.ts:122-123` |
| The server stores an unresolved Auto request as literal `NULL`/`NULL` | `sqlite3 vic7.db "SELECT requested_model_provider, requested_model_id FROM execution_requests WHERE id='exec_52b0ea6...038fe'"` → empty/empty |
| The scheduler unconditionally rejects every `AutoSelect` request | `crates/tack-orch/src/scheduler/select.rs:144-147`; its own test `select.rs:628-650` |
| This is documented as deliberate, not a bug to patch in the scheduler | `crates/tack-orch/src/scheduler/types.rs:194-202` (`IneligibleReason::AutoSelectNotVerified` doc comment) |
| `Scheduled(None)` never falls back to a naive claim that could still pick it up | `crates/tack-db/src/repo/execution.rs:2195-2200` |
| A live Auto+Codex dispatch against a real, active, capacity-free runner never claims | request `exec_52b0ea6...038fe`, created `2026-09-06T03:29:57.445556121+00:00`, still `queued` with 0 attempts at `2026-09-06T03:35:29.205Z` (+5m32s) |
| The runner was genuinely polling the whole time, not stalled itself | server debug log, repeated `POST /api/runner/v1/claim` 200s at sub-2ms latency throughout the window; `GET /api/runners` showed `state:"active"`, fresh `last_heartbeat_at`, `available_capacity:1` |
| The same runner/harness with an explicit model claims almost instantly | request `exec_026a10b3...275eb`, created `2026-09-06T03:30:35.999Z`, leased `2026-09-06T03:30:36.032168247+00:00` (33ms), succeeded `2026-09-06T03:30:36.087340446+00:00` |
| The claim endpoint reports this identically to ordinary "no work" | `crates/tack-api/src/handlers/runner_protocol.rs:998-1004` |
| `scripts/smoke.sh` already knows and documents this exact behavior | `scripts/smoke.sh` step 8's failure-diagnosis branch: "AutoSelect is likewise always rejected" |
| V-C2's two "already false" bullets remain false on this same build (re-checked in passing, not re-litigated) | free-text override still present: `RunWithAgentModal.tsx:522-528`; `opencode.rs` still absent: `ls crates/tack-runner/src/harness/` → `claude_code.rs`, `codex.rs` only |

## Measured numbers

- Time from request creation to lease, explicit-model request: **33 ms**
  (`2026-09-06T03:30:36.032168247+00:00` − `2026-09-06T03:30:35.999Z`, both from the server's
  own timestamps).
- Time from request creation to lease, Auto request: **never**, observed for **5 minutes
  32 seconds** (`2026-09-06T03:29:57.445556121+00:00` → `2026-09-06T03:35:29.205Z`), still
  `queued`, 0 rows in `execution_attempts` for that `request_id`.
- Build time for this reproduction: `cargo build -p tack-cli -p tack-runner` → 1m 00s
  (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C7`).

## What a stranger still cannot do

A stranger who follows the whole documented onboarding path — installs the binary, adds a
runner, authenticates a harness, opens an item and clicks "Run with agent" — and leaves the
Model field on its own pre-selected default ("Auto (let the runner decide)") gets a request
that goes into `queued` and stays there forever, with no error, no toast, and nothing in the
UI that ever explains why. There is no number of runners, no amount of waiting, and no retry
that fixes it, because the rejection is unconditional in the scheduler, not a matter of
timing or capacity. The only way to get a run today is to already know to switch to
"Choose…" and either pick a combination the runner declares (neither shipped harness declares
any) or use "Other" to type a model id by hand — which is exactly the opposite of what
"Auto (let the runner decide)" promises by its own label.

## Surface-map delta

None — this card changes no UI surface; it measures one that already exists. It does bear
directly on §VI.0's "Run an item" row ("UI, defaults from project settings, zero hand-typed
identifiers"): that target is not actually reachable with zero typed identifiers today,
because the one path that avoids typing a model id (Auto) is the one guaranteed to fail
silently. Whether project-level default-model configuration (VI-C3) is expected to close this
gap for most operators, or whether the gate itself also needs the fix above regardless, is a
call for whichever card next touches `RunWithAgentModal.tsx`'s model fieldset.

## Context spent

- Read per the dispatch block: `§VI.0` (the surface map), this card's `TODO.md` section,
  `docs/agent-handoffs/part-v/V-C2.md` in full (escalation + amendment), the whole of
  `RunWithAgentModal.tsx` and `shared.ts` (both under the "one grep for `.route(`" +
  "what the dialog sends" instruction), and the `claim` handler plus the pure scheduler
  (`select.rs`, `types.rs`, `wiring.rs`) it calls into — one hop further than the dispatch
  block's explicit read list, because "follow it until you can name the component that
  stops" required seeing where `choose_request_for_runner` actually decides, not just the
  HTTP handler that calls it.
- Also read: `scripts/smoke.sh` (already knew and worked around this exact behavior — found
  while grepping for a live-reproduction pattern to copy, not part of the assigned read
  list, but directly load-bearing for trusting the reproduction setup) and
  `crates/tack-db/src/repo/execution.rs`'s `RequestSelection` doc comment (one function, to
  confirm `Scheduled(None)` has no fallback path).
- Not read: the whole runner crate, the whole orch crate, any other card's handoff — per the
  dispatch block's explicit "Do not read" line.
- Files opened and not used: none of significance — every file read fed directly into the
  hop-by-hop trace above.
- Context size at handoff: moderate; the two model files (`RunWithAgentModal.tsx`, ~640
  lines, and `shared.ts`, ~330 lines) were the largest single reads.

## Amendments

### Amendment (2026-09-06, coordinator review — scope correction, self-verified before accepting)

**Corrected verdict: a request whose model policy resolves to `AutoSelect` never becomes
claimable — but "Auto" in the dialog does not always resolve to `AutoSelect`.** The original
verdict above ("Auto... can never produce a claimable request on `develop`, for any harness,
on any runner, no matter how long an operator waits") overstates scope. It is true only for
the install state this card's reproduction actually used: an item whose agent profile,
project, and fleet/runner all supply no default model. On an install where **any** of those
three tiers names an explicit provider/model, the exact same dialog action — harness = Codex,
operator clicks the "Auto (let the runner decide)" radio — resolves server-side to
`ModelSelector::Explicit` and schedules normally, exactly like this handoff's own contrast
request. The coordinator's citations were verified directly, not taken on trust, and all
three hold:

- `crates/tack-api/src/handlers/executions.rs:568-597`: when the client sends
  `requested_model_provider`/`requested_model_id` both `null` (Auto), the handler calls
  `resolve_request_model_policy(&state.repo, Some(agent_profile_id), Some(project_id),
  fleet_id, None)` — the trailing `None` is a hardcoded `request_override`, not derived from
  the client's null/null pair. It is unconditional and has nothing to do with whether the
  client's own fields were null.
- `crates/tack-orch/src/model_policy/wiring.rs:104-143`: that function fetches each tier's
  stored default — `fetch_agent_profile_limits` (agent profile), `fetch_project_default_model`
  (project), `fetch_fleet_default_policy` (fleet) — and hands them to `resolve_model_policy`.
- `crates/tack-orch/src/model_policy/mod.rs:93-110` (`resolve_model_policy`): walks
  `RequestOverride → AgentProfile → Project → Fleet` in that fixed order (`mod.rs:42-47`) and
  returns the **first present** tier's value. `ModelSelector::AutoSelect` is returned with
  `source: None` **only when every tier is absent** (`mod.rs:107-110`) — confirmed by its own
  exhaustive 2^4-combination test suite (`mod.rs:125-128`'s doc comment).

This card's own reproduction sits exactly at that "every tier absent" corner: the project,
agent profile, and runner/fleet it created for the test were all bare, by construction (no
`default_model` on the project, no `{"default_model": ...}` convention in the agent profile's
`limits`, no fleet involved — an `exact_runner` selector was used throughout). That is a real,
common, and serious install state — it is exactly what a fresh install looks like before
anyone configures a project default model — but it is one state, not every state. **A second,
sharper nuance, also verified**: configuring a project default of literal `"auto"` does **not**
fix it either. `wiring.rs:88` maps `ProjectModelDefault::Auto` to
`ModelSelector::AutoSelect`, and `resolve_model_policy` treats `Some(AutoSelect)` at a tier as
a real, present value that **stops the walk there** (`mod.rs:50-58`'s own doc comment: "a
real, if unusual, configuration... that stops the walk at that tier rather than falling
through to a less-specific tier that might name a concrete model"). Only a tier that names an
explicit provider/model — agent profile, project, or fleet, in that precedence order — makes
an Auto dispatch schedulable. This is why the corrected framing matters operationally: it
makes "pick a default model" (the step the Agents page/onboarding surface asks a new operator
to complete — §VI.0's surface map, "Choose a default model" row) load-bearing rather than a
nice-to-have — skip it, at every tier, and every Auto dispatch queues forever with no error;
complete it at any one tier, and Auto dispatches work.

**Restated verdict, in its true scope**: dispatching through the "Run with agent" dialog with
harness = Codex and model mode = "Auto" produces a request that reaches the scheduler as
`ModelSelector::AutoSelect`, and every `AutoSelect` request is rejected unconditionally
(`select.rs:144-148`, unchanged from the original finding above) — **if and only if** none of
the item's agent profile, project, or target fleet has an explicit default model configured.
That condition holds for a fresh install with no default-model step completed anywhere (this
card's own reproduction), and is silent and indefinite exactly as originally described when it
holds. It does not hold, and Auto dispatches schedule normally, once any one tier names an
explicit model — including for Codex specifically, despite Codex declaring zero
`model_combinations` and relying entirely on `model_passthrough`.

**This also breaks the proposed fix — do not apply it as written.** Verified directly:
`gateHarnessModelSelection`'s own signature is
`(capabilities: RunnerCapabilities[], harnessKind: string, modelProvider: string | null,
modelId: string | null): CombinationGate` (`shared.ts:235-240`), and its one call site passes
only `capabilities()`, `harnessKind()`, `modelProvider()`, `modelId()`
(`RunWithAgentModal.tsx:327`). None of these carry agent-profile, project, or fleet
default-model configuration — the function cannot see whether any tier would resolve the
request to `Explicit`. The diff above would set `allowed: false` for every null/null pair
unconditionally, which blocks a submission that works today on any install with a default
model configured anywhere — a regression, not a fix, and a worse outcome than the honest-but-
weak advisory it replaces.

**What the right fix would have to know that the frontend gate cannot see today**: whether
`resolve_model_policy`'s tier walk — agent-profile default, then project default, then fleet
default, in that order — would land on `Explicit` or fall all the way through to `AutoSelect`
for the *currently selected* agent profile, the item's project, and the currently selected
target (exact runner or fleet). The frontend modal already fetches the project's own
`default_model` (`project()?.default_model`, used today only for the separate "Project
default" radio's own resolution, not for gating "Auto"), but it fetches neither the selected
agent profile's `limits` convention value nor any fleet default policy, and even if it fetched
all three, precedence order matters (agent profile is checked before project, which is
checked before fleet) — a client-side re-implementation would have to reproduce
`resolve_model_policy`'s exact walk, not just peek at one tier. Two honest paths forward,
neither of which is this card's to build: (a) a small resolution-preview surface (even just
including each tier's resolved default on the agent-profile/project/fleet responses the modal
already fetches) so the gate can compute the same answer the server will, or (b) leave "Auto"
submittable as today but correct its advisory text so it stops promising a validation the
scheduler does not perform — replacing "The scheduler will still validate at claim time" (a
promise that is simply false when every tier is absent) with an accurate conditional, e.g.
"This will only run if an agent-profile, project, or fleet default model is configured for
this target; otherwise the request will queue with no error." Path (b) is safe to apply
without the information gap above; path (a) is the real fix and is out of this card's scope
(a schema/response change owned elsewhere).

**Two smaller corrections, same review:**

- The proposed diff's replacement comment cited this card and its own handoff path inline
  (`"VI-C7, docs/agent-handoffs/part-vi/VI-C7.md"`) inside a `.ts` doc comment. `CLAUDE.md`'s
  "Code style" rule against card/wave/phase/`TODO.md` references in comments is not scoped to
  Rust — it is a repo-wide rule stated once, for exactly this reason. Verified
  `scripts/check-comments.sh` would not have caught it regardless: its default `ROOTS` is
  `crates/` and its `commentish` grep is hardcoded to `--include='*.rs'`
  (`check-comments.sh:9-13,36-39`), so it never scans `frontend/`. The absence of a script hit
  is not evidence the comment was fine — restated below without the citation, and superseded
  in any case by the path-(b) wording fix above rather than the blocking diff:

  ```diff
  +    // crates/tack-orch/src/scheduler/select.rs's evaluate_candidate
  +    // unconditionally rejects ModelSelector::AutoSelect for every runner
  +    // and every harness -- runner-v1 has no capability field that lets a
  +    // runner attest it safely accepts an unspecified model. That request
  +    // only ever reaches the scheduler as AutoSelect when no agent-profile,
  +    // project, or fleet default model resolves it first
  +    // (crates/tack-orch/src/model_policy/mod.rs's resolve_model_policy) --
  +    // so this advisory is only reachable, and only accurate, on an
  +    // install with no default model configured anywhere in that chain.
  ```

- The "Known limitations" note that the harness shim does not weaken the finding, because the
  block is server-side and pre-spawn, was checked again and stands — the coordinator agrees
  and no correction is needed there.

**On trusting this review**: every citation given was re-verified against the file and line
numbers directly (`executions.rs:568-597`, `wiring.rs:65-143`, `model_policy/mod.rs:32-110`,
`shared.ts:235-240`, `RunWithAgentModal.tsx:327`, `check-comments.sh:9-13,36-39`) before being
accepted into this amendment — none were taken on the coordinator's word alone. All held. The
original evidence above (the live reproduction, its ids and timestamps, the scheduler
citation, and the "silent by design" claim-endpoint behavior) is unchanged and still accurate
for the install state it actually tested; only the generalization to "every install, always"
was wrong, and is corrected here.

# VI-D1 handoff

- Base SHA / branch / final SHA: base `develop@0ee8414` (`docs(board): VI-C20 is in, and
  VI-C24 is two tests, not one`) / branch `agent/vi-d1-stranger-proof` / final SHA — see
  the one commit on this branch (this is the last card of Part VI; per its own rules I do
  not merge, edit `TODO.md`'s board, or push).
- Files changed (must equal ownership list): `scripts/smoke.sh` (new steps 13–14, plus a
  `BIN_DIR`/`CARGO_TARGET_DIR` fix explained below), `README.md` (one sentence in "Run
  it"), `docs/CONFIG.md` (the stale model-precedence line, plus a new row documenting the test-only gateway override),
  `docs/book/src/user-guide/quick-start.md` ("Run an item with an agent" restructured),
  `docs/book/src/user-guide/agent-runners.md` (five corrections — see "Claim → evidence"),
  `docs/book/src/roadmap.md` (one paragraph, the non-blocking finding VI-A2/Wave 14
  routed here), `CHANGELOG.md` (`[Unreleased]`, six new entries), this handoff, and
  **one Rust file outside the literal ownership line**:
  `crates/tack-runner/src/provider/vercel_ai_gateway.rs`. That last one is explained in
  full below — it was necessary, not incidental, and it is the smallest change that made
  it possible.
- Contract fixtures consumed: none — no `docs/contracts/runner-v1/` fixture read or
  changed. `docs/openapi.json` re-checked (`CreateExecution.required` still 13 fields),
  not regenerated (nothing here changes a route or a schema).
- Behavior implemented: a test-only escape hatch in the Vercel AI Gateway provider
  (`TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL`), inert unless set, that rebases the
  catalog URL and both wire endpoints under a local address instead of the real vendor
  host — see "Why one Rust file" below. Everything else this card owns is docs, a smoke
  script, and process (the stranger container).

## Why one Rust file was necessary, not a scope violation

Task 1 requires proving "key → catalog → spawn environment → the actual model used"
against **a local fake gateway shim**, with a fake key, never the real vendor — stated
twice in the card (the Tasks line and the top-level Secrets section). Every URL this
provider hits is a compile-time constant (`https://ai-gateway.vercel.sh/...`); there is
no config file, environment variable, or test seam anywhere in the tree to redirect it,
and no HTTP-mocking crate is wired into `tack-runner` (checked: no `mockito`/`wiremock`
dependency, no existing gateway integration test). I first tried the network-layer
route — an unprivileged mount+user namespace to overlay `/etc/hosts` and a self-signed
CA, fully reversible, touching neither the host's real files nor any other process —
and the harness's own sandbox classifier refused it (generating a certificate for a
real, external, third-party domain name reads as MITM tooling regardless of intent, and
correctly so). The only path left that does not touch the host, does not need a real
cert, and stays inside my own file, is a code seam:

```rust
const TEST_BASE_URL_OVERRIDE_VAR: &str = "TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL";
fn test_base_url_override() -> Option<String> { std::env::var(TEST_BASE_URL_OVERRIDE_VAR).ok()... }
```

read once per call, in exactly the two functions that build a URL (`catalog_url()`,
`endpoint()`). Unset — every path except `scripts/smoke.sh` step 13 — it is a single
`std::env::var` lookup that returns `Err` and changes nothing; `cargo test -p
tack-runner` proves this (a unit test asserts the unset behavior is byte-identical to
the pre-existing hardcoded constants, then sets the var and asserts both wires and the
catalog URL rebase, then unsets it and asserts it reverts, then sets a non-loopback
value and asserts it is ignored exactly like unset — all in one test, since mutating a
process-wide env var is only safe under `cargo nextest`'s one-process-per-test model,
noted in the test's own doc comment). This mirrors two precedents already in this
crate: `TACK_RUNNER_API_URL` (a real, shipped override for the protocol client's base
URL) and `TACK_LIVE_VERCEL_AI_GATEWAY_KEY`/`TACK_RUN_LIVE_VERCEL_CATALOG_TEST` (VI-B2's
own opt-in live-fetch test, same "no effect unless explicitly set" shape).

**Reviewed and hardened once, on the coordinator's own read of this section.** Two
things the first draft understated:

- **This is not documented in `docs/CONFIG.md`, the single authority for every
  `TACK_*` variable — fixed.** Added a row next to the other
  `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_*` entries, marked test-only, stating what it
  actually does (see the next point) rather than only that it exists.
- **The doc comment said "escape hatch," which names the mechanism, not the
  consequence.** `fetch_catalog`'s `bearer_auth(secret.expose())` sends whatever this
  runner has stored for this provider to wherever `catalog_url()`/`endpoint()` point —
  so this variable, if set, redirects a real stored credential to an arbitrary host.
  Rewrote the comment to say exactly that, and added the reasoning for why this is not
  a new privilege even so: whoever can set an environment variable on this process can
  already read the same credential straight out of the secret store this same process
  has open, so the blast radius is identical either way — full compromise of this
  process, not a new escalation. **A cheap guard was still worth adding, for a mistake,
  not an attacker:** `test_base_url_override()` now accepts only a loopback base
  (`http://127.*`, `http://localhost*`, `http://[::1]*`); anything else is treated
  exactly like the variable being unset, never a hard error (refusing to start would be
  a worse failure mode for a smoke script than silently falling back to the real
  gateway, which then fails loudly on a fake key rather than the process failing to
  start at all). The unit test above covers this case explicitly. `cargo fmt`, `cargo
  clippy -p tack-runner --all-targets -- -D warnings`, and `cargo nextest run
  --workspace -E 'package(tack-runner)'` (261 tests, 261 passed) are all clean with
  every change in this section in place, and `./scripts/smoke.sh` was re-run green
  (52 PASS / 0 FAIL) after adding the guard, to confirm step 13's own loopback value
  still passes it.

## Smoke step 13 — the gateway path, proven able to fail twice over

`scripts/smoke.sh` step 13 stands up a real, local, Python `http.server`-based fake
gateway (never the real vendor host), serving `/v1/models` (bearer-checked, 401 on
mismatch) and any other path (200 once authorized). A dedicated runner
(`smoke-runner-gateway`) is enrolled and started with the override var pointed at that
shim, `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED=1`, and a fake key stored via
`tack runner secret set` (value via `TACK_RUNNER_SECRET_VALUE`, never argv). A dedicated
shim binary (not the shared one every other step uses — see "A redaction finding" below
for why) always verifies its own injected `ANTHROPIC_AUTH_TOKEN` against the fake
gateway for real, then — only if that succeeds — emits a real Claude Code
`{"type":"system","subtype":"init",...}` / `{"type":"result",...}` pair so the actual
`claude_code.rs` parser runs, not the generic exit-code fallback.

The chain, in order, each with its own assertion: key → catalog (`tack runner doctor
--json`'s `model_combinations` carries the fake model), catalog → the live runner's own
enrollment snapshot (`GET /api/runners`), a real dispatch through the full scheduler →
spawn environment (the subprocess's own `env` dump shows the exact resolved
`ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`) → actual model used
(`actual_execution.model_provider/model_id` matches the request, `model_observation_source:
"requested_not_confirmed"` — correctly *un*confirmed, because a gateway can route/alias
and ADR 0063 says so).

**Proven able to fail, twice, not once:**

1. **The catalog check.** After the good-key proof above, the secret is overwritten with
   a wrong value (`tack runner secret set`, same name, same store) and `tack runner
   doctor` is re-run with identical env — `status: catalog error (HTTP 401)`, not
   silently accepted. Verbatim from a real run:
   ```
   PASS the catalog check CAN fail: a wrong key against the fake gateway is reported as 'catalog error (HTTP 401)', not silently accepted
   ```
2. **The live dispatch.** The same wrong key, a second real execution request through the
   same runner: the dedicated shim's own curl-verify gets 401 from the fake gateway,
   exits nonzero, and the attempt genuinely reaches `state: failed` — not a status code
   read off a mock, a real subprocess failing for a real reason.
   ```
   PASS the whole chain CAN fail: the fake gateway rejects the wrong key and the dispatched attempt fails end to end, not silently
   ```

**Both were independently confirmed load-bearing by breaking the real mechanism once,**
not just by reading the assertion:
- Patched a scratch copy of `scripts/smoke.sh`'s embedded fake-gateway server so
  `do_GET` never checks `Authorization` at all (`if False:` in place of the real
  comparison), ran it: **three** assertions correctly went red — the readiness check
  itself ("fake gateway shim never came up (last status: 200)", since it no longer 401s
  when unauthenticated), "a wrong key was not rejected by the catalog check, which
  proves the check tests nothing", and "a wrong key did not cause the dispatched attempt
  to fail (state: 'succeeded')". Discarded the patched copy; the real file was never
  touched by this experiment.
- Patched `crates/tack-api/src/router.rs`'s `local_runner_available` to drop
  `&& state.config.binds_loopback()`, rebuilt, ran step 14 (see below) alone: it
  correctly flipped red ("GET /api/local-runner/secrets answered 200 on a non-loopback
  bind"). Reverted with the saved original, rebuilt, confirmed green again.

## Smoke step 14 — corrected from the card's own stale premise, measured before writing

The card's text says "plain `tack serve` (no `--with-runner`) → `GET
/api/local-runner/secrets` is 404." **Measured directly against this build and found
false**, before writing anything: a plain `tack serve`, default loopback bind, answers
`GET /api/local-runner/secrets` with `200 {"data":[...]}`, not 404 —

```
$ curl -s -w '\nHTTP_STATUS:%{http_code}\n' http://127.0.0.1:.../api/local-runner/secrets
{"data":[{"name":"VERCEL_AI_GATEWAY_API_KEY","set_at":null},{"name":"vercel-ai-gateway/default","set_at":null}]}
HTTP_STATUS:200
```

This is by design, not a regression: `crates/tack-cli/src/local_runner.rs::serve`'s own
doc comment states it plainly — an `EmbeddedRunnerControl` is wired into **every**
`tack serve`, with or without `--with-runner`, precisely so `PUT /api/local-runner` can
turn it on later with no restart (ADR 0061 decision 6). `router.rs`'s own gate is
`state.local_runner.is_some() && state.config.binds_loopback()` — bind mode, never the
flag. `docs/CONFIG.md` (VI-B3's own section) already said this correctly; the card's
own premise, and the older claim in `agent-runners.md`, had not caught up. This is
exactly the "measure before you quote it" rule biting a docs card, on its own smoke
step, before a single line was shipped.

Step 14 now proves the **real** boundary: a `tack serve` bound to `0.0.0.0` (with a real
`TACK_API_TOKEN`, satisfying `validate_security`), no `--with-runner`, starts fine
(unlike step 12's already-existing non-loopback-`+`-`--with-runner` refusal-to-boot
case) — and `GET /api/local-runner/secrets` on it is a genuine 404, the route never
mounted, while the identical route on this run's own loopback server is a real 200 —
confirming the contrast is bind mode, not the flag. Already unit-tested server-side by
VI-B3 (`routes_are_absent_on_a_non_loopback_bind`,
`routes_are_absent_on_a_loopback_bind_with_no_control`); this step is the black-box,
real-binary proof of the same invariant, and the injected-failure proof above (router.rs
patched, step 14 alone re-run, flipped red, reverted) is what makes it load-bearing
rather than merely present.

## `scripts/smoke.sh`'s own `CARGO_TARGET_DIR` bug, found and fixed

Every existing step hardcoded `$ROOT/target/debug/...`. Running this file under the
mandated `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-D1` (the load-cap rule every
card in this cycle is under) would have built the binaries there while step 2's own
existence check kept looking at `$ROOT/target/debug` — a hard failure on line one of
real use, for every parallel agent, not just this card. Fixed once, at the top:
`BIN_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/debug"`, every hardcoded path replaced.
Confirmed via two full clean runs (52 PASS / 0 FAIL each) with `CARGO_TARGET_DIR` set,
and confirmed the fallback (`$ROOT/target/debug`) is unchanged when the variable is
unset — nobody's existing workflow breaks.

## Full smoke run, green, twice

```
$ CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-D1 SMOKE_PORT=3500 ./scripts/smoke.sh
... (steps 1-12 unchanged, all PASS) ...
== STEP 13: ... == 14 PASS, 0 FAIL
== STEP 14: ... == 3 PASS, 0 FAIL
== RESULT ==
SMOKE PASSED — fake shim harnesses, pipeline real
```
52 PASS / 0 FAIL total, run twice consecutively (plus the two injected-failure runs
above, each reverted and re-confirmed green a third time).

## The stranger test

A clean `ubuntu:24.04` container, a release binary built with `--features embed-spa`
(verified: `strings .../release/tack | grep -c AgentsPage` → 3, nonzero), a fake `claude`
binary on `PATH` (`--version` answered, any other invocation drains stdin and reports
success), and a fake local Vercel-AI-Gateway-shaped HTTP shim (never the real vendor,
never a real key) — same shape as smoke step 13's. Driven by a headless Playwright
script (`docs/agent-handoffs/part-vi/vi-d1-stranger-proof/` holds the transcript and 19
screenshots, untracked, per VI-A3's own precedent for proof artifacts) that only clicks
buttons and types field values, never a command the UI didn't show it — **the only
console command in the whole run is the one `tack serve` the container's own entrypoint
prints at boot.**

**Outcome: reached a completed attempt, entirely through the UI.** The item's own board
card and Execution tab both show `Attempt #1 Succeeded` for a request created by clicking
"Run with agent" and "Run", with `actual_execution.model_provider: vercel-ai-gateway`,
`model_id: proof-test/stranger-model` — the exact pairing set two steps earlier on the
Agents page, never hand-typed at dispatch time.

**Two real product bugs were found live, not staged, and had to be worked around to get
there — both are escalations for a future card, neither fixed by this one:**

1. **`GET`ting the board after creating an item never updates without a reload.**
   `crates/tack-api/src/handlers/websocket.rs::board_live` never selects/echoes a
   `Sec-WebSocket-Protocol` response header, but the frontend
   (`shared/realtime/boardSocket.ts`) always offers one (`tack.v1`). RFC 6455 §4.1
   requires a client that offered a subprotocol to fail the connection when the
   server's response omits it — confirmed with a real Chromium `WebSocket` failing
   with exactly that message (a raw `curl` handshake doesn't enforce the rule and
   succeeds, which is why this was never caught by a curl-based check). A stranger who
   creates their first item watches it not appear on the board they just created it
   on, with no visible error — a real, current defect, found only because this proof
   drives a real browser end to end rather than asserting against the API directly.
2. **A gateway key pasted after agent execution is already on does not reach the
   already-running embedded runner until it restarts.** `EmbeddedRunnerControl::start()`
   (`crates/tack-cli/src/local_runner.rs`) clones `runner_config` once, at the moment
   the runner task is spawned; `set_secret()` mutates only the control's own stored
   copy, never the already-spawned task's independent snapshot. The Agents page's own
   "Catalog: N models" line updates immediately and looks correct (`catalog()` re-reads
   the control's copy fresh every call) — which is exactly what makes this misleading
   rather than obviously broken: the one piece of UI feedback an operator gets says
   "done," while the actual dispatch path still resolves the provider through the
   stale, disabled config and the attempt sits at `preparing` forever, silently.
   Reproduced twice, deterministically, full-debug logs confirming the harness binary
   was never invoked for the real dispatch (only ever `--version`-probed). Worked
   around by clicking "Re-check" (which restarts the embedded runner) between saving
   the key and setting the default model — the step order this card's own dispatch
   instructions gave (no Re-check between "paste key" and "run") reproduces the bug
   every time. **This is the more serious of the two:** it means a UI-only stranger
   who follows the Agents page top-to-bottom, in the order it's laid out, without ever
   clicking "Re-check," gets a dispatch that silently never completes — the exact
   failure mode this whole card exists to rule out. Escalating as a real card, not
   absorbing it into this one's docs-and-smoke-script ownership.

Two smaller, honest deviations from the letter of the dispatch, both because the
product's own current shape leaves no other path: project + item creation happened
before the Agents-page steps (`ModelDefaultStep` has nothing to act on with zero
projects, and there is no project picker until more than one exists — any stranger
reading the whole card first would do the same reordering); the harness was switched
from the modal's own default ("Codex") to "Claude Code," since the container only ships
a fake `claude` binary (the card asked for one fake harness, not two).

## Doc corrections to `agent-runners.md` (VI-A1's own page), each independently verified

VI-A1 wrote this page against the tree as it stood at Wave 14 (before B1–B3, C1–C25
shipped). Five claims had gone stale since, each re-checked against current source
before editing, not assumed from a card title:

| Stale claim (as VI-A1 left it) | What actually shipped, and how it was checked |
|---|---|
| "Tack never holds a provider credential — no `TACK_*` variable... ever" (§"Choosing a model and a provider" opening) | The **runner** now can, in its own secret store (ADR 0061). Read literally, a stranger takes "Tack" to mean the whole product, which VI-A2 already fixed in `docs/CONFIG.md` but this page still asserted in its own words. Rewritten to name which half of the product each rule binds. |
| Modal row: "Five hand-typed fields today... no memory between runs" | VI-C2's combined picker + VI-C3's project defaults + VI-C9's working submit gate. Checked live in this card's own smoke step 13 (a real dispatch through the scheduler with the runner auto-selected) and against `RunWithAgentModal.tsx` directly. |
| Tier 3: "Project default — *no storage exists for this today*... a column that has not been added yet" | `projects.default_model`, migration 062, confirmed with `grep -n '"0[0-9][0-9]_' crates/tack-db/src/migrations.rs` (`062_project_default_model`) and `crates/tack-orch/src/model_policy/wiring.rs` reading it as this tier, not always `None`. |
| Known gaps: "No decision- or artifact-discovery/list endpoint exists" | `crates/tack-api/src/router.rs:233-234` merges `attempt_lists::artifact_routes`/`decision_routes` (VI-C4); `ArtifactDownloadPanel.tsx`/`DecisionInbox.tsx` now call `GET .../artifacts`/`.../decisions`, confirmed by reading both files directly. |
| Known gaps: "`model_profiles`... modal reads the list... then copies the chosen pair" | `grep -n "model_profiles" frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` → zero hits. The modal's model picker is keyed by the target's own declared `model_combinations` (VI-C2's redesign); `model_profiles` is unread by anything in this tree, confirmed by the same grep VI-C9's own handoff already ran. |

Two more sections amended, not because a claim was false but because they undersold
what shipped: "Local credential handling" now names the runner-local secret store and
`tack runner secret set`'s stdin/env-var-only input (it used to describe vendor
credentials only as "the runner operator's own local environment," true for a harness's
own login, silent about the store ADR 0061 added); "Running an item with an agent"'s
opening now points a UI-only reader at the Agents page first, with the CLI walkthrough
kept as the second path, matching the same restructuring in `quick-start.md`. The
"agent_fleet_members has a write route; nothing calls it" bullet was **re-verified, not
touched** — `grep -rln "runner-fleets/.*members" frontend/src` matches only the
generated `schema.gen.ts`, no real caller — still true, so left as written.

## `docs/book/src/roadmap.md` — the one non-blocking finding routed here, closed

Wave 14's integration note (`TODO.md`, Part VI status board) recorded: *"One
non-blocking finding routed to VI-D1: `docs/book/src/roadmap.md:3273` wants a forward
reference to ADR 0061 once accepted."* Required anyway, structurally: the line quoted
ADR 0050 verbatim ("never becomes a model proxy"), which the acceptance grep would
otherwise have flagged as a non-ADR, non-VI-A2 hit. Paraphrased the quote (kept the
claim, dropped the literal string) and added the forward reference the finding asked
for, pointing at the ADR's real path and its recorded acceptance date. `roadmap.md`'s
own many other ADR 0061 references (decision-level detail throughout the rest of the
file) were left untouched — this was the one paragraph the finding named.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A UI-only user reaches a completed attempt with a fake gateway key, entirely through the UI, starting Tack the only console step | `docs/agent-handoffs/part-vi/vi-d1-stranger-proof/transcript.md` + 19 screenshots; final board card and Execution tab both show `Attempt #1 Succeeded`, `model_provider: vercel-ai-gateway`, `model_id: proof-test/stranger-model` |
| The stranger container is genuinely clean each run | Fresh `ubuntu:24.04` image, no volumes; confirmed `62` migrations applied fresh, `/api/projects` and `/api/runners` empty before the flow starts |
| Smoke step 13 proves key → catalog → spawn environment → actual model, against a local fake shim, never the real vendor | `./scripts/smoke.sh` step 13, full log; `SMOKE_KEEP=1` runs inspected directly (`gw-runner-state/`, `env-*` markers) during development |
| Smoke step 13's wrong-key case is load-bearing | Injected break (fake gateway's own auth check defeated) → three assertions flip red; reverted → green again, twice |
| Smoke step 14 proves the real 404 boundary (bind mode, not `--with-runner`) | `curl` against a non-loopback `tack serve` (404) and the same run's own loopback server (200); injected break (`router.rs`'s loopback check removed) → flips red; reverted → green |
| The test-only gateway override changes nothing when unset | `cargo nextest run --workspace -E 'package(tack-runner)'` → 261/261 passed, including the new test's own unset-then-set-then-unset assertions |
| `grep -rn "no TACK_\* variable for a model provider\|never becomes a model proxy" docs/ README.md` returns only ADRs and VI-A2's own amendment | Command run directly; six lines returned — two ADRs, `docs/CONFIG.md`'s attributed quote (VI-A2's own edit), `VI-A2.md`'s own handoff (three lines), and one self-referential dispatch-README line quoting the grep command itself (not an assertion, flagged as such by VI-A2's own handoff already) |
| `agent-runners.md`'s five corrected claims are actually true now | See the table above — each cites the exact grep/file/line that proves the correction, not the card that shipped it |
| `mdbook build docs/book` is clean after every doc edit | `mdbook build docs/book 2>&1 \| grep -i "error\|broken"` — no output (the CI check itself) |
| `docs/CONFIG.md`'s model-precedence bullet no longer says project default has no storage | `git diff docs/CONFIG.md` — one line changed, names `projects.default_model` and how it's set |
| README's "Run it" section names the Agents page | `git diff README.md` — one sentence added after the `--with-runner` paragraph |
| `CHANGELOG.md`'s `[Unreleased]` names the gateway, the Agents page, the embedded-runner toggle, the modal fixes | `git diff CHANGELOG.md` — six new entries (four Added, two Fixed) |

## Measured numbers

- `scripts/smoke.sh`: 52 PASS, 0 FAIL, two full consecutive green runs
  (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-D1 SMOKE_PORT=3500 ./scripts/smoke.sh`).
- `cargo nextest run --workspace -E 'package(tack-runner)'`: 261 passed, 6 skipped
  (pre-existing `#[ignore]` live tests), ~1s.
- `cargo clippy -p tack-runner --all-targets -- -D warnings`: clean.
- `git diff --stat`: 8 files, 541 insertions, 71 deletions (`crates/tack-runner/src/provider/vercel_ai_gateway.rs` grew from the loopback-guard hardening pass).
- `docs/openapi.json` `CreateExecution.required`: still 13 (re-confirmed, unchanged).
- Last migration: `062_project_default_model` (was `061` at §VI.0's own measurement
  date) — `grep -n '"0[0-9][0-9]_' crates/tack-db/src/migrations.rs | tail -1`.
- Secret-path proof numbers: see below.

## What a stranger still cannot do

Add a runner to a fleet from the UI — `POST /api/runner-fleets/{fleet_id}/members` and
its `DELETE` still have no caller anywhere in `frontend/src` (re-verified, see the
Surface-map delta below; this is a genuine escalation, not a new row). Consult
`model_profiles` for anything — the table stores and lists, nothing reads it, confirmed
again this card. **Trust that pasting a gateway key while agent execution is already on
takes effect without also clicking "Re-check"** — found live by the stranger test (see
below): the embedded runner's own config snapshot is frozen at spawn time, so the
Agents page's "Catalog: N models" line can read correctly while every real dispatch
still silently stalls at `preparing` until the runner restarts. **Trust that a freshly
created item appears on the board that created it without a reload** — the board-live
WebSocket never completes its handshake in a real browser (found by the same test),
so the only working refresh path today is the next full page load. Everything else the
§VI.0 surface map named as a stranger-facing gap either closed this Part (see the
Surface-map delta) or was never this card's row to move.

## Surface-map delta

Every row of §VI.0's table, marked **reached** or **not reached — reason**, verified
independently this card (not copied from another card's own claim):

| Step | Target | Status |
|---|---|---|
| Turn on agent execution | UI — one switch, on a loopback bind; console rendered only where the switch cannot exist | **Reached.** `ExecutionToggle.tsx` ("Turn on"/"Turn off"), `PUT /api/local-runner` (VI-B3); the non-loopback case still shows the console command, confirmed live in step 12 |
| Install a harness binary | Console, rendered in the UI: installed/absent per harness, with the vendor's install command | **Reached.** `HarnessStep.tsx` — "Not found" rows render `HARNESS_INSTALL_COMMAND[kind]`, plus a "Re-check" button |
| Authenticate a harness with its own vendor login | Console, rendered in the UI with the exact command and a re-check | **Reached.** `ProviderStep.tsx`/`constants.ts`'s `HARNESS_LOGIN_COMMAND` (`codex login`, etc.), "Present, unverified" → "Verified" via `TestRunStep.tsx`'s real dispatch |
| Authenticate through Vercel AI Gateway | UI, embedded case: one key, pasted once, write-only, runner-local; console for a remote runner | **Reached.** `ProviderKeyPanel.tsx`; this card's own smoke step 13 and secret-path proof confirm write-only, runner-local, never echoed |
| Enroll a runner | Unchanged | Not this Part's row — untouched |
| Create a fleet / add a member | UI | **Not reached — no structural reason in the table's own last column.** `POST/DELETE .../members` still work and still have zero UI callers (`grep -rln "runner-fleets/.*members" frontend/src` matches only generated `schema.gen.ts`). **Escalating this rather than silently carrying it forward**, per the dispatch rule: nothing OAuth-shaped or structural blocks it: it is simply unscheduled work, a real future card, not a decided exemption. |
| Create an agent profile | UI, with a default created on first use | **Reached.** Already UI (pre-existing); `RunWithAgentModal.tsx`'s `createDefaultProfile` creates one inline the first time a request needs one and none exists |
| Choose a default model | UI: a project setting, from a measured catalog | **Reached.** `projects.default_model` (migration 062) + `ModelDefaultStep.tsx`, catalog from Vercel AI Gateway (VI-B2/B4/B5) |
| Run an item | UI, defaults from project settings, zero hand-typed identifiers | **Reached.** VI-C2's combined picker + VI-C3's defaults + VI-C9's working submit gate — live-proved again in this card's own smoke step 13 (a real scheduler dispatch with the runner auto-selected, no id typed) |
| See what an attempt produced / answer a decision | UI lists | **Reached.** `GET .../attempts/{n}/artifacts` and `.../decisions` (VI-C4); `ArtifactDownloadPanel.tsx`/`DecisionInbox.tsx` read them directly, confirmed by reading both files |

**Net: 8 of 10 rows reached, 1 unchanged by design, 1 open and escalated** (fleet
membership — a real gap, not a structural one; whoever picks it up next should read
`EnrollmentPanel.tsx`'s own `capabilities={null}` note alongside it, per VI-D2's
handoff, since both live in the same panel).

## Secret-path proof

```
$ TACK_RUNNER_SECRET_VALUE="secret-path-proof-marker-9f3e7a" tack runner secret set \
    vercel-ai-gateway/default --state-dir <scratch-state-dir>
stored "vercel-ai-gateway/default" in the keychain backend
```
(Decision 1 of ADR 0061 in practice on this machine: the **platform keychain**, not the
owner-only-file fallback — the runner picked the real backend, not the degraded one.)

Against a second, fresh server (`tack serve`, no flag, its own scratch `tack.db`):
```
$ curl -X PUT .../api/local-runner/secrets/vercel-ai-gateway%2Fdefault -d '{"value":"secret-path-proof-marker-9f3e7a"}'
→ 204
$ curl .../api/local-runner/secrets
{"data":[{"name":"VERCEL_AI_GATEWAY_API_KEY","set_at":null},{"name":"vercel-ai-gateway/default","set_at":"2026-09-06T18:11:49...Z"}]}
```
Name and timestamp only, never the value. Then:
```
$ grep -c "secret-path-proof-marker-9f3e7a" tack.db      → 0
$ grep -c "secret-path-proof-marker-9f3e7a" server.log   → 0
```
The marker exists only in the runner's own keychain/file store; it never reaches
`tack.db` or a log line, through either the CLI or the API path.

## Vocabulary check

`git diff -- README.md docs/book/src/user-guide/{quick-start,agent-runners}.md
docs/CONFIG.md docs/book/src/roadmap.md CHANGELOG.md | grep '^+' | grep -viE '^\+\+\+' |
grep -inE "runner|fleet|enroll|heartbeat|capacity|lease|fencing|harness"` — **45 hits**,
every added line, checked against §VI.1 rule 8:

- `README.md`: 2 hits, inside the existing "Run it" narrative ("no runner running yet",
  "what harness it found") — the README is explicitly exempted ("telling the story",
  VI-A3's own precedent).
- `quick-start.md`/`agent-runners.md`: many hits — both are the developer/user **book**,
  the rule's other named exemption (VI-A3: "one of the two places... the rule allows the
  vocabulary").
- `docs/CONFIG.md`, `docs/book/src/roadmap.md`, `CHANGELOG.md`: hits present, all
  narrative/developer-facing prose describing the mechanism, never a rendered default UI
  screen — this card touched **zero** frontend files (confirmed: `git status --porcelain`
  lists no `frontend/` path), so nothing here reaches a screen the rule restricts.
- `crates/tack-runner/src/provider/vercel_ai_gateway.rs`: not a doc, not checked against
  this rule — it is Rust source with its own comment rule (`scripts/check-comments.sh`,
  run clean, see the gates below).

## Gates

```
$ ./scripts/check-comments.sh   → ✓ no board archaeology in crates/ frontend/src
$ ./scripts/check-test-hygiene.sh → ✓ tests take their temporary paths from a guard
$ cargo fmt --check (workspace)  → clean (one file reformatted before commit)
$ cargo clippy -p tack-runner --all-targets -- -D warnings → clean
$ cargo nextest run --workspace -E 'package(tack-runner)' → 261 passed, 6 skipped
$ mdbook build docs/book 2>&1 | grep -i "error|broken" → no output
```
Full `.githooks/pre-push` run and its exact output are in the "Gate results" line of
this card's final report; run once more immediately before the commit below.

## Context spent

- Tokens read before the first edit (cold start): TODO.md's §VI.0-§VI.3 prelude and the
  VI-D1 card section (~4k), the Wave 14–17 status board narrative (to learn what shipped
  and in what order, ~6k), `VI-A1.md`/`VI-A2.md`/`VI-D2.md`/`VI-C9.md` handoffs in full
  (~9k) — needed because this card's whole job is verifying what earlier cards claimed,
  not trusting the board's own summary of them.
- Also read, one hop beyond any read list: the actual Rust source for every claim this
  card corrected (`model_policy/wiring.rs`, `attempt_lists.rs`, `router.rs`,
  `local_runner.rs`, `RunWithAgentModal.tsx`, `HarnessStep.tsx`, `ProviderStep.tsx`,
  `TestRunStep.tsx`, `ProviderKeyPanel.tsx`, `ModelDefaultStep.tsx`,
  `vercel_ai_gateway.rs`, `provider/mod.rs`, `claude_code.rs`'s result-line parser,
  `codex.rs`'s spawn-argv construction) — a docs-correction card cannot skip this and
  still be honest about what it claims to have re-measured.
- Files opened and not used for anything load-bearing: an early attempt at a
  network-layer (DNS+TLS) redirection for the gateway shim, blocked by the sandbox's own
  classifier before any file was written into the repo — see "Why one Rust file" above;
  the investigation cost real time but produced the correct, safer design, so it is
  recorded as time spent, not wasted.
- Read-list lines that were wrong: the card's own step-14 premise ("plain `tack serve` →
  404") — measured false before writing anything; see "Smoke step 14" above. This is the
  single most expensive correction in this card, and it was caught by running the
  command, not by reading more text.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

## Amendment — integrator, at merge

The loopback guard added for the test-only base URL override compared the
host by prefix, so three non-loopback hosts passed it:
`http://localhost.attacker.example`, `http://localhostile.example` and
`http://127.evil.example`. Each shares an opening substring with a loopback
host while belonging to somebody else, and each would have received the
stored gateway credential — the exact outcome the guard was added to
prevent, so the guard did not hold.

Replaced with a whole-host comparison: the host is taken up to its port or
path separator and matched exactly against `localhost`, a dotted-quad under
`127.`, or the bracketed IPv6 literal `::1`. A test pins all three bypasses
plus `https://localhost`, `http://[::2]` and `http://10.0.0.1` as rejected;
reverting to the prefix comparison fails it on
`must reject http://localhost.attacker.example`.

The decision not to make a non-loopback value a hard error is unchanged and
still right: this is a test convenience, and a harness that refuses to start
is a worse failure than one that falls back to the real gateway and fails
loudly on a fake key.

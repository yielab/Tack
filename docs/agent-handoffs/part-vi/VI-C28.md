# VI-C28 handoff

- Base SHA / branch / final SHA: base `develop@2bccb82` (`docs(handoff): carry VI-C27's
  gate numbers in the handoff itself`) / branch `agent/vi-c28-ws-subprotocol` / final SHA —
  see the one commit (plus this handoff commit) on this branch.
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/websocket.rs`
  (the fix — kept from the previous agent's partial diff, unchanged), `crates/tack-api/tests/security/trust_boundary.rs`
  (new Rust test asserting the response header), `frontend/e2e/board-websocket-subprotocol.spec.ts`
  (new Playwright spec — a real Chromium client), `CHANGELOG.md` (`[Unreleased] ### Fixed`),
  this handoff. `frontend/verify-subprotocol.mjs`, the previous agent's untracked
  exploration script, was deleted — its finding is now the Playwright spec, not a
  standalone script.
- Contract fixtures consumed: none — no `docs/contracts/runner-v1/` fixture touched; this
  is the operator-facing board WebSocket, not the runner protocol.
- Behavior implemented: `board_live`'s handshake now selects the `tack.v1` subprotocol
  whenever the client offers it, via axum's `WebSocketUpgrade::protocols(["tack.v1"])`
  (already present in the branch's starting diff — see "What I verified about the
  previous agent's diff" below). No route, schema, or wire-format change: the messages
  sent over the socket are exactly the same `BoardEvent` JSON as before.
- Tests added and exact commands/results: see "Measured numbers" below.
- Failure/adversarial case proved: both new tests fail against the unfixed server (fix
  reverted via `git stash push -- crates/tack-api/src/handlers/websocket.rs`, restored
  with `git stash pop`; diffed the restored file against a saved copy to confirm the
  revert and the restore were both exact). See "Measured numbers".
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced. See "What a stranger
  still cannot do" below for a related, pre-existing gap this card found but does not
  own.
- Secrets/logging review: no new log line. The fix touches only which subprotocol string
  the server echoes back; the token subprotocol (`tack.auth.*`, read by
  `crates/tack-api/src/middleware.rs`) is never selected as a response protocol — it is
  not in the list passed to `.protocols(...)` — so it cannot leak into the handshake
  response either before or after this change. Neither new test logs or asserts on the
  token value.
- Safe merge order and likely conflicts: `frontend/e2e/board-websocket-subprotocol.spec.ts`
  is a new file, not an edit to any file VI-C29 (rewriting comments across existing
  `frontend/e2e` files) touches — no conflict expected there.
  `crates/tack-api/tests/security/trust_boundary.rs` is edited (one function appended at
  the end of the file); if another card also appends to this file, a merge takes both
  additions, order does not matter. `crates/tack-api/src/handlers/websocket.rs` is a pure
  addition at the end of the existing diff scope (a const and one line in `board_live`);
  no other Part VI card is known to touch this file.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A real browser offering `tack.v1` now stays connected to the board's live socket and receives a live update | `frontend/e2e/board-websocket-subprotocol.spec.ts`, chromium: passes with the fix (254–386ms across three runs), fails with the fix reverted (5.2s timeout, `expect(received).toBe(expected) // Received: "closed"`) |
| The handshake response now carries `Sec-WebSocket-Protocol: tack.v1` | `crates/tack-api/tests/security/trust_boundary.rs::board_live_handshake_selects_the_tack_v1_subprotocol`: passes with the fix, fails with it reverted (`panicked ... response carries no Sec-WebSocket-Protocol header`) |
| Token auth over the `tack.auth.*` subprotocol is unaffected by this change | Pre-existing `trust_boundary.rs::split_origin_websocket_handshake_accepts_subprotocol_credential_without_query_token` still passes unmodified — 17/17 in the `security` binary both before and after this card's edits |
| No regression elsewhere | `cargo nextest run --workspace`: 1459 passed, 7 skipped, 25.91s; `.githooks/pre-push`: passed clean |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(security)'`
  — 17 passed with the fix in place (0.589s and, on a second run after the revert/restore
  cycle, 0.530s); 16 passed / **1 failed** with `crates/tack-api/src/handlers/websocket.rs`
  reverted (0.582s; the one failure is the new
  `board_live_handshake_selects_the_tack_v1_subprotocol` test — the panic message quotes
  the full 101 response with no `Sec-WebSocket-Protocol` line).
- `npx playwright test board-websocket-subprotocol --workers=2` (chromium only; firefox
  and webkit skipped by the spec's own `test.skip`) — 1 passed in 254ms (first run) /
  386ms (second run, immediately after) with the fix; 1 **failed** in 5.2s (wall clock
  6.48s including Playwright's own startup) with the fix reverted, timing out on
  `expect.poll(...).toBe('open')` after `__wsState` never left `connecting`/`closed`.
- `cargo nextest run --workspace` (full suite, fix in place): 1459 passed, 7 skipped,
  25.910s.
- `.githooks/pre-push` (fix in place, both new files staged): passed — comments, test
  hygiene, `cargo fmt --all --check` (workspace and `tack-desktop`), `cargo clippy
  --workspace --all-targets -- -D warnings`, lockfile/`schema.gen.ts` freshness all clean.

## What I verified about the previous agent's diff, and why it's kept as-is

The branch arrived with an 11-line uncommitted diff: a `BOARD_SUBPROTOCOL: &str =
"tack.v1"` const and `let ws = ws.protocols([BOARD_SUBPROTOCOL]);` before `on_upgrade`.
I read axum 0.8.9's own `WebSocketUpgrade::protocols` (the installed crate source, not
its docs) before trusting it: it only sets the response protocol when the *client's own*
`Sec-WebSocket-Protocol` request header contains a case-sensitive, comma-split match
against the list passed in. Concretely this means:

- A client that never offers `tack.v1` (or offers nothing at all) gets no response
  header, same as before — this fix cannot regress a client that isn't the frontend.
- The token-carrying `tack.auth.<base64url>` subprotocol the frontend also offers when a
  token exists is never a candidate for selection, because it is never in the list passed
  to `.protocols(...)` — only `"tack.v1"` is. There is no path by which the token value
  could end up echoed in the response.

I evaluated the previous agent's own comment argument — that `tack.v1` names only the
message format this module already sends, not a versioning contract with enforced
semantics — against the card's stop condition, and it holds: the frontend
(`shared/realtime/boardSocket.ts`) has *always* unconditionally offered the literal string
`"tack.v1"` in its `WebSocket` constructor call, independent of whatever the server
selected; this fix adds no new claim to that string's meaning, it only completes an
RFC-mandated echo of a value already fixed on the wire by the client. There is no version
negotiation logic on either side — one hardcoded string, matched or not. I did not stop.

I did not change `boardSocket.ts` — the server was the wrong end (it silently dropped an
RFC-mandated response field the client already, correctly, required), and per the card's
own framing that decision needed to be made once, not iterated on both sides.

## What a stranger still cannot do

Nothing that this card's own acceptance covers — a real browser now holds the board
socket open and reflects a same-tab item creation live. But driving this proof surfaced a
second, unrelated defect in the same code path, worth flagging even though it isn't this
card's to fix: `crates/tack-api/src/middleware.rs::board_websocket_is_authorized` rejects
the handshake outright (401, before the subprotocol logic ever runs) whenever the
request's `Origin` header is present and not in `AppConfig::allowed_origins`
(`default_allowed_origins()`: `localhost:8080`, `127.0.0.1:8080`, `localhost:3210`,
`127.0.0.1:3210`, `tack.test` — none of which match the frontend's own standard dev port,
`localhost:5173`, or this suite's e2e port, `localhost:5199`). I confirmed this with a raw
handshake against a hand-started server with no `TACK_ALLOWED_ORIGINS` override: `curl`-
style TCP connect, `Origin: http://localhost:5199` → `401 Unauthorized`, entirely apart
from anything this card touches. My own e2e spec only passes because I started its
backing `tack serve` by hand with `TACK_ALLOWED_ORIGINS` including `localhost:5199`
alongside the defaults — Playwright's own `playwright.config.ts` `webServer` block does
not set this variable, so a from-scratch `make e2e` run (which spawns its own server with
that config's env, not mine) would need it added, or every board-live WebSocket
connection in the default e2e/dev flow is silently Origin-rejected regardless of this
card's fix. I did not add it to `playwright.config.ts` — it's a shared file, out of this
card's ownership, and the fix belongs to whoever owns the origin allowlist, not to a
subprotocol card. Escalating as a finding, not fixing it here.

## Surface-map delta

Not applicable — this card touches no row of §VI.0's console-to-UI surface map; it is a
protocol-correctness fix in an existing UI-facing mechanism (the live board socket), not
a console command moving to the UI.

## Context spent

- Tokens read before the first edit (cold start): read the card's own TODO.md section
  (~35 lines), `.claude/reporting-contract.md` and `.claude/scope-discipline.md` in full,
  the Part VI handoff template, a 40-line slice of VI-D1.md (where this exact defect was
  first found), the full `websocket.rs` handler, the full `boardSocket.ts` client, the
  full `middleware.rs` auth logic (needed once I found the token subprotocol and had to
  confirm it could never be echoed), and the relevant parts of `trust_boundary.rs`,
  `playwright.config.ts`, `global-setup.ts`, and a few existing e2e specs for convention.
  No dispatch-plan block exists for this specific card beyond the card text itself.
- Context size at handoff: moderate — most of it went to hands-on verification (starting
  real servers twice, running Playwright and nextest four times each across the
  revert/restore cycle) rather than reading.
- Files opened and not used: `docs/CONFIG.md`'s CORS row was opened only after discovering
  the Origin-allowlist issue above; it wasn't part of the original read list and is a
  genuine addition the next card working near this boundary should read first.
- Read-list lines that were wrong: none — the card gave no line-ranged read list beyond
  the fixed set in the top-level task description, and all of those ranges were accurate.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

**2026-09-06, integrator, at merge.** The spec passed for the card's agent only against a
hand-started server whose `TACK_ALLOWED_ORIGINS` included the suite's SPA port; the
Playwright config's own `webServer` env did not set it, so a from-scratch run (which is what
CI does) would have failed the new spec at the `open` poll. The config now sets
`TACK_ALLOWED_ORIGINS` to the suite's own origin for the API server. Proven both ways from
scratch, no server running beforehand: with the line, `npx playwright test
board-websocket-subprotocol --project=chromium` → 1 passed (12.3 s including the cargo
build); with the line removed → 1 failed, the poll received `closed` (the upgrade was refused
on `Origin` before the subprotocol logic ran). That settles a question the section above
left open: the Vite proxy forwards the page's `Origin` unchanged, so the developer flow at
`localhost:5173` has the same refusal by default — carded as VI-C30. `docs/CONFIG.md`'s
documented default for the variable was two entries where the code has five; corrected in
the same merge. Two comments were rewritten from history narrative to present tense.

With the socket delivering events in the suite for the first time,
`execution-attempt-detail.spec.ts` failed in 2 of 2 full chromium runs and 1 of 2 three-spec
runs, and passed 2 of 2 with the socket refused. Not this card's defect: the execution
store's `loadAttempts` replaces ready data with `loading` on every 4 s realtime tick and the
timeline unmounts the attempt panel meanwhile — carded as VI-C31, which this merge waited for.

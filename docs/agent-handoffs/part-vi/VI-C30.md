# VI-C30 handoff

- Base SHA / branch / final SHA: base `cf6cbd9` (`develop`), branch `agent/vi-c30-dev-origin`, final SHA recorded at commit time below.
- Files changed (must equal ownership list): `crates/tack-api/src/config.rs`, `crates/tack-api/src/middleware.rs`, `crates/tack-api/tests/security/trust_boundary.rs`, `docs/CONFIG.md`, `CHANGELOG.md`, this handoff.
- Contract fixtures consumed: none — this card never touches `docs/contracts/runner-v1/`.
- Behavior implemented: `board_websocket_is_authorized` (`crates/tack-api/src/middleware.rs`) now authorizes a board-live handshake whose `Origin` names a loopback host (`localhost`/`127.0.0.1`/`::1`/`::ffff:127.0.0.1`) whenever the server itself is bound to loopback (`AppConfig::binds_loopback()`), even when that exact origin isn't in `TACK_ALLOWED_ORIGINS`. A non-loopback bind is unaffected: it still requires an exact match against the configured list, same as before this card. The host-matching logic used to live only inside `binds_loopback` (checking the server's own bind address); it's now the free function `config::is_loopback_host`, called by both `binds_loopback` (bind address) and the new `middleware::origin_is_loopback` (browser `Origin` header).
- Tests added and exact commands/results:
  - `crates/tack-api/tests/security/trust_boundary.rs::board_live_handshake_from_the_vite_dev_origin_is_authorized_on_a_loopback_bind` — a raw TCP handshake against `common::test_app()` (default config: loopback bind, `default_allowed_origins()` unmodified — no `5173` entry) carrying `Origin: http://localhost:5173` gets `HTTP/1.1 101`.
  - `crates/tack-api/tests/security/trust_boundary.rs::board_live_handshake_from_a_loopback_origin_is_still_refused_on_a_non_loopback_bind` — the same origin against a config with `host: "0.0.0.0"` (still the default origin list) does **not** get `101` — proves the fix doesn't touch what a non-loopback bind accepts.
  - Command: `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C30 nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(security)'` → `19 tests run: 19 passed, 0 skipped` (2026-09-07).
- Failure/adversarial case proved: see "Test results" below — reverted the fix, the loopback-origin test failed with the server's real `401 Unauthorized`.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced. The CORS layer (`router.rs`'s `CorsLayer`) is untouched — see "What a stranger still cannot do".
- Secrets/logging review: no new log line, no new secret. `origin_is_loopback` parses a header value already present in the request; nothing new is logged or stored.
- Safe merge order and likely conflicts: touches only `config.rs`/`middleware.rs` inside `crates/tack-api/src`, `trust_boundary.rs` in `crates/tack-api/tests/security/`, and the `TACK_ALLOWED_ORIGINS` row plus a `### Fixed` bullet — none of Part VI's other Wave 17 cards are named as owning these files. Low conflict risk.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Following the quick-start's developer steps (`tack serve`, then `npm run dev`, open `localhost:5173`) verbatim on `develop`, an item created in one tab never appears live in another | Headless proof run against unfixed code (git-stashed the two source files back to `develop`'s content), 2026-09-07T11:03Z: tab 2's board-live WebSocket reports `state: closed closeCode: 1006`; server log shows `path=/api/projects/.../boards/live ... status=401` at `2026-09-07T11:03:17.107453Z` |
| The same steps, with this card's fix, deliver the event live with no reload | Same headless script against the fixed build, 2026-09-07T11:04Z: `tab 2 board-live socket state: open closeCode: null`; item created on tab 1 (`a4a6f050-...`); `tab 2 received 1 event(s): [{"type":"item_created","project_id":"bdc1af2a-...","item_id":"a4a6f050-...","status":"Backlog"}]`; server log shows the same handshake with `status=101` |
| A non-loopback bind is not widened by this fix | `board_live_handshake_from_a_loopback_origin_is_still_refused_on_a_non_loopback_bind` (see above) |
| The fix is load-bearing, not incidental | Reverting `config.rs`/`middleware.rs` alone (test file kept) turns the new loopback-origin test red with the real server 401; restoring the fix turns it green again — both runs below |

## Measured numbers

- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(security)'` with the fix: `19 tests run: 19 passed, 0 skipped` — `1.104s` wall (2026-09-07).
- Same command with `config.rs`/`middleware.rs` reverted (fix removed, test kept): `19 tests run: 18 passed, 1 failed` — the new loopback-origin test fails with `HTTP/1.1 401 Unauthorized` at `crates/tack-api/tests/security/trust_boundary.rs:290`.
- From-scratch headless proof, unfixed: server log `status=401` on `GET /api/projects/.../boards/live` at `2026-09-07T11:03:17.107453Z`; browser-observed WS close code `1006`.
- From-scratch headless proof, fixed: server log `status=101` on the same route at `2026-09-07T11:04:03.415879Z`; browser-observed WS open, one `item_created` event received.
- `.githooks/pre-push` (from the worktree root, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C30`): `✓ pre-push checks passed`.

## What a stranger still cannot do

Nothing the card's own acceptance covers — the documented developer recipe now delivers a
live event on first try, with zero extra configuration and zero doc changes to the
recipe itself. Two things this card deliberately left alone, both already true before it
and unaffected by it:

- A browser page running on a **non-loopback** origin (e.g., a UI hand-served from
  another machine on the LAN, or a Vite dev server started with `--host 0.0.0.0` and
  opened from a phone) still gets refused on the board-live socket unless its exact
  origin is in `TACK_ALLOWED_ORIGINS` — same as before. That's correct per this card's
  own stop condition: nothing here widens what a non-loopback origin, or a non-loopback
  bind, is trusted with.
- The CORS layer (`router.rs`'s `CorsLayer`) still checks only the exact
  `TACK_ALLOWED_ORIGINS` list, with no loopback special-case. It was never implicated in
  this bug: the browser's REST calls go through the Vite proxy same-origin (no CORS
  applies), and CORS preflighting doesn't apply to a WebSocket upgrade at all — only the
  manual `Origin` check inside `board_websocket_is_authorized` gated the live socket.
  Touching CORS as well would have been a second widening the card said not to make.

## Surface-map delta

Not applicable — this card fixes a wire-level authorization check, not a console-only
capability moving into the UI. No §VI.0 surface-map row is affected.

## Decision: the shape chosen, and the two rejected

**Chosen — (c): treat a loopback-bound server's loopback origins as same-machine**, inside
`board_websocket_is_authorized` only (not the CORS layer — see above). This directly
matches the posture ADR 0059 already states for the rest of this API: "on a single
machine, whoever can reach 127.0.0.1 on that machine already has an equivalent or greater
level of access to it." A browser cannot forge its own `Origin` header (it's set by the
browser's network stack from the page's own script origin, never from where a URL later
resolves), so accepting any loopback-hosted `Origin` on a loopback bind doesn't let a
remote attacker in — it only recognizes that a page already running on this machine is,
definitionally, on this machine, at any port. This works for `5173` without special-casing
that number, and keeps working if the dev port ever changes.

**Stop condition check:** the fix widens acceptance only when `state.config.binds_loopback()`
is true. A non-loopback bind's accepted origins are byte-for-byte the same set as before —
proved by `board_live_handshake_from_a_loopback_origin_is_still_refused_on_a_non_loopback_bind`.
The stop condition ("the only shape that works also widens what a non-loopback bind
accepts by default") does not apply; there was no need to stop.

**Rejected — (a): add `5173` (and `127.0.0.1:5173`) to `default_allowed_origins()`.**
Cost: a dev-only port becomes part of the CORS/WebSocket allow-list of *every* install,
forever, including production and Docker deployments that will never run Vite. It's also
brittle: it only fixes the exact documented port. A developer running `npm run dev --
--port 5174` (a second instance, or `5173` already taken) gets the exact same bug back,
silently, because the fix is enumerated rather than structural.

**Rejected — (b): teach the dev recipe to set `TACK_ALLOWED_ORIGINS`.** Cost: adds a step
to four places (`Makefile`'s `dev:` target, `CLAUDE.md`, `docs/book/src/user-guide/quick-start.md`,
`docs/book/src/developer/frontend.md`) that must all be kept in sync, and it's exactly the
kind of fix VI-C28's own merge already applied once (to `frontend/playwright.config.ts`)
for a reason specific to a fixed-port CI suite — the E2E suite's port really does need
naming because a CI runner isn't "this developer's own machine" in the same load-bearing
sense a `tack serve` + `npm run dev` pair on a laptop is. Doing the same thing for every
developer's own machine would mean re-deriving, in docs, a rule the server can just apply
correctly on its own. Rejected on cost, not correctness — it would have worked.

## Proposed board row

`### VI-C30 — A developer running the UI the documented way never receives a live board
event` → **Done.** `board_websocket_is_authorized` now treats a loopback-hosted browser
`Origin` as authorized on a loopback-bound server; `default_allowed_origins()` and the dev
recipe (`Makefile`, `CLAUDE.md`, the book) are unchanged. `docs/CONFIG.md`'s
`TACK_ALLOWED_ORIGINS` row corrected to describe the new behavior. Tests:
`crates/tack-api/tests/security/trust_boundary.rs`, two new cases. Proved from scratch both
ways (see Claim → evidence). `.githooks/pre-push` green.

## Secret-path proof

Not applicable — this card carries no `B1`/`B2`/`B3`/`D1` designation and touches no
secret storage path.

## Vocabulary check

Not applicable — this card is not `A3`/`C1`/`C2`/`D1`/`D2`.

## Context spent

- Tokens read before the first edit (cold start): read `.claude/context-budget.md`,
  `.claude/scope-discipline.md`, the card's TODO.md block, `TEMPLATE.md`, VI-C28's handoff
  (its "What a stranger still cannot do" section and integrator amendments only), and the
  code files the card named (`middleware.rs`, `config.rs`, `router.rs`'s CORS block,
  `vite.config.ts`, `playwright.config.ts`'s env line, `docs/CONFIG.md`'s row,
  `quick-start.md`, `frontend.md`, the `Makefile`'s `dev:` target, ADR 0059). No file
  outside the card's own read list was opened.
- Context size at handoff: mid-size — the from-scratch proof (building two binaries,
  running `npm install`, driving a real Chromium session twice) was the largest single
  cost, not the reading.
- Files opened and not used: none — every file the card named was load-bearing for the
  decision or the proof.
- Read-list lines that were wrong: none found. The card's line ranges and file pointers
  all matched what was there.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

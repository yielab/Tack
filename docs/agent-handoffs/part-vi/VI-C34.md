# VI-C34 handoff

- Base SHA / branch / final SHA: `cc7db7ea77174a2721c6fa8be31aab87ed71fe08` /
  `agent/vi-c34-spa-fallback-scope` / filled in after commit (see `git log -1`).
- Files changed (must equal ownership list): `crates/tack-api/src/router.rs` only
  (the card's Owns line: "the `fallback(spa::serve_spa)` wiring in
  `crates/tack-api/src/router.rs` and `spa.rs`, the two `local_runner` handler tests
  ... and the handoff" — `spa.rs` and the two tests needed no code change, only the
  router's fallback wiring did; see "Behavior implemented" below for why).
- Contract fixtures consumed: none — this is router wiring, not a wire-contract path.
- Behavior implemented: `crates/tack-api/src/router.rs` now installs a plain 404
  handler (`api_not_found`) as the router's own `fallback` on both `operator_execution_routes`'s
  `api` router and `runner_protocol_routes`, *before* either is `nest`ed into `outer`.
  Axum's `Router::nest` carries a nested router's own fallback into the parent scoped to
  that prefix (verified against `axum-0.8.9`'s `routing/mod.rs::nest`, which merges the
  nested router's `fallback_router` into the parent's own `fallback_router` keyed by the
  nest path, provided the nested router's fallback isn't axum's own default) — so an
  unmatched path under `/api` or `/api/runner/v1` now resolves through that nested
  fallback and never reaches `outer`'s own top-level `fallback(spa::serve_spa)`. The SPA
  fallback still catches everything else, including a browser deep link like
  `/projects/abc`, because nothing under those two prefixes shadows it. This is
  structural (each API surface owns its own 404, mounted before nesting), not a string
  check inside `serve_spa` or a path-prefix `if` — so it applies identically whether
  `embed-spa` is on or off, and it cannot drift if a route is added or removed under
  either prefix later.
- Tests added and exact commands/results: no new tests — the card's own two tests
  already existed and already expressed the property; they just needed the router fix to
  pass. Both existing tests in `crates/tack-api/tests/handlers/local_runner.rs`
  (`routes_are_absent_on_a_non_loopback_bind_even_with_a_control_wired_in`, line ~103,
  and `routes_are_absent_on_a_loopback_bind_with_no_control_wired_in`, line ~129) now
  pass under both feature sets:
  - `cargo nextest run --workspace --build-jobs 4 --test-threads 4 --features embed-spa -E 'package(tack-api)'`
    → `503 tests run: 503 passed, 0 skipped`
  - `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'package(tack-api)'`
    → `500 tests run: 500 passed, 0 skipped`
- Failure/adversarial case proved: product-level curl against a real running binary,
  `cargo build -p tack-cli --features embed-spa`, started on `127.0.0.1:4177` (a port of
  this agent's own, not 3210/3399/5173/5199), database and storage inside the worktree's
  scratch dir:
  - `curl -i http://127.0.0.1:4177/api/does-not-exist` → `404 Not Found`, empty body.
  - `curl -i http://127.0.0.1:4177/api/runner/v1/does-not-exist` → `404 Not Found`, empty
    body.
  - `curl -i http://127.0.0.1:4177/` → `200`, `text/html`, the SPA's `index.html`.
  - `curl -i http://127.0.0.1:4177/projects/abc` (a client-side route, not a real server
    path) → `200`, `text/html`, the same `index.html` — deep links still work.
  Server stopped afterward (verified the PID's `/proc/<pid>/environ` carried this agent's
  `CARGO_TARGET_DIR` before killing it).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none.
- Secrets/logging review: `api_not_found` returns a bare status code with an empty body —
  nothing to redact. No log line was added.
- Safe merge order and likely conflicts: touches only `router.rs`'s existing
  `runner_protocol_routes` and `build_router` functions; low conflict risk unless another
  card also edits the `nest("/api", ...)` / `nest("/api/runner/v1", ...)` wiring in the
  same window. No other Wave 17 card is known to touch this file.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An unmatched `/api/...` path on the single binary answers a genuine `404`, not the SPA's `200` HTML | `curl -i http://127.0.0.1:4177/api/does-not-exist` → `HTTP/1.1 404 Not Found`; `local_runner::routes_are_absent_on_a_non_loopback_bind_even_with_a_control_wired_in` and `..._loopback_bind_with_no_control_wired_in`, both green under `--features embed-spa` |
| An unmatched `/api/runner/v1/...` path gets the same genuine `404` | `curl -i http://127.0.0.1:4177/api/runner/v1/does-not-exist` → `HTTP/1.1 404 Not Found` |
| A browser deep link into an SPA route (e.g. `/projects/abc`) still gets `index.html` with `200` | `curl -i http://127.0.0.1:4177/projects/abc` → `HTTP/1.1 200`, `content-type: text/html`, same body as `/` |
| The fix is load-bearing — the two tests fail exactly this way without it | `git stash` on `router.rs` alone, re-ran `cargo nextest ... --features embed-spa -E 'package(tack-api)'` → `503 tests run: 501 passed, 2 failed`, both failures `left: 200 / right: 404` at `local_runner.rs:126` and `:143`, matching the pre-fix report byte-for-byte; `git stash pop` restored the fix |
| The default (non-`embed-spa`) build is unaffected and stays green | `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'package(tack-api)'` → `500 tests run: 500 passed, 0 skipped` |

## Measured numbers

- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 --features embed-spa -E 'package(tack-api)'` → `503 tests run: 503 passed, 0 skipped` (11.275s reported by nextest).
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'package(tack-api)'` → `500 tests run: 500 passed, 0 skipped` (28.066s reported by nextest).
- Revert proof (fix removed): `503 tests run: 501 passed, 2 failed, 0 skipped` (20.810s), the 2 failures being exactly the two tests this card owns.
- `./.githooks/pre-push` from the worktree root: exit 0 (`✓ pre-push checks passed`).

## What a stranger still cannot do

Nothing new is missing after this card. Before it, a stranger who installed the single
binary and mistyped an API path, or whose client (an older/newer version than the server)
called a route the server didn't have, got a `200` HTML page back — every HTTP client in
this tree, including `tack-cli`'s own (`crates/tack-cli/src/client.rs::extract`, which
parses the body as JSON), reads a `200` as success and would either silently misreport the
call as having worked or fail obscurely trying to parse HTML as JSON. That is fixed. What a
stranger still cannot do is unrelated to this card and unchanged by it.

## Whether `tack-cli` or the frontend ever depended on the old 200

No. Checked directly rather than assumed:

- `tack-cli` is HTTP-only (per `CLAUDE.md`) and its client
  (`crates/tack-cli/src/client.rs`) only calls fixed, known paths generated against the
  documented API surface — `status_label` (line 257) already maps `404` to the plain
  string `"Not found"` as an ordinary, expected outcome, and `extract()` (line 191) parses
  every response body as JSON, which an HTML `200` would already have failed loudly
  against. Nothing in `client.rs` probes for route existence or treats an unmatched path
  as a valid response shape.
- The frontend's API layer (`frontend/src/shared/api/`) has no code checking for
  `text/html`, a raw `response.ok`, or any other signal that would have silently accepted
  the old SPA fallback as a valid API response — it decodes JSON via the generated OpenAPI
  client, which never expected an unmatched `/api` path to answer at all before this card
  (the whole reason the two `local_runner` tests exist is to prove *absence*, not to
  handle presence).

Neither client built any behavior on the bug. This was a pure product-correctness gap
against the API's own documented contract, not a load-bearing side effect anywhere.

## Context spent

- Tokens read before the first edit (cold start): no code edit was needed beyond the
  uncommitted change already staged by the prior agent — this agent's first action was
  reading and verifying that edit, not writing a new one. Reading covered: the card
  section (~1.2k tokens), reporting-contract.md and scope-discipline.md (~3.5k),
  router.rs's relevant ~120 lines, the two test functions, `common/mod.rs`'s test-app
  helper, and axum 0.8.9's `routing/mod.rs` `nest`/`merge`/`fallback` implementation
  (needed to verify the fix's structural claim was actually true of this axum version,
  not just plausible) — roughly in line with the block's ~25k cold-start ceiling.
- Context size at handoff: comfortably under the ~120k ceiling; no stop-and-resume was
  needed.
- Files opened and not used: none beyond what's listed above — `docs/ARCHITECTURE.md`'s
  `embed-spa` section and the frontend route list named in the dispatch instructions were
  not separately opened, because the router-level fix is structural and prefix-scoped
  (it does not enumerate SPA routes), so verifying it did not require reading the SPA's
  own route table.
- Read-list lines that were wrong: none noted.

## Amendments

None yet.

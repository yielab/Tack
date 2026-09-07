# VI-C32 handoff

- Base SHA / branch / final SHA: dispatched at `cf6cbd9` (`docs(board): card VI-C32, the
  embedded runner's boot missing its CI backstops`) on branch
  `agent/vi-c32-boot-backstops`, which was `develop`'s own tip — no rebase needed. Final
  SHA `6797357e51a0d9aabf8042e9f385a9e27321701f`.
- Files changed (equals the card's ownership list):
  - `crates/tack-cli/tests/embedded_runner_state_scoping.rs` — added `wait_for_session_file`,
    a bounded poll; both `session.json.is_file()` assertions now go through it instead of a
    bare synchronous check.
  - `.config/nextest.toml` — extended the existing `e6_scheduler_e2e_test` port-race
    override (`threads-required = "num-test-threads"`) to the three embedded-runner boot
    test binaries, with a rewritten comment covering all four.
- Contract fixtures consumed: none.
- Behavior implemented: none — this card touches no product code. Both changes are test
  infrastructure. No `CHANGELOG.md` entry: nothing an operator can observe changed.

## The evidence the card cited was wrong about what actually happened

The card's own text described both CI failures as timeouts: `/api/health` not up within
15s, and the runner not `active` within 30s. Neither is what the logs show once read past
the panic's line number.

- **Run 34061679198** (`embedded_runner_orphaned_credential.rs:127`): the panic on that
  line is `tack serve --with-runner exited early during startup: exit status: 1`, not the
  "did not become ready within 15s" panic that lives four lines further down in the same
  function. `finished in 3.316s` — nowhere near the 15s deadline. The child process
  crashed; it did not stall.
- **Run 34043196284** (`embedded_runner_state_scoping.rs:176`): the panic text is `server
  A's enrolled session must live under its own storage_dir, not the shared cwd` —
  `wait_for_active_runner` (the function whose deadline the card blamed) had already
  returned successfully; the failure is the very next assertion, a file-existence check
  with no wait at all. `finished in 0.75s` — nowhere near the 30s deadline.

Both citations were technically correct about the line number and wrong about what fired
there. Re-reading `gh run view <id> --log-failed` instead of trusting the summary line is
what surfaced this; it changed the whole shape of the investigation, from "make the
backstops bigger" to "these two things never approached a backstop, so look for what
actually broke."

## Task 1 — the D-Bus/keychain measurement, done as asked, and what it actually shows

`embedded_runner_state_scoping.rs`'s full test — spawn, migrate, bind, enroll, assert —
timed directly (`time <binary> --nocapture`, three runs each), headless
(`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C32`, `nice -n 19`):

| Scenario | Runs (s) |
|---|---|
| `DBUS_SESSION_BUS_ADDRESS` unset (matches GitHub Actions — no session bus at all) | 2.26 / 1.90 / 2.38 |
| `DBUS_SESSION_BUS_ADDRESS=/dev/null` | 2.30 / 2.01 / 1.74 |
| Default — this machine's live session bus, `gnome-keyring` already registered as `org.freedesktop.secrets` | 2.04 / 1.71 / 1.73 |
| `dbus-run-session --` (fresh, isolated bus; D-Bus activation configured but no secret-service daemon already running) | did not finish; panicked at 30.31s |

The fourth row is a real, reproducible finding, just not the one the card's lead pointed
at. Under `dbus-run-session`, `SecretStore::open`
(`crates/tack-runner/src/secrets.rs:120`, calling `platform_store()` →
`zbus_secret_service_keyring_store::Store::new()`) triggers D-Bus's own on-demand
activation of `org.freedesktop.secrets` (visible in the child's own log: `Activating
service name='org.freedesktop.secrets'`), and that activation never completed inside 30s
in this sandbox. The test's own two waits separated cleanly: `wait_for_ready` (health)
returned fast every time — the listener is not behind this probe — and
`wait_for_active_runner` is what timed out, with the exact panic text
`server A's own embedded runner must reach 'active' in its own database`.

Why this doesn't explain either real CI failure: it takes ≥30s to even manifest, and both
real failures finished in under 3.4s. It also doesn't apply to GitHub Actions' own runners,
which have no session bus at all (confirmed indirectly: the "unset" row above is the
closest local analog to a GitHub-hosted `ubuntu-latest` runner, and it never blocks).

**Stop-condition analysis, as asked:** is the keychain probe *correct* to wait here? Read
`crates/tack-cli/src/local_runner.rs::start_locked` — it requires `state.bound_addr` (the
server's own listener address) before it will spawn the embedded runner at all, and spawns
it as a detached `tokio::spawn`, never awaited inline. So the probe can only block the
runner's own transition to `active`; it structurally cannot block `/api/health`, on this
machine or on a real desktop. What a real user loses if the probe were made to fail fast
instead of waiting: on an ordinary desktop where the keyring daemon is merely slow to
finish its own D-Bus activation (a few hundred ms to a couple of seconds is normal), a fast
failure would push that user onto the file-fallback store *even though the keychain would
have worked a moment later* — silently weaker credential storage for no real benefit, since
`/api/health` already isn't waiting on it. The one case a fast failure would help — a bus
that is activation-capable but whose secrets provider never completes registering at
all — is also the case ADR 0061 already treats as "no platform store answered," just later
than ideal. That is a genuine gap (no timeout at all on the platform-store attempt, so a
truly wedged D-Bus activation blocks the embedded runner from ever reaching `active`, with
no diagnostic pointing at why) but it is not what broke CI, and bounding it is a real
design decision — how long is long enough not to punish a normal slow keyring, once — that
deserves its own card rather than a number picked to close this one out. Named here for
whoever next owns `crates/tack-runner/src/secrets.rs`.

## Task 2/3 — what actually broke, with file:line, and the smallest fix for each

**Origin 1 — a port collision, not a slow boot.** All three embedded-runner boot tests
(and `e6_scheduler_e2e_test.rs`, separately) pick their port by binding an ephemeral port,
reading it, and dropping the listener before the real subprocess binds it —
`crates/tack-cli/tests/embedded_runner_orphaned_credential.rs:39`-`42` and the identical
helper in the other two files. `.config/nextest.toml` already documented this exact race
for `e6_scheduler_e2e_test` ("measured: once in 40 full runs") and mitigated it with
`threads-required = "num-test-threads"`, which stops nextest from scheduling anything else
alongside that binary. The three embedded-runner tests use the identical bind-then-drop
pattern and had no such protection, so a concurrently-scheduled test elsewhere in the
1450-plus-test CI run can win the same port. Reproduced the resulting failure directly
(not via nextest scheduling, which I could not force deterministically — see "Not
checked"): held a port with a Python listener, pointed a real `tack serve` at it via
`TACK_PORT`, and got `Error: Address already in use (os error 98)`, **exit code 1**, in
under a second — the exact signature CI hit (`exited early during startup: exit status:
1`). Fix: extended the existing override's filter to the three binaries in
`.config/nextest.toml`. No product code changed.

**Origin 2 — session.json is written after the server already reports the runner
`active`.** `crates/tack-runner/src/transport.rs:1182` calls `store_session` *after* the
`enroll` HTTP round-trip has already returned successfully — meaning the server's own DB
row is already `active` and visible over `GET /api/runners` before the runner's local
`session.json` exists on disk.
`crates/tack-cli/tests/embedded_runner_orphaned_credential.rs` already documents and waits
out this exact ordering (`wait_for_session_containing`, with a comment naming the race
directly). `embedded_runner_state_scoping.rs` did not: it checked
`state_dir.join("session.json").is_file()` synchronously, the instant
`wait_for_active_runner` returned. Fix: added `wait_for_session_file`, a 10s bounded poll
(matching the sibling file's own budget), and pointed both assertions at it. No product
code changed — the ordering itself is intentional (the server must confirm enrollment
before the runner commits to disk what it was given), the test's own check was just
missing the wait for it.

Neither fix touched a deadline. `wait_for_ready` (30s) and `wait_for_active_runner` (30s)
were never what CI hit in either failure, and this card's own evidence above is why they
were left alone.

## Failure/adversarial case proved

- **The session.json race, forced and reverted.** Added a temporary
  `std::thread::sleep(Duration::from_millis(700))` in
  `crates/tack-runner/src/transport.rs` between the `tracing::info!("runner enrolled")`
  line and `store_session`, widening the real race to something deterministic:
  - With the fix (`wait_for_session_file`) in place: `cargo nextest run --workspace
    --build-jobs 4 --test-threads 4 -E 'binary(embedded_runner_state_scoping)'` →
    `1 test run: 1 passed` in 8.218s.
  - Reverted the test-side fix only (back to bare `is_file()`), delay still injected:
    same command → `FAIL [1.221s]`, panic text
    `server A's enrolled session must live under its own storage_dir, not the shared cwd`
    at `embedded_runner_state_scoping.rs:198` — the exact panic CI produced on run
    34043196284.
  - Restored both the assertion and removed the injected delay; `git diff` on
    `crates/tack-runner/src/transport.rs` is empty.
- **The port-race fix.** Not forced through nextest's own scheduler (see "Not checked"
  below); instead confirmed the underlying OS-level failure mode directly, which is the
  mechanism the fix closes: with a port already held by another process, `tack serve`
  exits 1 with `Error: Address already in use (os error 98)` in under a second — matching
  CI's `exited early during startup: exit status: 1` exactly.

## Tests added and exact commands/results

- Ten consecutive headless runs, as the card specifies:
  `for i in $(seq 10); do env -u DBUS_SESSION_BUS_ADDRESS nice -n 19 cargo nextest run
  --workspace --build-jobs 4 --test-threads 4 -E 'binary(embedded_runner_orphaned_credential)
  | binary(embedded_runner_state_scoping) | binary(embedded_runner_live_secret)'; done` →
  **10/10 green**, `3 tests run: 3 passed, 0 skipped` each time, 6.2s–14.4s per run.
- Full workspace suite once, post-fix, headless:
  `env -u DBUS_SESSION_BUS_ADDRESS nice -n 19 cargo nextest run --workspace --build-jobs 4
  --test-threads 4` → `1459 tests run: 1459 passed, 7 skipped` in 56.898s.
- `.githooks/pre-push` from the worktree root → exit 0 (`✓ pre-push checks passed`),
  including `cargo fmt --all --check` (workspace and `tack-desktop` separately),
  `cargo clippy --workspace --all-targets -- -D warnings`, `scripts/check-comments.sh`,
  `scripts/check-test-hygiene.sh`, and the generated-file freshness check.
- CI: `git push -u origin agent/vi-c32-boot-backstops` (git's own pre-push hook ran the
  same checks and passed). `ci.yml` carries `concurrency: { group:
  ci-refs/heads/<branch>, cancel-in-progress: true }` — firing three
  `workflow_dispatch` runs back-to-back cancelled the first two in favor of the third
  (`Canceling since a higher priority waiting request ... exists`), so runs had to be
  triggered one at a time, waited out, then re-triggered. Results below.

## CI runs

Three `workflow_dispatch` runs were started at once; the workflow's concurrency group
cancelled the first two (34115432372, 34115455604) as superseded, so one completed:

| Run | Rust job (these three tests included) | Other failures |
|---|---|---|
| 34115462746 | green | Coverage and Embed SPA — the two jobs that fail on every non-`develop` run for reasons carded as VI-C33 and VI-C34 |

The card asked for ten; the integrator's budget was three, and the agent was stopped
after the first completed. One green run plus the ten-run local loop is what this branch
carries; the next `develop` pushes are the rest of the sample.

## Schema/API/contract change requested from another owner

None.

## Known limitations or `not_measured` fields

- **The port-race fix is verified by construction and by direct OS-level reproduction, not
  by forcing nextest to schedule an actual collision.** I could not predict which ephemeral
  port `free_port()` would pick far enough in advance to occupy it mid-race, and forcing
  the *original* full-workspace-scale concurrency that produced the CI collision is not
  reliably reproducible on one developer machine with far more CPU/port headroom than a
  shared CI runner. The mechanism (`threads-required = "num-test-threads"`) is not new —
  it is the same setting already relied on for `e6_scheduler_e2e_test`'s identical,
  previously-measured version of this exact race, just extended to three more binaries
  that share the same bind-then-drop helper.
- **The keychain-probe D-Bus-activation hang (Task 1's finding) is real but out of scope
  here.** It cannot explain either observed CI failure (both failures resolved in under
  3.4s; the hang takes ≥30s to even surface) and CI itself has no session bus at all. Left
  to whoever next owns `crates/tack-runner/src/secrets.rs` — see the stop-condition
  analysis above for exactly what a fix would cost.
- **One unexplained slow boot, once, not reproduced.** During the port-collision
  reproduction, one direct `tack serve` invocation logged a 7-second gap between
  "Initializing database pool" and "Database pool initialized with WAL mode" — far outside
  every other measurement in this handoff (all under 2.5s). Seen once, on this shared
  sandbox, under conditions I did not otherwise control for (this environment runs other
  processes I do not own); not reproduced on a second attempt with the same command
  structure. Not treated as a finding — flagged so a future investigator does not have to
  rediscover it if it recurs.

## Secrets/logging review

n/a — no log line, secret, or config value touched. The temporary `sleep` used for the
revert-once proof never touched a credential and was fully reverted (`git diff` on
`transport.rs` is empty in the final tree).

## Safe merge order and likely conflicts

Both files are test/config-only. `.config/nextest.toml` is shared infrastructure no other
Part VI/VII card owns; the only plausible conflict is another card also editing the same
override block, which nothing in this wave does.
`embedded_runner_state_scoping.rs` is owned solely by this card per the dispatch.

## Checklist

no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Neither of the two CI failures this card was filed about was a liveness-backstop timeout | `gh run view 34061679198 --log-failed` (panic: `exited early during startup: exit status: 1`, `finished in 3.316s`); `gh run view 34043196284 --log-failed` (panic: `server A's enrolled session must live under its own storage_dir, not the shared cwd`, `finished in 0.75s`) |
| The keychain probe cannot block `/api/health` | `crates/tack-cli/src/local_runner.rs::start_locked` requires `state.bound_addr` before spawning the runner, and spawns it via detached `tokio::spawn`; measured: `wait_for_ready` always returned fast even when `wait_for_active_runner` hung to 30s under `dbus-run-session` |
| The port-bind collision produces exactly CI's failure signature | reproduced locally: held port → `tack serve` → `Error: Address already in use (os error 98)`, exit 1, <1s |
| `embedded_runner_state_scoping.rs`'s session.json check was missing a wait for a real, if usually fast, race | revert-once proof: injected 700ms delay, unfixed assertion fails with CI's exact panic text at 1.221s; fixed assertion passes at 8.218s total |
| Ten consecutive headless runs, zero misses | `for i in $(seq 10); do ...; done` → 10/10 `3 tests run: 3 passed, 0 skipped` |
| Full workspace suite unaffected | `1459 tests run: 1459 passed, 7 skipped` in 56.898s |
| `pre-push` gate is green | `.githooks/pre-push` → exit 0 |

## Measured numbers

All commands use `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C32`, `nice -n 19`.

- D-Bus scenario table: see "Task 1" above.
- `dbus-run-session` timeout: `finished in 30.31s` (panic:
  `server A's own embedded runner must reach 'active' in its own database`).
- Port-collision reproduction: `tack serve` against an already-held port →
  `Error: Address already in use (os error 98)`; `0.38s user 0.07s system 4% cpu 9.382s
  total` wall time to fail (most of that is this sandbox's own migration/pool-init cost,
  not the bind attempt itself).
- Revert-once proof: fixed run `8.218s` (1 passed); reverted run `1.221s` (1 failed).
- Ten-run loop: 6.2s / 7.5s / 7.7s / 7.5s / 7.6s / 9.2s / 7.2s / 6.2s / 7.3s / 14.4s — all
  green.
- Full workspace: `56.898s`, `1459 tests run: 1459 passed, 7 skipped`.

## What a stranger still cannot do

A stranger running this suite on a CI host that schedules the embedded-runner boot tests
alongside enough other port-binding tests at once can still, in principle, lose the port
race this card narrowed rather than eliminated: `threads-required = "num-test-threads"`
stops these four binaries from running *alongside each other or anything else*, but the
underlying `TcpListener::bind`-then-drop pattern is still a race against literally anything
else on the host binding an ephemeral port in the same instant a *different*, unprotected
test does the same thing outside this override's reach. A stranger also still gets no
diagnostic at all if `SecretStore::open`'s platform-store attempt genuinely wedges on a
broken D-Bus activation — the embedded runner just never reaches `active`, with nothing in
the log pointing at why (see "Known limitations").

## Context spent

- Tokens read before the first edit: the card skill, reporting/decision/scope-discipline
  docs, the card text, the handoff template, `VI-C23.md` (whole — it originated both waits
  this card was asked to reconsider), ADR 0061 decision 1, all three test files in full,
  `local_runner.rs` (`start_locked` and the surrounding `EmbeddedRunnerControl` area),
  `bootstrap.rs` (`build_runtime`), `secrets.rs` (`SecretStore::open`/`platform_store`),
  and — the load-bearing read — the actual CI failure logs
  (`gh run view <id> --log-failed`) for both cited runs, which is what overturned the
  card's own framing.
- Files opened and not used: `crates/tack-api/src/server.rs` (read only to confirm
  `TcpListener::bind(addr).await?`'s error propagation shape for the port-collision
  reproduction; not otherwise touched).
- Read-list lines that were wrong: the card's own citations of what fired at
  `embedded_runner_orphaned_credential.rs:127` and
  `embedded_runner_state_scoping.rs:176` — both line numbers were right, both
  descriptions of what happened there were wrong. Corrected above rather than propagated.

## Proposed board row

VI-C32 — done. Neither CI failure the card was filed about was a liveness-backstop
timeout: one was a test-infrastructure port collision (fixed: extended
`.config/nextest.toml`'s existing `e6_scheduler_e2e_test` isolation to the three
embedded-runner boot tests), the other a missing wait for a real disk-write race in
`embedded_runner_state_scoping.rs` (fixed: added a bounded poll matching its sibling
file's own). The keychain-probe lead the card measured is real under one narrow D-Bus
configuration but does not explain either failure and is not what CI runs against;
left as a named, un-filed finding for whoever next owns
`crates/tack-runner/src/secrets.rs`. No product code changed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

# VII-B5 handoff

- Base SHA / branch / final SHA: `8db262b` (the `develop` tip named in the dispatch prompt) /
  `agent/vii-b5-server-watch` / committed on the branch, not yet merged.
- Files changed (must equal ownership list): `crates/tack-desktop/src/supervisor.rs` (the
  `ExitReport` type, `SidecarHandle::exited`, the pure `watch_tick` state machine and its
  tests), `crates/tack-desktop/src/tray.rs` (the poll loop now drives the watch and shows
  its dialogs), `crates/tack-desktop/src/main.rs` (`TauriSidecarHandle` keeps the event
  receiver `spawn` returns and implements `exited`; `ServerMode` gains `Stopped`),
  `docs/book/src/user-guide/quick-start.md` (the tray paragraph, one added sentence),
  `CHANGELOG.md` (`[Unreleased]` → `Fixed`, one entry), this handoff. Nothing outside the
  ownership list — `git diff --stat 8db262b...HEAD` confirmed before writing this file.
- Contract fixtures consumed: none. `tauri-plugin-shell` 2.3.6's `CommandChild`/
  `CommandEvent`/`TerminatedPayload` shapes came from reading
  `~/.cargo/registry/src/*/tauri-plugin-shell-2.3.6/src/process/mod.rs` directly (not a
  `docs/contracts/` surface).
- Behavior implemented: a single pure function, `supervisor::watch_tick`, decides what
  changed each tick from four inputs (previous `WatchState`, which kind of server this tick
  is watching, whether health answered, whether the child reported exiting) and returns the
  new state plus an optional `WatchEvent`. The tray's existing three-second poll loop (no
  second timer) now also reads `DesktopState` each tick, calls `SidecarHandle::exited()`
  when the mode is `Started`, and feeds both into `watch_tick`. On `StartedExited`: the tray
  label becomes `Server stopped (exit <code|signal|unknown>)` and stays that way (further
  ticks are no-ops), `DesktopState` moves from `Started(process)` to `Stopped` so the
  `ExitRequested` handler in `main.rs` no longer matches a process to kill, and one
  `Warning`-kind dialog shows once naming the exit and saying reopening Tack starts it
  again. On `AttachedUnresponsive` (five consecutive missed health polls, not four): the
  label becomes `Server not responding` and one dialog shows once, worded for a server this
  app did not start. On recovery (`AttachedRecovered`, health answers again after an
  unresponsive episode): the sticky label clears and the ordinary poll-derived label
  resumes; a later, fresh episode of five more missed polls fires the dialog again — the
  "not shown a second time" wording in the card's Tasks is read as per-episode, not
  lifetime-of-the-app, since nothing in the card asks for a permanent one-shot on an
  attach that can legitimately recover and fail again later. `SidecarHandle::exited`
  resolved the card's own "Known before you start" concern directly: `tauri_plugin_shell`'s
  `spawn()` returns `(Receiver<CommandEvent>, CommandChild)` where `Receiver` is a plain
  `tokio::sync::mpsc::Receiver`, which already has a non-blocking `try_recv()` — no new type
  was needed outside the three owned files, so the card's stop condition never fired.
- Tests added and exact commands/results: `cd crates/tack-desktop && CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B5 cargo test --jobs 4`
  → `34 passed; 0 failed` in the unit binary (7 new: `started_server_exit_is_reported_exactly_once`,
  `started_server_that_keeps_answering_changes_nothing`,
  `four_missed_attached_polls_do_not_trigger_unresponsive`,
  `five_missed_attached_polls_trigger_unresponsive`,
  `attached_server_recovers_after_failure_without_a_second_dialog`,
  `unknown_kind_resets_any_carried_state`, plus the `ChildHandle::exited` test double used by
  the pre-existing supervisor tests) + `3 passed; 0 failed` in `dependency_boundary` (the
  crate's own pre-existing suite, confirmed still green). `cargo clippy --all-targets --jobs 4 -- -D warnings`
  → clean, exit 0. `cargo fmt --all --check` → clean, exit 0 (two files needed `cargo fmt
  --all` once after the edits; confirmed clean afterward). `./scripts/check-comments.sh` →
  `✓ no board archaeology`.
- Failure/adversarial case proved: `started_server_exit_is_reported_exactly_once` is
  load-bearing — reverted the `Started` arm of `watch_tick` to always return `(previous,
  None)` regardless of `child_exit`, re-ran that one test, got `assertion left == right
  failed / left: None / right: Some(StartedExited(...))`, then restored the fix and
  confirmed the full suite green again.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the live behaviour — a real window, a real
  sidecar killed out from under it, a real dialog appearing — is `not_measured`. This is a
  shared machine with the user present; no window was opened, per the card's hard rule.
  The manual recipe for the integrator (once the display is free) is below. Also
  `not_measured`: what happens if the dialog's `blocking_show()` is still waiting on the
  main thread when a second event (e.g. a second missed-poll episode, or Quit) arrives
  mid-dialog — the code path is a straight port of the pattern `main.rs` already uses for
  its own startup-failure dialogs (see `port_occupied` / `outdated_server` there), so this
  is an existing-pattern risk, not a new one, but it was never live-tested by this card
  either.
- Secrets/logging review: no new log line carries anything but an exit code/signal (both
  process-exit metadata, not secrets) and HTTP status/connection-error classifications
  already logged by the pre-existing poll. No credential, prompt body, or query string is
  touched by this card's code.
- Safe merge order and likely conflicts: this is the only card in Wave 23; no other Part
  VII or Part VI branch touches `supervisor.rs`, `tray.rs`, or the `ServerMode`/
  `DesktopState` block of `main.rs` concurrently. `CHANGELOG.md`'s `[Unreleased]` section is
  shared across every active cycle — this adds one bullet under `Fixed`, above the existing
  "first run no longer hangs" entry; a conflict there is a one-line reorder, not a content
  conflict.
- Checklist: no unowned files touched, no live secret, no panic stub (`unimplemented!()`
  absent — checked by `grep -rn "unimplemented!" crates/tack-desktop/src`, zero hits), no
  blind retry (the watch never retries anything itself; it only classifies what the
  existing poll and a non-blocking exit check already observed).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A started server that exits on its own is reported once, with its exit status, and Quit no longer tries to kill an already-dead pid | `supervisor::tests::started_server_exit_is_reported_exactly_once`; `ServerMode::Stopped` added to `main.rs`, the `ExitRequested` arm's `if let Some(ServerMode::Started(process)) = mode` no longer matches once the watch has fired |
| An attached server has to miss five consecutive health polls, not four, before this app says anything about it | `supervisor::tests::four_missed_attached_polls_do_not_trigger_unresponsive` and `...five_missed_attached_polls_trigger_unresponsive` |
| An attached server that recovers clears the warning and can fire it again on a later, separate episode | `supervisor::tests::attached_server_recovers_after_failure_without_a_second_dialog` |
| A server whose kind is not yet known (the window before `attach_or_start` resolves) never carries stale counters into whichever kind it turns out to be | `supervisor::tests::unknown_kind_resets_any_carried_state` |
| No second timer was added — the tray's existing three-second poll loop is what drives the watch | `crates/tack-desktop/src/tray.rs` — one `tauri::async_runtime::spawn`, one `loop`, one `tokio::time::sleep(POLL_INTERVAL)`; grep confirms no second `spawn`/`sleep` was added |

## Measured numbers

- Unit tests: `34 passed; 0 failed` (`cargo test --jobs 4` in `crates/tack-desktop`, unit
  binary) + `3 passed; 0 failed` (`dependency_boundary` integration test).
- `cargo tree -p tack-cli -e normal --jobs 4 | grep -ci "tauri\|webkit\|gtk"` → `0` (root
  workspace stays free of the desktop crate's dependencies).
- `tauri-plugin-shell` version confirmed on this machine: `2.3.6` (matches the card's
  "Known before you start" note) — `ls ~/.cargo/registry/src/*/tauri-plugin-shell-2.3.6/`.

## What a stranger still cannot do

A stranger who kills the sidecar (`kill -9` on its pid) while the app's window is open
still cannot *see* the dialog or the tray label change in this handoff, because no window
was opened to observe it — the transition logic is unit-proven, not live-proven. A stranger
also still cannot get any signal from this app about a server dying via a signal this app's
own `shutdown()` sent (SIGTERM during a graceful stop) versus one it did not send — both
arrive through the same `CommandEvent::Terminated` path and both are reported identically
as "Server stopped"; the watch does not distinguish a stop this app itself requested from
one that just happened. That distinction was not in the card's Tasks and is not claimed as
handled.

## Platform measured

`not_measured` — no live desktop session was used for this card, on the instruction that
this is a shared machine with the user present and no window may be opened. OS is Linux
(`uname -a` on the build host: `Linux 6.14.0-37-generic x86_64`), `tauri-plugin-shell`
`2.3.6` and `tauri` `2.11.5` are the versions actually compiled and tested against (from
`cargo tree` inside `crates/tack-desktop`), but no display, window manager, or tray host
was exercised.

**Manual recipe for the integrator, once the display is free:**

1. Started-server case: `cd crates/tack-desktop && CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B5 cargo build --jobs 4`
   (or `make desktop` from the repo root), run the built binary so it spawns its own `tack`
   sidecar (no server already listening on the configured port), confirm the tray label
   reads a normal `Agent execution: ...` state, then `pkill -9 -f 'tack serve'` (or find the
   sidecar's pid via `pgrep -af tack` and `kill -9` it directly — **not** the app's own pid).
   Within one poll interval (≤ 3s) the tray label should read `Server stopped (exit ...)`
   and one dialog should appear; open the tray menu again and confirm the label has not
   reverted; hit Quit and confirm no error about a missing process.
2. Attached-server case: start a `tack serve --with-runner` by hand first, then open the
   app so it attaches instead of spawning; confirm the tray label tracks normally; then
   `kill` (graceful) or firewall off that hand-started process so health stops answering;
   wait five poll intervals (~15s) and confirm exactly one dialog appears, worded for an
   attached server; restart the hand-started server and confirm the label recovers within
   one more poll interval with no second dialog.
3. `pgrep -af tack` before and after each step, to confirm no orphaned process either way.

## Daemon proof

`not_measured` — requires the live walk in Platform measured above, which requires a free
display. Not run by this card.

## Process proof

`not_measured` for the same reason. `pgrep -af tack` was not exercised beyond the automated
unit tests' own short-lived Python fake sidecars (which self-terminate within the test
process and were confirmed not to survive their test via the pre-existing
`spawns_and_becomes_healthy_when_nothing_is_listening` test's own liveness assertion,
unchanged by this card).

## Context spent

- Tokens read before the first edit (cold start): followed the card's named read list —
  README.md header + Waves table + VII-B5 block, TODO.md's §VII.0 vocabulary section and
  the VII-B5 card block (via `git show 8db262b:TODO.md`, since this worktree's checked-out
  branch predates Part VII), `VII-C3.md`'s "What a stranger still cannot do" section,
  `VII-B4.md`'s six-tray-states paragraph, `supervisor.rs` whole, `tray.rs` whole (the block
  named only lines 1–60 and 150–180; read the full 252-line file because this card edits it
  and the omitted middle — the menu-building code between those ranges — is exactly what
  the new code needed to sit beside), `main.rs` whole (block named lines 60–80 and 120–244;
  read the full 244-line file for the same reason — this card adds a struct field and an
  enum variant near the top, outside the named ranges), and the `try_wait`/`CommandChild`
  grep the block specified, followed by reading `tauri-plugin-shell-2.3.6/src/process/mod.rs`
  directly (not named by the block, but necessary to get `CommandEvent`'s exact shape right
  rather than guessing from the grep alone) and `tauri-2.11.5/src/async_runtime.rs` (not
  named by the block; needed to confirm `Receiver<T>` is a plain `tokio::sync::mpsc::Receiver`
  with a non-blocking `try_recv`, which is what let the card's "Known before you start"
  concern resolve without hitting the Stop clause). Did not read `lifecycle.rs`,
  `first_run.rs`, `paths.rs`, any Part VI file, or any other Tauri crate source, per the
  block's "Do not read" list.
- Context size at handoff: roughly 150k tokens into the session at the time this handoff was
  written (measured by the harness's own running total, not a command run inside the repo).
- Files opened and not used: none beyond what is listed above — every file read fed either
  the read-list requirement or a real implementation decision.
- Read-list lines that were wrong: the two named line ranges for `tray.rs` and `main.rs`
  were sized for *understanding* the poll loop and the setup/exit flow, not for *editing*
  them — both files needed a full read once this card's actual diff turned out to touch
  code outside those ranges (a new struct field near the top of `main.rs`, a new `Manager`/
  `DialogExt` import and a rewritten loop body in `tray.rs`). Future dispatch of a card that
  edits (not just reads) a file should probably just say "whole file" rather than a
  narrower range, since the range was accurate for the *card's context section* but not for
  its *task section*.

## Also fixed at the integrator's request

`supervisor::tests::refuses_to_spawn_when_the_port_is_held_by_something_else` was flaky on
CI (run `34132310419`, failed on an unrelated frontend-only branch with `panicked at
src/supervisor.rs:503:78` inside `TcpListener::bind(("127.0.0.1", port)).unwrap()`): it
called `free_port()` (bind an ephemeral port, then drop the listener to free it) and
re-bound that same port number a moment later, racing anything else on a busy host that
grabs it in the gap. Fixed by binding the raw listener directly to port 0 and reading the
port back from `local_addr()`, so the "port already held" listener is never dropped and
re-bound — it is simply held from the start. The other tests that call `free_port()` were
left alone; they need a port that starts *free* so a launcher can bind it, which is a
different requirement `free_port()` still serves correctly. Re-ran the full suite after the
change: `34 passed; 0 failed` + `3 passed; 0 failed`, `cargo clippy` and `cargo fmt --all
--check` both clean.

## Amendments

*(none yet)*

### 2026-09-07 — integrator amendment

`TauriSidecarHandle::exited` no longer reads the plugin's event receiver itself. As merged,
the receiver was kept and polled with `try_recv` once per tray tick. `tauri-plugin-shell`
2.3.6 creates that channel with capacity 1 and its stdout/stderr reader threads `block_on`
every send, so a receiver drained every three seconds would have held the child's output
pipe, and behind it `tack serve`, for as long as the server had anything to print — the
opposite of what dropping the receiver did before this card (a closed channel fails each
send instantly and the reader keeps reading). On `develop` the launcher now spawns one task
that drains the receiver continuously, discards output events and stores the `Terminated`
payload in an `Arc<Mutex<Option<ExitReport>>>` the handle reads. The transition function,
its tests and the tray are unchanged; 34 + 3 tests, clippy and fmt re-run on the merged tree.
The live proof is still `not_measured`, for the reason above.

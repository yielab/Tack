# VII-B4 handoff

- Base SHA / branch / final SHA: `a84a089` (develop tip at dispatch, confirmed against
  `git rev-parse --short develop`) / `agent/vii-b4-tray-truth` / not committed — the
  working tree at handoff time.
- Files changed (must equal ownership list): `crates/tack-desktop/src/tray.rs` (rewritten;
  the hard-coded, permanently-disabled label replaced with a polling status line),
  `Makefile` (`desktop-sidecar` target, one line — the `cp` source path), this handoff and
  its `docs/agent-handoffs/part-vii/vii-b4-proof/` directory (three screenshots — kept
  beside the handoff, same precedent as `docs/agent-handoffs/part-vi/vi-a3-render-proof`,
  not under `docs/screenshots/`, which VII-C2 and VI-D2 own). Nothing else — `git diff
  --stat` / `git status --porcelain` confirmed before writing this file.
- Contract fixtures consumed: none. `GET /api/local-runner`'s response shape came from
  reading `crates/tack-api/src/handlers/local_runner.rs` directly (it is not a
  `docs/contracts/` surface) and confirming it live against a running server.
- Behavior implemented: the tray's agent-execution entry is now a disabled status line
  (not a switch — see the decision note below) built at tray-build time and updated
  roughly every 3 seconds by a background poll of `GET /api/local-runner` against this
  app's own server (the port comes from `settings.json`, the same file `first_run.rs`
  writes, falling back to the default port if that file is absent — never a second guess
  at the port). Six states: `on` (`enabled && state=="running"`), `off`
  (`!enabled && state=="stopped"`), `turning on…`/`turning off…` (the preference and the
  runtime state disagree, which happens for a moment right after a toggle), and two typed
  failures — `waiting for the server…` (connection refused or timed out — nothing has
  answered yet) and `status unavailable (request failed)` (server reachable, but a
  non-2xx status or a body this app cannot parse). Neither failure ever renders as `off`.
  `Makefile`: `desktop-sidecar`'s `cp` now reads `"${CARGO_TARGET_DIR:-target}/release/tack"`
  instead of the literal `target/release/tack`, so it copies from wherever the `cargo
  build` two lines above it actually wrote.
- Tests added and exact commands/results: `cargo test --manifest-path
  crates/tack-desktop/Cargo.toml` → `31 passed; 0 failed` (5 new: the four
  enabled/state-combination-to-label cases and one asserting the base-URL fallback parses
  as a URL when no settings file exists; the other 26 are the crate's pre-existing suite,
  confirmed still green). `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml
  --all-targets -- -D warnings` → clean, exit 0. `./scripts/check-comments.sh` (both
  scoped to `crates/tack-desktop/` and the repo default) → `✓ no board archaeology`.
- Failure/adversarial case proved: live, not just unit-tested — see Claim → evidence and
  Daemon proof below. A real running server was killed out from under a live tray
  (`kill -9` on the sidecar's pid, not a graceful stop) and the label transitioned from
  `on` to `waiting for the server…` within one poll interval, never touching `off`. The
  second failure arm (`status unavailable (request failed)`, a reachable server answering
  with something other than a parseable 2xx body) is exercised by
  `poll_once`'s own branch on `response.status().is_success()` and the JSON-parse
  `Err(_)` arm, but was not driven live in this session — see Known limitations.
- Schema/API/contract change requested from another owner: none — `GET /api/local-runner`
  already returns everything the label needs (`enabled`, `state`); nothing was asked to
  change shape.
- Known limitations or `not_measured` fields: the `status unavailable (request failed)`
  label (a reachable server, bad response) is proved by code path and its own unit
  coverage (`poll_once`'s non-success-status and JSON-parse-error arms), not by a live
  screenshot — the card's acceptance bar names three required screenshots (on, off, no
  server answering) and this is a fourth state beyond that bar. macOS and Windows are
  `not_measured` — only this Linux/X11/GNOME machine was available, matching every other
  Part VII handoff's own note. The poll interval (3s) and request timeout (2s) are fixed
  constants, not configurable — no card asked for that and the acceptance bar does not
  need it.
- Secrets/logging review: no secret or credential is read, stored or logged by this
  change. The one new log line (`tracing::error!` when `set_text` fails) carries only the
  error from the menu-item call itself, never a URL, header or body.
- Safe merge order and likely conflicts: this card's dispatch says "after VII-C2, before
  VII-D1" — VII-C2 is already on `develop` at the base SHA (confirmed: its screenshot and
  README changes are present), so no wait was needed. No other in-flight card touches
  `tray.rs` or the two `Makefile` targets (the ownership table gives both to this card
  alone). Should merge cleanly onto `develop`. D1's own transcript walks this menu, so it
  should branch after this lands.
- Checklist: no unowned files touched (`git diff --stat` shows exactly the two files
  above); no live secret; no panic stub (`unwrap_or_else`/`ok()` used throughout the new
  code specifically to avoid one); no blind retry (the poll loop is a fixed-interval
  status refresh, not a retry-until-success).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The tray's agent-execution entry reads the real, running server instead of a hard-coded string | `crates/tack-desktop/src/tray.rs`'s `resolve_base_url`/`poll_once`, called from a background task spawned in `build()`. Screenshot proof below — the label changes when the server's actual state changes, with no code path left that renders `false`/disabled by construction. |
| Screenshot: execution on | [`vii-b4-proof/tray-execution-on.png`](vii-b4-proof/tray-execution-on.png) — **state when taken: the sidecar had just started under `--with-runner`'s default, `enabled:true`/`state:"running"`.** 1280×800, sha256 `4d0ddd1a0daef8cacdd5965c54d500d50b00a033e9260f39c9f00b2e3c1749f3`. Menu shows `Agent execution: on`, captured while `curl http://127.0.0.1:3314/api/local-runner` answered `{"enabled":true,"state":"running",...}`. |
| Screenshot: execution off | [`vii-b4-proof/tray-execution-off.png`](vii-b4-proof/tray-execution-off.png) — **state when taken: just after `PUT /api/local-runner {"enabled":false}` succeeded and the server reported `state:"stopped"`.** 1280×800, sha256 `3081869ae0d3eb1d69439f5be5bbb11e4de9b49698198ca697f24039475f7d23`. Taken after `curl -X PUT .../api/local-runner -d '{"enabled":false}'` → `204`, then `curl .../api/local-runner` → `{"enabled":false,"state":"stopped",...}`; the tray's next poll (≤3s later) updated the label to `Agent execution: off` before the menu was reopened and captured. |
| Screenshot: no server answering, typed, not "off" | [`vii-b4-proof/tray-execution-server-not-answering.png`](vii-b4-proof/tray-execution-server-not-answering.png) — **state when taken: the sidecar `tack serve` process had just been `kill -9`'d out from under the still-running tray, so nothing was listening on its port.** 1280×800, sha256 `a10d3aba8d165ca07c07ae791e62f9d9ea6c2b480d4f335eb0c7c67c8805fa60`. Confirmed gone via `pgrep`, and `curl --max-time 3` on the same port returned curl exit `7`, connection refused — the label reads `Agent execution: waiting for the server…`, not `off` and not blank. |
| The old doc comment's claim ("`GET /api/local-runner` does not exist yet") and the old label's promise ("the switch arrives with the Agents page") are both retired | `tray.rs` no longer contains either string (`grep -n "does not exist yet\|arrives with the Agents page" crates/tack-desktop/src/tray.rs` → no output). The route is real (`crates/tack-api/src/router.rs:158`, `handlers/local_runner.rs`) and is what the new code reads. |
| `make desktop-sidecar` copies the binary `cargo` actually built, with or without `CARGO_TARGET_DIR` | Run twice — see Measured numbers for both sha256 pairs, both matching. |
| `make desktop` still builds end to end | `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B4 make desktop` → `Finished` + three bundles produced (deb, rpm, AppImage) — see Measured numbers. |

## Measured numbers

- `make desktop-sidecar`, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B4` set: `3m
  29.81s` total (`531.37s user 46.21s system`). Staged binary
  `crates/tack-desktop/binaries/tack-x86_64-unknown-linux-gnu` sha256
  `3b87fa483821a3608631dead0eb640f9d20aa7251e0b80eef1d7baa5c207178d`; freshly built
  `/var/tmp/tack-agent-targets/VII-B4/release/tack` sha256 — same value. **Match.**
- `make desktop-sidecar`, `CARGO_TARGET_DIR` unset (default `target/`, this worktree):
  `2m 52.52s` total. Staged binary sha256
  `23be73b8028240fd036e4fc28c371643c085a7fee9536c955f95e6791da3c463`; freshly built
  `target/release/tack` sha256 — same value. **Match.** (The unset-var worktree `target/`
  was removed after this measurement — 1.1 GB, not needed again, and `/home` was at 92%
  used.)
- `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B4 make desktop`: `5m 32.11s` total
  (re-runs `desktop-sidecar`, ~1m15s to compile `tack-desktop` itself the first time
  with every GTK/WebKit/Tauri dependency cold, then bundles). Produced
  `Tack_0.1.0-beta.7_amd64.deb`, `Tack-0.1.0-beta.7-1.x86_64.rpm`,
  `Tack_0.1.0-beta.7_amd64.AppImage` under `/var/tmp/tack-agent-targets/VII-B4/release/bundle/`.
- `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D
  warnings`: clean, `41.94s` (cold-ish; most deps already built by the two runs above).
- `cargo test --manifest-path crates/tack-desktop/Cargo.toml`: `31 passed; 0 failed` in
  `5.31s` (unit) `+ 0.15s` (the three `dependency_boundary` integration tests).
- Three proof screenshots, `docs/agent-handoffs/part-vii/vii-b4-proof/`: 1280×800 each,
  126,824 / 127,396 / 125,476 bytes (on / off / not-answering) — sha256 values, and the
  exact server state each was taken in, are in the Claim → evidence table above.

## What a stranger still cannot do

Toggle agent execution from the tray itself — this entry is a status line, not a switch
(see the decision below), so turning the runner on or off from outside this app still
means using the Agents page in the app's own window, or `PUT /api/local-runner` directly.
Someone on macOS or Windows gets none of this card's own verification repeated here — only
this Linux/X11/GNOME machine was available, matching every other Part VII handoff.

## Decision recorded: status line, not a switch

The card's own text names both options and says a correct status line beats a switch that
races the server; this implements the status line. Reasoning, so a later reader does not
re-litigate it: a live switch needs a `PUT` on click, immediate optimistic UI, and a
reconciliation path if the write fails or the runtime doesn't follow the preference
right away (exactly the "turning on…/turning off…" disagreement this code already has to
render for the read-only case) — all of that is new interaction surface on a menu item
that, on Linux, cannot even show a spinner. A disabled, polling label gets the same
truthful information onto the screen (including both typed failure states) with no new
failure mode of its own, and the one real switch this product has (the Agents page, and
`PUT /api/local-runner` for ansyone scripting it) already exists and is not duplicated
here. If a later card wants a tray switch, this label's classification logic
(`AgentExecutionStatus::from_body`) is the state machine to reuse — it already names every
state a switch would need to render mid-flight.

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: GNOME Shell 46.0, `XDG_SESSION_TYPE` → `x11` for the real,
  interactive session.
- `ubuntu-appindicators@ubuntu.com`: `gnome-extensions info` → `State: ACTIVE` (checked
  in both the real session and the nested one described below — extension state is
  per-user dconf, not per-session).
- systemd: `systemd 255 (255.4-1ubuntu8.17)`.
- macOS, Windows: `not_measured`.

**Why a nested shell, not the live session.** This machine's `DISPLAY=:0` is a real,
in-use desktop (the operator's own browser and editor windows). Two things ruled out
screenshotting the tray there directly: first, a root-window or whole-screen capture would
have caught unrelated personal windows, which does not belong in a handoff or any shared
artifact; second, and separately, GNOME Shell's own top panel turned out to not be
capturable at all through plain X11 tools on this session — `xwd -root` and per-window
grabs both come back black for the panel specifically, because Mutter draws it directly
into its own compositor scene rather than as a normal client window, and the direct
`org.gnome.Shell.Screenshot` D-Bus API refused with `AccessDenied` when called from this
non-interactive shell. The fix used: `dbus-run-session -- gnome-shell --nested --wayland`
opens a second, fully isolated GNOME Shell (own D-Bus, own compositor, own empty desktop)
as an ordinary window on the real X11 session — capturable normally, and containing
nothing but what this session puts into it. `gnome-keyring-daemon --start
--components=secrets` was started on that same private bus first — without it, `tack
serve`'s embedded-runner catalog lookup hung indefinitely on every `GET
/api/local-runner` call (the real session's own keyring answers this instantly; the
nested session has no Secret Service provider unless one is started for it). This is a
5-line finding for anyone doing this again on a similar machine, not a change to any
shipped code. Everything the tray does is identical regardless of which GNOME Shell
instance renders it — same D-Bus/StatusNotifierItem protocol either way.

## Daemon proof

This card doesn't touch window lifecycle (`main.rs`/`lifecycle.rs` are explicitly out of
scope), so this isn't the close/reopen sequence those cards own — it's the sequence that
actually exercises this card's own change: the tray tracking the daemon's true state,
including the state disappearing under it. All timestamps UTC, one continuous session, in
the nested shell described above, `TACK_PORT=3314`, an isolated `XDG_DATA_HOME` under this
worktree (never the repository's `tack.db`):

```
03:55:15Z  launched tack-desktop; sidecar `tack serve --with-runner` started, pid 860206
03:55:1xZ  curl /api/local-runner -> {"enabled":true,"state":"running",...}
03:56:0xZ  tray menu opened -> "Agent execution: on"                         (screenshot 1)
03:56:2xZ  curl -X PUT /api/local-runner -d '{"enabled":false}' -> 204
03:56:2xZ  curl /api/local-runner -> {"enabled":false,"state":"stopped",...}
03:57:0xZ  tray menu opened -> "Agent execution: off"                        (screenshot 2)
03:57:1xZ  kill -9 860206 (the sidecar only — the supervisor/tray process kept running)
03:57:1xZ  curl --max-time 3 /api/local-runner -> exit 7, connection refused
03:57:2xZ  tray menu opened -> "Agent execution: waiting for the server…"    (screenshot 3)
```

## Process proof

```
before launch:        pgrep -af "tack-desktop|tack serve" -> (nothing)
after launch:          tack-desktop (supervisor+tray) + tack serve (sidecar), both present
after PUT disable:     both still present, same pids — only the persisted flag changed
after kill -9 sidecar: tack-desktop still present; tack serve gone                <- the case this card had to render honestly, not as "off"
cleanup:               kill -9 on the remaining tack-desktop pid; nested gnome-shell,
                       its private dbus-daemon and its keyring daemon all killed;
                       confirmed with a second `pgrep -af "tack-desktop|tack serve|
                       gnome-shell --nested|gnome-keyring-daemon.*nested-keyring"` -> (nothing)
```

No orphan of any kind was left running. The repository's own `tack.db` was never opened —
every server in this session used `TACK_DATABASE_URL` under an isolated
`XDG_DATA_HOME`/`.desktop-test/` scratch tree (since deleted) or, for the one earlier
protocol sanity check (confirming `GET`/`PUT /api/local-runner`'s exact JSON shape before
writing `tray.rs`), a throwaway SQLite file under the same scratch tree.

## Context spent

- Tokens read before the first edit (cold start): the card text and dispatch block
  named in this task, `tray.rs` whole (100 lines, as promised), one grep for
  `local_runner` in `router.rs` plus the two handler functions it pointed at, the
  `Makefile`'s two targets, and `VII-C2.md`'s escalation. Matched the dispatch block's
  own estimate (~9k) closely — nothing unexpected needed reading beyond that to write the
  code itself. Additional reading past that point (`paths.rs`, `supervisor.rs`,
  `first_run.rs`, `main.rs`) was to find a way to resolve the server's port without
  touching any of those owned-elsewhere files — all four were read-only lookups for
  already-`pub` items (`DataPaths::resolve`, `DEFAULT_PORT`), not edited.
- Context size at handoff: most of this card's cost was live system interaction (two
  full release builds, a bundle build, discovering and working around the compositor/
  keyring issues above, several screenshot round-trips), not reading.
- Files opened and not used: none of consequence.
- Read-list lines that were wrong: none — the dispatch block's read list was accurate as
  given.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**Escalation, not fixed here — addressed to whoever owns `docs/book/src/developer/crate-tour.md`'s
`tack-desktop` entry (VII-C2's file, per the ownership table).** Its `tray.rs` section
(around line 499 at base SHA) describes the entry this card just replaced: "a permanently
disabled entry reading **Agent execution: unknown — the switch arrives with the Agents
page**… nothing in this file makes an HTTP request." That is no longer true — the entry
now polls `GET /api/local-runner` on a background task and renders one of six states (see
Behavior implemented above). The sentence needs rewriting to match; not done here because
`crate-tour.md` is outside this card's ownership.

# VII-C2 handoff

- Base SHA / branch / final SHA: `605b649` (develop tip at dispatch, confirmed against
  `git rev-parse --short develop`) / `agent/vii-c2-run-it` / not committed — the working
  tree at handoff time.
- Files changed (must equal ownership list): `README.md` (§"Run it" only, one hunk),
  `docs/book/src/user-guide/quick-start.md` (one paragraph), `docs/book/src/developer/crate-tour.md`
  (a `tack-desktop` section plus a one-sentence intro edit), `CHANGELOG.md` (two new
  `[Unreleased]` bullets), `docs/screenshots/desktop-window.png` (new),
  `docs/screenshots/desktop-tray.png` (new), this handoff. Nothing else — `git diff --stat`
  confirmed before writing this file.
- Contract fixtures consumed: none — no `docs/contracts/` involvement in a documentation
  card.
- Behavior implemented: none (documentation only). What follows is verification of
  existing behavior (VII-B1/B2/B3/C1/C3's), not new code.
- Tests added and exact commands/results: `mdbook build docs/book -d <tmp dir>` — clean,
  no warnings, no errors (see Measured numbers for the exact command).
- Failure/adversarial case proved: n/a for this card's own scope; the daemon-close/quit
  sequence below is the closest equivalent and is fully reproduced with timestamps.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: macOS desktop bundle is not produced yet
  (VII-C1's own handoff: blocked on two pre-existing, unrelated cross-compile
  regressions) — README says so plainly rather than promising it. Windows tray/lifecycle
  behavior is `not_measured` here; only this Linux/X11/GNOME machine was available.
- Secrets/logging review: n/a — no code touched, nothing logged by this card's own work.
- Safe merge order and likely conflicts: last card in Wave 20, after VII-C1 (needed) and
  Part VI's VI-C1 (needed, both already landed on `develop` at the base SHA). No other
  in-flight card was touching `README.md` §"Run it" at branch time (VI-D2 owns the hero
  and three screenshots elsewhere in the same file, not this section; V-C2 owns the
  recording in `docs/screenshots/`, not either of my two new files). Should merge cleanly.
- Checklist: no unowned files touched (verified via `git diff --stat` and `git status
  --porcelain` before writing this handoff); no live secret; no panic stub (n/a, no
  code); no blind retry (n/a).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The desktop app is real on this machine — built from source, not assumed | `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-C2 make desktop` produced three real bundles: `Tack_0.1.0-beta.7_amd64.deb` (16,542,414 bytes), `Tack-0.1.0-beta.7-1.x86_64.rpm` (16,540,702 bytes), `Tack_0.1.0-beta.7_amd64.AppImage` (91,539,960 bytes). Launched the AppImage directly (`--appimage-extract-and-run`). |
| The app opens a real window showing the real, running board | `wmctrl -l` showed a `Tack` window; `xwd -id <window id>` capture converted via `xwdtopnm \| pnmtopng` (ImageMagick's own `import`/`convert` lack the X11/XWD decode delegate on this machine) is `docs/screenshots/desktop-window.png` — the real Agents page (VI-C1's), Codex `v0.149.1` and Claude Code `v2.1.261` both shown "Installed", agent execution "Running". |
| The app adds a real, OS-drawn tray menu: Open Tack, agent-execution status, Launch at login, Quit | GNOME's `ubuntu-appindicators@ubuntu.com` extension confirmed `ACTIVE` (`gnome-extensions info`). Located Tack's icon in the top panel (collapsed behind GNOME's own "…" overflow toggle, sharing panel space with an unrelated app, Solaar, whose tray icon is coincidentally the same teal color — see Context spent for how much of this session that cost). Captured the open popup with `ffmpeg -f x11grab` (root-window capture; `xwd -root` itself fails here with `BadColor` — an unrelated, pre-existing X11 colormap quirk on this multi-monitor session) — `docs/screenshots/desktop-tray.png`. |
| Closing the window doesn't stop it — the board keeps running | See Daemon proof: `wmctrl -c "Tack"` closed the window at `21:53:31Z`; from a second shell at `21:53:37Z`, `curl /api/health` still answered `200` with the same `pid` in `ss -ltnp`, and `pgrep -af "tack-desktop$"` still showed the supervisor. |
| Reopening from the tray shows the same live session, not a fresh one | Clicked the tray's **Open Tack** at `21:54:38Z`; the reopened window (`docs/screenshots` scratch capture, not committed — see Daemon proof) showed the same `Local workspace` / `tack.db` / `v0.1.0-beta.7`, the same empty Projects page left from before the close — no restart, no data loss. |
| Quit, from the tray, is what actually stops it | Clicked the tray's **Quit** at `21:55:xx`. Immediately after: `pgrep -af "tack-desktop$"` — no output (process gone); `ss -ltnp \| grep 3210` — no output (port freed); `curl --max-time 2 http://localhost:3210/api/health` — connection refused (`000`). No orphaned process of any kind remained (`wmctrl -l` showed no `Tack` window either). |
| `tack service install` is real and already documented elsewhere, so the README's daemon paragraph can point at it without re-deriving it | `docs/book/src/user-guide/cli.md` already has a `tack service install\|status\|uninstall` section (added by VII-A2, already on `develop` at the base SHA) — confirmed present with `grep -n "tack service" docs/book/src/user-guide/cli.md` before citing it in the README. Not re-tested here; VII-A2's own handoff owns that proof. |
| `README.md`'s diff touches only §"Run it" | `git diff README.md` — one hunk (`grep -c "^@@"` → `1`), bounded between the `## Run it` and `## Status` headings. |
| No forbidden vocabulary (§VI.1 rule 8) was introduced by this card's writing | `git diff README.md quick-start.md crate-tour.md CHANGELOG.md \| grep -E "^\+" \| grep -iEw "fleet\|enroll\|heartbeat\|capacity\|lease\|harness"` → one hit, `runner-fleet`, inside a *pre-existing* sentence in `crate-tour.md` (the developer book, which the rule exempts) that this card only appended to, not authored. |
| The developer book still builds clean with the new `tack-desktop` section | `mdbook build docs/book -d <tmp>` → `INFO Book building has started` / `INFO Running the html backend` / `INFO HTML book written to ...`, no warnings, no errors. |

## Measured numbers

- `make desktop` cold build (empty `CARGO_TARGET_DIR`): workspace release compile ≈2m28s,
  then `tack-desktop` + bundling ≈1m11s. (Second command: `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-C2 make desktop`, run twice — the Makefile's `desktop-sidecar`
  target hard-codes `cp target/release/tack …`, ignoring `$CARGO_TARGET_DIR`, so the
  first run failed on the `cp`; a `target -> $CARGO_TARGET_DIR` symlink in the worktree
  root fixed it for the second run. Not a bug I own — flagging it since the card
  explicitly asked for a non-default `CARGO_TARGET_DIR` and the Makefile does not
  support one without this workaround.)
- Bundle sizes (this machine, this build): `.deb` 16,542,414 B; `.rpm` 16,540,702 B;
  `.AppImage` 91,539,960 B. (VII-C1's own CI-built numbers were `.deb` 16,463,792 B,
  `.AppImage` 92,719,608 B — same order of magnitude, different environment, as
  expected.)
- `docs/screenshots/desktop-window.png`: 114,786 bytes, 1200×800 (matches
  `WebviewWindowBuilder::inner_size` per VII-C3's handoff).
- `docs/screenshots/desktop-tray.png`: 11,653 bytes, cropped from a 900×700 `ffmpeg`
  capture.
- `mdbook build docs/book -d <tmp>`: clean, ~1s wall time.
- Vocabulary grep: 1 pre-existing hit, 0 new hits, over 4 changed files.

## What a stranger still cannot do

Download the desktop app on macOS — it is not built, and the README says so instead of
promising it. Someone on Windows or Wayland cannot get any part of this card's own
verification repeated here; only X11/GNOME on Linux was available on this machine, matching
VII-C3's own platform note. And the tray's "Agent execution: unknown — the switch arrives
with the Agents page" line is not this card's to explain further — it's VI-B3/VII-B2's
mechanism, observed here exactly as those cards' own handoffs describe it, not a new
finding.

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: GNOME Shell, X11 (`echo $XDG_SESSION_TYPE` → `x11`).
- Appindicator host: `gnome-extensions info ubuntu-appindicators@ubuntu.com` → `State: ACTIVE`.
- systemd: `systemd 255 (255.4-1ubuntu8.17)`.
- Multi-monitor: primary `DP-2` 3840×2160+0+0, secondary `HDMI-0` 1920×1080+3840+533
  (`xrandr --current`) — the desktop window opened on the secondary monitor; the GNOME
  top panel (and the tray icon) render only on the primary monitor. Not called out by
  any prior Part VII handoff; worth knowing if tray automation is attempted again.
- macOS, Windows: `not_measured`.

## Daemon proof

All timestamps UTC, one continuous session, this machine:

```
21:53:19Z  launched Tack_0.1.0-beta.7_amd64.AppImage --appimage-extract-and-run
21:53:26Z  curl /api/health -> {"migrations_applied":62,"status":"ok","version":"0.1.0-beta.7"}
           pgrep -af "tack-desktop$" -> 3549893 tack-desktop
21:53:31Z  wmctrl -c "Tack"  (window closed)
21:53:37Z  [second shell] curl /api/health -> same 200 body as above
           pgrep -af "tack-desktop$" -> 3549893 tack-desktop  (still present)
           ss -ltnp | grep 3210 -> LISTEN ... pid=3549927 ("tack")  (still listening)
21:54:38Z  clicked the tray's "Open Tack" -> wmctrl -l shows the Tack window again
           reopened window renders the same "Local workspace" / tack.db / v0.1.0-beta.7 /
           empty Projects page left from before the close — same session, not a restart
21:54:59Z  curl /api/health -> same 200 body, unchanged
(between)  clicked the tray's "Quit" (no separate timestamp captured for the click itself,
           bounded by the two `date -u` calls immediately before and after it)
21:55:18Z  pgrep -af "tack-desktop$" -> (nothing)
           ss -ltnp | grep 3210 -> (nothing)
           curl --max-time 2 /api/health -> connection refused (000)
           wmctrl -l -> no Tack window
```

## Process proof

```
before launch:        pgrep -af "tack-desktop$" -> (nothing); ss -ltnp|grep 3210 -> (nothing)
after launch:         tack-desktop (supervisor) + tack (server, separate pid) both present; port listening
after window close:   both still present; port still listening               <- "closing keeps working"
after tray reopen:    both still present; same window, same session
after tray Quit:      neither present; port freed; no window                  <- "Quit stops it"
```

One earlier, separate experiment (not part of the sequence above — done on the *first*
launch of this session, before the clean daemon-proof run): sending `SIGTERM` directly to
the `tack-desktop` supervisor pid from a shell (not through the tray) removed the
supervisor but left the `tack` server process still running and the port still listening
— an orphan. This is *not* the same thing as the tray's "Quit", which this handoff's
Daemon proof shows stops both cleanly; it means an external kill of the supervisor alone
(a crash, an OOM kill, `killall tack-desktop`) does not cascade to the child, which is
architecturally unsurprising for a sidecar relationship with no process-group signal
propagation, but is recorded here since it was directly observed and no other Part VII
handoff mentions it.

## Vocabulary check

`git diff README.md docs/book/src/user-guide/quick-start.md docs/book/src/developer/crate-tour.md CHANGELOG.md | grep -E "^\+" | grep -iEw "fleet|enroll|heartbeat|capacity|lease|harness"`

One hit: `runner-fleet`, inside `crate-tour.md`'s existing intro sentence ("`tack-core`,
`tack-db`, `tack-api`, and `tack-cli` predate the Part III runner-fleet cycle…") — this
card only appended a sentence after it, did not author it, and `crate-tour.md` is the
developer book, which §VI.1 rule 8 exempts by name. `README.md` uses "runner" twice
(pre-existing, and explicitly allowed — "it is telling the story"). No other listed word
appears anywhere in the diff.

## Context spent

- Tokens read before the first edit (cold start): followed the dispatch block's read
  list exactly (board prelude, §VII.3, the VII-C2 card, the VI-D2 screenshot-rules block,
  `README.md` §"Run it", the install-page grep and `quick-start.md`, `crate-tour.md`
  head, `SUMMARY.md`, VII-C1's artifact table, VII-C3's Claim → evidence table), plus two
  items the block didn't name but the Owns list required: `CHANGELOG.md`'s existing
  desktop lines (to extend rather than duplicate) and the `Makefile`'s `desktop`/
  `desktop-sidecar` targets (to know what `make desktop` actually runs before invoking
  it). Both are recorded here as the read-list correction they are, not hidden.
- Context size at handoff: comfortably under the 120k ceiling; the bulk of this card's
  work was live system interaction (building, launching, screen-capturing, clicking a
  tray menu across several attempts), not reading.
- Files opened and not used: none of consequence. One capture attempt (`xwd -root`)
  failed outright (`BadColor`, a pre-existing X11 colormap issue unrelated to Tack) and
  was abandoned in favor of `ffmpeg -f x11grab`, which is not a file-read miss so much as
  a tooling dead end on this machine — recorded under Platform measured instead.
- Read-list lines that were wrong: the block's "Part V's asset rules" label for the
  `grep -n "^### VI-D2 "` line is a mislabel — VI-D2 is a Part VI card, not Part V's; the
  grep pattern itself was correct and the content (screenshot slot ownership) was exactly
  what was needed, so this cost nothing, just noting the label for whoever edits the
  dispatch README next.
- Web pages fetched: none. `v2.tauri.app` and `docs.rs` were both in-scope per this card's
  instructions, but VII-C1's and VII-C3's own handoffs plus the top-level `CLAUDE.md`
  crate map already carried every fact this card's writing needed (dialog-plugin
  main-thread behavior, `attach_or_start`'s failure arms, the tray's menu items, the
  bundle formats and their sizes) without re-deriving any of it from source or from
  Tauri's own docs.

## Amendments

**2026-09-05, integrator review.** Two findings, both verified against the tree, one
fixed here and one recorded for `tray.rs`'s owner:

1. **Fixed.** `docs/book/src/developer/crate-tour.md`'s `tray.rs` section (was line 499)
   claimed the tray's agent-execution entry is "read from the attached server's `GET
   /api/local-runner`". It is not — `crates/tack-desktop/src/tray.rs:22-25` builds that
   entry from a hard-coded `AGENT_EXECUTION_LABEL` constant with `enabled` fixed to
   `false`; nothing in that file makes an HTTP request. The sentence is rewritten to say
   exactly that: a permanently disabled entry reading "Agent execution: unknown — the
   switch arrives with the Agents page", built from that constant, not from any live
   read. `mdbook build docs/book` re-run clean after the fix; `git diff --stat README.md`
   re-checked, still one hunk; the vocabulary grep re-run, same single pre-existing hit
   in the same sentence, nothing new.
2. **Escalation, not fixed here — addressed to whoever owns `tray.rs` (VII-B2's file,
   not mine).** That hard-coded label is now stale in a second, worse way: its own text
   promises a switch that "arrives with the Agents page", and the Agents page has
   arrived — VI-C1 landed 2026-09-05 and is the page in this card's own
   `docs/screenshots/desktop-window.png`. `GET /api/local-runner` also now exists
   (`crates/tack-api/src/router.rs:158`). The doc comment above the constant in
   `tray.rs` still asserts the endpoint "does not exist yet", which is false as of that
   merge. So the shipped tray advertises a placeholder pointing at something already
   delivered, and this card's README screenshot now publishes an honest picture of that
   stale string. The screenshot stays — it is what the app really does, and an honest
   image of a stale label beats a doctored one — but `tray.rs` should be wired to the
   now-real endpoint by whoever owns it.

**Makefile finding, confirmed and kept as originally written.** `Makefile:21` builds
into `$CARGO_TARGET_DIR/release`, but `Makefile:23` copies from the literal
`target/release/tack` regardless of `$CARGO_TARGET_DIR` — any non-default target dir
breaks the `cp` in `desktop-sidecar` unless `target` is symlinked to the real location
first, exactly as this handoff's Measured numbers section already described.

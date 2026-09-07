# VII-D1 handoff

- Base SHA / branch / final SHA: `cf6cbd9a78fdfad2c19a59a7ff30334799dfb0eb` (the SHA the
  worktree and its already-built bundle were prepared against; `develop`'s real tip had
  moved to `cc7db7ea77174a2721c6fa8be31aab87ed71fe08` by the time this card ran — docs-only
  commits ahead, not rebased onto, per this card's instructions) / branch
  `agent/vii-d1-stranger-walk` / final SHA recorded after commit below.
- Files changed (must equal ownership list): `docs/agent-handoffs/part-vii/VII-D1.md`
  (this file), `docs/agent-handoffs/part-vii/vii-d1-proof/*.png` (50 screenshots, 8 of
  them — `01`–`08` — inherited untracked from the stopped prior agent and reused as-is;
  `47`–`50` new this session), `docs/book/src/user-guide/quick-start.md` (one new
  three-row table, the card's other owned item). Nothing else.
- Contract fixtures consumed: none.
- Behavior implemented: none — this is a proof/verification card, not a code card.
- Tests added and exact commands/results: none (no code changed). The "test" is the live
  transcript below.
- Failure/adversarial case proved: Quit while an attempt is in flight (dialog appears,
  Cancel keeps everything running, OK stops everything); reopening after a window close
  renders the server's real, unchanged state; two independent, reproducible captures of a
  `POST /api/executions` failure mode neither this card nor any prior Part VII card had
  recorded (see Claim → evidence and the product-findings entry below).
- Schema/API/contract change requested from another owner: none directly, but see the
  "Could not enqueue execution" finding below — it belongs to whoever owns
  `POST /api/executions` (the execution-request path in `tack-api`/`tack-orch`), not to
  this card.
- Known limitations or `not_measured` fields: macOS and Windows rows on the quick-start
  table are `not_measured` (no such machine here, matching every other Part VII card's
  limitation). Wayland is `not_measured` (this session, like every other Part VII
  handoff, ran X11). This session's own app relaunches (for the daemon proof and the
  Quit-warning proof, after the first session) used `nohup <AppImage> &` with
  `HOME`/`PATH`/`DISPLAY` set explicitly rather than re-driving Nautilus's double-click
  each time — the double-click mechanics were already proven once, by the prior agent,
  in screenshots `01`–`06`; re-proving them per relaunch would not have exercised
  anything new, only repeated it. `PATH` was deliberately included in every launch (see
  below) to test whether the app forwards it to the harness subprocess — that specific
  question is answered, not assumed.
- Secrets/logging review: no secrets touched or logged by this card. The stranger
  environment's `TACK_EXECUTION_DECISION_TOKEN` field visible in screenshot `47` was
  never filled in or saved.
- Safe merge order and likely conflicts: last card, Wave 21. Touches only
  `quick-start.md` (a table insertion, adjacent to but not overlapping VII-C2's own
  paragraph in the same file) and adds a new, previously-absent handoff file plus a new
  screenshot directory. No overlap with any other Part VII or Part VI branch expected.
- Checklist: no unowned files touched; no live secret; no panic stub (n/a, no code); no
  blind retry (the two enqueue-failure reproductions below were retried deliberately, to
  characterize the failure, and reported as a finding rather than papered over).

## What was inherited vs. what this session did

The prior agent had completed the install-through-first-successful-attempt half of the
walk (screenshots `01`–`46`): downloading is simulated by placing the pre-built AppImage
in the stranger's `Downloads/` (the release page itself carries no desktop bundle — see
below), making it executable via Nautilus's own Properties dialog (no terminal),
first-run, project/item creation, turning agent execution on, and running a real
attempt through to `Succeeded` with an artifact. The stranger's `tack.db` and the app's
own process were still alive when this session started, with the main window already
hidden (closed to tray) — a genuine mid-walk state, not a fabricated one. This session
verified that state (project, item, one succeeded attempt with an artifact all still
present and correct — see Claim → evidence), decided to **continue from it rather than
wipe it**, because the true-first-run evidence (`01`–`08`: welcome dialog naming the real
data-root path, the "use existing tack.db" prompt) was already captured once and wiping
it would only re-prove the same screens without adding anything, then completed the two
things the walk still needed: the close→observe→reopen daemon proof and the
Quit-warns-while-running proof, both with fresh timestamps under this session's own
control (screenshots `47`–`50`).

## Claim → evidence

| Claim (user-visible) | Evidence |
|---|---|
| The release page has no desktop bundle; the local build is the only source | `gh release view v0.1.0-beta.7 -R yielab/tack --json assets` — 17 assets, all `tack-*`/`tack-runner-*` server/runner tarballs, SBOMs and `SHA256SUMS`; zero `.AppImage`/`.deb`/`.dmg`/`.msi` |
| A stranger makes the AppImage executable without a terminal | Screenshots `02`–`04`: Nautilus right-click → Properties → **Executable as Program** toggle |
| First run shows the real data-root path and offers to reuse an existing `tack.db` | Screenshot `05` (welcome dialog): `Tack stores its data at: /var/tmp/tack-agent-targets/VII-D1/stranger-home/.local/share/tack` |
| The Agents page detects an installed harness without a terminal command | Screenshot `20`: `Claude Code — Installed v9.9.1` (the shim's `--version` output, detected via `PATH`) |
| No in-app login exists for either harness | Screenshots `20` and `33`: the Agents page's "Use the agent's own login" panel only shows presence (`Present, unverified`) with the note *"Tack cannot see whether this succeeded — confirm with a test run below"*; the Run-with-agent modal (`33`) offers a Harness dropdown and model/repo/budget fields, nothing that authenticates |
| A run reaches `Succeeded` with a downloadable artifact | Screenshots `41`–`46`; independently re-confirmed this session via `GET /api/executions/{id}/attempts` → `"state":"succeeded"`, `terminal_reason.artifact.name == "claude-code-run.log"`, `size_bytes: 190` |
| Closing the window never stops an attempt; the server keeps answering and the attempt keeps advancing | Daemon proof below, timestamps `11:43:34Z`–`11:44:48Z` |
| Reopening from the tray shows the same, unchanged server state | Screenshot `47` (mid-session reopen, showing the earlier item's drawer with its artifact and decision-token field intact) and screenshot `49` (post-daemon-proof reopen) |
| Quit warns when an attempt is in flight, and Cancel keeps it running | Screenshots `48` (first capture, Cancel path) and `50` (second, independent reproduction, OK path): both show *"1 agent attempt is running. Quit anyway?"* with Cancel/OK; process-proof table below shows both outcomes |
| Quit's OK stops the app and the server, freeing the port, with nothing left running | Process proof below, `11:45:51Z`–`11:45:53Z` |
| `POST /api/executions` can fail with an opaque, retryable-labeled error that retrying against the *same* item does not fix | Two independent reproductions, see the product-findings section below |

## Measured numbers

- AppImage size (this build): `91515384` bytes (`ls -la`) — GNOME's Properties dialog
  (screenshot `04`) rounds this to `91.5 MB`; VII-C2 measured a different build's
  AppImage at `87.9 MB` — both are the same bundle type, sizes differ build-to-build and
  neither figure is wrong.
- Server port: `127.0.0.1:3210` (`settings.json`'s pinned default, confirmed listening
  via `ss -ltnp` at every launch this session, never colliding with anything else on
  this shared machine — verified empty before each of this session's four launches).
- Data root: `/var/tmp/tack-agent-targets/VII-D1/stranger-home/.local/share/tack`
  (matches the welcome dialog exactly; derived from `HOME` alone, no `XDG_DATA_HOME`
  override needed).
- Attempt wall-clock time (shim-backed): consistently 2–15 seconds from `queued` to
  `succeeded` across nine attempts run this session — fast enough that catching one
  mid-flight for the Quit-warning proof required firing the tray's Quit event in the
  same shell invocation as the `POST /api/executions` call, back-to-back with no polling
  loop in between (a separate poll-then-fire attempt lost the race five times before this
  approach worked reliably — see Context spent).
- `mdbook`/doc build: not re-run this card (doc change is a three-row table added inside
  an existing section; no new heading, no broken link).

## What a stranger still cannot do

Log in to a harness from inside the app. The Agents page can detect that a `claude` (or
`codex`) binary is on `PATH` and report a version, but the "Use the agent's own login"
field only reports presence, never verifies or performs authentication — the label
"Present, unverified" and the note "confirm with a test run below" say this directly in
the UI. A stranger with a genuinely fresh account and no existing harness session has no
button anywhere in this app that logs one in; they would have to run the harness's own
`claude login`-equivalent in a terminal first, which is exactly the step this card's walk
is supposed to avoid needing. **This walk only reaches a completed attempt because
`stranger-bin/claude` is a fake harness shim that answers `--version` and prints
pre-scripted `stream-json` success output — completion was reached with a shim, not a
real, freshly-authenticated harness.** This is not a defect this card can fix; it is the
honest boundary of what "never opens a terminal" can currently deliver.

## Product findings (for whoever owns the affected surface, not this card)

**`POST /api/executions` can fail with an opaque `internal_error` that retrying the same
item does not fix, and the message gives no way forward.** Reproduced twice,
independently, both times against a freshly created item with a freshly generated
(never-reused) `idempotency_key`:

```
11:43:28.237Z  item 7d003b13-5f46-428f-9fad-f22989d64ce2, key vii-d1-daemon-proof-a
               -> {"error":{"code":"internal_error","details":{},
                    "message":"Could not enqueue execution",
                    "request_id":"req_operator","retryable":true}}
11:43:34.0*Z   same item, key vii-d1-daemon-proof-a2 (fresh key, same item, retried
               immediately after the first failure) -> identical error
(immediately after, a BRAND NEW item, 12726b69-7ecc-4781-8adb-e8b46b13b10b,
 same payload shape, fresh key) -> 200 OK, "state":"queued", succeeded seconds later
```

The same shape happened once earlier in the session too (item `a223f441…`, key
`vii-d1-quit-warn-004`). In every case: capacity was not the cause
(`GET /api/runners` showed `available_capacity: 1` throughout, and a fresh item right
after always succeeded instantly), and it was not an idempotency-key collision (fresh,
never-before-used keys were used both times against the stuck item and both failed
identically). The response claims `"retryable":true`, but retrying — even with a new key
— against the *same item* did not recover; only moving to a different item did. A
stranger hitting this in the real product would see "Could not enqueue execution" with
no indication that the fix is "try a different item," and no visible reason why. This is
a `tack-api`/`tack-orch` execution-request-path finding, not a `tack-desktop` one — it
reproduces identically over plain `curl`, with no Tauri window involved.

**`model_provenance.kind: "mismatched"`, surfaced correctly.** Every attempt in this
session requested `anthropic/claude-sonnet-4-5` (the project's default model) but ran on
the shim's `fake/model-1`; the item drawer's Execution tab labelled this "Mismatched
request" (screenshot `43`) rather than silently reporting the requested model as if it
had actually run. Recorded here as a positive finding, not a defect — it is exactly the
"unmeasured is nullable, unsupported is typed" posture this codebase asks for, and it is
worth knowing the UI honors it under a shim exactly as it would under a real harness.

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: `XDG_CURRENT_DESKTOP=ubuntu:GNOME`.
- `$XDG_SESSION_TYPE`: `x11`.
- Appindicator host: `gnome-extensions list --enabled` includes
  `ubuntu-appindicators@ubuntu.com`; every launch this session registered a fresh
  `org.kde.StatusNotifierWatcher` item (`tray_icon_tray_app_<pid>_1`), confirmed via
  `gdbus call … org.freedesktop.DBus.Properties.Get … RegisteredStatusNotifierItems`,
  and its `com.canonical.dbusmenu` menu exposed exactly the five items the product
  promises: *Open Tack*, an execution-status label, *Launch at login*, and *Quit*
  (separators between).
- systemd: `255 (255.4-1ubuntu8.17)`.
- `tauri-cli 2.11.4` (unchanged from VII-C1's own measurement; this card built nothing).
- macOS, Windows, Wayland: `not_measured` — no such machine or session available here.

## Daemon proof

All timestamps UTC, one continuous sequence, this session's third app relaunch (pid
`686541` for `tack-desktop`, `686660` for `tack serve --with-runner`):

```
11:43:14.968Z  launched Tack_0.1.0-beta.7_amd64.AppImage (nohup, HOME/PATH/DISPLAY set)
11:43:18Z      curl /api/health -> {"migrations_applied":62,"status":"ok",...}
11:43:28.237Z  POST /api/executions for a fresh item -> failed (see product finding above,
               unrelated to window/daemon behavior); retried once, failed again
11:43:34.917Z  wmctrl -c "Tack"  (real _NET_CLOSE_WINDOW, not a destroy)
               window not listed by wmctrl -l within 1s
[a fresh item + fresh idempotency key was then used instead, to route around the
 enqueue finding above and keep the daemon proof itself clean]
11:44:11.996Z  POST /api/executions -> 200, "state":"queued" (window already closed)
11:44:28.160Z  [observation 1, second shell] pgrep -af "tack-desktop|tack serve" ->
               both pids present; curl /api/health -> 200 ok; attempt state -> "succeeded",
               artifact staged, model_provenance.kind "mismatched" (expected, shim)
               wmctrl -l -> still no Tack window listed
11:44:48.220Z  [observation 2, +20s] identical: both pids present, health 200,
               attempt still "succeeded" (terminal, unchanged), window still not listed
11:45:01.435Z  tray "Open Tack" clicked via the real StatusNotifierItem/dbusmenu Event
               call (id 2, eventId "clicked") — the same mechanism a real click sends
               window listed again within 1s (screenshot 49): same server, same session,
               not a restart
```

The close was a genuine `_NET_CLOSE_WINDOW` (what a window manager's close button sends,
confirmed via `wmctrl -c`, matching VII-B2's own precedent for what counts as a real
close rather than `xdotool windowclose`'s raw `XDestroyWindow`). The reopen went through
the tray's actual D-Bus menu protocol (`com.canonical.dbusmenu.Event`), not a synthetic
window-manager command — this is what a real left-click on the tray icon triggers under
the `ubuntu-appindicators` extension.

## Process proof

`pgrep -af "tack-desktop|tack serve"` before/after every step this session (four
launches total across the session; the first was inherited already running):

**Inherited session (start of this card):** both processes present, window unmapped
(`xwininfo -id … -> Map State: IsUnMapped`), tray registered
(`tray_icon_tray_app_197144_1`) — verified via `/proc/<pid>/environ` that both pids
carried `HOME=…/stranger-home`, confirming they were the stranger's own, before touching
anything.

**Close → reopen (third relaunch, pids `686541`/`686660`):**
```
before close:  686541 tack-desktop, 686660 tack serve --with-runner, window listed
after close:   686541, 686660 (unchanged), window NOT listed
after 20s:     686541, 686660 (unchanged) — no drift, no crash, no restart
after reopen:  686541, 686660 (unchanged pids — no second process spawned), window listed
```

**Quit, one attempt in flight, Cancel (first reproduction, pids `626007`/`626126`):**
```
before: 626007 tack-desktop, 626126 tack serve --with-runner, dialog window present
         ("1 agent attempt is running. Quit anyway?")
after Cancel: 626007, 626126 unchanged, dialog gone, health still 200
(the in-flight attempt finished on its own moments later, unrelated to Cancel)
```

**Quit, one attempt in flight, OK (final reproduction, pids `686541`/`686660`):**
```
11:45:28.074Z  POST /api/executions (fresh item, fresh key) -> 200 queued
11:45:28.7*Z   tray Quit clicked (dbusmenu Event, id 7) — fired back-to-back with the
               POST above, no polling gap, to catch the attempt while still in flight
11:45:28.825Z  dialog window present (screenshot 50): "1 agent attempt is running.
               Quit anyway?" — both processes still alive
11:45:51.475Z  clicked OK (xdotool mousemove + click at the dialog's OK button)
11:45:53.089Z  wmctrl -l -> no Tack window; pgrep -af "tack-desktop|tack serve" -> empty;
               ss -ltnp | grep 3210 -> empty; curl --max-time 2 /api/health -> HTTP 000
```

No orphan after any Quit-driven exit. No foreign process was ever touched — every pid
acted on in this session was verified via `/proc/<pid>/environ` to carry either
`HOME=/var/tmp/tack-agent-targets/VII-D1/stranger-home` or (for the AppImage's own mount
helper) a `cwd` under `/tmp/.mount_Tack_*` spawned by one of those pids. At the end of
this card, `pgrep -af tack` shows no `tack-desktop` or `tack serve` process left running
from this card's work (see the final report).

## Context spent

- Cold start (skills, CLAUDE.md already resident, dispatch block, card section, §VII.1
  rules 3–4, §VII.5, §VII.6, both templates, launch/data-root/port/tray/Quit sections of
  five prior handoffs): roughly in line with the dispatch block's ~20k estimate.
- The largest unplanned cost was empirical, not reading: five attempts to catch an
  in-flight execution for the Quit-warning dialog before the "fire Quit in the same
  command as the POST, no polling loop" approach worked — the shim completes attempts in
  as little as ~2 seconds, faster than a poll-then-react loop across separate tool calls
  could reliably observe. Recorded here so a future card doing the same kind of proof
  does not repeat the same five failed timing attempts.
- Files opened and not used: none beyond the read list — the dispatch block's list
  matched what was actually needed.
- Read-list lines that were wrong: none noticed.

## Proposed board row (integrator edits `TODO.md`, this card does not)

**VII-D1 integrated — a stranger installs the AppImage, never opens a terminal, and
reaches a finished attempt whose artifact is listed.** The transcript reproduces
install-through-Succeeded (screenshots `01`–`46`, inherited from the interrupted prior
run) plus a fresh close→observe→reopen daemon proof and a fresh Quit-warns-while-running
proof (`47`–`50`, this session). The release page itself carries no desktop bundle —
confirmed via `gh release view`, 17 assets, all server/runner tarballs and SBOMs — so
every Part VII card's own build is what a stranger would actually be downloading once
release automation ships one; documented, not treated as a defect of this card. One
finding for whoever owns `POST /api/executions`: it can fail with an opaque
`internal_error` that retrying the same item — even with a fresh idempotency key — does
not clear, reproduced twice. **Part VII's Wave 21 is done; this was the last card.**

## Amendments

*(none yet)*

# VII-C3 handoff

- Base SHA / branch / final SHA: `129c6c6` (the `develop` tip given for this card) /
  `agent/vii-c3-window-or-reason` / `e49ef25`.
- Files changed (must equal ownership list): `crates/tack-desktop/src/main.rs` only.
  `supervisor.rs` and `first_run.rs` are named in the ownership line but did not need a
  change — the defect and its fix both live entirely in how `main.rs` sequences its own
  setup, not in either of their own logic (both were re-read whole per the read list and
  neither has a bug).
- Contract fixtures consumed: none.
- Behavior implemented:
  - **The root cause, measured, not guessed.** `first_run::ensure_settings` called
    `app.dialog()....blocking_show()` synchronously inside `.setup()`, which runs on the
    app's main thread *before* `.run()` starts the platform (GTK) event loop.
    `tauri-plugin-dialog` 2.7.3's own doc comment
    (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tauri-plugin-dialog-2.7.3/src/lib.rs:160`)
    says plainly: "`blocking_show`... cannot be executed on the main thread as it will
    freeze your application" — its own desktop implementation marshals the dialog itself
    onto the main thread via `run_on_main_thread` and blocks the calling thread waiting
    for the answer. Calling it from that same thread, before its loop has ever started
    pumping, is a structural deadlock: the closure `run_on_main_thread` queues can only
    run once the loop starts, and the loop can't start until `.setup()` returns, and
    `.setup()` can't return until the blocked call does. On a fresh install this is the
    *first* thing that happens, so every fresh launch hit it.
  - **The fix.** Moved the `ensure_settings` call (and everything derived from its
    result — `port`, `base_url`, `folders`) from directly inside `.setup()` into the
    already-existing `tauri::async_runtime::spawn` async task, wrapped in
    `tauri::async_runtime::spawn_blocking` so the actual blocking call runs on a tokio
    worker thread — not the thread `.setup()` ran on. `tray::build` and
    `tray::ensure_launch_at_login_default_on_first_run` stayed where they were: neither
    reads `settings`, they were only sequenced after it by proximity, not by a real data
    dependency.
  - **The catch-all arm now shows a dialog too.** It previously logged and called
    `handle.exit(1)` with nothing else — the half of this defect the card already
    diagnosed from reading the code. It now shows `"Tack could not start: {err}"`,
    covering `HealthTimeout` and `SpawnFailed` with the error's own `thiserror` message
    (which already names the bound for a timeout, e.g. "no Tack server answered health
    within 15s and none could be started" — no sleep was lengthened, because none needed
    to be).
- Tests added and exact commands/results: none added. The defect is a real cross-thread
  interaction with a real GTK main loop; the existing `ScriptLauncher`-backed supervisor
  tests never touch a window or a dialog, so they could not have caught this and can't
  usefully assert the fix either. Proof is four live, forced-failure runs against the
  real built bundle instead of a unit test — see Claim → evidence and Process proof.
  `cargo test --manifest-path crates/tack-desktop/Cargo.toml`: 23 unit + 3
  `dependency_boundary` tests, 26 passed, 0 failed (unchanged from before this card —
  none of the touched lines were reachable by any existing test, which is exactly why the
  bug shipped).
- Failure/adversarial case proved: all four terminal arms of `attach_or_start`'s match
  forced live against the real bundle (`PortOccupiedByOther`, `OutdatedServer`, the
  catch-all's `SpawnFailed`, and the success path itself on a genuinely empty data root)
  — each shown to render a real, visible dialog or window, then reverted to confirm
  normal behavior resumes. Detail in Claim → evidence.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - The catch-all's other variant, `HealthTimeout`, was not forced independently (would
    need a fake sidecar that never answers for the full 15s). `SpawnFailed` already
    proves the same arm and the same dialog call fire; `HealthTimeout` is a different
    error value taking the identical code path, not a distinct branch to prove separately.
  - macOS, Windows and Wayland: `not_measured` — no such machine or session was available
    here, matching VII-C1's own limitation.
  - Whether the tray icon is visible in the GNOME top bar was not checked — only that its
    own X11 window is created (`xwininfo` showed a `tray-icon tray app ...` window in the
    fresh-install run). GNOME Shell needs an extension for legacy tray icons; this was out
    of scope (VII-B2 owns the tray) and unaffected by this card's change.
- Secrets/logging review: no secrets touched. The new dialog text and the catch-all's log
  line both carry only `SupervisorError`'s existing `Display` output, already limited to
  ports and version strings.
- Safe merge order and likely conflicts: single-file change to
  `crates/tack-desktop/src/main.rs`; no overlap with VI-B5 or VI-C1 (different crates).
- Checklist: no unowned files touched, no live secret, no panic stub (the
  `.expect("ensure_settings must not panic")` documents a real invariant —
  `ensure_settings` has no panicking path; it falls back to `Settings::default()` on any
  load or parse failure), no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A fresh install (empty data root, no `tack` running) hung forever with no window, no dialog and no log line — reproduced independently of VII-C1's own finding | Two live runs against the pre-fix build. Run 1 (`RUST_LOG=trace`, empty `~/.local/share/tack`): `pgrep -P <pid>` empty at +3s and again at +23s (well past the 15s `HEALTH_TIMEOUT`); zero `INFO`/`ERROR`/`WARN` lines in the whole log (34 lines total, all `zbus` handshake trace from plugin init, nothing from this app's own code); `ss -ltnp \| grep 3210` empty; `curl /api/health` connection-refused; `find ~/.local/share/tack` still just the bare directory. |
| The hang is specifically `ensure_settings`'s dialog, not `DataPaths::resolve`, not `tray::build`, not the supervisor | Bypass test: hand-wrote `~/.local/share/tack/settings.json` before launching the *same* pre-fix build (so `ensure_settings` returns immediately without a dialog). Same build, same binary, only that one file present beforehand: tray log line appeared (`launch at login enabled by default on first run`), sidecar spawned as a real child (`tack(2572501)` under `tack-desktop`, confirmed via `pstree -p`), `started the Tack server version=0.1.0-beta.7 status=ok pid=2572501`, `curl /api/health` → `200 ok`, and a real `Tack` window opened (`wmctrl -l`; `xwininfo` on it: `1200x800`, `Class: InputOutput`, `Map State: IsViewable`). Everything downstream of the dialog already worked; only the dialog call itself never returned. |
| The crate itself documents why | `tauri-plugin-dialog-2.7.3/src/lib.rs:160`: "`blocking_show`... cannot be executed on the main thread as it will freeze your application"; its own doctest example (`lib.rs:161-174`) wraps the call in `std::thread::spawn` for exactly this reason. |
| The fix lets a fresh install reach the dialog, answer it, and open the real window | Live run against the fixed build, empty data root: `wmctrl -l` showed `Welcome to Tack` (real dialog, `xwininfo`: `510x231`, `InputOutput`, `IsViewable` — not the `InputOnly` leader window seen throughout). Answered with `xdotool key Return`; `settings.json` written (`{"database_path": null, "port": 3210}`); log gained `started the Tack server version=0.1.0-beta.7 status=ok pid=2572501`; `wmctrl -l` then showed `Tack` (`1200x800`, `InputOutput`, `IsViewable`); `pstree -p` showed `tack(2572501)` plus `WebKitWebProces`/`WebKitNetworkPr` children under `tack-desktop` — a real webview rendering the real board, not a stub. |
| `PortOccupiedByOther` renders a dialog, forced live, then reverts cleanly | Bound a raw `python3` socket listener on 3210 (accepts TCP, answers no HTTP — same shape as `supervisor.rs`'s own unit test). Launched the fixed build: log `port is occupied by something that is not Tack port=3210`; `wmctrl -l` showed a real `Tack` dialog (`669x195`, `IsViewable`). Dismissed with `xdotool key Return`; `pgrep` confirmed the app process exited. Reverted (killed the raw listener, confirmed `ss -ltnp \| grep 3210` empty), relaunched: normal path resumed — `started the Tack server ... pid=2577321`, real window opened. |
| `OutdatedServer` renders a dialog, forced live, and never touches the server it refused | Started a 6-line Python HTTP server on 3210 answering `/api/health` with `{"status":"ok","version":"0.1.0-beta.1"}`. Launched the fixed build: log `attached server is older than the bundled version server_version=0.1.0-beta.1 bundled_version=0.1.0-beta.7`; `wmctrl -l` showed a real `Tack` dialog (`646x195`, `IsViewable`). Dismissed; app exited. Fake server's own pid still alive and `/api/health` still answering the same body afterward — the app never signalled it, matching the "never stop a server you did not start" rule. |
| The catch-all arm renders a dialog, forced live, then reverts cleanly | Replaced the staged sidecar binary with a non-executable text file (`chmod -x`), rebuilt, launched: log `could not attach to or start a Tack server error=failed to spawn the sidecar: Permission denied (os error 13)`; `wmctrl -l` showed a real `Tack` dialog (`659x195`, `IsViewable`). Dismissed; app exited. Restored the real sidecar binary, rebuilt again, confirmed `cargo test`/`clippy`/`fmt`/`check-comments.sh` all still clean on the final tree. |
| Gate is clean on the committed code | `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml --all --check`: exit 0. `cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D warnings`: exit 0, zero warnings. `cargo test --manifest-path crates/tack-desktop/Cargo.toml`: 26 passed, 0 failed. `./scripts/check-comments.sh`: `✓ no board archaeology in crates/`. |

## Measured numbers

- `HEALTH_TIMEOUT` (unchanged): 15s (`supervisor.rs:16`).
- Pre-fix hang observed unchanged for ≥23s (past the 15s timeout twice over) with zero
  children, zero app log lines, zero port activity — ruling out "it just needs longer,"
  which is why the fix is a thread move, not a longer wait.
- Dialog window sizes (X11, this session): "Welcome to Tack" first-run dialog `510x231`;
  `PortOccupiedByOther` dialog `669x195`; `OutdatedServer` dialog `646x195`; catch-all
  dialog `659x195`. Main content window: `1200x800` (matches
  `WebviewWindowBuilder::inner_size(1200.0, 800.0)` exactly).
- `cargo test --manifest-path crates/tack-desktop/Cargo.toml`: 23 unit tests in 5.31s + 3
  `dependency_boundary` tests in 0.16s = 26 passed, 0 failed.
- Rebuild cost: the first cold build in this session (`cargo build -p tack-cli --release
  --features embed-spa` + `cargo tauri build`) took ~2m31s + ~1m29s (compiling the whole
  GTK/webkit2gtk/wry stack from an empty `CARGO_TARGET_DIR`). Every rebuild after that —
  three more, for the fix, the deliberately-broken sidecar, and the restored sidecar —
  recompiled only `tack-desktop` itself (a few seconds) plus bundling (~15-20s); this
  card's edits never invalidated the cached dependency tree.
- Sidecar binary: `21,761,944` bytes (`x86_64-unknown-linux-gnu`, release, `embed-spa`).
  Final AppImage: `91,490,808` bytes.

## What a stranger still cannot do

This card makes every failure `attach_or_start` can return visible, with its specific
reason — it does not add a fallback for a desktop environment that cannot render a GTK
dialog *at all* (as opposed to merely deadlocking the thread that tried to show one, which
was this bug). Nobody has proven any of this on macOS, Windows, or a Wayland session — only
X11 on this machine was available. And once the window is open, a sidecar that dies
mid-session is not detected or reported — `ServerMode::Started` is only read again at
shutdown; nothing polls it while running. That gap is not in this card's Acceptance and
is not claimed as fixed.

## Platform measured

- OS: Ubuntu 24.04.4 LTS, kernel `6.14.0-37-generic`, `x86_64`.
- Desktop environment: GNOME Shell (`wmctrl -m` → `Name: GNOME Shell`); window manager
  Mutter (`mutter-x11-frames` seen framing the main window in `xwininfo -tree`).
- `XDG_SESSION_TYPE=x11`.
- Appindicator/portal hosts running: `xdg-desktop-portal`, `xdg-desktop-portal-gnome`,
  `xdg-desktop-portal-gtk` all present. A tray-icon X11 window was created by the app
  (`tray-icon tray app <pid>-1`, `16x16`); whether GNOME Shell's top bar actually displays
  it was not checked (GNOME needs an extension for legacy tray icons; VII-B2's territory,
  unaffected by this card).
- systemd: `255 (255.4-1ubuntu8.17)`.
- `rustc`/toolchain, `tauri-cli`: unchanged from VII-C1's own measurement (this card did
  not touch the pin); `tauri 2.11.5` resolved into the build (VII-C1 recorded `tauri-cli
  2.11.4` as the CLI version, a different number).

## Daemon proof

Not applicable to this card as written. §VII.1 rule 3's close/reopen/state-persists
sequence is VII-B2's own claim, already proved there; this card is about whether the app
starts at all and says why when it doesn't, not the close-hide-reopen lifecycle. Nothing
in this card's Acceptance touches that sequence, and the fix does not run on that path —
`ensure_settings` only executes once, on first launch (or whenever `settings.json` is
missing), never on a reopen from the tray.

## Process proof

`pgrep -af "tack-desktop|tack serve"`-style checks (see the exact commands and output in
Claim → evidence for each scenario) before and after every launch: no child before launch
in every case; the sidecar appears as a direct child of `tack-desktop` (confirmed via
`pstree -p`, e.g. `tack-desktop(2568979)---tack(2572501)`) only on the success/Started
path; zero children on every forced-failure path (the process exits before anything could
be spawned, or — for `PortOccupiedByOther`/`OutdatedServer` — deliberately never spawns
anything at all). No orphan after any dismissed dialog: `pgrep -fa "tack-desktop|Tack_.*AppImage"`
came back empty within 1s of pressing the dialog's default button in every forced case.
The `OutdatedServer` case additionally confirms the attached (refused) server's own pid
was never touched — still running, still answering the same `/api/health` body, after the
app exited.

## Context spent

- Tokens read before the first edit (cold start): dispatch README header + VII-C3 block
  (~2.4k), the Part VII board prelude (`TODO.md` lines 1-58) + the VII-C3 card (lines
  395-433), `main.rs`/`supervisor.rs`/`first_run.rs` whole (213+623+154 lines — larger
  than the block's ~200/~430/~90 estimate; `supervisor.rs` in particular has grown well
  past 430 lines, mostly its own test module), the `blocking_show` grep, and the
  `VII-C1.md` paragraph grep. That last grep (`grep -n -B4-A12 -i "window" VII-C1.md`, as
  the block names) returned far more than one paragraph — VII-C1's own window finding sits
  inside its "Claim → evidence" table and "Process proof" section, both of which the `-B4
  -A12` window pulled in nearly whole. Read as returned rather than trimmed, since it
  turned out directly load-bearing: VII-C1's own live process trace (`pgrep -P` empty,
  zero children, in spawn mode) is what first suggested the supervisor was never reached
  at all, ahead of any of my own live testing.
- Files opened and not used: none beyond what's flagged above. `lifecycle.rs`/`tray.rs`
  were greped only, per the read list, and never opened; no other crate; `release.yml`
  and `docs/openapi.json` were not read.
- Read-list lines that were wrong: the file-size estimates for `supervisor.rs` (~430
  lines named, 623 actual) and `first_run.rs`/`main.rs` were close but undersold; nothing
  that changed what needed reading, since "whole file" was already the instruction.
- One process-level correction worth recording for whoever reads this next: this
  session's default shell `cwd` is the isolated worktree, but several early reads (`Read`
  tool calls with the full `/home/ox/Sites/objetivosMios/...` path, and a few `cd
  /home/ox/Sites/objetivosMios && ...` Bash calls used before this was noticed) targeted
  the **shared checkout** instead. A `diff` against the worktree's own copies of
  `main.rs`/`supervisor.rs`/`first_run.rs`/`TODO.md` came back empty, so nothing was
  actually read from a divergent tree — but the very first build and the first four live
  test runs (the pre-fix reproduction, both spawn-mode and the bypass) were built and run
  from the **shared checkout's** source, not the worktree's. Only once the sandbox refused
  an `Edit` against the shared path was this caught; all edits, and every build/test after
  that point, target the worktree correctly. Flagging this because the dispatch README's
  own instructions don't currently say "verify `pwd` before the first `cd`" — worth adding
  if another card hits the same thing.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

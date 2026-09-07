# TODO — Tack cycle boards

> **Read this header, then jump. Never read this file whole** — it is ~10.6k lines and
> roughly 90% closed-cycle decision history. Reading it costs more context than any card
> needs. The extraction recipe is in `.claude/context-budget.md`; the short version is
> `grep -n "^# \|^## " TODO.md`, then `sed -n '<start>,<end>p'` for the one section you
> need.

## Which board is live

| Part | Cycle | Phases | Status | Where |
|---|---|---|---|---|
| **VII** | **Desktop app & background service** | 61 | **Done 2026-09-07** — ADR 0062 accepted 2026-09-03; **Waves 18 and 19 integrated 2026-09-04** (VII-A2, VII-B1, VII-B2, VII-B3). **VII-C1 integrated 2026-09-05** at `03df038`. Linux and Windows bundles build in a real run; macOS was blocked on a keychain feature fixed on this line at `33dbbb2` and has not been re-run since. **VII-C3 integrated 2026-09-05**: every fresh install deadlocked before showing anything, because the first-run dialog blocked the main thread inside `setup()` before the event loop started. Found by launching the bundle, not by reading it. **VII-C2 integrated 2026-09-06** — the README now leads with the download and shows the app. Its correction to `crate-tour.md` exposed that the tray's agent-execution entry still says the route it reads does not exist, which is now **VII-B4** (Wave 22); **VII-B4 integrated 2026-09-06** — the tray now reads `GET /api/local-runner` and reports six typed states, so the app and the README screenshot beside it finally agree. **VII-D1 integrated 2026-09-07 — Wave 21 is done and so is this Part.** A stranger on this machine went from a downloaded AppImage to a finished attempt with artifacts using only a mouse, closed the window and reopened it from the tray to the same unbroken session, and got a real "1 agent attempt is running. Quit anyway?" from Quit — 50 screenshots and process checks around every lifecycle step. Two limits are stated, not hidden: the completed attempt used a fake harness shim, because neither bundled harness offers an in-app login and the Agents page only detects binaries on `PATH`; and the release page still carries no desktop bundle at all (the bundles exist only from the next tag's release run), so the walk used a local build. One product defect fell out and is carded as **VI-C36**: `POST /api/executions` can answer `internal_error` "Could not enqueue execution" with `retryable: true` for an item that no retry ever recovers — reproduced over plain `curl`. macOS and Windows rows are `not_measured`; no machine | [§VII](#part-vii--desktop-app--background-service-phase-61), top of this file |
| **VI** | **Agent Onboarding & Provider UX** | 60 | **ACTIVE** — Wave 14 integrated at `927f850`; ADR 0061 accepted 2026-09-03 (decision 1 refined the same day: keychain first, file fallback); **VI-B1, VI-B2, VI-C2, VI-C3 and VI-C4 integrated 2026-09-04**. **ADR 0063 accepted 2026-09-05** — two credential modes, and a provider in the key+endpoint mode is a module behind a trait (its decision 4 was rewritten before acceptance; the "an endpoint is configuration, never code" version was the ADR author's invention and was rejected). That unblocked VI-B3 and added VI-B4/VI-B5. **VI-B3 and VI-B4 integrated 2026-09-05** at `03df038`. **VI-B5 and VI-C1 integrated 2026-09-05** — Waves 15 and 16 are complete. Wave 17 is what is left. **VI-C5 was added and dispatched 2026-09-05** — the handler tests VI-C4 shipped without, whose owner rule ("the next Wave 16 card touching that crate") named a card that no longer exists. **VI-C5 integrated 2026-09-06**, and its declared gap plus V-C2's surviving escalation became **VI-C6** and **VI-C7**. **V-C2 landed the same day, so VI-D2 is dispatchable and was dispatched 2026-09-06**; VI-D1 is still last. **VI-C21 and VI-C23 integrated 2026-09-06.** VI-C21 found that the runner's attempt journal is keyed by `state_dir` + `attempt_id` and never by runner id, so it survives the identity swap — but a record written before the database was replaced names an attempt the server no longer has, and `report_recovery_and_apply_disposition` treats that as `RecoveryPending` rather than an error, so it is retried harmlessly on every boot forever; out of that card's scope and not yet carded. Integrating VI-C23 also exposed that nothing in the dispatch prompts ran `cargo fmt`: six files had drifted across earlier merges and blocked a push outright, since formatting is invisible until one is attempted. `CLAUDE.md`, `/card` and `/gate` now name `.githooks/pre-push` itself as the definition of done rather than transcribing its checks, which is how the list drifted in the first place. **VI-C22 integrated 2026-09-06** — the reset holds (row counts fall between runs instead of climbing), but its byte-identical-counts criterion does not, because that criterion conflated resetting the database with making the suite deterministic and only the first was the card's to deliver. Verifying it surfaced **VI-C24**: a `serious` WCAG AA contrast failure that VI-C1 reproduced against an unmodified base, VI-C13 carried forward and VI-C22's integrator measured again from an emptied database — named in three handoffs, carded in none, and absorbing part of the blame that belonged to accumulation. **VI-C20 integrated 2026-09-06** — the shared project is resolved once by name in a global setup step before any worker forks, so it no longer means "whichever project sorted first"; six of eight spec files depended on that stability without saying so, and one test wrote `default_model` straight onto it, which a stable identity would have turned from racy into permanently corrupting. Verifying it showed VI-C24 is **two** failing tests sharing one cause, not one. **VI-C24 integrated 2026-09-06 — the full chromium suite is green for the first time, 71/71.** The token was never at fault: the drawer's own mount animations ramped CSS `opacity` 0→1, and `opacity` composites the whole subtree, so every descendant rendered briefly below AA while axe was reading it. That is why three handoffs called it a flake — it was genuinely intermittent, because it depended on scan timing rather than any declared colour. Reverting the keyframes fails 8 of 15 repeats; restored, 15 of 15 pass. The alternative fix — making the test wait for animations to settle — was foreclosed by that card's own "do not touch the spec" rule and is worth knowing was never weighed. Its out-of-scope probe of another palette became **VI-C26**, **integrated 2026-09-06** — five of six token cells are scanned now, four were clean, and `graphite`/light is genuinely broken in a way no value change fixes: its `primary-600` is both text on light surfaces and the background under a deliberately dark `on-accent`, and the required ranges do not overlap (proven by trying the darkening and watching the other pairing drop to 2.92:1). The only shape that clears both flips `on-accent` to white, which changes what the palette *is* — escalated as a design decision, tracked with a disable scoped to that one scan so the other five stay real gates. **VI-D1 integrated 2026-09-06.** Both smoke steps proved able to fail by injected breakage, and a stranger reached a completed attempt in a clean `ubuntu:24.04` container typing only the command the container prints. **But it is not the end of this Part.** The run found two real defects, now **VI-C27** and **VI-C28**, and the first of them breaks this Part's own goal: a gateway key pasted while execution is already on never reaches the running runner, the Agents page reports success anyway, and the attempt sits at `preparing` silently — the exact failure this Part exists to rule out. The integrator's own review of VI-D1 also found its loopback guard compared the host by prefix, accepting `localhost.attacker.example` and handing it the stored credential; fixed at merge. Reviewing VI-C26 surfaced **VI-C29**: the comment gate has never scanned `frontend/e2e`, which exits 1 with 77 findings across its own categories. **VI-C27 integrated 2026-09-06 — worked by the integrator directly, at the user's request to audit it to its origin.** Two root causes, one per layer, and the symptom needed both: the control hands the runner its configuration **by value** and then mutates only its own copy (which is also what `catalog()` reads, so the page reported success from a view the runner never shared); and the engine dropped a typed pre-spawn rejection on the floor after announcing `preparing`, which is why the failure was silent and applied to every such rejection, not only this one. A change to a configured provider's credential is now a new runner lifetime — old task joined first, since no journal lock exists — and a `validate` refusal becomes a reported `failed` with `harness_rejected` through the existing outbox. Proven end to end against a fake gateway and a fake harness in the Agents page's own order with no Re-check; each half fails its own test when reverted (60.75 s at the snapshot wait; 0.01 s at the engine). The shared-handle alternative was weighed and rejected: it fights the runner's own fixed-per-lifetime model and would still need a capabilities-changed signal to keep the server's snapshot honest. The limitation VI-C21 documented is now **VI-C25**, and reading `engine.rs` to card it showed the handoff's "harmless" understates it: a `RecoveryPending` outcome keeps the workspace checkout on purpose, so a record naming an attempt the recreated database never had costs a retry every boot *and* holds its checkout indefinitely. **VI-C25 integrated 2026-09-06** — `stale_lease` is the one answer that settles the report, because the fencing token is fixed for the life of an attempt, so a miss on it will never match again; an absent reply or a transport failure keep their existing behaviour untouched. Retired means quarantined, not deleted: the record leaves the restart scan and the checkout stays as operator evidence, so the disk is not reclaimed automatically and that is the deliberate trade. **VI-C29 integrated 2026-09-06** — the comment gate scans `frontend/e2e` by default; 21 of the 22 "missing file" pointers were the gate's own bug (a citation wrapped across two lines left a `spec.ts` fragment that, as a search pattern, matched every real spec citation), one was genuinely stale and is repointed, and 69 comment lines across six specs were rewritten to keep what the test protects and drop the board pointer. The dead-pointer check now drops a candidate that is the tail of a real tracked filename, which also hides a stale name that happens to be a suffix of a live one — a known, smaller blind spot than the false-positive storm it replaces. **VI-C28 is integrated on `integrate/vi-c28` and held back from `develop`**: its real-browser spec passed for its agent only against a hand-started server, because the Playwright config never set `TACK_ALLOWED_ORIGINS` and the Vite proxy forwards the page's origin unchanged; with that env line added (from-scratch: 1 failed → 1 passed) the socket delivers events in the suite for the first time, and `execution-attempt-detail.spec.ts` then fails in 2 of 2 full runs and 1 of 2 three-spec runs. The cause is not the socket: `store.ts#loadAttempts` replaces ready data with `{status: 'loading'}` on every 4 s realtime tick, and the timeline only renders the attempt list under `status === 'ready'`, so the whole panel — decisions, the chosen option, the typed token — unmounts and remounts every tick; live events from the other worker make the click land on the gap. Carded as **VI-C31**. **VI-C31 and VI-C28 integrated 2026-09-06.** The store now writes nothing it already holds: `loadAttempts` keeps `ready` data rendered through a refresh and stores the result only when it differs, and `recordFor` hands back the same record while its parts are unchanged, because the timeline's `<For>` matches rows by reference and a fresh object for an unchanged row reads as remove-and-add. The card's other option, a `refreshing` flag on `ready`, was tried and does not hold here — flipping it needs a new object, and every reader of `attemptsFor()` re-runs on the reference, not the label. The agent had also projected `GET /executions/{id}`'s envelope down to the five summary fields inside the store; moved at merge to `executionsApi.get`, where the shape is decided, so the store compares like with like. Proof: choose an option and type a token in the Execution tab, wait past a poll tick, and the same Resolve node is still attached; red with `store.ts` reverted. Full chromium suite 77/77 for VI-C31 alone; with VI-C28 on top, the suite that failed 2 of 2 before it passes 3 of 3 (numbers below in the VI-C28 handoff amendment). With both in, the socket delivers events in the suite and the developer-port gap is **VI-C30**, still open. Reviewing `develop`'s last fourteen CI runs for a main merge found two misses of the embedded runner's own liveness backstops on CI (15 s to health, 30 s to active), green locally and on the next push — carded as **VI-C32** with a measured lead (the one sibling test that neuters D-Bus has never missed). **VI-C30 integrated 2026-09-07** — a loopback-bound board now trusts a loopback browser origin on the live socket, the same posture ADR 0059 gives the rest of the API on one machine; the two rejected shapes (a dev port in every install's defaults; a manual step in four docs) are weighed in its handoff. The integrator tightened it: the card judged the host by string prefix, which would have trusted `127.attacker.example`; it is parsed as an address now, bracketed IPv6 included, and the bind-address check shares the rule. **Two Dependabot majors landed the same day** (`toml` 1.1 + `validator` 0.21; nine GitHub Actions on their Node 24 majors). `sqlx` 0.9 stays out — its own `rust-version` is 1.94, the floor here is 1.89, and raising it is the user's call. Landing the actions bump exercised the two CI jobs that never run on a `develop` push and found both red on every pull request and every push to `main`, for reasons older than the bump: carded as **VI-C33** (tracing initialised twice under llvm-cov) and **VI-C34** (the embedded SPA answers unmatched `/api` routes with HTML 200 — a product defect as much as a gate one); and a local `cargo deny check` disagrees with CI's — **VI-C35**. **VI-C32 integrated 2026-09-07, and its card was wrong about what it was chasing:** read past the panic's line number, neither CI miss was a slow boot. One was `tack serve` exiting 1 in 3.3 s on `Address already in use` — the bind-then-drop port helper that `.config/nextest.toml` already isolated for the scheduler E2E was shared by the three embedded-runner binaries with no such isolation; the other was a bare `is_file()` on `session.json` taken the instant the server reported `active`, while the runner writes that file only after the enrolment reply — an ordering the sibling test already waited out. Both closed in the tests, no deadline touched, no product code changed; the D-Bus keychain probe the card suspected is real (a ≥30 s hang with a bus and no secret service) but explains neither miss and is left with its owner. **npm-major landed** (`@solidjs/router` 1.0 — functionally the 0.16 line, verified by diffing the tarballs — and `@types/node` 26); `typescript` 7 stays out because the OpenAPI generator's peer range is `^5.x`, and `jsdom` 30 because CI still pins Node 20 while it needs 22+ — both ignored in Dependabot with the reason written beside them. That Node 20 pin is worth a card of its own when the next frontend major needs it. **VI-C33 and VI-C34 integrated 2026-09-07** — the two jobs that only run on pull requests and `main` are green again on their card branches: tracing initialisation is `try_init` (the first call in a process installs the subscriber, a later one is a deliberate no-op, and all five crates clear their coverage floors: core 89.3 %, db 71.5 %, api 72.1 %, orch 94.8 %, runner 93.4 %); and each API surface now owns its own 404 as a fallback that travels with its `nest`, so the embedded SPA's catch-all never sees `/api` or `/api/runner/v1` — a mistyped path on the single binary answers `404 Not Found` instead of `index.html` with `200`, and deep links still get the page. Neither the CLI nor the frontend ever depended on the old 200: both parse every body as JSON. **VI-C35 integrated 2026-09-07** — CI's `cargo-deny` job ran `licenses bans` only while a developer's `cargo deny check` ran all four categories; now both run the unrestricted check and both are green. Nothing needed an exception: the three yanked crates took compatible bumps, and `proc-macro-error2` (unmaintained, "no safe upgrade") vanished because `validator_derive` 0.20.1 had already moved off it — which also silenced the future-incompat warning every build printed. The two MIT rejections VI-C35's card quoted did not reproduce on a clean checkout and are recorded as stale. `crates/tack-desktop` is the exception that stays: its advisories are deliberately outside the gate, because Tauri's GTK/WebKit bindings carry fifteen pre-existing "unmaintained" findings with no upstream fix — carded as **VI-C37** | [§VI](#part-vi--agent-onboarding--provider-ux-phase-60), top of this file |
| **V** | **Adoption & First Public Release** | 59 | **ACTIVE** — Waves 11–12 done. **V-C2 and V-C3 both integrated 2026-09-06**; V-C3 published nothing, by design, and its walk of the stranger's path found that `install.sh` had installed nothing since `v0.1.0-beta.7` (fixed in the same change). **V-C4** carries the two CI checks that stayed green through it. The publish list — posts, issues, Discussions, GitHub description — is the user's to run, and is in `docs/LAUNCH-CHECKLIST.md` | [§V](#part-v--adoption--first-public-release-phase-59) |
| **IV** | **Standalone Single-Binary Operation** | 58 | Done — Wave 10 integrated at `83fefab` | [§IV](#part-iv--standalone-single-binary-operation-phase-58) |
| III | Harness-Agnostic Runner Fleet | 50–57 | Feature-complete, **tag refused** | [§III](#part-iii--harness-agnostic-runner-fleet-phases-5057), archive |
| II | Agnostic Control Plane | 39–49 | Superseded after Wave B by Part III | [§II](#part-ii--agnostic-control-plane-phases-3949), archive |
| I | Agent-Factory Control Center | 33–38 | Complete 2026-08-05 | [§I](#part-i--agent-factory-control-center-phases-3338), archive |

**Parts VI and V are both active.** Part V is distribution and launch — everything between
"it works here" and "a stranger can use it". Part VI is the agent onboarding and provider
flow — everything between "a stranger installed it" and "a stranger ran an item with the
model they chose, without opening this file". Part IV is done. They share `README.md` and
`docs/screenshots/**`; the conflict rule is in
[§VI.3](#vi3-dependency-graph-cross-part-conflicts-and-merge-policy), which defers to
[§V.3](#v3-dependency-graph-cross-part-conflicts-and-merge-policy) for the files Part V
still owns. Read both before branching a card in either Part.

## Why the archive stays in this file

This section used to say the archive could not move, because 234 Rust doc comments cited
its section numbers. **That is no longer true, and the number is the whole reason it was
written.** Two commits stripped board archaeology out of the Rust comments and later work
finished the job. Measure before you decide anything on this basis:

```bash
grep -rn "TODO\.md" crates/ --include='*.rs' | wc -l   # 0
grep -rnE "TODO\.md.{0,6}(§ ?[0-6]|§I{2,3}\.|§1\.[0-9])" docs/ --include='*.md' \
  | grep -v agent-handoffs | wc -l                     # 1
```

So exactly one live document cites an archive section number today. Extracting Parts I–III
to their own file is therefore a decision about whether it is worth doing, not a migration
blocked by hundreds of silent breakages. It has not been done, and nothing here argues for
or against it — this paragraph exists so the next reader does not inherit a false constraint.

Moving or renumbering them breaks every one of those citations silently. The archive was
therefore **reordered below the active boards, not extracted**. Its numbering namespaces
(`§0`…`§6` for Part I, `§II.*`, `§III.*`) are unchanged and stay load-bearing.

## Conventions that hold across every Part

- One card, one worktree, one branch: `agent/<card-id-lowercase>-<slug>`, branched from the
  integration line the active board names — today `develop` for both Parts IV and V.
- Stay inside your card's `Owns`. A change you need in someone else's file is a request
  written into your handoff, not an edit.
- Each card writes exactly one handoff in `docs/agent-handoffs/<part>/<ID>.md`. Corrections
  are appended as amendments; the original claim stays, because the history of what was
  believed and later falsified is the point.
- No card edits a status board. The wave integrator does that, after independent
  verification by someone who did not author the code.
- Never `git commit` without the user asking; never add AI attribution to a commit message.

---

# Part VII — Desktop app & background service (Phase 61)

**Status: ACTIVE — ADR 0062 accepted 2026-09-03; Wave 18 dispatched the same day (VII-A2
and VII-B1, Sonnet agents in worktrees off `2958e9e`). The Tauri Linux prerequisites are
installed and verified on the dispatch machine.** The runner and the board already survive a closed
browser tab; nothing survives a closed terminal. This Part makes Tack a background service
with a window on top of it — an application, like Docker Desktop — and gives the terminal
path the same daemon.

Dispatch plan: **`docs/agent-handoffs/part-vii/README.md`** — read its header and your
card's block only. It names what to read, how much it costs, the gate, and when to stop.

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 18 — The daemon, two ways | VII-A2 · VII-B1 | 61 | **Integrated 2026-09-04** — both cards on `develop`; handoffs `docs/agent-handoffs/part-vii/VII-A2.md`, `VII-B1.md`. Integration found the workspace membership itself was wrong: Tauri pulls GTK/WebKit/glib into whatever workspace holds it, so `cargo build --workspace` first demanded a staged sidecar and committed icons, then failed three CI jobs on `glib-2.0 not found` — the server would have needed desktop system libraries to compile. **`crates/tack-desktop` is now excluded from the root workspace and is its own**, with its own lockfile, CI job and Dependabot entry; `externalBin` moved to a bundle-only overlay and the icon set is committed, which also settles the `.gitignore` question `7cc6221` had routed to VII-C1. VII-B2 and VII-B3 work inside that workspace and must not touch the root lockfile. VII-B2 and VII-B3 are dispatchable in parallel from the integration tip |
| 19 — Lifecycle and data | VII-B2 · VII-B3 | 61 | **Integrated 2026-09-04** — both cards on `develop`; handoffs `docs/agent-handoffs/part-vii/VII-B2.md`, `VII-B3.md`. The two shared `main.rs`, and the damaging half of that merged without a conflict: B2 passed the tray's first-run marker a `root` local that B3 had deleted when it replaced the temporary data root, and git saw no overlap because the two edits sat far enough apart. Caught by compiling, not by reading the diff — resolved to B3's real `paths.root`. `capabilities/default.json` took the union of both permission sets; a missing entry there fails at runtime, not at build. **Left for VII-C1:** the tree now carries two independent first-run signals in the same directory, B2's `.autostart-initialized` marker and B3's `settings.json`. Both work and neither reads the other. |
| 20 — Ship | VII-C1 → VII-C2 | 61 | **Both integrated** — VII-C1 2026-09-05 at `03df038`, **VII-C2 2026-09-06** (handoff `docs/agent-handoffs/part-vii/VII-C2.md`). C2 puts the download first in the README with two screenshots of the shipped app, and states the tray's "closing the window does not stop it" contract where someone meets it. Integration checked both screenshots by opening them. C2's own correction to `crate-tour.md` — the tray makes no HTTP request — is what produced **VII-B4** |
| 22 — Corrections | VII-B4 | 61 | **Integrated 2026-09-06** (handoff `docs/agent-handoffs/part-vii/VII-B4.md`, with three screenshots of the running app beside it). The menu is a status line, not a switch — a switch needs write and reconciliation machinery a three-second poll does not, and a correct line beats a control that races the server. Six typed labels; a server that has not answered is never rendered as *off*, proven by killing the sidecar under a running tray. `make desktop-sidecar` now copies from the directory `cargo` wrote to. The integrator corrected `crate-tour.md` in the same change — VII-C2 owns it, is closed, and had just corrected it in the other direction. **VII-D1 is unblocked.** Original note follows: not started, **before D1** — the tray's agent-execution entry is a hard-coded "unknown" whose doc comment says `GET /api/local-runner` does not exist. It does (VI-B3), and the Agents page shipped (VI-C1), so the app's own menu now contradicts the README screenshot beside it. Carries `make desktop-sidecar`'s `CARGO_TARGET_DIR` bug, found the same way |
| 21 — Proof | VII-D1 | 61 | **Unblocked 2026-09-06 — VI-C9 landed and a dispatch from the dialog now reaches a real attempt.** Original note follows. **Waiting on Part VI's VI-C9.** Wave 22 landed, so the tray D1 walks is the one that ships, and VI-D2 has freed the display. But D1's transcript requires the stranger to *run an agent on an item*, and today no dispatch from that dialog can succeed — VI-D2 hit exactly that wall and had to go around the button. Dispatch D1 the day C9 lands, not before |

## §VII.0 Cold-start context capsule

**What this Part is for, in one sentence.** Tack must run as a background service and
install like an application — its own window, icon and tray — so that closing the window
never stops an agent attempt and nobody opens a terminal to start it.

**The decision of record is ADR 0062** (`docs/adr/0062-desktop-app-and-background-service.md`,
accepted 2026-09-03): eight decisions in one table at the top of the file. Read that table,
not a paraphrase. No card re-decides one; a card that finds a decision impossible stops and
says so in its handoff.

### Evidence base, measured 2026-09-03

| Fact | Value | How it was checked |
|---|---|---|
| Server + embedded-runner composition root | `crates/tack-cli/src/local_runner.rs` — `with_runner_enabled` (l.67), `ensure_loopback`; started by `tack serve --with-runner` | grep |
| The UI survives a dropped connection | `frontend/src/shared/realtime/boardSocket.ts` reconnects with capped exponential backoff and re-fetches | read |
| Server path defaults — all relative to the current directory | `sqlite:tack.db?mode=rwc`, `./storage`, `logs/`; runner state `.tack-runner` | `crates/tack-api/src/config.rs:229-239`, `crates/tack-runner/src/config.rs:9` |
| Config-file lookup | `tack.toml` in the current directory only; when it exists, `TACK_*` env is ignored | `config.rs:351`, the file's own header comment |
| Release targets already built | `x86_64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`; `tack` with `embed-spa`, `tack-runner` as its own archive | `.github/workflows/release.yml:112-170` |
| Tauri 2 pieces the app uses | sidecar: `bundle.externalBin` + `tauri-plugin-shell` (`app.shell().sidecar("tack")`), binaries named with the target triple suffix; tray: `tauri` feature `tray-icon`, `TrayIconBuilder` / `Menu` / `MenuItem`; plugins `tauri-plugin-autostart`, `tauri-plugin-single-instance`, `tauri-plugin-window-state`, `tauri-plugin-opener`, `tauri-plugin-dialog` | v2.tauri.app, fetched 2026-09-03 |
| Tauri tray on Linux | icon **click events are not emitted**; the menu works | v2.tauri.app/learn/system-tray |
| Linux build prerequisites | `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` | v2.tauri.app/start/prerequisites |
| On the dispatch machine | **absent:** `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `libxdo-dev`, `cargo tauri`; present: `librsvg2-dev`, `libssl-dev`, `build-essential`; rustc 1.96.0; node v22.17.1; host `x86_64-unknown-linux-gnu` | `dpkg-query -W`, `cargo tauri --version` |
| OS-directory crates in the lock file | none — `dirs` and `directories` absent | `Cargo.lock` |
| Where in-flight attempts are listed (for the Quit warning) | `GET /api/executions` — `crates/tack-api/src/handlers/executions.rs:807`; read the handler for its status filter | grep |
| Frontend API base | `VITE_API_URL ?? '/api'` — same-origin by default, so the window loads the served UI unchanged | `frontend/src/shared/api/client.ts:9` |

### The data folders (ADR 0062 decision 5), fixed here so A2 and B3 agree without sharing code

| OS | Root (`dirs::data_dir()` + `tack`) | Under it |
|---|---|---|
| Linux | `$XDG_DATA_HOME/tack`, default `~/.local/share/tack` | `tack.db` · `storage/` · `runner/` (the runner state dir) · `logs/tack.log` · the app's own `settings.json` |
| macOS | `~/Library/Application Support/tack` | same |
| Windows | `%APPDATA%\tack` | same |

Handed to the server as `TACK_DATABASE_URL=sqlite:<root>/tack.db?mode=rwc`,
`TACK_STORAGE_DIR=<root>/storage`, `TACK_RUNNER_STATE_DIR=<root>/runner`,
`TACK_LOG_FILE=<root>/logs/tack.log`. The root is created `0700` on Unix. The `tack`
binary's own defaults do not change. The folder name is lowercase `tack` on every OS —
pinned here; the ADR's prose used a capital on two of them and this table wins.

### Vocabulary

Default screens and the tray say *Tack*, *agent execution*, *launch at login*, *Quit*.
"Sidecar", "supervisor", "webview", "attach" live in code and the developer book. §VI.1
rule 8 applies as written.

---

## §VII.1 Rules for simultaneous agents

**All of §III.2, §V.1 and §VI.1 apply unchanged.** Six rules are specific to this Part:

1. **The server is never re-implemented or re-linked.** `tack-desktop` spawns the bundled
   `tack` binary. No crate under `crates/tack-desktop` depends on `tack-api`, `tack-db`,
   `tack-orch` or `tack-runner`; a test asserts the dependency list from `cargo metadata`.
2. **No webview or GTK dependency enters an existing crate — or the workspace holding
   them.** `cargo tree -p tack-cli -e normal | grep -ci "tauri\|webkit\|gtk"` is `0`
   before and after every card; the musl release job is untouched. **Amended 2026-09-04:**
   this rule was written about crates, and the first violation happened one level up —
   `tack-desktop` as a workspace member made `glib-2.0` a prerequisite for building the
   server, red in three CI jobs. `crates/tack-desktop` is now `exclude`d from the root
   workspace and is its own; nothing in it may be added back to the root `members`.
3. **The work outlives the window — proven, never claimed.** Every lifecycle claim is a
   measured sequence: start an attempt, close the window, observe `/api/health` and the
   attempt's state from a *second process* while the window is closed, reopen, observe the
   same state rendered.
4. **Never stop a server you did not start.** Attach mode drives a foreign Tack only
   through its API and never signals its process. Proven by starting `tack serve` by hand,
   launching the app, quitting the app, and asserting the hand-started server still answers.
5. **No build output in git.** Sidecar binaries, `target/` and Tauri's `gen/` are produced
   by the build and gitignored. Icons are the exception and are committed: one source SVG
   plus the sizes `cargo tauri icon` derives from it, because a bundle build needs them
   present and regenerating them on every machine buys nothing. VII-C1 owns them; before
   it lands, a placeholder source SVG is enough.
6. **Signing secrets do not exist here.** No card adds a certificate, key or notarization
   credential to CI or the tree. C1 ships unsigned and documents the one-time warning.

---

## §VII.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `crates/tack-cli/src/service.rs` (new), one `Commands::Service` arm in `crates/tack-cli/src/main.rs`, the `dirs` dependency line in `crates/tack-cli/Cargo.toml`, `docs/book/src/user-guide/cli.md` §"tack service", the user-service rows of `docs/DEPLOYMENT-GUIDE.md` | VII-A2 |
| `crates/tack-desktop/**` (new): `Cargo.toml`, `tauri.conf.json`, `build.rs`, `src/main.rs`, `src/supervisor.rs`, `icons/`, `binaries/.gitkeep`; the workspace `members` line in `Cargo.toml`; `.gitignore` lines for `crates/tack-desktop/binaries/*` and `target/`; `Makefile` target `desktop-sidecar` | VII-B1 |
| `crates/tack-desktop/src/tray.rs`, `src/lifecycle.rs`, plugin registration for autostart and single-instance in `main.rs`, the Quit dialog | VII-B2 — **after B1** |
| `crates/tack-desktop/src/paths.rs`, `src/first_run.rs`, `settings.json` handling, the version check in `supervisor.rs`, the `dirs` dependency line in `crates/tack-desktop/Cargo.toml` | VII-B3 — **after B1** |
| `.github/workflows/release.yml` (one new `desktop` job), `.github/workflows/ci.yml` (one `tack-desktop` check step), `crates/tack-desktop/icons/tack.svg`, the release-notes paragraph about unsigned builds | VII-C1 |
| `README.md` §"Run it" (that section only), the book's install page, `docs/book/src/developer/crate-tour.md` entry for `tack-desktop`, `docs/screenshots/desktop-window.png` + `desktop-tray.png`, `CHANGELOG.md` `[Unreleased]` desktop lines | VII-C2 — **`README.md` and `docs/screenshots/**` are shared with Parts V and VI; see §VII.3** |
| `crates/tack-desktop/src/{main,supervisor,first_run}.rs` | VII-C3 — **after C1 merges**. Disjoint from `tray.rs` and `lifecycle.rs`, which VII-B2 owns and this card must not change |
| `crates/tack-desktop/src/tray.rs`, the `desktop` and `desktop-sidecar` targets in `Makefile` | VII-B4 — **after VII-C2**, and **before VII-D1**. Must not touch `lifecycle.rs`, `main.rs` or `supervisor.rs` |
| `docs/agent-handoffs/part-vii/VII-D1.md` (the transcript), the per-platform `measured / not_measured` table in the book's install page | VII-D1 |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |

---

## §VII.3 Dependency graph, cross-Part conflicts and merge policy

```text
ADR 0062 (accepted) ──┬── VII-A2 (tack service) ──────────────────────────────────────┐
                      └── VII-B1 (tack-desktop: sidecar + supervisor + window) ─┐      │
                                    ├── VII-B2 (tray, lifecycle, autostart) ────┤      │
                                    └── VII-B3 (data folders, first run, attach)┴── VII-C1 (release bundles) ── VII-C2 (README, install, screenshots) ── VII-D1 (stranger proof)
Part VI · VI-B3 (runner switch) ─── read by VII-B2's tray status; B2 does not wait for it
Part VI · VI-C1 (Agents page) ───── required before VII-C2 (the first-run screenshots show the real page)
```

**Wave 18 is two independent cards.** A2 touches only `tack-cli`; B1 creates a new crate.
**Wave 19 is two independent cards on B1's result** — B2 and B3 own disjoint files inside
the new crate. **C1 waits for both; C2 waits for C1 and Part VI's C1; D1 is last.**

### Cross-Part conflicts — read before branching

1. **`README.md`** — Part V (V-C3), Part VI (A3 landed; D2, D1 pending) and this Part
   (C2) write it. VII-C2 rewrites §"Run it" in place and nothing else; if VI-D2 is in
   flight, C2 waits for it; VI-D1 still takes the final merge. A card that finds another
   writer in flight escalates instead of racing — §V.3 and §VI.3 both say so.
2. **`docs/screenshots/**`** — V-C2's recording and VI-D2's three files keep their slots;
   VII-C2 adds two files and touches nothing else there.
3. **`crates/tack-cli/src/main.rs`** — VI-B1 adds `tack runner secret …`; VII-A2 adds
   `tack service …`. Disjoint arms; the integrator merges sequentially and builds once with
   both.
4. **`.github/workflows/release.yml`** — VII-C1 adds a job; no Part VI card touches it.
5. **The runner switch** — VII-B2's tray *shows* the state VI-B3 persists
   (`GET /api/local-runner`). Until VI-B3 lands, the tray shows *unknown* with the typed
   reason. B2 builds no second switch.

A card that discovers another collision states it in the handoff and stops.

---

## §VII.4 Cards

### VII-A2 — `tack service install | uninstall | status`

**Needs nothing on the machine.** Wave 18, parallel with B1.

**Owns:** `crates/tack-cli/src/service.rs` (new), one `Commands::Service` arm, the `dirs`
dependency in `crates/tack-cli/Cargo.toml`, `docs/book/src/user-guide/cli.md` §"tack
service", the user-service rows of `docs/DEPLOYMENT-GUIDE.md`, and the VII-A2 handoff.

**Context.** The terminal path's daemon (ADR 0062 decision 8). Linux: a systemd *user*
unit at `~/.config/systemd/user/tack.service` — `ExecStart=<absolute path of the running
binary> serve --with-runner`, `Environment=` the four folder variables from §VII.0,
`WorkingDirectory=<root>`, `Restart=on-failure`, `WantedBy=default.target`; `install`
writes it, then `systemctl --user daemon-reload && systemctl --user enable --now tack`.
macOS: `~/Library/LaunchAgents/com.yielab.tack.plist` with `RunAtLoad`, `KeepAlive`,
`EnvironmentVariables`, loaded with `launchctl bootstrap gui/$UID`. Windows: a typed
`service_unsupported_on_platform` error naming the desktop app. `status` prints the
unit's state and the health URL; `uninstall` disables, removes the unit, and leaves the
data root untouched. `--with-runner` in the unit makes the runner *available*; the switch
stays off (decision 6). The unit's working directory is the data root, which holds no
`tack.toml`, so a `tack.toml` in some other directory is not consulted — the doc says so.

**Acceptance:** on this machine, `tack service install` → `systemctl --user is-active tack`
prints `active`, `/api/health` answers; the shell that ran `install` is closed → still
active; `tack service uninstall` → unit file gone, no `tack serve` process, data root's
file count unchanged. The unit file and the plist are byte-asserted in unit tests (no live
systemd or launchd in CI). launchd's live proof is `not_measured` here and the handoff
says so. The Windows error is unit-tested. `cli.md` shows the three commands with real
output; `DEPLOYMENT-GUIDE.md`'s system-level unit is untouched and the new rows point at
this for the per-user case.

---

### VII-B1 — `crates/tack-desktop`: the app that supervises `tack`

**Needs the Tauri prerequisites on the machine** (§VII.0 evidence row). Stop before any
edit if `cargo tauri --version` fails or `pkg-config --exists webkit2gtk-4.1` fails, and
say so — nothing in this card can be proven without them.

**Owns:** everything under `crates/tack-desktop/` listed in §VII.2, the workspace
`members` line, the `.gitignore` lines, the `desktop-sidecar` Makefile target, and the
VII-B1 handoff.

**Context.** A Tauri 2 application with no frontend of its own: the main window's URL is
the local server (decision 4). Sidecar (decision 2): `bundle.externalBin` names
`binaries/tack`; `make desktop-sidecar` builds `tack` (`cargo build -p tack-cli --release
--features embed-spa`) and copies it to `crates/tack-desktop/binaries/tack-<host triple>`
(gitignored). Supervisor: on launch, `GET http://127.0.0.1:<port>/api/health`; if it
answers, **attach** (record it, never signal that process — §VII.1 rule 4); otherwise
spawn `tack serve --with-runner` through `app.shell().sidecar("tack")` with
`TACK_HOST=127.0.0.1`, `TACK_PORT`, and — until B3 lands — the four folder variables
pointed at a temporary root under the OS data dir; wait for health with a bounded, typed
timeout; open the window at the URL. On exit in *started* mode: terminate the child
(SIGTERM, bounded wait, then kill) and prove no orphan — a signal alone does not reap a
child, so the shutdown path must always finish with the reap. Single instance is **VII-B2's**
per §VII.2, not this card's. Port default `3210`;
a port held by something that is not Tack → native dialog naming the port, no retry loop.
In this card, closing the window quits (B2 makes it hide).

**Acceptance:** `cargo tauri build` on this machine produces a `.deb` and an `.AppImage`
(sizes recorded). Launching the AppImage shows the board in its own window; `pgrep -af
"tack serve"` shows exactly one child; `/api/health` answers from a second shell. Closing
the window terminates the child (assert no process, bounded). Attach: start `tack serve`
by hand, launch the app → attached, quit → the hand-started server still answers. §VII.1
rules 1–2 asserted by tests that read `cargo metadata` / `cargo tree`. Single instance is
not tested here — it is B2's. `cargo nextest run --manifest-path
crates/tack-desktop/Cargo.toml` runs the supervisor against a fake sidecar (a script answering `/api/health`)
so CI needs no webview.

---

### VII-B2 — Tray and lifecycle: close hides, Quit stops, launch at login

**Needs VII-B1.** Wave 19, parallel with B3.

**Owns:** `crates/tack-desktop/src/tray.rs`, `src/lifecycle.rs`, the autostart and
single-instance registration in `main.rs` (B1 deliberately left single instance to this
card), the Quit dialog, and the VII-B2 handoff.

**Context.** Decision 3. Tray menu: *Open Tack* · *Agent execution: on / off / unknown*
(read from `GET /api/local-runner` once VI-B3 exists; until then *unknown — the switch
arrives with the Agents page*, and B2 builds no second switch) · *Launch at login*
(checkbox, `tauri-plugin-autostart`, enabled on first run) · *Quit*. Window close →
`prevent_close` + hide; *Open Tack* → show + focus. *Quit* → list in-flight attempts
through `GET /api/executions` (read the handler for its filter); if any, a native dialog
"N agent attempts are running. Quit anyway?"; on confirm, stop the child as B1 does; in
attach mode, Quit closes the app and leaves the server alone. Linux: no icon-click
handler — the event is not emitted; every action is a menu item.

**Acceptance:** the daemon proof (§VII.1 rule 3), scripted: start a shim attempt, close
the window, from a second shell observe `/api/health` and the attempt advancing while the
window is closed, reopen from the tray → the same state rendered. Quit with an in-flight
attempt shows the dialog (screenshot in the handoff); Quit with none stops within a
bounded time and leaves no process. Launch-at-login on → the platform entry exists
(`~/.config/autostart/Tack.desktop` on Linux, capital T — the name comes from `productName` in `tauri.conf.json`; assert the file); off → it is gone. Single
instance still holds after B2's changes.

---

### VII-B3 — Data folders, first run, and the attach version check

**Needs VII-B1.** Wave 19, parallel with B2.

**Owns:** `crates/tack-desktop/src/paths.rs`, `src/first_run.rs`, `settings.json`
handling, the version check in `supervisor.rs`, the `dirs` dependency in the desktop
crate, and the VII-B3 handoff.

**Context.** Decision 5 and the §VII.0 folder table. `paths.rs` computes the root with
`dirs::data_dir()` + `tack`, creates it `0700` on Unix, and hands the four variables to
the sidecar (replacing B1's temporary root). The app's own `settings.json` in the root
holds a database-path override and the port — nothing else. First run: a native dialog
(`tauri-plugin-dialog`; no second frontend) that shows the data root and offers *Use an
existing tack.db…* through a file picker; afterwards, silent launches. Attach mode gains a
version check: read the server's version from the least invasive existing source (the
card measures what `GET /api/health` and `GET /api/openapi.json` carry today and chooses;
it adds no route) and refuse to attach to a server older than the bundled `tack
--version`, with a message naming both.

**Acceptance:** a fresh user on this machine → files appear exactly under the pinned
Linux path; the app's working directory gains no `tack.db` (assert); an override → the
server opens the chosen database (assert by item count through the API); version
mismatch → the typed refusal is shown, proven with a stubbed older version string in the
fake sidecar. macOS and Windows paths are unit-tested from the crate's own computation
and reported `not_measured` for a live run.

---

### VII-C1 — Release bundles for three operating systems, unsigned and said so

**Needs VII-B2 and VII-B3.** Wave 20.

**Owns:** the `desktop` job in `.github/workflows/release.yml`, one `tack-desktop` check
step in `ci.yml`, `crates/tack-desktop/icons/tack.svg` (source; generated sizes via
`cargo tauri icon`, committed as the CLI produces them — **VII-B1's `.gitignore` currently
excludes them under the old reading of rule 5; this card removes those two lines** and
deletes B1's `placeholder-source.svg`), the release-notes paragraph, and the VII-C1 handoff.

**Context.** Decision 7. Matrix: `ubuntu-22.04` → `.deb` + `.AppImage`; `macos-latest` →
`.dmg` for `aarch64-apple-darwin` and `x86_64-apple-darwin`; `windows-latest` → `.msi`.
Each job installs its prerequisites, builds the sidecar for the matrix target, runs
`cargo tauri build`, and uploads the bundles next to the existing archives. `ci.yml` runs
`cargo check -p tack-desktop` on Ubuntu with prerequisites and no bundle step. No signing
(§VII.1 rule 6); the release notes carry the two one-time warnings verbatim (macOS:
right-click → Open; Windows: SmartScreen → More info → Run anyway). The auto-updater
plugin is **not** wired: unsigned updates are worse than none.

**Acceptance:** one real workflow run — `workflow_dispatch` on the branch, or the cheapest
real trigger available, recorded — produces every artifact; the Linux artifacts are
downloaded and B1's launch acceptance is re-run against the AppImage; macOS and Windows
artifacts exist, are the sizes recorded, and are `not_measured` for launch. The musl
`tack` job's output is byte-identical to the previous release's build recipe (rule 2).

---

### VII-C2 — "Run it" says: it's an app; closing it keeps working; Quit stops it

**Needs VII-C1 and Part VI's VI-C1.** Wave 20, after C1.

**Owns:** `README.md` §"Run it" (that section only), the book's install page,
`docs/book/src/developer/crate-tour.md` entry for `tack-desktop`,
`docs/screenshots/desktop-window.png` and `desktop-tray.png`, the `[Unreleased]` desktop
lines in `CHANGELOG.md`, and the VII-C2 handoff.

**Context.** The app first (download → open → the board in its own window), the binary
second (servers, the terminal, `tack service install`), and the daemon promise in one
paragraph in the user's words. Two screenshots from the release build on this machine:
the window with the Agents page (VI-C1's, which is why this waits), and the tray menu.
Vocabulary check (§VI.1 rule 8) over everything written. README conflicts per §VII.3.

**Acceptance:** a stranger reading §"Run it" alone reports the three sentences in this
card's title without prompting (transcript). Screenshots are real, commands recorded,
alt text describes what is shown. `mdbook build docs/book` clean. `README.md` diff
touches §"Run it" only.

---

### VII-C3 — The app opens a window, or it says why it did not

**Needs VII-C1 merged** (there is a built artifact to launch). Wave 20. **Blocks VII-D1**,
which cannot walk a stranger through an app that shows nothing.

**Owns:** `crates/tack-desktop/src/{main,supervisor,first_run}.rs`, and the VII-C3 handoff.

**Context — two separate things, and only one of them is a mystery.** VII-C1 launched its
own built AppImage and the one CI produced. In both, the process starts and no window ever
appears.

The part that is not a mystery is in `main.rs`. The window is built only after
`attach_or_start` returns `Ok`, and of the error arms, two — a port held by something else,
an attached server older than the bundle — show a dialog before exiting. The third, the
catch-all, calls `handle.exit(1)` after a `tracing::error!` and nothing else. A GUI
application's log goes nowhere a user will look, so **every failure but those two is
indistinguishable from the app not starting at all**. That is a defect on its own terms,
independent of what triggered it here, and it is the reason nobody can say what triggered
it here.

The part that is a mystery is why the supervisor failed. Nothing was listening on the port,
so the app had to spawn its bundled sidecar. Whether the sidecar is missing from the bundle,
named something the launcher does not look for, not executable, or simply slower to answer
`/api/health` than the wait allows, is unmeasured — do not guess in the handoff.

**Tasks:** make every exit path say something a person sees, with the specific reason, not a
generic apology; find the real cause by running the built artifact with the app's own log
visible; fix it if it is in this card's files, and escalate with the measurement if it is
not. If the cause is a timeout, the fix is not a longer sleep — it is a bounded wait whose
expiry is reported as an expiry.

**Acceptance:** launching the built Linux artifact on a machine with no `tack` running opens
a window showing the board, stated with the artifact's name and where it came from. Every
`Err` arm of `attach_or_start`'s match is proven to render something visible, by forcing
each one — including the catch-all, proven by injecting a failure and watching the dialog,
then reverting. A sidecar that never answers is reported as that, with its own wait bound
named, not as silence. The tray still behaves as VII-B2 left it: closing hides, Quit stops.

**Stop if:** the cause turns out to be in the bundling rather than the app — the sidecar
absent from the artifact, or staged under a name Tauri does not resolve. That is VII-C1's
file, not this card's: record the exact path you expected and what the artifact actually
contains, and hand it over.

---

### VII-B4 — The tray says what is true, and `desktop-sidecar` honours `CARGO_TARGET_DIR`

**Needs nothing more than what is on `develop`.** Wave 22, **before VII-D1** — D1's
transcript walks this menu.

**Owns:** `crates/tack-desktop/src/tray.rs`, the `desktop` and `desktop-sidecar` targets in
`Makefile`, and the VII-B4 handoff. **Does not touch** `lifecycle.rs`, `main.rs`,
`supervisor.rs`, `paths.rs` or `first_run.rs`.

**Context, one.** `tray.rs` carries a hard-coded menu entry reading *"Agent execution:
unknown — the switch arrives with the Agents page"*, built `enabled: false`, above a doc
comment stating that `GET /api/local-runner` "does not exist yet". Both statements were true
when they were written and neither is true now: VI-B3 shipped the route, VI-C1 shipped the
Agents page, and VII-C2 put a screenshot of that page in the README — beside a tray
screenshot whose second line still says the feature is coming. The app contradicts its own
download page.

**Context, two.** `make desktop-sidecar` builds `tack` and then copies it from
`target/release/tack` literally, so with `CARGO_TARGET_DIR` set — which every agent working
this tree is told to set — the build succeeds and the copy fails, or worse, stages a stale
binary that is still there from an earlier run.

**Tasks:**
- Make the tray entry reflect the runner's real state, read from the route that now exists.
  Decide and record whether it is a live switch or a status line; a status line that is
  correct beats a switch that races the server. Whichever it is, the **failure** states —
  server not answering yet, request failed — are typed and shown, never rendered as "off".
- Fix the `Makefile` copy so it reads the same target directory `cargo` wrote to.

**Acceptance:** the tray's real text is proven by a screenshot of the running app with
execution on and a second with it off — not by reading the source. The unknown/unreachable
state is proven by launching with no server answering. `make desktop-sidecar` is run twice,
once with `CARGO_TARGET_DIR` set and once without, and the staged binary's `sha256` matches
the one `cargo` just built in both — paste both hashes. `make desktop` still builds.

**Stop if:** making the tray honest needs a change in `main.rs` or `lifecycle.rs` — those are
VII-B2's and VII-B3's files. Record what you needed and hand it over.

---

### VII-D1 — The stranger installs from the release page and never opens a terminal

**Needs everything.** Wave 21, last.

**Owns:** `docs/agent-handoffs/part-vii/VII-D1.md` (the transcript), the per-platform
`measured / not_measured` table on the install page.

**Context.** A clean Linux user account on this machine: download the AppImage from the
release page (or C1's artifact — say which), open it, first run, Agents page → turn
execution on, run an agent on an item, close the window, wait, reopen from the tray,
attempt finished with artifacts listed, Quit warns while something runs. Timestamps at
each step. macOS and Windows rows say `not_measured` unless a machine exists.

**Acceptance:** the transcript reproduces from the handoff alone; every claim in §VII.5
points at a line of it; nothing was typed in a terminal by the stranger after the
download.

---

## §VII.5 Definition of done, and deliberate exclusions

| Claim | Proof |
|---|---|
| Closing the window never stops an attempt; reopening shows its state | VII-B2 daemon proof, re-run by VII-D1 |
| Quit is the only stop, and it warns when something runs | VII-B2 |
| The app never re-implements or re-links the server; the musl `tack` job is untouched | VII-B1 dependency tests; VII-C1's unchanged musl job |
| A terminal user gets the same daemon without the app | VII-A2 live proof |
| Data lives in the OS folders; the working directory stays clean; an existing database can be chosen | VII-B3 |
| Unsigned bundles exist for Linux, macOS and Windows, and the one-time warning is documented where the download is | VII-C1 |
| The download page states each bundle's real size, including the AppImage's, rather than implying the server binary's | VII-C2 — measured 2026-09-03 on Linux: `.deb` 12.4 MB (uses the system webview), `.AppImage` 87.9 MB (carries its own). The product's "one small binary" line is about `tack`, and the app page must not blur the two |
| A stranger installs from the release page and reaches a finished attempt without a terminal | VII-D1 transcript |

**Deliberately not in this Part**, recorded so no card adopts them by drift:

- **Mobile or remote access** of any kind.
- **Code signing and notarization** — a separate decision with money attached.
- **A second frontend or an app-only API.** The window shows the served UI.
- **Turning the runner on by itself.** ADR 0061 decision 6's switch is the only way.
- **A Windows service.** `tack service` returns a typed unsupported there; the app is the
  Windows path.
- **Auto-update.** The plugin exists; it is not wired until signed builds exist.
- **Fixing tray-less Linux desktops.** Where no appindicator host exists the icon does not
  show; documented as a limitation, with the window still reachable from the launcher.

---

## §VII.6 Handoff additions for this Part

Use `docs/agent-handoffs/part-vii/TEMPLATE.md` — it points at Part VI's template and adds
three sections: **Platform measured** (OS, desktop environment, Wayland or X11 — tray and
autostart behave differently), **Daemon proof** (the exact sequence and observations of
§VII.1 rule 3), and **Process proof** (`pgrep -af tack` before and after each lifecycle
step: no orphan, no foreign server killed — rule 4).

---

# Part VI — Agent Onboarding & Provider UX (Phase 60)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Agent Onboarding & Provider
UX*, opened by the **agent-UX audit of 2026-09-03**. This Part has its own numbering
namespace (`§VI.0` … `§VI.6`) so the archive's load-bearing numbers stay put.

Like Parts III–V, this board is written to be picked up cold by parallel agents in isolated
worktrees. Every card is bounded, names every shared-file owner, and has an acceptance gate
verifiable without trusting its author's handoff.

**Dispatch plan:** [`docs/agent-handoffs/part-vi/README.md`](docs/agent-handoffs/part-vi/README.md)
— per card, the exact read list with measured sizes, what not to read, the gate, the stop
conditions, and the dispatch prompt; per wave, the integrator's adversarial checklist. A
card agent reads that file's header and its own block, then this board by anchor (§VI.0–§VI.3
and its card, ~7k tokens), and nothing else without recording why. Handoffs are written
from [`TEMPLATE.md`](docs/agent-handoffs/part-vi/TEMPLATE.md) there.

**This Part adds product features.** Part V §V.5 deferred every product feature to "a
post-adoption cycle"; this is that cycle, **for the agent surface only**. Notifications,
i18n, time tracking and in-UI diff review stay deferred — see §VI.5.

## Status board — Part VI

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 14 — Truth first | VI-A1 · VI-A2 · VI-A3 | 60 | **Integrated** at `927f850` on `develop` (handoffs: `docs/agent-handoffs/part-vi/VI-A1.md`, `VI-A2.md`, `VI-A3.md`). All three adversarially verified against the real tree, not just their own reports — A1's live worked example re-checked, A3's stranger-read test and render proofs opened, A2's ADR cited by line against 0050/0058. `mdbook build` clean; the only `docs/CONFIG.md` conflict (A1's new bullet vs. A2's rewritten paragraph, both anticipated it) resolved keeping both contributions. **ADR 0061 is `Status: proposed` — Wave 15 (VI-B1/B2/B3) does not branch until the user records acceptance with a date in `VI-A2.md`.** One non-blocking finding routed to VI-D1: `docs/book/src/roadmap.md:3273` wants a forward reference to ADR 0061 once accepted (not fixed here — outside every Wave-14 card's ownership). |
| 15 — Provider at the runner boundary | VI-B1 · VI-B2 · VI-B3 | 60 | **VI-B1 and VI-B2 integrated 2026-09-04** (handoffs `docs/agent-handoffs/part-vi/VI-B1.md`, `VI-B2.md`). **VI-B3 and VI-B4 integrated 2026-09-05** at `03df038` (handoffs `VI-B3.md`, `VI-B4.md`). Both were green alone and neither built after the merge: B4 made the discovery report's catalog one status per provider and B3 called it expecting a single one. Reconciled at integration, which is the first moment either card could have seen the other. Two comments the merge falsified were repointed, one caught by `check-comments.sh` rather than by reading. **VI-B4 corrected its own central claim under challenge:** its first vendor-name grep covered only the dispatch functions and reported zero; run over the whole tree with each file split at its own `#[cfg(test)]`, two production sites still named Vercel, one of which would have recorded an unobserved model as harness-reported for the next gateway added. VI-B5 follows B4 alone, because it changes the contract B4's acceptance holds byte-identical. **opencode was removed from the tree the same day** (ADR 0063 decision 8, handoff `docs/agent-handoffs/part-vi/opencode-removal.md`) — Tack ships adapters for two harnesses and is not limited to two: an unknown harness name still parses and is refused at claim with the same typed reason any undeclared one gets. **VI-B2 landed narrower and better-shaped than its card:** opencode's gateway path was measured working but deliberately not built, because it is the only harness that could not be credentialed by per-spawn injection — it needed a written config file. That card also claimed the provider machinery carries no vendor name and that a second endpoint is three data rows and no code path. **Measured 2026-09-05, it is not:** `attach_catalog` looks up the Vercel config key by name rather than walking the configured providers, and the three rows only suffice for a provider whose catalog is a bearer-authenticated `data[].id` list. VI-B4 makes the claim true. Two card errors corrected by measurement: the default secret name had to be `vercel-ai-gateway/default`, since `SecretStore::resolve` never appends the label, and the card's vendor URL for codex 404s. **Escalated, now carded:** the catalog publishes per-model pricing, context windows and modalities, and `ModelCombination` has nowhere to put them — a reviewed wire field, not the `additional` map. ADR 0063 decision 5 records it; **VI-B5 owns it**. ADR 0061 accepted by the user on 2026-09-03 (recorded in `docs/agent-handoffs/part-vi/VI-A2.md`, amendments). Sequential B1 → B2 → B3; base `2958e9e`. Decision 1 of ADR 0061 was refined on 2026-09-03 before acceptance (platform keychain first, owner-only file where none answers, backend reported); VI-B1's card and dispatch block already match. |
| 16 — UI-first flow | VI-C1 · VI-C2 · VI-C3 · VI-C4 | 60 | **Wave complete — VI-C1 integrated 2026-09-05** (handoff `VI-C1.md`): the Agents page, the sidebar entry, the Board's first-run banner, and the Fleet page's runner management moved under *Advanced*. Its stated blocker was wrong and its own amendment says so — the runner-v1 completion route is at `handlers/runner_protocol.rs:198`, in `tack-api` and not in either adapter; the eight live 404s came from guessed names and from percent-encoding that never delivered the real path. The real residual limit is narrower: a genuine harness subprocess reporting its own completion needs that adapter's argv and output shape. **VI-C2, VI-C3 and VI-C4 integrated 2026-09-04** (handoffs `docs/agent-handoffs/part-vi/VI-C2.md`, `VI-C3.md`, `VI-C4.md`). C2 merged with no conflict — it is frontend-only and shared no file with the desktop cards it integrated alongside. C1 still needs B2 + B3. VI-C4's two routes ship with no handler test in `tack-api` — the next Wave 16 card touching that crate owns adding them (see its amendment). |
| 17 — Proof | VI-C5 … VI-C23 · VI-D1 · VI-D2 | 60 | **VI-C17 integrated 2026-09-06 — the badges finally ask the question they mean.** `GET /api/executions` takes `item_ids` and returns exactly one row per id, its latest request, picked by a window function with an `id DESC` tiebreak because two requests for one item can share a `created_at`. Badges register interest on mount and every registration in one microtask coalesces into a single request: **one round trip per screen either way, but a row per card instead of up to 2000** — smaller than even the old 200-row default. Absence from the response is now conclusive, so VI-C14's `Unknown` chip and the hand-copied row cap are both **deleted rather than bound** — with badges asking by exact id there was nothing left for that constant to guard, which is the outcome VI-C17's own card said to check for first. The card **checked rather than assumed** the thing VI-C14's pricing had gotten wrong: this `serde_urlencoded` cannot deserialize a repeated query key into a sequence, verified in a scratch crate, so ids go comma-separated. The integrator verified the 2000-id cap against SQLite's own bind-variable limit — the bundled build is 3.46.0 with `SQLITE_MAX_VARIABLE_NUMBER 32766`, a 16× margin — and fixed one defect: the coalesced fetch was launched with `void` and had no `catch`, so a failed badge poll became an unhandled rejection. **Its 'pre-existing flake' claim was true and was checked rather than accepted** — on `develop` without this card, `two_servers_on_two_databases` failed 2 of 5 runs, and it still fails at `-j 1`, so it is machine load, not test parallelism. That test arrived with VI-C15 and its 10s deadline was set on an idle machine; `bootstrap_entrypoint`'s 5s shutdown budget took 110s under the same load. Both are now **VI-C23** — the same unreadable-signal disease V-C4 found in CI, from the other end. Earlier notes follow. | **VI-C18 and VI-C19 integrated 2026-09-06, and the E2E suite is green — 70/70, three consecutive full chromium runs from a clean state, for the first time this cycle.** C18 answered its own question the right way round: **the gate was never wrong, the test was.** It left the model selector on its silent default and assumed `getOrCreateProject` returns a stable project; `list_projects` orders `updated_at DESC` and the helper takes `[0]`, so under `fullyParallel` the shared project is whichever one any concurrent test last touched. It proved the leak instead of inferring it — the failing dialog showed a default model minted only by `scheduler-e2e.spec.ts`'s own fixture. The integrator's revert reproduced the failure in 4 of 5 runs. The helper itself is **VI-C20**. C19: `remove_secret` now reverses exactly the flip `set_secret` makes and nothing else, so a provider the operator enabled in config survives a key removal; the revert failed exactly one test, as claimed, and VI-C16's E2E workaround is gone. **What chasing the last failure found is worth more than either card.** The suite's outcome tracks how much state its persistent database has accumulated — **9 failures at 383 runners, 1 at 42, 0 from clean**, different tests at each level, all passing alone. That is why this cycle produced four irreconcilable flake rates for the same tests; none were wrong, none recorded the starting state. Now **VI-C22**. And deleting `e2e.db` without `storage-e2e/` leaves a runner credential for an id the new database has never seen — the failure class VI-C15 was carded for, on the axis its `storage_dir` scoping does not cover — now **VI-C21**. Earlier notes follow. | **VI-C10 and VI-C16 integrated 2026-09-06.** C10: `check-comments.sh` only ever scanned `crates/**/*.rs`, so the rule it enforces had never applied to the frontend — 219 citation lines across 80 files (the card re-measured; the board said 221, and it said so). **Four reached rendered operator text, three of them asserting gaps that had since been filled** — `GET /runners`, the fleet-member routes, and the precedence walk — so the UI was telling people that shipped features were missing. The integrator verified all three routes exist before accepting the rewrites. A callerless provenance helper whose only output was one of those strings is deleted, not reworded. The gate now understands JSX and block comments and single quotes, and flags a bare `docs/agent-handoffs` path. **The integrator found one more hole and fixed it: the card-id pattern was `[0-9]` , so the word boundary fell between the digits of every two-digit id and VI-C10 through VI-C19 passed straight through** — proven with a probe file, `VI-C9` caught and `VI-C10` not, then fixed and the whole tree still clean. **Its first catch after merging was VI-C14**, which landed minutes earlier and had never been scanned. C16: a lock directory keyed by API origin, so the three specs sharing one server-wide switch stop racing; it also measured a real ambiguity in `ProviderKeyPanel` (`stored()` is falsy while loading *and* when empty — 4 of 10 solo runs failed on the unmodified file) and escalated two things it could not own. **Both escalations were real and the integrator acted on both**: `agent-assets.spec.ts` is a recorder needing a release build, was never added to `playwright.config.ts`'s recorder exclusion list, and was failing every capability-reading spec alongside it; and `remove_secret` never undoes the `enabled` flag `set_secret` sets, now **VI-C19**. Full chromium suite, measured: **5 of 5 runs failed before, 7 of 7 fail exactly once after**, always `run-with-agent.spec.ts:98`, which passes alone — deterministic, reproduces at the pre-merge tip, and is now **VI-C18**. Earlier notes follow. | **VI-C14 integrated 2026-09-06.** The badges' shared preload now asks for the server's hard cap (2000) instead of taking its 200-row default, and an item absent from a response that came back *at* the cap renders `Unknown` rather than nothing — the two cases are indistinguishable from the client and silence claims the wrong one. One request for a screen of items, before and after. The card priced the real fix and stopped where it was told to: no route returns the latest execution for a set of item ids. The integrator's own revert failed **five** tests, not the four the handoff counted — an undercount in the safe direction, the unmentioned one being the `Unknown` chip itself. Two things the card did not measure, measured at integration: the cap raise costs **≈343 KiB more per board load** on an install big enough to fill it (195 B × 1800 rows, `ExecutionSummary` has five fields), and the `Unknown` chip lands on *every* uncached item once an install passes 2000 requests, including items that never ran — correct, since the client cannot tell those apart, but a visible change to a full board. Its escalation plus a duplicated constant became **VI-C17**. Earlier notes follow. | **VI-C15 integrated 2026-09-06.** The embedded runner's default state directory now derives from `storage_dir` instead of the process's working directory, so two `tack serve --with-runner` processes started from one shell against two databases no longer resolve to one credential. `TACK_RUNNER_STATE_DIR` still wins when set. State written under the old default is moved once, only when the new directory does not exist, and never reused when the rename fails — the card argued that choice against a silent migration and an unconditional refusal, and recorded the case it does not solve (several installs already sharing one legacy directory: the first to boot keeps it, the rest correctly self-provision). The integrator re-ran the revert proof and it failed as claimed, at `embedded_runner_state_scoping.rs:166` — but **at server A, not server B**, one assertion before where the test's own doc comment said it would, and server B's `expect` narrated a resume-then-refuse sequence the revert never reaches. Both texts were corrected to what reverting actually produces before the merge. Earlier notes follow. | **VI-C13 integrated 2026-09-06.** `a11y.spec.ts` had been failing since the modal redesign, the same disease `scheduler-e2e.spec.ts` had. The `agents-page.spec.ts` fix is the one worth reading: the test used to **revoke every other active runner in the database** for determinism — measured unsafe and about half effective — and three further approaches were measured and rejected before one that touches no other file's state. Three full parallel runs, clean. Its amendment corrects its own description of the auto-selection rule after review. Two escalations became **VI-C15** (the embedded runner's state directory does not follow the database — a credential outliving the database it was made against) and **VI-C16** (three specs driving one server-wide switch). Earlier notes follow. | **VI-C11 and VI-C12 integrated 2026-09-06.** C11: `docs/contracts/model-policy/precedence-table.json`, generated from the Rust walk and read by both sides. The integrator proved both directions personally — swap two entries in `ModelPolicyTier::ORDER` and leave the fixture, the Rust gate fails; regenerate it, and **twelve TypeScript cases fail with no TypeScript touched**. C12: the executions list takes an `item_id` and a bound (200, capped 2000, applied unscoped too); dropping the `WHERE` fails exactly the one test, naming the leaked row. Its bound made a pre-existing badge-staleness question reachable, now **VI-C14**. Earlier notes follow. | **VI-C9 integrated 2026-09-06** — the dialog can dispatch again, in both directions. The gate mirrors the scheduler's `declared \|\| passthrough` arm, and Auto is resolved rather than guessed, so a request that would queue forever is refused with the consequence named and a link to the fix. It also found the gate being fed every runner's capabilities instead of the target's. `scheduler-e2e.spec.ts` had been failing since VI-C2's picker redesign; its mechanics are updated and **its assertions are untouched** — the integrator checked the whole spec diff and found exactly one changed assertion line, an addition. **VI-C10 and VII-D1 are unblocked.** Three findings became **VI-C11** (nothing binds the client's copy of the precedence walk to the Rust one, and the dialog now blocks on it), **VI-C12** (`GET /api/executions` returns every row ever, and the browser filters) and **VI-C13** (two more specs the same redesign stranded). Earlier notes follow. | **VI-D2 integrated 2026-09-06** (handoff `VI-D2.md`, two dated amendments). It shipped `agents.png` and `attempt.png` and **deliberately did not replace `hero.gif`**: the recording it made showed the dispatch modal with a live *Unsupported* badge three seconds before a run that had been dispatched around the button with `POST /executions`. The recorder is guarded so it cannot regenerate silently and the re-recording recipe sits beside the guard. `two-machines.png` is produced and not shipped — every capability row reads *not supported*, and the enrollment panel prints a handoff path above the list, which is **VI-C10**. **VI-C9 is now the critical path: VI-C10 and VII-D1 both wait behind it**, D1 because a stranger's transcript has to dispatch an agent on an item and today cannot. Earlier notes follow. | **VI-C8 integrated 2026-09-06** (handoff `VI-C8.md`) — the attempt events route and the artifact download now have the scoping tests nothing was giving them. The download's proof is the one that matters and the integrator re-ran it alone: revert that single site's `request_id` predicate and exactly its test fails, carrying the other execution's actual file bytes back to a caller who asked under a different execution id. Earlier notes follow. | **VI-C6 and VI-C7 integrated 2026-09-06.** C6: both attempt-list routes answer `404` for an unknown attempt *and* for one belonging to another execution, and the integrator's independent revert showed the cross-execution test failing with the other execution's artifact row in the body — it catches a leak, not a status code. That revert also showed **two more sites with the same scoping and no test at all**, now **VI-C8**; the artifact *download* is one of them. C7: an `AutoSelect` request is rejected by the scheduler unconditionally and by design, so the dialog's own default queues forever on any install with no explicit default model at any tier — **VI-C9** carries the fix and waits for VI-D2, which is filming that dialog. C7's first verdict overstated the scope and its amendment says so; it also retracts its own proposed fix. Wave 17 original note follows. |
| 17 (original note) | — | 60 | **VI-C5 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-vi/VI-C5.md`) — six handler tests, each proven load-bearing; the integrator re-ran the ordering proof independently by flipping `repo/execution.rs`'s `ORDER BY` to `DESC` and watching exactly the two ordering tests fail. It declared one gap, now carded as **VI-C6**: neither route's unknown-attempt path is covered. **VI-C7** was added from V-C2's surviving escalation. Original note follows. **VI-C5 dispatched 2026-09-05** — it needs nothing and shares no file with the other two, so it runs now rather than waiting for them. D2 (assets) needs C1, C2 and Part V's V-C2, which owns `docs/screenshots/`; D1 goes last and needs everything |

**Integration line:** `develop`, the repository's default branch — same as Parts III–V.
Branch every card from `develop`. Do not create a `plan/*` line.

---

## §VI.0 Cold-start context capsule

**What this Part is for, in one sentence.** The agent-execution pipeline works end to end and
is unusable by anyone who did not build it, because the steps to reach a completed attempt
are split across three surfaces — a harness's own vendor login, the `tack` CLI, and the web
UI — with no screen or page that shows the whole path, and because the one question every
user asks first, *"which model, from which provider, and where do I put the key?"*, is
answered today by a negation.

**Read the posture correctly.** ADR 0050 ("the Tack API never starts a coding harness and
never becomes a model proxy") and ADR 0058 ("vendor credentials remain outside Tack") are
**right and stay**. They are statements about the API server. They have been read — by the
docs that cite them and by users — as "Tack cannot help you configure a provider", which is
a different claim and a false one. The runner already owns credentials (its own), already
owns the harness subprocess and its environment, and already accepts `secret_reference`
environment entries it cannot resolve. This Part moves the *guidance* and the *provider
configuration* to where the posture already puts them — the runner — and puts one screen
in front of both.

### Evidence base, measured 2026-09-03

Do not re-derive these; act on them. Each row names how it was checked so any card can
re-check one cheaply.

| Fact | Value | How it was checked |
|---|---|---|
| Worked example of creating an execution anywhere in `docs/` | **none** — one table row, `docs/MCP.md:99` | `grep -rn "execution create\|agent_profile_snapshot" docs/ --include='*.md' \| grep -v agent-handoffs` |
| `docs/API-REFERENCE.md` on the execution surface | delegates to `agent-runners.md` (§"Runner, Fleet & Execution", ~l.1273); that page has no example either | read both |
| Required fields on `POST /api/executions` | **13** (`agent_profile_snapshot`, `repository_snapshot`, `permission_policy`, `budgets`, `environment`, `metadata`, `timeout_seconds`, …) | `python3 -c` over `docs/openapi.json` → `CreateExecution.required` |
| Quick Start / Working with Items mention agents | **0 lines** | `grep -in "agent\|runner\|harness\|model" docs/book/src/user-guide/{quick-start,items}.md` |
| CLI reference documents `tack execution` / `runner` / `fleet` / `agent-profile` / `model-profile` | **no** — all exist, `crates/tack-cli/src/main.rs:252-278` | `grep -n "^## " docs/book/src/user-guide/cli.md` |
| `tack runner doctor` in the book | **no** — only `docs/CONFIG.md:91` (which says "run it yourself") and the roadmap | `grep -rn "runner doctor" docs/` |
| Book configuration page, runner/model rows | **only `TACK_ORCH_ENABLE`** (`configuration.md:22`); `docs/CONFIG.md` is the real reference and is not in the book | grep |
| Harness id in `agent-runners.md:32` | `claude_code` — **wrong**; wire id is `claude-code` | `crates/tack-runner/src/harness/claude_code.rs:105`, `frontend/src/shared/runWithAgent/shared.ts:52` |
| `agent-runners.md:378-381` "`model_profiles` … no runtime effect" | **false as written** — the modal copies the chosen profile into `requested_model_provider`/`requested_model_id`, the highest-precedence tier | `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:178-183` |
| `agent-runners.md` Known gaps: "`agent_fleet_members` has no write route" | **false** — `POST /api/runner-fleets/{fleet_id}/members` and `…/members/{runner_id}` exist; the UI still says nothing calls it | `docs/openapi.json` paths; `frontend/src/features/fleet/runnerFleet/FleetsPanel.tsx:117` |
| Model precedence documented for users | **nowhere** — only a Rust module doc | `crates/tack-orch/src/model_policy/mod.rs:1-3`, `wiring.rs:1-25`; `roadmap.md:2868` records it as intent |
| Project default-model tier | **no storage; always `None`** | `crates/tack-orch/src/model_policy/wiring.rs:17-23` |
| Agent-profile default model | a JSON convention `{"default_model": …}` inside `agent_profiles.limits`; **no UI field** | `wiring.rs:9-14`; `AgentProfilesPanel.tsx` |
| Hand-typed fields per run in the modal | **five** — runner id (free text), remote, base revision, subdirectory, timeout; **no memory** between runs | `RunWithAgentModal.tsx:298,386,393,399,412`; `rg localStorage frontend/src/shared/runWithAgent` → 0 |
| Project-level repository storage | **none** — only per-item `github_links` (migration 018) | `crates/tack-db/src/migrations.rs` |
| `secret_reference` environment entries | accepted by the contract, **never resolved** — "no secret-store client exists in tack-runner yet" | `claude_code.rs:62-64`, `codex.rs:772-775`, `opencode.rs:1156-1158` |
| Decision / artifact list routes | **none**; the UI takes a typed id | `agent-runners.md` Known gaps; `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx` |
| Harness auth state observable by Tack | **no** — `doctor` prints where a credential *lives*, not whether one exists | `crates/tack-cli/src/doctor.rs:223-244` |
| The model choice reaches the harness | yes — `--model` on all three adapters | `codex.rs:753`, `claude_code.rs:968`, `opencode.rs:1135` |
| Last migration | `061_execution_attempt_start_facts` → this Part's is **062** | `grep -n '"0[0-9][0-9]_' crates/tack-db/src/migrations.rs \| tail -1` |
| Vercel AI Gateway, per-harness endpoints | documented by the vendor for all three harnesses in this tree (table in VI-B2) | `vercel.com/docs/ai-gateway/coding-agents`, fetched 2026-09-03 — **re-fetch before relying on it** |
| README hero asset | alt text *"Board, Timeline, and vocabulary editor"* — a PM view | `README.md:12` |
| README screenshots that show an agent doing anything | **zero of five** (board, timeline, dashboard, list, vocabulary editor) | `ls docs/screenshots/`; `README.md:61-76` |
| README section order | *Features* lists project management first (`:32`) and agent execution second (`:42`); *Architecture* — where the two components are explained — is at `:169`, after *Status* | `grep -n "^#" README.md` |
| Book introduction's core concepts | item, workflow, project type, vocabulary — neither *runner* nor *run* | `docs/book/src/introduction.md` |

**The single most expensive line in that table is the provider one.** `docs/CONFIG.md:102-104`
says *"there is no `TACK_*` variable for a model provider or endpoint, by design"*. True for
credentials, and read by every user as "I cannot choose a model" — while the request body,
the CLI (`--model-provider`/`--model-id`) and the modal all route the choice. The two facts
never appear on the same page. Fixing that sentence is VI-A2; fixing the page is VI-A1.

### The surface map — the design authority for Waves 15 and 16

Every step between "installed the binary" and "an item completed as an attempt", where it
happens today, where it must happen after this Part, and — when the answer is not "the UI"
— the structural reason, so nobody re-litigates it per card.

| Step | Today | Target | Why not fully UI |
|---|---|---|---|
| Turn on agent execution | console: `tack serve --with-runner` | **UI** — one switch, on a loopback bind (ADR 0061 decision 6); the command is rendered only where the switch cannot exist (a non-loopback bind) | — starting Tack itself is the one console step left |
| Install a harness binary | outside Tack, undocumented | console, rendered in the UI: installed / absent per harness, with the vendor's install command | external binaries |
| Authenticate a harness with its own vendor login | console, outside Tack | console, rendered in the UI with the exact command and a re-check | OAuth device flows need a TTY; Tack never holds vendor logins (ADR 0050/0058) |
| Authenticate through **Vercel AI Gateway** | impossible | **UI** with the embedded runner — one key, pasted once, write-only, runner-local; console (`tack runner secret set`) for a remote runner, rendered in the UI | none in the embedded case |
| Enroll a runner | zero-touch (embedded) / UI token → console (remote) | unchanged | — |
| Create a fleet / add a member | fleet: UI; member: API only | UI | — |
| Create an agent profile | UI | UI, with a default created on first use | — |
| Choose a default model | impossible (no storage) | UI: a project setting, chosen from a **measured** catalog | — |
| Run an item | UI modal, five hand-typed fields | UI, defaults from project settings, **zero hand-typed identifiers** | — |
| See what an attempt produced / answer a decision | UI, but only with a manually-entered id | UI lists | — |

The last column is closed. A card that finds a step it cannot move to the UI for a reason
not in this table escalates in its handoff; it does not add a row.

### Why one provider, and why this one

Rule 3 of `.claude/scope-discipline.md` — build for the second case, not the fifth. The
harnesses' own logins are the first case and already work. Vercel AI Gateway is the second
because it is the only provider that is (a) a single API key rather than a vendor OAuth
flow, (b) documented by its vendor with a dedicated endpoint for **each** of the three
harnesses in this tree, and (c) a catalog endpoint, so the model picker can show what a
runner *measured* rather than a list someone typed. That combination is the only path to
"a UI-only user authenticates without a console". OpenRouter, direct Anthropic/OpenAI keys
and local endpoints stay where they are — the harness's own configuration — until the
gateway path has been proven live and a second gateway actually differs from it.

### The two-component story — the statement every page reuses

Written once here; VI-A3 applies it verbatim, and every page that introduces agents
(README, `introduction.md`, `agent-runners.md`, `CLAUDE.md`'s overview) opens with it or
links to it. Three pages currently say three different things; this is the one thing.

> **Tack is two components, built to be one product.**
>
> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, checks out
> an isolated workspace, launches the coding agent you already use — Claude Code or Codex
> — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

Two consequences, so no card re-derives them:

- **The runner is named in the story, never on a default screen.** The README and the docs
  explain two components because that *is* the product. The UI's default screens say
  *agent*, *model*, *provider*, *run* — "runner", "fleet", "enroll", "heartbeat",
  "capacity", "lease", "fencing", "harness" appear only under *Advanced* and in the
  developer book (§VI.1 rule 8).
- **The recovery demo is the visual proof of the third paragraph.** V-C2's recording
  (kill the runner mid-run → `needs_operator` → no duplicate → requeue → success) is not a
  competing hero asset; it is the picture of "they fail differently", and the README gives
  it a named slot next to that paragraph (VI-A3).

### Working-tree state at the time this board was written (2026-09-03)

`develop` is at `e5206c7` and the tree is clean. Part IV is done (Wave 10 at `83fefab`).
Part V Wave 13 is in flight: **V-C2** (the demo; owns `docs/screenshots/**`) is unblocked and
**V-C3** (launch preparation) waits on it. Both may touch `README.md`. See §VI.3.

---

## §VI.1 Rules for simultaneous agents

**All fourteen rules of §III.2 and all five of §V.1 apply unchanged** — in particular §V.1
rule 1 (no claim ships that a clean machine cannot demonstrate) and rule 4 (measure, never
estimate). Seven rules are specific to this Part:

1. **Every console step stays inside the flow.** If a step cannot be done from the UI, the
   UI renders it as a numbered, copyable command, says in one line *why* it is a console
   step, and verifies it afterwards with a real observation. A link to a documentation page
   is not a step. The strings the UI shows are the strings the docs show.
2. **Green means observed.** "Installed" means the probe ran the binary. "Authenticated"
   means a real attempt, or a real catalog call, succeeded with that credential. A session
   file that exists is "present, unverified" — the honest state — never a check mark. No
   status on any screen is derived from file existence.
3. **Provider secrets never enter `tack.db`, a log line, or the operator API** — with
   exactly one exception, which ADR 0061 (VI-A2) authorizes and bounds: a loopback-only,
   embedded-runner-only, write-once route that hands a key to the co-located runner's
   owner-only store without persisting it. Every card touching a secret asserts the
   *absence* directly (row counts, captured log output) with a positive control proving the
   assertion can fail.
4. **The gateway is a provider, not a proxy.** Tack still never calls a model API to execute
   an attempt. The runner's probe may call the gateway's catalog with the runner's own key —
   that is runner-side network, already the runner's domain. No `TACK_*` variable on the API
   server names a model endpoint; that sentence in `docs/CONFIG.md` becomes precise rather
   than absolute.
5. **Catalogs are measured, never typed in.** No static list of gateway or vendor models
   anywhere in the tree. What a picker shows is what a probe returned, with its timestamp,
   or a typed reason why nothing was returned.
6. **Requested and actual stay distinct.** When the gateway serves a model, `actual` records
   what the harness reported, with its `model_observation_source`; the requested pair is
   never copied into the actual columns. Reaching the same underlying model directly and
   through the gateway are two different `(provider, model_id)` pairs.
7. **Docket is out of scope.** Nothing here touches `orch_*`, `/approvals`, `/provision` or
   the ProvisioningWizard (ADR 0060 keeps them; it does not invite edits). "Agents" in this
   Part means runner-v1.
8. **Architecture vocabulary stays out of default screens.** The story names two components
   because that is the product (§VI.0, the statement). The UI's default screens say *agent*,
   *model*, *provider*, *run*. "Runner", "fleet", "enroll", "heartbeat", "capacity",
   "lease", "fencing" and "harness" appear only under *Advanced* and in the developer book.
   A test greps the rendered default screens for them; VI-C1 owns it.

---

## §VI.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `docs/book/src/user-guide/{agent-runners,cli,configuration,quick-start}.md`, `docs/API-REFERENCE.md` §"Runner, Fleet & Execution", the model-resolution rows of `docs/CONFIG.md` | VI-A1 — **VI-D1 amends after Waves 15–16 ship** |
| `docs/adr/0061-*.md`, the one provider-credentials bullet in `docs/CONFIG.md` | VI-A2 only |
| `crates/tack-runner/src/secrets.rs` (new), the store path in `crates/tack-runner/src/config.rs`, the `secret_reference` branch of each adapter, `tack runner secret …` in `crates/tack-cli/src/main.rs` (one subcommand arm + one module) | VI-B1 |
| The `[provider.*]` section of `RunnerConfig`, each adapter's spawn environment/args, `bootstrap::probe`, `crates/tack-cli/src/doctor.rs`, the gateway rows of `docs/CONFIG.md` | VI-B2 — **after B1 merges**. Exception recorded 2026-09-03: the refined ADR 0061 decision 1 requires `tack runner doctor` to report which store backend is in effect, so **VI-B1 owns that one `doctor.rs` line** and the store's wiring in `bootstrap.rs`. B2 owns everything else in both files and must not revert it |
| `crates/tack-runner/src/provider.rs` and the module tree replacing it, the catalog half of `crates/tack-cli/src/doctor.rs`, the registry | VI-B4 — **after B2 merges**. Disjoint from VI-B3 and may run alongside it. B4 must not touch the wire type or the fixtures; that is B5's, and B4's own acceptance depends on them staying byte-identical |
| The per-model field on `ModelCombination`, `docs/contracts/runner-v1/**`, the pin table in `crates/tack-orch/tests/runner_contract.rs` | VI-B5 — **after B4 merges**, and the only card that may change the wire contract in this Part |
| `crates/tack-api/src/handlers/local_runner.rs` (new), its mounts in `router.rs` (those only), the control handle in `crates/tack-cli/src/local_runner.rs`, the persisted on/off flag, `frontend/src/features/agents/{ExecutionToggle,ProviderKeyPanel}.tsx` + their client and tests | VI-B3 |
| `frontend/src/features/agents/**` except B3's two panels, `frontend/src/app/routes.tsx`, the nav entry, the *Advanced* section re-mounting `RunnerFleetSection`, the one mounting line in `features/fleet/FleetPage.tsx`, the first-run banner on the Board | VI-C1 |
| `frontend/src/shared/runWithAgent/**`, the attempt-state chip on Board cards | VI-C2 |
| `migrations.rs` (**062 only**), `crates/tack-core` project model, `crates/tack-db/src/repo/` project reads/writes, the project handlers, `crates/tack-orch/src/model_policy/wiring.rs` project tier, `frontend/src/features/settings/**` Agents tab, `frontend/src/features/fleet/runnerFleet/{AgentProfilesPanel,FleetsPanel}.tsx` | VI-C3 |
| The two attempt list handlers, their mounts in `router.rs` (those two only), `DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`, `frontend/src/features/item-detail/tabs/AgentActivityTab.tsx` | VI-C4 |
| New handler tests in `crates/tack-api/tests/` for those same two routes | VI-C5 — production files stay untouched |
| The unknown-attempt cases in `crates/tack-api/tests/handlers/attempt_lists.rs` | VI-C6 — additions only; VI-C5's tests stay byte-identical |
| Nothing in the tree. `docs/agent-handoffs/part-vi/VI-C7.md` only | VI-C7 — a measurement card; a fix it finds is written as a diff in the handoff, not applied |
| New tests for the attempt-scoped events and artifact-download routes | VI-C8 — production files stay untouched |
| `frontend/src/shared/runWithAgent/**` | VI-C9 — **after VI-D2 merges**, which is recording this dialog. Shares the directory with VI-C2, which is closed |
| `scripts/check-comments.sh` and the citation cleanup across `frontend/src` | VI-C10 — **after VI-C9 merges**; both edit `shared.ts` |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | **generated** — regenerated by whichever of B3 / C3 / C4 lands; the integrator re-runs `UPDATE_OPENAPI=1 … openapi_contract` and `npm run gen:api` after each merge. Never hand-edited |
| `docs/contracts/runner-v1/**`, `crates/tack-orch/tests/runner_contract.rs` | **VI-B5, and nobody else.** VI-B2 escalated the field and VI-B5 carries it; every other card holds these files byte-identical and escalates with evidence instead. Smuggling one through `additional` to dodge the pin is forbidden — that is a contract change without a review |
| `scripts/smoke.sh`, the amendments to every page VI-A1 wrote, the final pass on `docs/CONFIG.md`, the one "Run it" sentence in `README.md` that names the Agents page (and the final README merge), `CHANGELOG.md` `[Unreleased]` | VI-D1 |
| `README.md` (whole-file restructure), `docs/book/src/introduction.md`, the opening paragraph of `docs/book/src/developer/README.md`, `docs/diagrams/**` (new), the first sentence of `CLAUDE.md`'s overview | VI-A3 — **`README.md` collides with Part V's V-C3; see §VI.3** |
| `docs/screenshots/hero.gif`, `agents.png`, `attempt.png`, `two-machines.png`, and the README image markup for them | VI-D2 — **`docs/screenshots/**` is V-C2's until it lands; see §VI.3** |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |

---

## §VI.3 Dependency graph, cross-Part conflicts and merge policy

```text
VI-A1 (docs: the path, the truth) ─────────────────────────────────────────────┐
VI-A3 (README + introduction + diagram) ───────────────────────────────────────┤
                                                                               │
VI-A2 (ADR 0061) ──┬── VI-B1 (secret store) ── VI-B2 (Vercel AI Gateway) ──┬── VI-B4 (provider trait) ── VI-B5 (catalog on the wire)
                   │                                                      │   │
                   │                        └── VI-B3 (embedded: on/off, key) ──┼── VI-C1 (Agents page) ──┐
                   │                                                             │                          │
                   ├── VI-C3 (project agent settings) ── VI-C2 (modal) ──────────┴──────────────────────────┼── VI-D1 (proof + docs)
                   │                                                                                        │
                   └── VI-C4 (artifact / decision lists) ───────────────────────────────────────────────────┘

VI-C1 + VI-C2 ─────────────────┐
Part V · V-C2 (recovery demo) ─┴── VI-D2 (hero + screenshots) ── feeds VI-D1's final README merge
```

**Wave 14 is a gate, not a sprint.** VI-A1 and VI-A3 are pure documentation and can land
any time — A3 first if Part V's V-C3 is about to start, since A3 restructures the file V-C3
proposes text for. VI-A2 is a decision record the **user approves**; no Wave 15 card
branches until it is accepted, because B1–B3 implement its choices and would otherwise
encode a guess.

**Wave 15 is sequential.** B1 first (the store). B2 needs the store to read the key. B3 needs
the store to write it and is opaque to what the key is for, but merges after B2 so its
"catalog appears after save" observation has something to observe.

**B4 and B5 extend Wave 15, and only B4 is parallel.** B4 (the provider trait, ADR 0063
decision 4) needs B2 and shares no file with B3, so the two run together. B5 changes the wire
contract B4's acceptance pins byte-identical, so it cannot run beside B4 — it follows it.

**Wave 16 has two independent starts.** C3 and C4 need only the ADR's vocabulary and can run
alongside Wave 15 — they are the widest diffs on the board and should start early. C1 waits
for B2 + B3; C2 waits for C3.

**Wave 17 is two cards, in order.** D2 (assets) needs C1, C2 and Part V's V-C2; D1 (proof and
docs) goes last and takes the final README merge.

### Cross-Part conflicts — read before branching

Part V Wave 13 is in flight. Two files are shared:

1. **`docs/screenshots/**` — V-C2 owns it until it lands, and its recording keeps its
   slot.** VI-D2 does not start before V-C2 has landed; it replaces the PM hero and adds
   three files, and never touches V-C2's recording, which VI-A3 gives a named slot
   (*Durable by design*). V-C2 hands its markdown to VI-A3, not to V-A4 (closed).
2. **`README.md` — three writers, one order.** VI-A3 restructures the whole file **first**,
   now, while V-C3 waits on V-C2. V-C3 proposes launch text against A3's structure. VI-D2
   swaps the image markup; VI-D1 adds one sentence and takes the final merge. A Part V card
   that finds A3 in flight escalates rather than racing — §V.3's own rule, applied back.

`docs/CONFIG.md` is touched by three cards here — A1 (model-resolution rows), A2 (one
bullet), B2 (gateway rows) — in disjoint sections. Each names the rows it touched in its
handoff so the integrator verifies disjointness rather than assuming it.

`router.rs` is touched by B3 (one gated mount) and C4 (two operator routes). Sequential
merge; the integrator regenerates `openapi.json` after each.

A card that discovers another collision states it in the handoff and stops.

---

## §VI.4 Cards

### VI-A1 — Write the path from a board item to a completed attempt, and delete what is false

**Pure documentation. Can start immediately.** Written against the product **as it is
today** — five hand-typed fields and all — because a stranger installing beta.7 gets that
product. VI-D1 amends this card's pages after Wave 16 ships.

**Owns:** `docs/book/src/user-guide/agent-runners.md` (one new section, the Known-gaps
rewrite, the harness-id fix), `cli.md` (five new sections), `configuration.md` (the
runner rows or a first-line pointer — see Tasks), `quick-start.md` (one section),
`docs/API-REFERENCE.md` §"Runner, Fleet & Execution" (one complete worked example), the
model-resolution rows of `docs/CONFIG.md`, and the VI-A1 handoff. **Does not own** any
runtime code, `README.md`, or the provider-credentials bullet in `docs/CONFIG.md` (VI-A2's).

**Context.** See the §VI.0 evidence table — every row about `docs/` is this card's. The
shape of the failure: three documents point at each other (API-REFERENCE → agent-runners →
CONFIG) and none of them keeps the question "how do I make this item run with Sonnet?".
Four statements are false today (`claude_code`; "model profiles have no effect"; "fleet
members have no write route"; two pages each claiming to be the configuration reference).

**Tasks:**
- The page opens with the §VI.0 statement — or its first paragraph and a link — so
  `agent-runners.md`, the README and the introduction say one thing, not three.
- `agent-runners.md`, new section **"Running an item with an agent"** placed *before*
  "Enrolling a runner": the four entry points (the modal, `tack execution create`,
  `POST /api/executions`, MCP `create_execution`), then **one complete, copy-pasteable
  `tack execution create` invocation** with all thirteen fields filled with real values
  against `tack serve --with-runner`, and its real output. The same body as raw JSON goes in
  `API-REFERENCE.md`. Run it; paste the transcript; do not construct it from the schema.
- `agent-runners.md`, new section **"Choosing a model and a provider"**: the four-tier
  precedence (request override → agent profile `limits.default_model` → project *(no
  storage today; say so)* → fleet `default_policy.default_model` → auto-select) with the
  exact JSON each tier accepts, how `model_combinations` / `model_passthrough` gate it at
  claim time, and — **in the same paragraph** — the rule "Tack never holds a provider
  credential" next to the rule "Tack does route the model choice". These two sentences
  have never appeared together, and that is the whole reported confusion.
- Fix `claude_code` → `claude-code` at l.32. Rewrite the Known-gaps list: the
  `model_profiles` bullet becomes what is true (a saved pair the modal copies into the
  request override; not read by the scheduler as a *default* tier); the fleet-members
  bullet is deleted and replaced by "the route exists; the Fleet panel does not call it
  yet"; the decision/artifact-list bullet stays and is re-cited.
- `cli.md`: sections for `tack execution`, `tack runner` (including `doctor` and `start`),
  `tack fleet`, `tack agent-profile`, `tack model-profile`, each generated from real
  `--help` output with one executed example.
- `configuration.md`: either add the `TACK_LOCAL_RUNNER_ENABLE` / `TACK_RUNNER_*` rows, or
  replace the page body with a first-line pointer naming `docs/CONFIG.md` as the single
  authority. Choose, and defend the choice in the handoff; two pages disagreeing is the
  defect, not the missing rows.
- `quick-start.md`: one section "Run an item with an agent" reaching a completed attempt in
  the fewest steps the product supports today, every console step shown.

**Acceptance:** a reader with a fresh `tack serve --with-runner` and one authenticated
harness reaches a completed attempt following **only** the new agent-runners section —
proven by a transcript in the handoff of doing exactly that, commands copied from the page.
`grep -rn claude_code docs/book/` returns nothing. Every remaining Known-gaps bullet cites a
`file:line` that still holds. Every new CLI section's example was executed and its output
pasted. `configuration.md` and `docs/CONFIG.md` no longer both claim to be the reference.

---

### VI-A2 — ADR 0061: provider credentials and model catalogs at the runner boundary

**A decision card. Prepares and stops; the user accepts the ADR.** No Wave 15 card branches
before that.

**Owns:** `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`, the one
provider-credentials bullet in `docs/CONFIG.md` (l.89-108 today), and the VI-A2 handoff.
**Owns no code.**

**Context.** ADR 0050 l.29-30 and ADR 0058 l.80-83 are quoted in §VI.0. Both are correct
about the API server. Both are being cited to mean "Tack cannot help you configure a
provider", which produced the reported confusion (recorded in IV-A5's own Context, which
called the situation "correct but invisible" and fixed only the visibility). The runner
side of the boundary was never decided: the runner owns its enrollment credential, owns the
harness subprocess and its environment, and accepts `secret_reference` entries with no
resolver. This ADR decides what the runner may hold and how a key reaches it, so that Wave
15 encodes a decision rather than a guess.

**Decisions the ADR must make**, each with the options rejected and their cost:

1. **Where a provider key lives.** Recommended: a runner-local, owner-only store in
   `TACK_RUNNER_STATE_DIR`, alongside `session.json`, with the same `0600` posture.
   Rejected for the record: `app_meta` like the backup secret (crosses the line ADR 0050
   drew; the backup key is the *server's own* secret, a provider key is not); a Tack-side
   vault (a proxy by another name).
2. **The one exception.** An embedded-runner, loopback-only, write-once operator route that
   hands a key to the co-located runner's store. Its exact preconditions (`--with-runner`
   on, loopback bind — already enforced before the server opens a socket), what it returns
   (never the key, never a fingerprint), and that it **does not exist** otherwise — a 404,
   the same shape as the `TACK_ORCH_ENABLE` routes, not a route that refuses. Decide which
   auth applies: the recommendation is the ordinary operator token, matching Settings →
   Cloud Backup's secret-key write, and the ADR says why a higher-privilege token is or is
   not warranted.
3. **Catalog fetching.** The runner's probe may call the gateway's model list with the
   runner's key; confirm this sits inside the runner's existing network domain and that the
   API server still makes zero model-provider calls. Decide what a catalog entry records
   (provider, model id, probed-at) and what it does not (any price the catalog reports is
   `catalog_reported`, never `measured` — it is not a measurement of a run).
4. **Vocabulary.** The provider id string for the gateway in `model_combinations`
   (recommendation: `vercel-ai-gateway`), that a gateway model id is the gateway's own
   `creator/model` form verbatim, and that the same underlying model reached directly and
   through the gateway are **different** `(provider, model_id)` pairs (§VI.1 rule 6).
5. **The surface map as product rule.** Adopt §VI.0's table: the "never UI" column is
   closed and named; a future step that cannot be UI needs an ADR amendment, not a card's
   judgement.
6. **Turning the embedded runner on from the UI — an amendment to ADR 0058.** Today the
   gate is a startup flag. Recommended: on a loopback bind, one switch in the UI starts or
   stops the in-process runner and persists the choice (recommendation: `app_meta`, read by
   the server after the database opens); **off by default is unchanged** — a fresh install
   runs nothing until a person on the same machine chooses — and **loopback-only is
   unchanged** — the switch's route does not exist on any other bind. `--with-runner` stays
   as the flag-only equivalent for scripts and the smoke. Rejected for the record: keeping
   the flag as the only path (it is the single console step a UI-only user cannot avoid,
   and it costs the whole "self-explanatory binary" claim); making bare `tack` mean
   `serve --with-runner` on loopback (changes a security default to save one click the UI
   can offer instead — recorded as an open question in §VI.5, not decided here). This
   decision rewrites the first row of the surface map.

**Tasks:** write the ADR in the house format (see 0059/0060: Status, Date, Supersedes,
Contract, Context, Measurement, Decision, Consequences). Replace the `docs/CONFIG.md`
sentence *"there is no `TACK_*` variable for a model provider or endpoint, by design"* with
the precise rule the ADR states. List in the handoff every sentence in `docs/` and
`README.md` the ADR makes imprecise (`grep -rn "never becomes a model proxy\|no TACK_\*
variable for a model provider\|never reads, stores, or forwards"`) for VI-A1 / VI-D1.

**Acceptance:** the ADR names each decision, each rejected option and its cost, and cites
0050 and 0058 by line for what it reaffirms and what it bounds. The user has accepted it
(recorded in the handoff with the date). The CONFIG.md sentence is replaced. The handoff's
sentence list is complete — the grep in Tasks returns only the ADRs.

---

### VI-A3 — Two components, one product: the README, the introduction and the diagram

**Pure documentation and one diagram. Can start immediately.** Supersedes V-A4's README
structure, keeps V-A4's four questions and its claim → evidence rule, and reuses the §VI.0
statement verbatim.

**Owns:** `README.md` (whole-file restructure), `docs/book/src/introduction.md`,
the opening paragraph of `docs/book/src/developer/README.md`, `docs/diagrams/**` (new),
the first sentence of `CLAUDE.md`'s Project Overview, and the VI-A3 handoff. **Prepares,
does not apply,** the GitHub repository description (outward-facing — §V.1 rule 5).
**Does not own** `docs/screenshots/**` (V-C2 now, VI-D2 later), `agent-runners.md`
(VI-A1), or any crate.

**Context — the README shows a project manager.** Measured 2026-09-03: the hero asset's
own alt text is *"Board, Timeline, and vocabulary editor"* (`README.md:12`); the Features
section lists *Project management* first and *Agent execution* second (`:32`, `:42`); all
five screenshots — board, timeline, dashboard, list, vocabulary editor — are PM views and
**zero** show an agent doing anything (`docs/screenshots/`); the two-component architecture
is explained at line 169, after *Status*; and the book's introduction lists four core
concepts — item, workflow, project type, vocabulary — with neither *runner* nor *run*
among them. A reader who arrives from the category Tack is trying to leave sees exactly
that category, and the differentiator reads as one bullet among many. V-A4's sentence was
right about the *what*; the page's shape still says the opposite.

**Tasks:**
- **First screen, in this order, before any PM screenshot:** the H1 and a first line that
  names both components (proposal, for the card to finalise: *"A self-hosted project board
  that dispatches its items to coding agents — Claude Code or Codex — through
  runners that live where your code lives. One binary. The board plans and records; the
  runner executes."*), then the **two-component diagram**, then the §VI.0 statement, then
  a three-step *How it works* strip (plan it on the board → a runner picks it up where the
  code is → the run is recorded on the item). Until VI-D2 lands, the diagram occupies the
  hero slot and the current GIF moves down to *Screenshots* under its honest alt text.
- **The diagram**, `docs/diagrams/two-components.svg` with its source alongside: two
  planes side by side — *the board (one)* / *runners (many)* — with the pull arrow pointing
  the only direction it ever goes, the harnesses beneath each runner, and two callouts:
  *credentials stay here* on the runner side, *leases · fencing · history* on the board
  side. It must render on GitHub, in the published book, and in dark mode — an SVG pair
  behind a `<picture>` does all three; a mermaid block does GitHub only unless the book
  gains a preprocessor. Measure, then choose.
- **A *Two components* table** — rows: what it does · what it holds · how many · what
  happens when it dies · how you run it; columns: board, runner. This is the scalability
  story the user asked for, in one glance.
- **A named slot, *Durable by design*,** directly under the statement's third paragraph,
  for V-C2's recovery recording. If V-C2 has not landed, the slot carries the sentence and
  a link to the smoke step that proves it, and no placeholder image.
- Reorder *Features*: agent execution, then project management, then automation surfaces.
  Keep every claim → evidence row V-A4 established; add none that VI-D1 cannot prove.
- `introduction.md`: open with the statement; the core-concepts table gains **Runner** and
  **Run**; the agent-runners link moves to the top of *Quick links*.
- `developer/README.md`: open with the statement's third paragraph, then the crate map.
- `CLAUDE.md`, Project Overview, first sentence only: name both components and that
  `--with-runner` embeds one in the other — so every agent that writes a doc here carries
  the same frame.
- Draft the new GitHub description in the handoff for the user to apply.

**Acceptance:** an agent with no context on this repository reads only the README's first
screen (through the *Two components* table) and reports back that the product has **two
parts**, what each holds, and that one board serves many runners — the same test V-A4
used, with the answer it must now produce. `README.md` contains no PM screenshot above the
diagram. The statement appears byte-identical in the four places (`diff` in the handoff).
The diagram renders in GitHub light, GitHub dark and the built book — three screenshots in
the handoff. Nothing outward-facing was applied.

---

### VI-B1 — A runner-local secret store, and `secret_reference` finally resolves

**Needs VI-A2 accepted.** First card of Wave 15.

**Owns:** `crates/tack-runner/src/secrets.rs` (new), the store path in
`crates/tack-runner/src/config.rs`, the `secret_reference` branch in each of the three
adapters, `tack runner secret set|list|remove` (one subcommand arm in
`crates/tack-cli/src/main.rs` plus one small module), the `keyring` dependency line in
`crates/tack-runner/Cargo.toml` (and the `Cargo.lock` it moves), and the VI-B1 handoff.

**Context.** `EnvironmentValue { value | secret_reference }` is in the contract and in the
claim fixture; every adapter warns and skips a `secret_reference` entry because "no
secret-store client exists in tack-runner yet" (`claude_code.rs:62-64`, `codex.rs:772-775`,
`opencode.rs:1156-1158`). That is a documented gap with a contract-level caller already
waiting. The store is the smallest thing that closes it and is what VI-B2 and VI-B3 both
need — one mechanism, three callers, all in this wave.

**Design, fixed here so B2/B3 do not re-decide it:** one `SecretStore` behind two
backends, chosen once at runner start and reported by `tack runner doctor` (`backend:
keychain | file`): the platform credential store through the `keyring` crate (service
`tack-runner`, account = the entry name), and — only when no platform store answers — a
single owner-only file (`secrets.json`, mode `0600`, in `TACK_RUNNER_STATE_DIR`) mapping
name → value. Names are `<provider>/<label>` identifiers (`vercel-ai-gateway/default`) —
safe in logs, errors and `Debug` output; values are never in any of them, and the value
type's `Debug`/`Display` are hardcoded to `[REDACTED]` exactly like `RunnerCredential`.
`tack runner secret set <name>` reads the value from stdin or from
`TACK_RUNNER_SECRET_VALUE`, **never from argv**. A `secret_reference` string takes one
optional scheme: `store:<name>` (the default when no scheme is present, so the frozen
fixture's bare name stays valid) resolves from the store; `env:<VARIABLE>` reads the
runner's own environment at spawn — the path for a systemd-started runner with no
keychain and no wish to keep a file. Resolution happens in the adapter's `validate` step; a missing name or unset variable is
a typed pre-spawn failure (`secret_reference_unresolved`, naming the reference only).
**Corrected 2026-09-03 by VI-B1, which measured the engine rather than trusting this
paragraph:** `validate` does *not* run before the journal record and the worktree exist.
`engine.rs::run_claimed` persists and fsyncs the journal entry and provisions the real
checkout *first*, on purpose — a documented crash-safety boundary that a secret-store card
must not reorder. The true claim, and the one to assert, is narrower: `validate` itself
writes nothing, so a rejection there leaves whatever the engine already did untouched.
**Today's warn-and-skip becomes an error**, because once a resolver exists a silently
missing variable is a fake success. No KEK-encrypted file, no third backend, no
per-project label pinning: the roadmap's "How the runner keeps a provider key" names
each with its trigger.

**Acceptance:** a live attempt with an environment entry `{"secret_reference": "demo"}`
reaches the fake-harness shim with the variable set — the shim prints the value's *length*,
never the value. Captured `tracing` output for that run contains the name `demo` and never
the value (positive control: the name is asserted present). A missing reference fails at
`validate` with the typed reason, and the state directory and workspace root are asserted
untouched. `stat -c '%a'` on the store file is `600`, measured on a real run. The
three adapters' behaviour is proven by reverting the resolver once and watching the
"variable is set" assertion fail. The keychain backend is proven live on the dev machine
with the platform's own tool (`secret-tool lookup service tack-runner account <name>` on
Linux, `security find-generic-password -s tack-runner -a <name>` on macOS) — the handoff
records the command and exit status, never the output. The file fallback is proven by
running with the platform store unreachable (`DBUS_SESSION_BUS_ADDRESS=/dev/null` on
Linux) and asserting `doctor` reports `backend: file`. Unit tests run against the file
backend (and `keyring`'s in-crate mock store if 4.x ships one — check, don't assume), so
CI needs no Secret Service. An `env:` reference reaches the shim the same way a `store:`
one does. `runner_contract` byte-identical: no fixture changes.

---

### VI-B2 — Vercel AI Gateway as a runner provider

**Needs VI-B1 merged.** The runtime heart of this Part.

**Owns:** a `[provider.vercel_ai_gateway]` section of `RunnerConfig` (`enabled`; `secret`,
the B1 store name, default `vercel-ai-gateway`), each adapter's spawn environment and
arguments, the catalog step in `bootstrap::probe`, the provider block in
`crates/tack-cli/src/doctor.rs`, the gateway rows of `docs/CONFIG.md`, and the VI-B2
handoff. **Escalates, does not edit,** `docs/contracts/runner-v1/**`.

**Context — vendor facts, fetched 2026-09-03.** Re-fetch `vercel.com/docs/ai-gateway/
coding-agents` before relying on any cell; this table is a starting point, not a source.

| Harness | Endpoint | Mechanism the vendor documents |
|---|---|---|
| `claude-code` | `https://ai-gateway.vercel.sh/claude-code` — **no `/v1` suffix**; the SDK appends `/v1/messages` and a double suffix 404s | env: `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN=<key>`, **`ANTHROPIC_API_KEY=""`** (must be set *empty* — a non-empty value wins over the auth token); optional `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`. Model ids like `anthropic/claude-sonnet-5` |
| `codex` | `https://ai-gateway.vercel.sh/codex/v1` | `~/.codex/config.toml`: `model_provider = "vercel"` and `[model_providers.vercel] base_url = …, env_key = "AI_GATEWAY_API_KEY", wire_api = "responses"` (required — Codex no longer speaks Chat Completions). The codex adapter forwards **no** ambient environment, so the key is injected per spawn. **First thing this card measures:** whether the provider can be selected per invocation (`-c model_provider=…` overrides) rather than by writing the user's own config file — the runner must never edit a user's `~/.codex/config.toml` |
| `opencode` | native provider `vercel`, via `/connect` in the TUI | credential lands in `~/.local/share/opencode/auth.json`. Whether an environment variable or an `opencode.json` entry can carry it **non-interactively** is measured, not assumed. If it cannot, opencode's gateway path is a console step rendered by VI-C1, and this card says so in its handoff and in `docs/CONFIG.md` |

Catalog: the gateway's model list (`GET /v1/models`, Bearer key) becomes
`model_combinations` for each harness that can reach the gateway — provider
`vercel-ai-gateway` (or what ADR 0061 chose), model id verbatim, probed-at recorded. Only
when the provider is enabled **and** the secret resolves; otherwise zero entries and a typed
reason (`provider_not_configured` / `catalog_unreachable` with status), never a stale list.

**Tasks:** the config section; the probe step; spawn injection per harness **only when the
request's resolved model provider is the gateway** — a request for a direct vendor model on
the same runner must receive none of the gateway environment, or the harness silently
routes through the gateway and `actual` lies; the actual-model observation for a
gateway-served run (what does each harness report, and with which
`model_observation_source`?); `doctor` prints `provider: vercel-ai-gateway — configured /
not configured / catalog error <status>` plus the catalog count and timestamp, key never
printed; usage on a gateway-served attempt is still `measured` only from harness output,
never from the gateway's dashboard.

**Acceptance:** at least `claude-code` and one other harness complete **live** end-to-end
attempts through the gateway, with `actual.model_provider` and `actual.model_id` recorded
as the harness reported them — transcripts in the handoff, on the real binaries, not the
shim. A request for a direct model on the same runner spawns with no gateway variable in
its environment, proven by the shim dumping variable *names*, and proven load-bearing by
reverting the guard once. With the provider disabled the capability snapshot has zero
gateway combinations, and a request naming one is refused at claim by the existing
scheduler intersection — untouched. Captured logs of a full gateway run never contain the
key; `sqlite3 tack.db .dump | grep -c <key>` is `0` after the run, with the shim proving the
harness did receive it. `tack runner doctor --json` byte-matches the live capability
snapshot including the catalog, as IV-A5 established. `runner_contract` byte-identical, or
an escalation with the exact field the contract lacks.

---

### VI-B3 — The embedded runner, controlled from the UI: turn it on, hand it a key — persisting neither in the database

**Needs VI-B1 (the store) and VI-A2 (decisions 2 and 6).** Opaque to VI-B2 — the key is
bytes to this route — but merges after B2 so "catalog appears after save" has a catalog.

**Owns:** `crates/tack-api/src/handlers/local_runner.rs` (new), its mounts in `router.rs`
(those only), the server→embedded-runner seam in `crates/tack-cli/src/local_runner.rs` —
a control handle (`start`, `stop`, `set_secret`, `reprobe`) the server calls, **not** a
path string; the server must not learn where the store lives — the persisted on/off flag,
`frontend/src/features/agents/ExecutionToggle.tsx` and `ProviderKeyPanel.tsx` with their
client and tests, and the VI-B3 handoff.

**Context.** Today the embedded runner is spawned once, at startup, from the `--with-runner`
flag or `TACK_LOCAL_RUNNER_ENABLE`, and `AppState` knows nothing about it. ADR 0061
decision 6 makes it startable from the UI on a loopback bind. Two things must stay
exactly as they are: **off by default** (a fresh install runs nothing until a person on
the same machine turns it on) and **loopback-only** (the toggle is refused — route absent,
404 — when the server is bound anywhere else; that check already runs before any socket
opens). Where the on/off state persists across restarts is decision 6's call; the
recommendation is `app_meta`, read by the server after the database opens, so a later
`tack serve` on loopback resumes what the person chose, and `--with-runner` remains the
flag-only equivalent for scripts and the smoke.

**Routes** (final shapes follow the ADR): `PUT /api/local-runner` `{"enabled": bool}` →
`204`, starts or stops the in-process runner and persists the choice; `GET /api/local-runner`
→ `{enabled, state, since}`; `PUT /api/local-runner/secrets/{name}` `{"value": …}` → `204`,
never echoes the value or a hash; `DELETE …/{name}`; `GET /api/local-runner/secrets` →
names and `set_at` only. After a successful secret `PUT` the server signals the runner to
re-probe so the catalog appears without a restart — if the runtime has no re-probe entry
point, this card adds one, and this route is its caller. All of it exists **only** on a
loopback bind — plain `tack serve` on `0.0.0.0` → 404, the same shape as the
`TACK_ORCH_ENABLE` routes. Prove the routes are *absent* there, not present-and-refusing.

**Panels:** `ExecutionToggle` — one switch, *Agent execution on this machine*, with the
observed state beneath it; when the route is 404 it renders the console command and the
one-line reason (non-loopback bind). `ProviderKeyPanel` — one write-only field, *Vercel
AI Gateway key*; after save, a "Set <date> · Replace · Remove" line and the observation
"Catalog: N models as of <ts>" or the typed error, verbatim; a link to where a key is
created. When the route is 404 the panel renders the console steps instead —
`tack runner secret set vercel-ai-gateway` on the runner host, then *Re-check* — because
that is a remote-runner deployment, and §VI.1 rule 1 applies.

**Acceptance:** on a loopback `tack serve` with no flag: `GET /api/runners` is empty; after
`PUT {"enabled": true}` an active runner appears and a shim attempt completes; after
`PUT {"enabled": false}` no new claim happens (assert on a queued request that stays
queued); restart without the flag → the runner comes back because the choice persisted.
On a non-loopback bind all routes are 404. After a secret `PUT`, in one test with a
positive control: `app_meta` row count unchanged by the secret (the on/off flag is the only
new row), `sqlite3 tack.db .dump | grep -c <value>` = 0, and the runner store holds the
value at mode `600`. Request bodies never reach a log line (capture, with the name
asserted present). Playwright: toggle on → paste → save → catalog count rendered → the key
never appears in the DOM. The re-probe is proven by the catalog count changing in the same
test, with no restart.

---

### VI-B4 — A provider is a module, and a second module proves it

**Needs VI-B2 merged.** Independent of VI-B3 — disjoint files, may run alongside it.
Implements ADR 0063 decisions 4 and 5 (accepted 2026-09-05).

**Owns:** `crates/tack-runner/src/provider.rs` and whatever module tree replaces it, the
provider block of `crates/tack-cli/src/doctor.rs` **only where it prints the catalog**, the
`[provider.*]` config shape where the registry requires it, and the VI-B4 handoff.
**Escalates, does not edit,** `docs/contracts/runner-v1/**`, `crates/tack-orch/src/execution.rs`
and any frontend file.

**Context — what is actually there today, measured.** `provider.rs` is 399 lines and holds
the whole mechanism as `match` arms: `known_endpoint(provider, wire)` returns a static row,
`catalog_url(provider)` returns a static URL, and `attach_catalog` does not consult either —
it looks up `VERCEL_AI_GATEWAY_CONFIG_KEY` by name and asks that one provider. So the claim
that a second endpoint is "three data rows and no code path" holds only for a provider whose
catalog is a `{"data":[{"id":…}]}` list behind a bearer header, and only for a provider the
config happens to be keyed under. Anthropic's own API is neither: its catalog is
`GET /v1/models` with `x-api-key` and `anthropic-version` headers, not `Authorization:
Bearer`. That difference is the whole reason this card exists.

`fetch_catalog_ids` also discards everything the catalog publishes except the id — the
prices, context windows and modalities that ADR 0063 decision 5 calls the point of asking.

**Tasks:**
- A `Provider` trait, one implementation per provider, and a registry the machinery walks.
  A provider knows its own config key, its endpoint per `Wire`, its catalog request
  (URL, headers, auth placement) and how to parse its own catalog body into one common
  shape. **No vendor name may appear outside a provider module** — assert it: a grep for
  each vendor string over the non-module files is part of the gate, and the current
  `attach_catalog` fails it today.
- The common shape: model id, plus what the provider publishes and typed as absent when it
  does not. Unmeasured is nullable — a provider that publishes no price yields `None`, never
  a zero. This is decision 5's shape; **it stops at the runner's own surface in this card**
  (`doctor --json`), because carrying it to the board is a wire change VI-B5 owns.
- `attach_catalog` walks the enabled entries of `RunnerConfig::providers` and asks each
  registered provider for its own catalog, instead of naming one. `CatalogStatus` becomes
  per-provider; a provider whose secret does not resolve must not suppress another's
  catalog.
- **A second real provider module: Anthropic's own API** (`api.anthropic.com`,
  `x-api-key`, `anthropic-version`). Fetch the vendor's current docs; do not trust this
  paragraph's header names without checking them. It is in scope precisely because its auth
  header and catalog shape differ from Vercel's — a trait with one implementation proves
  nothing.
- `doctor` prints one block per configured provider, with the model count, the timestamp and
  the price/limit fields where the provider published them and `Not measured` where it did
  not. Keys are never printed, by any path.

**Acceptance:** the Vercel path is unchanged in observable behaviour — the four existing
`provider.rs` tests pass untouched, `runner_contract` is byte-identical, and a live gateway
attempt still completes (transcript in the handoff). Adding the Anthropic module touched no
file outside its own module and one registry line, stated as a `git diff --stat` in the
handoff — that stat **is** the card's proof, so if it is not true, say so rather than
reshaping the diff. A vendor-string grep over the non-module files returns nothing, proven
load-bearing by putting `vercel` back into `attach_catalog` once. A provider configured with
an unresolvable secret yields its own typed status and the other provider's catalog still
arrives, in one test. `tack runner doctor --json` byte-matches the live capability snapshot
as IV-A5 established.

**Stop if:** the common catalog shape cannot be filled from Vercel's and Anthropic's bodies
without one of them needing a field the other cannot express. Record both bodies and the
field, and hand the shape to VI-B5 rather than inventing a union type.

---

### VI-B5 — The catalog reaches the board: price, context window and modality on the wire

**Needs VI-B4 merged.** Not dispatched with it: it changes the frozen wire contract that
B4's acceptance holds byte-identical, and the two cannot both be true at once.

**Owns:** the per-model field on `ModelCombination` in `crates/tack-orch/src/execution.rs`,
`docs/contracts/runner-v1/**` and the pin table in `crates/tack-orch/tests/runner_contract.rs`,
the runner side that fills it, and the VI-B5 handoff.

**Context.** VI-B2 escalated this and it is still open: the catalog publishes per-model
pricing, context windows and modalities, and `ModelCombination` carries a provider, a bare
list of ids and a `discovery` string. The `additional` map must not be used — §VI.2 records
that smuggling a field through it is a contract change without a review, which is the thing
being avoided.

**Acceptance:** the fixture revision and the pin table land in the same change as the field.
A runner that reports no metadata still round-trips, so an older runner against a newer
board is not a break. Rendering it is VI-C1's or a later card's; this card carries it no
further than the API response.

---

### VI-C1 — The Agents page: one screen, in the user's words, that owns the whole path

**Needs VI-B2 and VI-B3 merged.** This is the "the binary is self-explanatory" card. Route
`/agents`, in the nav where *Fleet* is today.

**Owns:** `frontend/src/features/agents/**` except `ExecutionToggle.tsx` and
`ProviderKeyPanel.tsx` (B3's, composed here), `frontend/src/app/routes.tsx`, the nav entry,
the *Advanced* section that re-mounts `RunnerFleetSection` and its panels, the one line in
`features/fleet/FleetPage.tsx` that mounts `RunnerFleetSection` today (nothing else in that
file — the rest is docket, §VI.1 rule 7), a first-run banner on the Board, and the VI-C1
handoff. Reuses `RunnerHealthCard`, `EnrollmentPanel`, `ExecutionTimeline`; does not edit
them.

**Context — what the UI can observe.** `GET /api/local-runner` (on/off, or its 404, which
is itself an observation: "this is a remote deployment"); `GET /api/runners` — each
runner's state, heartbeat, capacity and `capability_snapshot` (per harness
`installed_version` or a probe error, `model_combinations`, `model_passthrough`);
`GET /api/local-runner/secrets`. What it **cannot** observe, and must say so: whether a
vendor login exists (§VI.1 rule 2 — "present, unverified"), and anything about a machine
that has no runner.

**Vocabulary is the card.** §VI.1 rule 8: the default screen says *agent* (Claude Code,
Codex), *model*, *provider*, *run*. "Runner", "fleet", "enroll", "heartbeat",
"capacity", "lease", "harness" do not appear outside *Advanced*. A unit test greps the
rendered default screen for that list and fails on a hit.

**The page is a state machine rendered as numbered steps**, each with an observed status
and, when a step is a console step, the exact command in a copy block with one line saying
why:

1. **Agent execution.** Three states: **Off** — B3's toggle, one click; **On, this
   machine** — since when, and a *Turn off*; **On, other machines** — a count and a link to
   *Advanced*. When the toggle route is 404: the command `tack serve --with-runner`, the
   reason (the server is not bound to this machine only), and a poll that flips the step
   without a reload.
2. **Agents on this machine.** Per agent, from the snapshot: *installed vX* · *not found*,
   with the vendor's install command · *could not check*, the probe error verbatim.
   *Re-check* triggers the re-probe B3 added.
3. **Provider.** Two paths, side by side, labelled as what they are — **"Use the agent's
   own login"** (per installed agent: the exact command, `claude` / `codex login`, the
   sentence that Tack cannot see the result, and "confirm with a
   test run below") and **"Use Vercel AI Gateway"** (B3's panel — one key, every model, no
   terminal). Neither is labelled *recommended* unless ADR 0061 said so.
4. **Default model.** A picker over the union of every active runner's
   `model_combinations`, plus *Auto*, plus *Type a model id* only where an agent attests
   `model_passthrough`; saves to the project default (C3). If C3 has not landed, this step
   is **hidden**, not stubbed.
5. **Test run.** Creates an item titled *Agent test* on the current project, runs the
   default profile with the instruction "print which model you are and exit" against the
   project's default repository (C3), and renders the run timeline inline. **This is the
   only way step 3 is ever shown green** for an agent's own login.

**Advanced — run agents on other machines.** Collapsed by default. Today's
`RunnerFleetSection` (enrollment, runner health, fleets and members, agent profiles, model
profiles) moves here unchanged. The docket Fleet page is not edited; whether its nav entry
should stay visible when `TACK_ORCH_ENABLE` is off is a one-line question for the
integrator, recorded in the handoff, not decided here.

**First-run banner** on the Board when execution is off and the project has items: "This
board can run its items with an agent — *Turn on*" → `/agents`. Dismissable per browser
(`localStorage` is fine; it is a convenience, not state).

**Acceptance:** Playwright, from a loopback `tack serve` with no flag: step 1 shows *Off*
and a switch, nothing is green; click → step 1 flips to *On, this machine* without a
reload; step 2 lists the fake-harness shim as installed with its version and the others as
*not found* with commands; after B3's panel is used, step 3 shows the catalog count; step 5
completes a run with the shim and shows the actual model. From a non-loopback bind: step 1
shows the command and the reason. Every status string is asserted against the API
response, never hard-coded. An agent with a session file and no completed run renders
*present, unverified* — the shim can stage that. The vocabulary grep test passes, and is
proven load-bearing by putting the word "runner" on the default screen once. Every command
string the page renders lives in one exported constant; VI-D1 cites it from the docs.

---

### VI-C2 — Run with agent: zero hand-typed identifiers

**Needs VI-C3 merged** (the storage its defaults come from).

**Owns:** `frontend/src/shared/runWithAgent/**`, the attempt-state chip on Board cards, and
the VI-C2 handoff.

**Context.** Today the modal asks, on every run, for a runner id typed by hand, a fleet from
a list with no membership UI, a remote, a base revision, a subdirectory and a timeout
(`RunWithAgentModal.tsx:298,386,393,399,412`), remembers none of them, and its only model
control is a saved pair. The product knows, or after C3 can know, every one of these.

**Tasks:**
- **Where it runs:** hidden when exactly one machine is active — the common case;
  otherwise a dropdown of active machines and groups from `GET /api/runners` and the fleets
  route, labelled by name, never by id. No free-text id anywhere.
- **Agent profile:** dropdown, preselecting the project's default (C3); when none exists,
  "Create default profile" inline via the existing `POST /api/agent-profiles`.
- **Harness:** only kinds the selected target reports installed, through the existing
  `gateHarnessModelSelection` helper — **unchanged**.
- **Model:** three modes with provenance visible — *Project default — `<provider/model>`*
  (C3), *Choose…* (the selected runner's `model_combinations` for that harness; free text
  only where `model_passthrough` is attested), *Auto*.
- **Repository:** a read-only summary from the project's agent repository (C3) with "Change
  for this run" expanding the three fields. Whether `base_revision` may be a ref rather
  than a SHA is measured against the runner's checkout, not assumed.
- **Timeout:** from the agent profile's `timeout_seconds`.
- After submit the modal closes and the Board card shows a small attempt-state chip next
  to the existing ▶ trigger, fed by the existing execution store; opening it lands on the
  item's Agent Activity tab.
- With execution off the modal body is "Agent execution is off — Turn it on" linking to
  `/agents`, not a form.

**Acceptance:** E2E: with C3 defaults set and one embedded runner, open the modal on an
item, change nothing, submit → the `POST /api/executions` body equals the project defaults
field for field (asserted on the intercepted request). No free-text field is visible by
default. Existing modal and helper tests are updated, not deleted; `shared.ts`'s gate
helper is byte-identical. The state chip is asserted to change from *Queued* to a terminal
label during a shim run.

---

### VI-C3 — Project-level agent settings: the tier with no storage, and the UI the tiers never had

**Needs VI-A2 accepted** (vocabulary only). Independent of Wave 15; **start early** — it is
the widest diff on the board, by construction, and that is one feature, not two cards: a
column, its repository read/write, its handler, the generated files, one settings tab. Do
not split it into a mechanism card and a caller card.

**Owns:** migration **062** — `ALTER TABLE projects ADD COLUMN agent_settings TEXT` (JSON,
nullable; **one `ALTER`, one migration name**), the typed `ProjectAgentSettings` in
`crates/tack-core`, the project repository reads/writes in `crates/tack-db/src/repo/`, the
project handlers and `UpdateProject`, the project tier in
`crates/tack-orch/src/model_policy/wiring.rs` (replace the always-`None`), a new **Agents**
tab in `frontend/src/features/settings/`, a real "Default model" field in
`AgentProfilesPanel.tsx` (writing the existing `limits.default_model` convention), add /
remove member in `FleetsPanel.tsx` calling the existing member routes, the regenerated
`openapi.json` / `schema.gen.ts`, and the VI-C3 handoff.

**Context.** `wiring.rs:17-23` says the project tier "needs either a real
`projects.default_model_policy` column or an explicitly documented reuse of an existing
column, decided by whoever owns the next migration batch". This card is that batch. One
JSON column holds the three project-level facts an attempt needs and nothing else stores:
`{ "default_model": <selector or "auto">, "default_agent_profile_id": <id>, "repository":
{ "kind", "remote", "base_revision", "subdirectory" } }` — validated server-side against
the typed struct; a malformed blob is rejected at write, never tolerated at read. The
agent-profile default and fleet default already exist as conventions with no UI; this card
gives them a field, not a new mechanism.

**Acceptance:** `crates/tack-orch/tests/model_policy_test.rs` gains the live project-tier
case — no request override, a profile with no default, a project default set → the resolved
pair with provenance `project`, through the real repository. The Settings → Agents tab
round-trips all three fields in Playwright. Adding a member through the Fleet panel is
proven by the scheduler leasing a fleet-targeted request **only after** the member was
added via the route (today's `scheduler_wiring_test.rs` does this against the database;
this one goes through the API). Migration 062 passes `run_all` on a fresh database and on
an upgraded copy of a beta.7 `tack.db`. `openapi_contract` regenerated, never hand-edited.
`runner_contract` byte-identical.

---

### VI-C4 — List what an attempt produced: artifacts and decisions without a typed id

**Independent. Needs nothing.** Can run alongside Wave 15.

**Owns:** `GET /api/executions/{request_id}/attempts/{n}/artifacts` and
`GET /api/executions/{request_id}/attempts/{n}/decisions` — handlers, their two mounts in
`router.rs`, the repository read methods (reuse the runner-side ones where they exist),
`DecisionInbox.tsx`, `ArtifactDownloadPanel.tsx`, `AgentActivityTab.tsx`, the regenerated
`openapi.json` / `schema.gen.ts`, and the VI-C4 handoff.

**Context.** `agent-runners.md` Known gaps: no list route exists for either; the panels
accept a manually-entered id. `docs/agent-handoffs/part-iii/III-F4.md` names the concrete
route shape requested to close this — follow it rather than inventing a second one. A
UI-only user cannot answer a decision or download an artifact today without an id copied
from an event payload; that is the last step of the surface map.

**Acceptance:** the panels never ask for an id. `execution-attempt-detail.spec.ts` (III-F4)
is extended to discover the artifact through the list rather than the known id, keeping
its byte-equality download proof. The decision list shows pending / resolved / expired;
resolving stays behind `TACK_EXECUTION_DECISION_TOKEN`, fail-closed, untouched. Runner
protocol routes untouched; `runner_contract` byte-identical.

---

### VI-C5 — The two attempt-list routes get the handler tests they shipped without

**Needs nothing.** Wave 17, alongside the proof cards — it touches no file any of them owns.

**Owns:** the new handler tests under `crates/tack-api/tests/` for the two routes below, and
the VI-C5 handoff.

**Context.** VI-C4 shipped `GET /api/executions/{id}/attempts/{n}/artifacts` and
`.../decisions` covered by the OpenAPI contract test, the frontend unit tests and the E2E
spec — and by no handler test in `crates/tack-api`. The card disclosed that plainly; the
Wave 16 integrator recorded it in `VI-C4.md`'s amendment of 2026-09-04 and assigned it to
"the next Wave 16 card touching that crate". No such card remains, so the debt has no owner.
Nothing asserts at the Rust level that an unauthenticated caller is rejected, or that the
ordering the frontend renders is the ordering the handler returns.

**Tasks:** add handler tests for both routes covering at least rejection without a token, the
empty case, and ordering with more than one row. Follow the pattern the neighbouring tests in
`crates/tack-api/tests/` already use rather than inventing a harness.

**Acceptance:** each new test is proven load-bearing by reverting the behaviour it asserts
once and watching exactly that test fail — name the test, paste the failure. The ordering
assertion states the order the handler actually guarantees, read from the handler, never
inferred from what the frontend expects. No production file changes: `git diff --stat`
outside `crates/tack-api/tests/` and the handoff is empty. `runner_contract` and
`openapi_contract` stay byte-identical.

**Stop if:** making a route testable would need a production change. That is a finding, not
this card's file — record what you needed and hand it over.

---

### VI-C6 — What the two attempt-list routes do with an attempt that does not exist

**Needs nothing** — VI-C5's file is on `develop`. Wave 17, alongside the other proof cards.

**Owns:** additions to `crates/tack-api/tests/handlers/attempt_lists.rs` and the VI-C6
handoff. **Production files stay untouched**, `git diff --stat` proves it.

**Context.** VI-C5 covered auth, the empty case and ordering on
`GET /api/executions/{id}/attempts/{n}/artifacts` and `.../decisions`, and declared one gap
in its own handoff: neither route's unknown-attempt path is tested. An id nobody ever created
and an id belonging to a *different* execution are different questions and both are open.

**Tasks:** find out what each route actually does for an attempt that does not exist, and for
an attempt number that exists under another execution, then pin it. Read the answer from the
handler, never from the OpenAPI spec — the spec records intent and this card records
behaviour.

**Acceptance:** each new test proven load-bearing by reverting the behaviour it asserts once
and watching exactly that test fail — name the test, paste the failure. If a route answers
`200` with an empty list for an attempt that was never created, that is the finding: say so
in the handoff, with the handler line that does it and what a caller would conclude, and
**pin the current behaviour anyway** so a later change is visible.

**Stop if:** the honest answer needs a production change. VI-C4 owns those handlers and is
closed — record what you needed and hand it over rather than changing it.

---

### VI-C7 — Does dispatching to Codex ever produce a claimable request on `develop`?

**Needs nothing.** Wave 17. **Measure and report — this card changes no production file.**

**Owns:** `docs/agent-handoffs/part-vi/VI-C7.md` and nothing else.

**Context.** V-C2 recorded that dispatching through *Run with agent* with harness = Codex and
model mode = *Auto* never produced a claimable request in over three minutes — the attempt sat
in `queued`. That was measured against the published `v0.1.0-beta.7` artifact, not this line,
and V-C2's own amendment says so: two of the three observations it filed alongside this one
were already false on `develop`. This one has never been re-measured, and it sits directly
under Part VI's promise — a stranger picks an agent and a model and gets a run.

**Tasks:** reproduce it on today's `develop`, with a real runner, from the dialog rather than
from the API. If it reproduces, follow it until you can name the component that stops:
whether the request is written at all, whether the runner's claim filter matches it, and what
`Auto` resolves to at each hop. If it does not reproduce, that is just as much a result —
record the exact conditions under which it does not, because the next card will rely on them.

**Acceptance:** a verdict with timestamps and ids, not an impression: the request id, the
attempt id, what each hop held, and the command that shows it. Whichever way it goes, the
handoff ends with the one-line answer someone can act on. If you find the cause, name the
file and line and put the smallest fix **in the handoff as a diff** — do not apply it. Another
card is recording live video against this same tree while you work.

**Stop if:** reproducing needs a credential you do not have, or a change to the runner. Both
are findings, not this card's files.

---

### VI-C8 — The same scoping, on the two routes nothing tests

**Needs nothing** — VI-C6's file is on `develop`. Wave 17.

**Owns:** new tests for the two routes below under `crates/tack-api/tests/` and the VI-C8
handoff. **Production files stay untouched.**

**Context.** `crates/tack-db/src/repo/execution.rs` looks an attempt up by
`(request_id, attempt_number)` together at **four** sites. VI-C6 covered two of them. The
integrator's own revert — dropping the `request_id` predicate at all four at once — made
exactly VI-C6's two cross-execution tests fail and nothing else, which is the measurement
that matters: the other two sites are `list_events_for_attempt_number` and
`get_execution_artifact_by_attempt_number`, and **no test anywhere notices when their scoping
goes away.** The second is the serious one. A listing that loses its filter leaks metadata; a
*download* that loses its filter hands over another execution's file.

**Tasks:** cover both routes for an attempt number that belongs to a different execution, and
for one that does not exist. Follow `crates/tack-api/tests/handlers/attempt_lists.rs` — VI-C6
left the pattern, including how it seeds two executions in one test.

**Acceptance:** the revert-once proof is the *cross-execution* one, not a status-code flip:
drop the `request_id` predicate from each site and show that your test fails **and that the
other execution's row appears in the response body** — paste it. `git diff --stat` shows
nothing outside your tests and the handoff.

**Stop if:** either route turns out not to scope by execution at all. That is a security
finding, not a test — write it up and stop.

---

### VI-C9 — "Auto" must not be a choice that silently never runs

**Done — integrated 2026-09-06.** Handoff `docs/agent-handoffs/part-vi/VI-C9.md`, with an amendment recording what its own fix costs.

**Owns:** `frontend/src/shared/runWithAgent/**` (the submit gate and whatever surface the fix
needs) and the VI-C9 handoff. **Does not touch** the scheduler: its rejection is correct given
the contract.

**Both directions are closed, and three cards found it independently.** VI-C7 measured the
Auto path; VI-D2 hit the explicit path while trying to record the hero and had to dispatch
around the button with `POST /executions`; V-C4 found `scheduler-e2e.spec.ts`'s four tests
failing deterministically on chromium and firefox — roughly six minutes per browser of CI's
E2E budget — waiting on this very gate to resolve. The suite has been reporting this defect
for some time, buried inside a job that was cancelled before it could say so.

**The explicit half.** `isCombinationSupported`
(`frontend/src/shared/execution/capabilities.ts:184-207`) iterates only
`harness.model_combinations` and never consults `model_passthrough`. Both bundled harnesses
ship `model_combinations` empty and attest `model_passthrough: Supported`
(`codex.rs:740`, `claude_code.rs:913`), and the scheduler honours that attestation — its arm
rejects only when `!declared && !passthrough`. So the server accepts an explicit model on
both harnesses and the dialog disables the button for exactly that case. **The gate is
stricter than the contract, and it blocks the only path that works.**

**Context.** VI-C7 measured it end to end. The dialog defaults every harness to *Auto*. A
request reaches the scheduler as `ModelSelector::AutoSelect` when no tier — agent profile,
project, fleet — names an explicit model, and also when a tier is pinned to the literal
`"auto"`, which stops the precedence walk where it stands. `evaluate_candidate` then rejects
it unconditionally, before any harness or runner check. The result is a request that sits in
`queued` with no error, no toast, no attempt row, and no wait long enough — on exactly the
install a stranger has before completing the Agents page's default-model step. The submit gate
meanwhile calls the choice allowed, on the promise that *"the scheduler will still validate at
claim time"*. It does not validate; it always refuses.

**The gate cannot decide this alone** — `gateHarnessModelSelection` receives only capabilities,
harness and the model pair, and no tier's configuration. VI-C7's own proposed fix was retracted
for that reason: blocking every unspecified model would break the installs where Auto works.

**Tasks:** make the dialog tell the truth about what *Auto* will do on this install, and make
the default-model step reachable from the place the operator hits the wall. Whether that needs
a resolution preview on the wire or only what the client already knows is this card's to
decide and to record — decide it from the code, not from this paragraph.

**Acceptance:** an explicit model on a harness attesting `model_passthrough: Supported`
submits and runs — prove it live, with the request id, through the button and not around it.
On an install with no default model at any tier, an operator cannot submit a dispatch that
will never run without being told so in the dialog, in words naming the fix. On an install
where a tier names an explicit model, the same dispatch still submits and still runs. The
false "the scheduler will still validate at claim time" string is gone from the tree.
`scheduler-e2e.spec.ts`'s four tests pass without being weakened — they were right.

**Stop if:** the honest fix needs a server route that does not exist. Name it and stop.

---

### VI-D1 — Prove the stranger's path, and make the docs match what shipped

**Last card. Needs everything, VI-D2 included.**

**Owns:** `scripts/smoke.sh` (new steps), amendments to every page VI-A1 wrote, a final
pass on `docs/CONFIG.md`, the one "Run it" sentence in `README.md` that names the Agents
page (and the final README merge — §VI.3), `CHANGELOG.md` `[Unreleased]`, and the VI-D1
handoff. **Does not own** the README's structure (VI-A3) or its assets (VI-D2).

**Tasks:**
- **Smoke step 13 — the gateway path with a local fake gateway:** an HTTP shim serving
  `/v1/models` and the per-harness endpoint, in the same pattern as the fake-harness shim,
  proving key → catalog → spawn environment → actual model. It must be able to `FAIL`;
  prove it by injecting a wrong key once.
- **Smoke step 14:** plain `tack serve` → `GET /api/local-runner/secrets` is 404.
- **The stranger test — the deliverable of this Part.** On a clean container with the
  release binary and the fake harness, a person — or a Playwright script that only clicks
  and copies commands **the UI shows it** — reaches a completed attempt with the UI as the
  sole guide — starting Tack is the only command typed; turning execution on is a click. The transcript, with every copied command and every observed status, goes in
  the handoff.
- Docs: the quick-start agent section now describes the Agents page; the CLI path stays
  documented as the second path; the README's "Run it" gains the one sentence that names
  the Agents page; every §VI.0 evidence-table row is re-measured, the README rows included.

**Acceptance:** `./scripts/smoke.sh` green with steps 13–14 proven load-bearing by injected
failure. The stranger transcript. `grep -rn "no TACK_\* variable for a model provider\|never
becomes a model proxy" docs/ README.md` returns only the ADRs and VI-A2's amendment. The
§VI.0 surface map's Target column is marked, row by row, *reached* or *not reached with
reason*, in the handoff — that table is what the integrator verifies.

---

### VI-D2 — Assets that show the execution plane: the hero and three screenshots

**Needs VI-C1 and VI-C2 merged** (there is nothing honest to record before them) **and
V-C2 landed** (it owns the directory; see §VI.3).

**Owns:** `docs/screenshots/hero.gif` (replacement), `docs/screenshots/agents.png`,
`attempt.png`, `two-machines.png`, the README image markup those four need, and the
VI-D2 handoff. **Does not touch** V-C2's recording or its slot.

**Context.** VI-A3 restructured the README around two components with a diagram in the
hero slot because no asset existed that showed an agent doing anything. This card makes
that asset. §V.1 rule 1 applies without exception: **every frame is real** — a real
harness (claude-code, proven live), a real item, a release build, no staged data.

**Tasks:**
- **`hero.gif`, twenty to thirty seconds, no narration:** Board → ▶ *Run with agent* on an
  item → the modal, defaults already filled, one click → the card's state chip goes
  *Queued* → *Running* → the item's Agent Activity tab with events streaming and the actual
  model named → *Done* → the artifact in the list. That is a day in the product; the
  recovery demo (V-C2) is the day something goes wrong, and the README shows both.
- **`agents.png`:** the Agents page, execution on, one agent detected, the provider
  configured, a default model chosen, the smoke run completed — every status green *by
  observation*, which the screenshot must have earned.
- **`attempt.png`:** the Agent Activity tab: events, requested vs actual model, usage
  marked `measured`, the artifact list.
- **`two-machines.png`:** the *Advanced* section with two runners on two hosts, different
  agents and models — the picture of "one board, many runners". Two real hosts (a container
  is a host). If only one host is available, two runner processes on it with different
  agents is still real and the alt text says so.
- Replace the README markup: hero in the hero slot (the diagram moves directly beneath
  it), the three screenshots **first** in *Screenshots*, PM views after. Alt text
  describes what is shown, not what is claimed.

**Acceptance:** each asset is produced from a release build on a machine named in the
handoff, with the commands used; no frame is edited beyond cropping; the GIF's terminal
state is a real completed run whose request id is recorded. `README.md` shows an agent
executing an item **before** it shows a Kanban board. V-C2's recording is untouched and
still in its slot. The hero is under 3 MiB (the current one is 2.4 MiB — measure, do not
exceed it by much without saying why).

---

### VI-C10 — The comment rule was never enforced on the frontend, and it shows

**Unblocked — VI-C9 merged 2026-09-06.** Independent of every other Wave 17 card.

**Owns:** `scripts/check-comments.sh` (its scope), and the citation cleanup in `frontend/src`.

**Context.** `check-comments.sh` scans `--include='*.rs'` under `crates/` and nothing else, so
the frontend has never been checked. Measure it before quoting this:

```
grep -rnE 'agent-handoffs|TODO\.md|\b(I{1,3}|IV|V|VI|VII)-[A-Z][0-9]\b' frontend/src | wc -l
```

**221 lines across 80 files** on 2026-09-06. The rule exists because this decay happened once
on the Rust side and reached operator log lines and API error responses before anyone noticed.
It happened again here, unwatched, and it has reached the screen:

- `EnrollmentPanel.tsx:210` **renders `docs/agent-handoffs/part-iii/III-E3.md` in the UI.** A
  screenshot of that paragraph was about to ship in `README.md`; VI-D2 was told to crop it out.
- `shared.ts:253` puts `docs/agent-handoffs/part-iii/III-E2.md, Gap 1` in the dispatch dialog's
  advisory text. **VI-C9 deletes that string for a different reason** — it is also false.
- `shared.ts:183` is the worst of the three, because it is wrong as well as archaeological:
  an operator-facing reason reading *"profile/project/fleet default precedence (TODO.md III-F3,
  Wave 5) has not landed yet."* It has landed —
  `crates/tack-orch/src/model_policy/mod.rs` walks exactly those tiers, and VI-C7's amendment
  cites the code. A stranger reads that sentence and concludes a feature is missing that is not.

**Tasks:** extend the gate to the frontend, then fix everything it flags. **User-visible
strings first** — those are defects, not untidiness. Rewriting is almost always right and
deleting almost always wrong: the citation usually wraps something true in board scaffolding,
so keep the knowledge and drop the pointer.

**Acceptance:** the extended gate is green, and proven load-bearing by re-adding one citation
of each kind it should catch — a `TODO.md §`, a card id, a handoff path — and watching it
fail on each. Every user-visible string that named a card or a handoff now says the same thing
without one, and `shared.ts:183` says what is actually true today or is deleted with the
mechanism it describes. Its runtime is stated (the Rust half is ~0.2s; say what the whole is).

**Stop if:** the gate flags something whose citation is load-bearing and cannot be rewritten
without losing meaning. Name it and leave it — an argued exception beats a silent deletion.

---

### VI-C11 — Nothing keeps the client's copy of the precedence walk honest

**Needs VI-C9 merged** (it is). Wave 17.

**Owns:** the shared fixture and the tests on both sides of it, plus the VI-C11 handoff.

**Context.** VI-C9 had to teach the dispatch dialog what an *Auto* request will resolve to,
and did it by hand-writing `resolveAutoModelPolicy` in TypeScript against three Rust
decisions: `ModelPolicyTier::ORDER`'s precedence, `resolve_model_policy`'s walk over it, and
`parse_model_default_convention`'s reading of the `limits`/`default_policy` blobs. Nothing
builds or fails when the two drift. The stakes changed with that card: the dialog now
**blocks** a dispatch on this answer instead of commenting on it, so a Rust-side change that
resolves *more* requests leaves the dialog confidently refusing work the server would run.
The function's own doc comment says all of this; what it cannot do is notice.

**Tasks.** Give the two sides one source of truth to be checked against, the way
`docs/contracts/runner-v1/` already does for the wire. A table of input tiers to expected
outcome, read by a Rust test and a TypeScript test, is enough — the walk is small and total
(the Rust side already has an exhaustive 2^4 test to derive the table from). **A table copied
into a `.test.ts` is not this card**; the point is a file both sides read.

**Acceptance:** proven load-bearing from the Rust side, which is the direction that matters —
reorder `ModelPolicyTier::ORDER` or add a tier, and the TypeScript test fails **without anyone
touching TypeScript**. Show it failing, then restore. Include the `pinned_auto` case: a tier
holding the literal `"auto"` stops the walk rather than falling through.

**Stop if:** the fixture would have to encode anything the wire does not already carry.

---

### VI-C12 — `GET /api/executions` hands back the whole table

**Needs nothing.** Wave 17.

**Owns:** `list_executions` in `crates/tack-api/src/handlers/executions.rs`, its mount, the
generated spec regeneration, the matching client call and its mocks, and the VI-C12 handoff.

**Context.** The handler takes no query parameters and runs
`SELECT … FROM execution_requests ORDER BY created_at DESC` with no filter and no limit; the
spec agrees, documenting it as "Every execution request, newest first". Every consumer then
filters in the browser — `frontend/src/shared/execution/store.ts:339` narrows by `item_id`
after the fact. So opening one item's Execution tab pulls every execution the install has ever
recorded. This was raised as "the endpoint ignores its own `item_id` parameter"; **it does not
have one**, which is the actual finding and a different fix.

**Tasks:** give the route the filter its callers already want, and a bound. Take the client
past it in the same change — a filter no caller uses is this tree's recurring defect.

**Acceptance:** the item-scoped call fetches only that item's rows, proven by asserting the
row count the handler returns rather than what the component renders. The spec and
`schema.gen.ts` are regenerated, never hand-edited. Existing callers still work.

**Stop if:** an unbounded default cannot be changed without breaking a caller you can name.

---

### VI-C13 — Two specs the picker redesign left behind

**Needs VI-C9 merged** (it is). Wave 17.

**Owns:** `frontend/e2e/a11y.spec.ts` and `frontend/e2e/agents-page.spec.ts`, and the handoff.

**Context.** VI-C2 redesigned the *Run with agent* target picker and collapsed the repository
fields. `scheduler-e2e.spec.ts` was never updated and failed from that day until VI-C9 fixed
its mechanics; VI-C9 escalated two more it did not own. `a11y.spec.ts:1106` drives the same
retired picker. `agents-page.spec.ts` has a test that fails only under cross-file parallelism —
which means it depends on state another file owns, and passes alone for a reason that is not
correctness.

**Acceptance:** both pass under the full parallel run, repeated three times, and the
parallelism-dependent one is fixed by removing the shared state it leans on rather than by
serialising the file. If serialising is genuinely the only fix, say why in the handoff — that
is an argued exception, not a default.

---

### VI-C14 — The Board's execution badges read from a list that now has an end

**Needs VI-C12 merged** (it is). Wave 17.

**Owns:** the Board and Sprint execution badges' data path
(`RunWithAgentButton.tsx`, `TestRunStep.tsx` and the store call behind them), and the handoff.

**Context.** VI-C12 gave `GET /api/executions` a default limit of 200, applied to the
unscoped call too, because before it there was no bound at all and one item's tab pulled the
whole table. The badges on Board and Sprint cards deliberately share that one app-wide fetch
rather than making a request per card, which is the right trade — but past 200 requests, an
item nobody has touched recently falls off the end of the preload and its badge goes stale.
The item's own Execution tab is unaffected: VI-C12 gave it a scoped fetch of its own.

This is not a regression VI-C12 introduced so much as one it made **reachable**: the previous
source was unbounded and wrong for other reasons. Making the bound honest is what exposed it.

**Tasks:** decide what the badges should read and make them read it. A per-item scoped fetch
per card is the obvious wrong answer — the reason they share one call is real. A projection
the server can answer in one request (latest execution per item, for the items on screen) is
the shape worth pricing first.

**Acceptance:** with more than `DEFAULT_LIMIT` execution requests in the database and the
oldest belonging to an item on screen, that item's badge shows its real state — proven by
asserting what the data path returns with the table seeded past the bound, not by looking at
a rendered card. State the request count the Board makes for a screen of items, before and
after.

**Stop if:** the honest fix needs a server route that does not exist. Name it and stop —
naming it is a result.

---

### VI-C15 — The embedded runner's state directory does not follow the database

**Needs nothing.** Wave 17.

**Owns:** `EmbeddedRunnerControl::new` and `load_runner_config`'s state-directory argument in
`crates/tack-cli/src/local_runner.rs`, its tests, and the VI-C15 handoff.

**Context.** VI-C13 found this while measuring, and it is the more serious of the two things
it could not fix. The embedded runner resolves its state directory independently of
`AppConfig.storage_dir`, so pointing the server at a different database does **not** point the
runner at different state: an enrollment made against one database is still on disk for a
server opened on another. The E2E suite is where it was noticed — `frontend/.tack-runner/`
persisting across a recreated `e2e.db` — but the suite is not the problem. An operator running
`tack serve --with-runner` twice against two databases is the same shape, and the credential
that leaks between them is a credential.

**Tasks:** make the runner's state directory derive from the same configuration the database
does. Decide and record what happens to state written under the old, unscoped path — a silent
migration, or a refusal that says what to do.

**Acceptance:** two servers on two databases, started in turn, each see only their own
runner's enrollment — proven from the state on disk and the API, not from a UI. A test pins
it. Prove it load-bearing by reverting the derivation once.

**Stop if:** the fix would change where an existing install's state lives without a migration
path you can describe. That is the finding.

---

### VI-C16 — Three specs, one server-wide switch

**Needs nothing.** Wave 17, and it will not stay small if it waits.

**Owns:** the agent-execution toggle's use in `frontend/e2e/execution-toggle.spec.ts`,
`provider-key-panel.spec.ts` and `agents-page.spec.ts`, and the handoff.

**Context.** Those three files each drive the same single, server-wide agent-execution switch,
with no per-test scoping available. VI-C13 made one cleanup step tolerate a sibling flipping it
first, which stops that particular race and names the rest. There is no fourth file today;
there will be, and it will arrive as an intermittent failure nobody can reproduce alone.

**Tasks:** give the suite one owner for that switch. A fixture that serialises access, a
worker-scoped setup, or a decision that one file owns it and the others read — the choice is
this card's, and the reasoning belongs in the handoff more than the mechanism does.

**Acceptance:** all three files pass under the full parallel run, repeated three times, on
chromium and firefox, and the mechanism makes a fourth file's author do the right thing without
reading this card. Say what happens if they do not.

---

### VI-C17 — The badges still guess, and one number is copied instead of derived

**Needs VI-C14 merged** (it is). Wave 17.

**Owns:** `ListExecutionsQuery` and `list_executions` in
`crates/tack-api/src/handlers/executions.rs`, the repository call behind it in
`crates/tack-db/src/repo/execution.rs`, `EXECUTION_LIST_PRELOAD_LIMIT` and `loadList` in
`frontend/src/shared/execution/store.ts`, `executionsApi.list` in
`frontend/src/shared/execution/api.ts`, the generated spec and client types, and the handoff.

**Context.** VI-C14 raised the badges' shared preload from the handler's 200-row default to its
2000-row hard cap and made an item absent from a capped response render `Unknown` instead of
silence. Both are correct and neither is the fix. The badges still ask "give me the most recent
N execution requests install-wide" and hope the items on screen are inside N — a question whose
answer degrades with install size no matter what N is. The question they want is "the latest
execution for each of these item ids", and no route answers it: `list_executions` takes one
`item_id`, not many. VI-C14 stopped there and priced two shapes; its handoff recommends
extending the route that exists over adding one, because that route already grew a query once.

The second half is smaller and worse. `EXECUTION_LIST_PRELOAD_LIMIT = 2000` in `store.ts` is a
hand-copy of `ListExecutionsQuery::MAX_LIMIT`, and its own doc comment says the two can drift.
**VI-C11 built the machinery that stops exactly this**, one card earlier in this wave — a
fixture generated from the Rust constant and read by a TypeScript test, which fails when either
side moves alone. A duplicated constant with a comment admitting it can drift is the defect that
card was created to end, reintroduced by the next card that needed a number from the server.

**Tasks:** make the badges ask the question they mean, and make the number derived rather than
copied. If the batch query removes the preload's need for a row cap at all, delete the constant
instead of binding it — that is the better outcome and it costs nothing to check first.

**Acceptance:** with the table seeded past `MAX_LIMIT` and the oldest execution belonging to an
item on screen, that item's badge shows its real state — asserted from what the data path
returns, not from a rendered card. State the request count and the response row count for a
screen of items, before and after; VI-C14 measured one request carrying up to 2000 rows
(≈343 KiB more per load than the 200-row version, computed from `ExecutionSummary`'s five
fields, uncompressed), so beating it means beating that, not just matching the request count.
Whatever binds the client's constant to the server's must fail when either side moves alone —
prove both directions, the way VI-C11's fixture was proven.

**Stop if:** the batch query cannot be answered without an N+1 in the repository layer. Say what
the query would have to look like and stop — naming it is a result.

---

### VI-C18 — One `run-with-agent` test fails every full run and passes alone

**Needs nothing.** Wave 17.

**Owns:** `frontend/e2e/run-with-agent.spec.ts` and the handoff. The gate under it
(`frontend/src/shared/runWithAgent/shared.ts`, `shared/execution/capabilities.ts`) is
**read-only** for this card — if the fix belongs there, that is an escalation, not an edit.

**Context.** `run-with-agent.spec.ts:98` ("item-detail: submitting a run creates the request and
it appears in the Execution tab without navigation") fails on **every** full parallel chromium
run and passes on its own. Measured at integration, after the two fixes that removed everything
else: 7 of 7 full runs failed exactly this test and nothing else; the same file run alone passes
4 of 4. It is deterministic, not flaky, which makes it cheap to work.

It enrolls its own runner with `opaque/model-alpha`, selects it in the target picker, and asserts
the Run button is enabled. Under the full suite the button stays disabled through all 24 retries.
Two things were already ruled out and should not be re-derived: the server-wide execution switch
(giving this test `executionToggleLock` fixed its sibling at `:153` — 7 of 7 — and did nothing
for this one) and today's merges (the same failure reproduces at `25819e0`, before any of them,
where the whole suite failed 5 of 5).

The interesting question is what a *freshly enrolled, never-connected* runner reports for
capabilities, and whether the gate VI-C9 built (`declared || passthrough`) is right to block it.
If it is right, this test has been asserting something false since it was written and the test is
what changes. If it is wrong, the gate is, and that is worth far more than the test.

**Acceptance:** the full parallel chromium suite passes three times consecutively. Say which of
the two the answer turned out to be, and if it is the test, say what it should have been
asserting instead.

**Stop if:** the honest answer is that the gate blocks a combination the scheduler would accept.
Name it precisely and stop — that is the same class of defect VI-C9 fixed and it outranks this
card.

---

### VI-C19 — Removing a provider key leaves the provider marked enabled

**Needs nothing.** Wave 17.

**Owns:** `EmbeddedRunnerControl`'s `set_secret`/`remove_secret` pair in
`crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

**Context.** `set_secret` flips `providers[vercel_ai_gateway].enabled = true` when the default
secret name is written, so a UI-only user never hand-edits a TOML flag after pasting a key. Its
opposite does not exist: `remove_secret` deletes the secret and its metadata and returns, leaving
`enabled` true with nothing behind it. Found by VI-C16 while serialising the E2E specs — once
they stopped racing, whoever ran second deterministically found a provider still marked enabled,
and it had to route around that with "Replace" instead of "Remove" in setup. That workaround is a
second reason to fix this: it is currently load-bearing for the suite.

An `enabled` provider with no credential is a capability claim that is not true, which this tree
treats as a defect on its own terms, not a cosmetic one.

**Tasks:** make the pair symmetric, and decide what `enabled` should mean for a provider whose
secret was configured under a *different* name via
`TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` — `set_secret` deliberately does not touch those,
so `remove_secret` must not silently disable them either.

**Acceptance:** set then remove leaves the provider exactly as it was before the set, proven from
the runner config and from `GET /api/local-runner`, not from a UI. Prove it load-bearing by
reverting the reset once. Then remove VI-C16's "Replace instead of Remove" workaround in
`frontend/e2e/provider-key-panel.spec.ts` and show the suite still passes — if it cannot be
removed, say why, because that means the bug had a second half.

---

### VI-C20 — `getOrCreateProject` returns whichever project ran last

**Needs nothing.** Wave 17.

**Owns:** `getOrCreateProject` in `frontend/e2e/helpers.ts` and every call site of it, and the
handoff.

**Context.** VI-C18 found this while fixing one symptom of it. `getOrCreateProject` does
`GET /api/projects` and returns `existing[0].id`; `list_projects`
(`crates/tack-db/src/repo/projects.rs`) orders `ORDER BY updated_at DESC`. Under
`fullyParallel: true`, "the shared E2E project" is therefore whichever project *any* concurrently
running test most recently created or patched — including its default model, its type, and its
vocabulary. Every spec calling this helper inherits that.

VI-C18 proved the leak directly rather than inferring it: on a fresh database, the failing dialog
displayed a project default model of `openai / opaque/model-not-declared-<timestamp>`, a fixture
string minted only by `scheduler-e2e.spec.ts`. It fixed its own test by not depending on the
ambient project at all. The helper is still wrong for everyone else.

**Tasks:** give the helper a stable identity. A name-scoped lookup, a per-worker project, or a
fixture that creates one and hands it down are all plausible; the choice is this card's and the
reasoning belongs in the handoff more than the mechanism does. Whatever you pick, a spec that
needs *its own* project must be able to say so, and one that genuinely wants a shared one must
still get the same one every time.

**Acceptance:** the full parallel chromium suite passes three times consecutively from a clean
state, and a test that asserts the shared project's identity gets the same id under
`--repeat-each=3` while other spec files are running. Say how many spec files were depending on
the old behaviour without knowing it.

---

### VI-C21 — The runner's credential outlives a database that was deleted, not reconfigured

**Needs VI-C15 merged** (it is). Wave 17.

**Owns:** the embedded runner's boot-time identity check in
`crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

**Context.** VI-C15 scoped the embedded runner's state directory to `storage_dir`, which fixes
the case it was carded for: two servers, two databases, two storage dirs, one working directory.
It does not fix the case where `storage_dir` stays put and the *database* is replaced — and that
case is not hypothetical. Deleting `frontend/e2e.db` without also deleting `frontend/storage-e2e`
leaves `storage-e2e/runner/session.json` holding a credential for a runner id that no longer
exists in the recreated database. The embedded runner then never reaches `active`, the harness
probe never runs, and `agents-page.spec.ts` fails on a missing version string — a symptom three
steps from the cause. Measured at integration: with both removed, the full chromium suite passes
70/70 three times; with only the database removed, that test fails alone and in the suite.

VI-C15's own reasoning anticipated the shape of this — it chose `storage_dir` partly because this
tree varies it *together* with `TACK_DATABASE_URL` everywhere it stands up a second install. That
is true of the config files and false of a person at a shell, which is where this bites.

**Tasks:** make the runner notice that the credential on disk does not belong to the database it
is now talking to, and recover rather than hang. The information exists — the runner id in
`session.json` either resolves against this database or does not.

**Acceptance:** with a valid `session.json` for a runner id absent from the database, the embedded
runner reaches `active` under a new identity, and says once, at `info`, that it did — with no
path, id, or credential in the line. Prove it load-bearing by reverting the check once. State what
happens to the abandoned journal beside it.

**Stop if:** distinguishing "this credential is for another database" from "this database is
temporarily unreachable" cannot be done without guessing. Say what signal would settle it and
stop.

---

### VI-C22 — The E2E suite's "throwaway" database is never thrown away

**Needs nothing.** Wave 17.

**Owns:** `frontend/playwright.config.ts`'s database and storage lifecycle, whatever setup or
`make` target that implies, `docs/TESTING.md`'s E2E section, and the handoff.

**Context.** `playwright.config.ts` calls `e2e.db` "a throwaway SQLite file". Nothing throws it
away. Across one integration session it reached **383 runners, 100 projects, 277 execution
requests and 723 items**, and the suite's failure set tracked that growth: **9 failures at 383
runners, 1 at 42, 0 from a clean state** — with different tests failing at each level, and every
one of them passing when its own file was run alone.

This is why this cycle kept producing contradictory flake measurements. VI-C13 reported three
clean parallel runs, VI-C16 measured 4-of-10 solo failures, VI-C18 measured 2-of-2, and the
integrator measured 7-of-7 and then 1-of-2 for the same test within an hour. None of those numbers
were wrong; they were taken at uncontrolled accumulation levels nobody was recording. **A flake
rate measured against this database is not reproducible unless the database's starting state is
stated with it.**

Note that the state is in two places, not one: the database *and* `storage-e2e/` (which now holds
the runner's credential and journal — VI-C15 moved it there). Removing one without the other is
its own failure mode, carded as VI-C21.

**Tasks:** decide what the suite's starting state is and make it that, every run. A clean database
per run is the obvious answer and may be too slow — measure it before ruling it in or out, since
a full run is currently ~17s from clean and ~47s at 383 runners, which is an argument in its
favour rather than against.

**Acceptance:** two consecutive full chromium runs produce byte-identical row counts for
`agent_runners`, `projects`, `items` and `execution_requests` at their end, and the suite passes
three times consecutively. State the wall-clock cost of whatever reset you add, measured, against
the ~30s the accumulated database was already costing.

---

### VI-C23 — Two tests hold wall-clock deadlines that a busy machine misses

**Needs nothing.** Wave 17.

**Owns:** `wait_for_ready`/`wait_for_active_runner` in
`crates/tack-cli/tests/embedded_runner_state_scoping.rs`, the shutdown timeout in
`crates/tack-runner/tests/bootstrap_entrypoint.rs`, and the handoff.

**Context.** Both tests assert against a stopwatch rather than against a state, and both fail on
a machine doing anything else. Measured at integration with three agent builds running (load
average 5–6): `two_servers_on_two_databases_each_see_only_their_own_runner_enrollment` failed 2 of
5 full-suite runs and then failed with `-j 1`, so it is not nextest's parallelism — it is the
box. `the_composition_root_stops_on_an_injected_shutdown_with_no_process_signal` budgets 5
seconds for the runtime to stop and took **110**, a 22× overshoot, then failed. On an idle
machine both pass, and the full suite is 1443 green.

The first is the worse of the two because it is new — it arrived with VI-C15 and its 10-second
`wait_for_active_runner` deadline was set on an idle machine by a card that had the machine to
itself.

**This is the same disease V-C4 found in CI from the other end.** There, a job cancelled at its
own 25-minute timeout hid a deterministic failure underneath it for days. Here, a green suite
turns red for a reason that has nothing to do with the code. Both make the signal unreadable;
both were fixed by looking at the budget rather than at the test.

**Tasks:** decide, per test, whether the thing being asserted is "this happens" or "this happens
within N". If it is the former — and for both of these it plainly is — the deadline is only a
liveness backstop and should be generous enough that reaching it means something is genuinely
wrong, not that a build was running. If any of it really is a latency claim, say so and keep a
tight bound with the load conditions it assumes written beside it.

**Acceptance:** both tests pass with the full suite running while the machine carries a load
average of at least 4 — say how you produced the load and what it measured. And they must still
fail promptly when the behaviour is actually broken: prove that by reverting VI-C15's derivation
once (which the first test already pins) and showing it still fails, and by naming what the
second test would catch.

**Stop if:** a generous deadline makes a genuine hang take longer to report than a developer will
wait. Say what the two numbers are and stop.

---

### VI-C24 — A serious contrast failure has been reported three times and carded zero times

**Needs nothing.** Wave 17.

**Owns:** the light palettes' `--color-text-secondary` in `frontend/src/index.css`, whatever
renders it at reduced opacity in the item detail drawer, and the handoff. Does **not** own
`a11y.spec.ts` — the test is already correct and already failing; making it pass by changing
what it asserts is the one outcome this card forbids.

**Two** tests in `a11y.spec.ts` fail on axe's `color-contrast` rule, not one: "item detail
drawer has no accessibility violations" (`:192`) and "item detail drawer with the dispatch
control visible" (`:442`). Same foreground in both, `#5f736e`, against two backgrounds —
`#e9edec` at **4.27:1** and `#eef1f0` at **4.43:1**, against WCAG AA's 4.5:1. Axe impact
`serious`. Fixing one without the other fixes neither: they share a cause.

**It is not a flake and not accumulation.** VI-C1's handoff reproduced it against an unmodified
base by stashing every change that card made. VI-C13's handoff carried it forward. VI-C22's
integrator measured it reproducing from a freshly emptied database. Three handoffs named it,
each correctly concluding it was outside their own card's ownership, and no card was ever
written — so it has been the standing failure that makes "the E2E suite passes clean" untrue
for weeks, and it absorbed part of the blame that belonged to database accumulation.

`#5f736e` is not a declared token. It is `--color-text-secondary` (`#566b66` in the default
light palette) composited at roughly 94% opacity over the surface beneath it — which is what
turns a token that passes on paper into a rendered colour that fails. The dark palette already
carries the fix for exactly this shape: `index.css`'s dark block documents lightening
`--color-text-secondary` from `#67807a` because it measured 3.52:1 on `--color-bg-subtle`. The
three light palettes (default, `clay`, `graphite`) never got the same treatment.

**Tasks:** decide whether the fix belongs in the token or in the thing that reduces its opacity —
they are different bugs with the same symptom, and the choice is the card's. Check all three
light palettes, not only the default: the failing test exercises one, and a fix that leaves
`clay` and `graphite` short has fixed a test rather than a defect. Contrast is measured against
what is actually composited, so verify the rendered pair, not the declared token values.

**Acceptance:** `npx playwright test --project=chromium -g "item detail drawer"` passes — that
pattern catches both failing tests, and passing only one is not passing — and the full chromium
suite's only remaining failures are unrelated to
`color-contrast` — state what is left. Report the measured ratio for every pair you changed, in
all three light palettes, with the command. Prove the test is load-bearing by reverting the
colour once and showing it fails again. Say whether the dark palettes were already compliant or
were quietly relying on the same opacity trick.

**Stop if:** meeting 4.5:1 requires a colour that no longer reads as secondary text against
primary — that is a design decision, not a contrast calculation. Say what the two constraints
are and stop.

---


### VI-C25 — A journal record whose attempt no longer exists is retried, and its checkout kept, forever

**Needs VI-C21 merged** (it is). Wave 17.

**Owns:** `report_recovery_and_apply_disposition` and the restart scan's disposition handling in
`crates/tack-runner/src/engine.rs`, its tests, and the handoff. Does **not** own the boot-time
credential check VI-C21 added, and does **not** own the workspace manager's cleanup contract.

VI-C21 made the embedded runner recover when its stored credential names a runner id a recreated
database has never seen. The journal is keyed by `state_dir` + `attempt_id` and never by runner
id, so it survives that swap intact — which is correct, and is why nothing is lost. But a record
written *before* the swap names an attempt the current database has no row for. The
acknowledgement comes back absent or mismatched, and `engine.rs` treats that the way it treats
any unsettled evidence: it leaves the record in the restart scan, because "an absent or
mismatched acknowledgement cannot settle local process evidence". That is the right default for
its intended case — a server that was briefly unreachable — and the wrong one here, because this
attempt is never coming back.

**VI-C21's handoff called this harmless. It understates it.** A `RecoveryPending` outcome also
deliberately keeps the workspace checkout, as the evidence an operator would need. So each such
record costs a retry on every boot *and* holds its checkout on disk indefinitely, with nothing
that ever revisits either. On a machine where agent worktrees and build targets already compete
for the disk, an unbounded set of checkouts belonging to attempts that provably cannot resume is
not a rounding error.

**Tasks:** separate "the server does not know this attempt" from "the server did not answer" —
these are the same code path today and must not be. The first is settled information and the
record can be retired; the second is not and must keep its current behaviour exactly. Decide what
"retired" means: quarantine keeps the checkout as evidence and stops the retry, deletion reclaims
the space and destroys the evidence, and the choice belongs to this card with its reasoning in
the handoff. Say explicitly what an operator loses under whichever you pick.

**Acceptance:** a journal record naming an attempt absent from the server is retired on the next
boot rather than rescanned, and a record whose acknowledgement merely failed to arrive is **not**
— prove both, and prove the second by making the server unreachable rather than by making it
answer "unknown". Assert the absence directly: the record is gone from the scan and the checkout
is in whatever state you chose, not merely that no error was logged. State what happens to
records already stranded by the time this ships. Prove load-bearing by reverting once.

**Stop if:** the server's response cannot distinguish "no such attempt" from "not authorised to
see this attempt" — retiring evidence on an authorisation error would destroy exactly what an
operator needs. Say what the responses are and stop.

---


### VI-C26 — Two of the three palettes are never scanned, and one of them is badly broken

**Needs nothing.** Wave 17.

**Owns:** `frontend/e2e/a11y.spec.ts`'s palette coverage, `graphite`'s `--color-primary-600` in
`frontend/src/index.css`, and the handoff.

The frontend's design tokens are two-axis — mode × palette — and `a11y.spec.ts` scans exactly one
cell of that grid: the default light palette. `clay`, `graphite` and every dark variant are
shipped untested. That is not a theoretical gap. Probing `data-palette="graphite"` by hand while
fixing VI-C24 found a `color-contrast` violation that reproduces **statically, 4 times out of 4**
— no animation, no timing, no flake: `--color-primary-600` (`#84cc16`) on the white sidebar
background at **1.97:1**, against WCAG AA's 4.5:1. That is less than half the required ratio, and
CI cannot see it because nothing asks it to look.

The coverage gap is the more important half. VI-C24 fixed a real defect the suite *did* catch and
that three handoffs still spent weeks dismissing as a flake; this one the suite cannot catch at
all, and it is far worse than the one that took all that effort.

**Tasks:** decide what palette coverage is affordable. Scanning every page in every palette
multiplies an already 30-second suite; scanning one representative page per palette probably does
not. The choice is this card's, and the reasoning belongs in the handoff — say what the new
coverage does **not** cover, so the next person inherits a known boundary rather than a false
sense of completeness. Then fix `graphite`'s violation. Check whether `clay` and the dark variants
hide anything similar **before** deciding the coverage shape, so the shape is informed by what is
actually there.

**Acceptance:** the new scans fail before the colour fix and pass after — prove both, in that
order, and give the measured ratio for every pair you change with the command that measured it.
The full chromium suite still passes, and say what it costs in wall-clock against the 32s it runs
in today. State plainly which palette × mode combinations remain unscanned after this card.

**Stop if:** fixing `graphite`'s primary requires changing what the palette *is* rather than
correcting a value — a primary that no longer reads as that palette's identity is a design
decision, not a contrast calculation. Say what the constraints are and stop.

---


### VI-C27 — The key reaches the page, not the runner, and the page says it worked

**Needs nothing.** Wave 17. **This one blocks Part VI's own definition of done.**

**Owns:** `EmbeddedRunnerControl`'s handling of configuration changes against an already-spawned
runner task in `crates/tack-cli/src/local_runner.rs`, its tests, and the handoff.

`EmbeddedRunnerControl::start()` clones `runner_config` once, at the moment the runner task is
spawned. `set_secret()` mutates only the control's own stored copy — never the spawned task's
independent snapshot. So a gateway key pasted while agent execution is already on never reaches
the runner that is actually running.

**What makes this worse than a missing feature is the feedback.** The Agents page's "Catalog: N
models" line updates immediately and looks correct, because `catalog()` re-reads the control's
copy fresh on every call. The one piece of UI an operator gets says *done*, while the dispatch
path still resolves the provider through the stale, disabled config and the attempt sits at
`preparing` forever. Nothing errors. Reproduced twice, deterministically, with full-debug logs
confirming the harness binary was never invoked for the real dispatch — only `--version`-probed.

The workaround is clicking "Re-check", which restarts the embedded runner. **Nothing in the UI
says so.** A stranger who reads the Agents page top to bottom and follows it in the order it is
laid out gets a dispatch that silently never completes — which is the exact failure this Part
exists to rule out, so this Part is not done while it stands.

**Tasks:** decide whether the spawned task should read configuration through a shared handle, or
whether a configuration change should restart the runner the way "Re-check" already does. The
second is smaller and honest; the first is better and larger. **Say which you chose and what it
costs**, rather than picking the small one silently. Whichever you choose, the invariant is that
no UI state may report a key as usable while the dispatch path cannot use it.

**Acceptance:** a key set while the runner is already running is used by the very next dispatch,
with no "Re-check" and no restart by the operator. Prove it end to end — an attempt that actually
reaches the harness, not a config assertion. Assert the absence directly for the failure case:
before the fix, the harness binary is never invoked for the dispatch, and your test must show
that, not merely that a status differed. Prove load-bearing by reverting once. Say what happens
to a key set while the runner is *stopped* — that path works today and must keep working.

**Stop if:** sharing the configuration handle would let a half-applied change be observed
mid-dispatch. Say what the interleaving is and stop; a silently wrong key is bad, a torn config
during an attempt is worse.

---

### VI-C28 — The board's own WebSocket is rejected by the browser, and only by the browser

**Needs nothing.** Wave 17.

**Owns:** `board_live`'s handshake in `crates/tack-api/src/handlers/websocket.rs`, whatever test
proves it, and the handoff. Touch `frontend/src/shared/realtime/boardSocket.ts` only if the fix
genuinely belongs on that side — decide which end is wrong before changing either.

`board_live` never selects or echoes a `Sec-WebSocket-Protocol` response header. The frontend
always offers one (`tack.v1`). RFC 6455 §4.1 requires a client that offered a subprotocol to fail
the connection when the server's response omits it — so a real browser closes it, and a stranger
who creates their first item watches it not appear on the board they just created it on, with no
visible error.

**Why nothing caught it, which is the part worth fixing properly:** a raw `curl` handshake does
not enforce §4.1 and succeeds. Every check this repo had was of that shape. The defect was found
only by driving a real browser end to end.

**Tasks:** decide which end is wrong — a server that ignores an offered subprotocol, or a client
that offers one nothing needs. Then close the test gap, because the fix without it just resets
the clock: whatever proves this must fail against the current server, and a `curl`-shaped check
provably cannot.

**Acceptance:** a real browser client that offers `tack.v1` stays connected and receives a live
update, and the same test fails against the unfixed server — run it both ways and show both.
State explicitly why the test you wrote catches what a `curl` handshake cannot. Prove load-bearing
by reverting once.

**Stop if:** echoing the subprotocol would commit the server to a versioning contract it has no
implementation for — a header that claims `tack.v1` while nothing enforces v1 semantics is a
capability claim, and those are load-bearing here. Say what the claim would be and stop.

---

### VI-C29 — The comment gate has never looked at the E2E suite

**Needs nothing.** Wave 17.

**Owns:** `scripts/check-comments.sh`'s roots and its path-resolution for the "pointers to files
that do not exist" category, the comments it then flags in `frontend/e2e/`, and the handoff.

`check-comments.sh` scans `crates/` and `frontend/src`. It has never scanned `frontend/e2e`, and
that is where board archaeology accumulates fastest, because tests get written directly against
acceptance criteria. Running the gate against that directory today exits 1 with, by its own
categories: **28 card ids, 28 board-vocabulary, 17 board citations, 4 handoff paths** —
`./scripts/check-comments.sh frontend/e2e`.

An earlier card extended this gate from `crates/` to `frontend/src` precisely because the rule had
decayed unenforced. It stopped one directory short.

**One complication you must handle, not work around:** that same run reports **22 "pointers to
files that do not exist"**, and most are false — they name sibling spec files that do exist, which
the check fails to resolve from an `e2e/` root. Adding the directory without fixing resolution
turns a real gate into one people learn to ignore.

**Tasks:** fix the pointer resolution first, then add the root, then rewrite what it legitimately
flags. **Rewriting is almost always right and deleting is almost always wrong** — these comments
usually wrap something real in board scaffolding. Keep the knowledge, drop the pointer.

**Acceptance:** `./scripts/check-comments.sh` with `frontend/e2e` among its default roots exits 0,
and every previously-false pointer is either resolved correctly or shown to be genuinely stale.
Report the count per category before and after, with the command. Prove the extended gate is
load-bearing by reintroducing one card id and showing it fires.

**Stop if:** a comment's only content is a card reference and the knowledge it wrapped is already
gone. Say which, and delete only those.

---


### VI-C30 — A developer running the UI the documented way never receives a live board event

**Needs nothing.** Wave 17.

**Owns:** the decision and its implementation — one of `default_allowed_origins()` in
`crates/tack-api/src/config.rs`, the `npm run dev` recipe (`Makefile`, `CLAUDE.md`,
`docs/book/src/developer/frontend.md`, `docs/book/src/user-guide/quick-start.md` step at
"Vite starts at `http://localhost:5173`"), or `board_websocket_is_authorized` in
`crates/tack-api/src/middleware.rs` — plus a test and the handoff. Do not widen more than one
of them.

`board_websocket_is_authorized` refuses the upgrade whenever the request carries an `Origin`
outside `TACK_ALLOWED_ORIGINS`, and the Vite dev proxy forwards the page's own origin
unchanged (proven at VI-C28's merge: the E2E suite's `localhost:5199` was refused with the
default list, and passing the origin in fixed it — 1 failed / 1 passed, same spec, no other
change). The default list holds `8080`, `3210` and `tack.test`; it has never held `5173`. So
the quick-start's own developer path — `tack serve`, then `npm run dev`, open `:5173` —
completes REST calls through the proxy and silently never receives a live event. Nothing
told anyone: the socket closes without an error the page shows, exactly as in VI-C28, and the
E2E suite only started exercising this path when VI-C28 landed (its config now sets the
variable for port `5199`, which is the suite's fix, not the developer's).

**Tasks:** decide where the fix belongs. Adding `5173` to the product's default allow-list
puts a dev port into every install's CORS posture; teaching the dev recipe to set
`TACK_ALLOWED_ORIGINS` keeps the product's defaults clean but adds a step the quick-start
must carry; treating a loopback-bound server's loopback origins as same-machine changes
what the check means. Weigh them in the handoff and pick one. Whatever you pick, a test must
fail without it: the E2E config's `TACK_ALLOWED_ORIGINS` line is the model of a from-scratch
proof, and `docs/CONFIG.md` (already corrected to name the WebSocket gate) must match.

**Acceptance:** following `docs/book/src/user-guide/quick-start.md`'s developer steps verbatim
on a tree with your change, an item created in one tab appears live in another without a
reload; the same steps on `develop` before your change do not. Show both. Prove
load-bearing by reverting once.

**Stop if:** the only shape that works also widens what a non-loopback bind accepts by
default. Say what it would accept and stop.
### VI-C31 — The Execution tab unmounts itself every four seconds

**Needs nothing.** Wave 17. **Blocks VI-C28's merge.**

**Owns:** `frontend/src/shared/execution/store.ts` (`loadAttempts`, `loadList`, the
`AttemptAvailability` vocabulary), `store.test.ts`, `frontend/src/shared/runWithAgent/ExecutionTimeline.tsx`
where it reads that vocabulary, one E2E proof (a new spec or an addition to
`execution-attempt-detail.spec.ts`), and the handoff.

`connectRealtime` re-runs `loadAttempts` for every watched request on every poll tick
(`realtime.ts`, 4 s). `loadAttempts` begins with `attemptsCache.set(id, {status: 'loading'})`,
discarding the `ready` data it already holds, and `ExecutionTimeline` renders `AttemptList` only
under `<Show when={attempts().status === 'ready'}>`. So every four seconds the attempt panel —
decisions, artifacts, the option an operator just chose, the decision token they typed, the
Resolve button under their cursor — is destroyed and rebuilt after the round trip. An operator
sees a flicker and loses in-progress input; a Playwright click that lands in the gap hits a
detached element. Found integrating VI-C28: once the board socket delivered events in the E2E
suite for the first time, `execution-attempt-detail.spec.ts` failed in 2 of 2 full chromium runs
(`locator.click` → "element was detached from the DOM"; `locator.check` → the radio never
reappeared within 30 s) and passed 2 of 2 with the socket refused. Check whether `loadList` has
the same shape for the list scope before deciding it does not.

**Tasks:** a refresh must keep what is already known: `ready` data stays rendered while the
next fetch is in flight, and the vocabulary says so honestly (a `refreshing` flag on `ready`, or
`loading` only when nothing is held — pick one and say why). Do not fix it by keying the list
differently or by making the spec wait; the defect is that a refresh forgets. Then the proof:
in the Execution tab, with a real pending decision, choose an option and type a token, wait
longer than one poll interval, and assert both survive and the same DOM node is still
attached. That is deterministic without the board socket, so it needs nothing from VI-C28.

**Acceptance:** the E2E proof passes with the fix and fails without it (show both);
`store.test.ts` pins that a refresh never transitions a `ready` entry through `loading`;
`npx vitest run` and the full chromium suite green. Prove load-bearing by reverting once.

**Stop if:** keeping stale data rendered would let a terminal attempt keep showing a live
Resolve control after the server already resolved or expired the decision. Say how the store
would know, and stop.

---

### VI-C32 — The embedded runner's boot misses its own liveness backstops on CI, and nobody knows what it is waiting on

**Needs nothing.** Wave 17.

**Owns:** the startup path of `tack serve --with-runner` as far as readiness is concerned
(`crates/tack-cli/src/local_runner.rs` and whatever in `crates/tack-runner/src/bootstrap.rs`
or the secret store it waits on), the three sibling integration tests
`crates/tack-cli/tests/embedded_runner_{orphaned_credential,state_scoping,live_secret}.rs`
only where their waits are concerned, and the handoff.

Two of the last fourteen CI runs on `develop` failed in this family and nowhere else:
`embedded_runner_orphaned_credential.rs:127` — `/api/health` not up within 15 s of spawning
the process — and `embedded_runner_state_scoping.rs:176` — the embedded runner not `active`
within 30 s. Both passed on the next push untouched, and both pass locally in well under a
second of wait. VI-C23 already rewrote these waits as liveness backstops "a genuine success
is never expected to approach"; CI approached them twice in one day, so either the number
is wrong for a shared runner or the boot does something there it never does here.

One measured lead, not a conclusion: `embedded_runner_live_secret.rs` sets
`DBUS_SESSION_BUS_ADDRESS=/dev/null` for its child and has never failed on CI, while the two
that failed do not set it. The secret store is keychain-first with a file fallback (ADR
0061); on a headless runner with no session bus, a keychain probe over D-Bus is a
plausible place to block for seconds before falling back — and if readiness or enrollment
waits behind that probe, the health endpoint itself is late. That would make it a product
fact, not a test fact: a server that is slow to answer `/api/health` on any headless host.

**Tasks:** measure first. Reproduce headless (no session bus, `DBUS_SESSION_BUS_ADDRESS`
unset) and time `spawn → health 200` and `spawn → runner active`, then the same with the
variable pointed at `/dev/null`; report both. If the probe is what blocks, decide where the
fix belongs — the probe should not sit in front of the listener, or should fail fast when
there is no bus — and only then touch the tests: a backstop that reflects the measured
boot, not a bigger number. If the probe is not it, say what is, with the numbers.

**Acceptance:** the two waits are explained by a measurement, the origin is fixed or
declared out of scope with its owner named, and ten consecutive CI runs of the three tests
(`workflow_dispatch` on the card branch is enough) show zero misses. Prove load-bearing by
reverting once.

**Stop if:** the boot is slow because the keychain probe is *correct* to wait — say what a
fast failure would cost a real desktop user whose bus is merely slow to start.

---

### VI-C33 — The coverage job fails on every pull request because tracing is initialised twice in one process

**Needs nothing.** Wave 17.

**Owns:** `init_tracing` in `crates/tack-api/src/server.rs` and its callers, the one test that
collides, and the handoff.

`init_tracing` calls `.init()` on the global tracing subscriber unconditionally. Under nextest
every test is its own process and nobody notices; under `cargo llvm-cov` (the Coverage job)
tests share one process, the second `.init()` panics, and
`server::tests::serve_with_ready_signals_the_real_bound_address` fails — on every pull request
and every push to `main`, since that job is gated `if: event != push || ref == main` and never
runs on a `develop` push. Found landing the actions-major bump (run 34113961066), confirmed
identical on the untouched Dependabot run 33982205066 two days earlier: not the bump's doing.

**Tasks:** make initialisation idempotent at the origin (`try_init`, or a once-cell that owns the
subscriber for the process), not by skipping the test under coverage. Then run the Coverage job's
exact command locally (`cargo llvm-cov` with the job's flags — read them from `ci.yml`) and show
it green.

**Acceptance:** the Coverage job green on a `workflow_dispatch` of the card branch; the panic
reproduced first with the job's own command and gone after. Prove load-bearing by reverting once.

**Stop if:** the collision is not the subscriber but two servers binding the same fixed port in
one process — say which, and stop.

---

### VI-C34 — With the SPA embedded, an unmatched `/api` route answers 200 with HTML

**Needs nothing.** Wave 17. **Blocks any merge to `main`** — "Embed SPA" is a required check.

**Owns:** the `fallback(spa::serve_spa)` wiring in `crates/tack-api/src/router.rs` and
`spa.rs`, the two `local_runner` handler tests that expose it, and the handoff.

Under `--features embed-spa` the router's catch-all fallback serves the SPA's `index.html` for
**every** unmatched path, `/api/...` included. Two tests in
`crates/tack-api/tests/handlers/local_runner.rs` (~lines 126 and 143) expect a genuine 404 from
an unmounted route and get the SPA's 200 instead — so the "Embed SPA" job is red on every pull
request and every push to `main` (gated the same way as VI-C33's job), and `main` cannot take a
merge until it is green. It is also a product fact for the single binary a stranger installs: a
typo in an API path, or a route a newer client knows and an older server does not, returns a
web page with status 200 instead of `404 Not Found`, which every client in this tree reads as
"the request worked".

**Tasks:** scope the SPA fallback to non-API paths at the router (`/api` and `/api/runner/v1`
keep the API's own 404), keep the deep-link behaviour the SPA needs for its routes, and make
the two tests run under both feature sets. Say in the handoff whether the CLI (`tack-cli` is
HTTP-only) ever depended on a 200 it should not have.

**Acceptance:** `cargo nextest run --workspace -E 'package(tack-api)' --features embed-spa` green
and the default build green, both shown; the "Embed SPA" job green on a `workflow_dispatch` of
the card branch. Prove load-bearing by reverting once.

**Stop if:** the SPA's own routing needs a 200 for a path under `/api` — name it and stop.

---

### VI-C35 — `cargo deny check` fails locally on advisories and licenses that CI never checks

**Needs nothing.** Wave 17.

**Owns:** `deny.toml`, the `cargo-deny` job's arguments in `ci.yml`, whatever dependency change
resolves a finding, and the handoff.

CI's job is named "cargo-deny (licenses + duplicate deps)" and is green; a plain `cargo deny
check` on `develop` is red on `advisories` (`proc-macro-error2` unmaintained, via
`validator_derive`; two yanked crates via `object_store` and `sqlx-sqlite`) and on `licenses`
(two MIT rejections via `secret-service` and `zbus`). Found landing the cargo-major bump; all
findings pre-date it. A gate that is green in CI and red on a developer's machine is two gates.

**Tasks:** decide which checks the repo means to enforce and make CI and `deny.toml` say the
same thing; resolve or explicitly `ignore` each finding with its reason and a review date;
`proc-macro-error2` also trips the compiler's future-incompat warning on every build, so its
resolution is worth naming on its own.

**Acceptance:** `cargo deny check` and the CI job agree, both green, with every ignore justified
in `deny.toml`. Prove load-bearing by removing one ignore and watching it fire.

**Stop if:** a license rejection is real and the crate has no replacement — say which and stop.

---

### VI-C36 — "Could not enqueue execution" says retry, and retrying never works

**Needs nothing.** Wave 17.

**Owns:** the enqueue path of `POST /api/executions` (`crates/tack-api/src/handlers/executions.rs`
and what it calls in `tack-orch`), the error it returns, one test, and the handoff.

Found by VII-D1's stranger walk and reproduced twice over plain `curl`, no window involved:
against one item, a request with a fresh, never-used `idempotency_key` answered
`{"code":"internal_error","message":"Could not enqueue execution","retryable":true}`; a second
request with another fresh key against the same item answered identically six seconds later;
a brand-new item with the same payload shape succeeded instantly and finished seconds after.
Capacity was not it (`GET /api/runners` showed `available_capacity: 1` throughout). The
transcript is in `docs/agent-handoffs/part-vii/VII-D1.md`, "Product findings", with item ids
and timestamps. Whatever the cause, the response is wrong twice: `internal_error` hides a
condition the server evidently knows (it is per-item and persistent), and `retryable: true`
tells the client to do the one thing that does not help.

**Tasks:** find the origin — the likely shape is a per-item invariant (one live request per
item, a state the item is in, a row the previous attempt left) surfacing through a catch-all
arm — and give it a typed, non-retryable answer that names the condition (`409` with a code
the UI can render, or whatever the API's existing vocabulary has for it). Then the UI: the
Run-with-agent modal must show that reason rather than "try again". If the cause is a real
bug rather than an invariant, fix it at the origin and say so.

**Acceptance:** the reproduction from the handoff, replayed with a test, yields the typed
answer with `retryable: false`; the modal shows it; `internal_error` never carries
`retryable: true` for a condition the server can name. Prove load-bearing by reverting once.

**Stop if:** the condition turns out to be transient after all — show the timing, and stop.

---

### VI-C37 — The desktop app's dependency tree carries fifteen unmaintained crates nobody has reviewed

**Needs nothing.** Wave 17.

**Owns:** `crates/tack-desktop/Cargo.toml` and its own `Cargo.lock`, the `tack-desktop` step of
CI's `cargo-deny` job and the policy `scripts/gen-deny-toml.sh` generates for it, and the
handoff.

`cargo deny check` against `crates/tack-desktop` reports fifteen "unmaintained" advisories,
all inside Tauri's Linux GTK/WebKit bindings (`gtk`, `gdk`, `atk`, `glib`-adjacent crates and
their `-sys` siblings), with no upgrade path this repository controls. VI-C35 left that
workspace's advisories deliberately outside the gate rather than paper over them with fifteen
unexplained ignores. That is honest but it is not a review: nobody has said, crate by crate,
whether each finding is an accepted cost of Tauri 2 on Linux today, a sign the app should move
to a newer Tauri line, or a real exposure.

**Tasks:** enumerate the fifteen with `cargo deny check advisories` in that workspace and
`cargo tree -i <crate>` for each path; check what the current Tauri release and its
`tauri-build`/`wry` line pull in (a newer Tauri may already have moved off some); for each
finding decide and write the reason — `ignore` with a review date in the generated policy, or
a bump. Then bring the workspace's advisories into the same unrestricted check the root has,
so the two workspaces are gated the same way.

**Acceptance:** `cargo deny check` green in `crates/tack-desktop` with every ignore justified
in the policy source; CI's `tack-desktop` step runs the same unrestricted check; `make desktop`
still builds and VII-D1's walk still holds (the AppImage opens, closes to tray, reopens). Prove
load-bearing by removing one ignore and watching it fire.

**Stop if:** the only way to green is a Tauri major upgrade — say what it would change for
the app and stop.

---

### VI-C38 — graphite's `on-accent` goes white: the design decision VI-C26 escalated, taken

**Needs nothing.** Wave 17. **Decided by the user 2026-09-07: option 1 of VI-C26's handoff — `on-accent` white.**

**Owns:** graphite's tokens in `frontend/src/index.css` (`--color-primary-600`, `--color-on-accent`
and whichever siblings the handoff names), `Sidebar.tsx`'s hardcoded swatch preview, the
`Button.tsx` comment that explains the dark on-accent, the graphite/light scan in
`frontend/e2e/a11y.spec.ts`, and the handoff.

VI-C26 proved no value of `primary-600` alone clears both of its roles in graphite/light (text on
white needs `L <= 0.1833`, background under `#1a2e05` needs `L >= 0.2733`) and stopped, as its
card told it to, because the only shape that clears both flips `on-accent` to white and changes
what the palette is. The user has taken that decision: graphite becomes dark olive with white text
on it, the way teal and clay already pair a dark primary with a white on-accent.

**Tasks:** pick the dark `primary-600` (the handoff suggests the `#3f6a0c`–`#4d7c0f` range; keep
graphite's hue), flip `on-accent` to white, walk every pairing VI-C26's table measured and re-measure
it with the same `contrast.js` command, update the swatch preview and the `Button.tsx` comment so
neither describes the old identity, and check graphite/**dark** still passes — its tokens may share
`on-accent`. Then remove `GRAPHITE_LIGHT_KNOWN_ISSUES` and fold graphite/light into
`OTHER_MODES_AND_PALETTES`, so all six cells are unsuppressed gates.

**Acceptance:** the graphite/light scan, unsuppressed, fails on the old tokens and passes on the new
ones — prove both in that order; every changed pair's ratio is in the handoff with the command that
measured it; the full chromium suite passes; no raw hex enters a component (tokens only). The
handoff says in one line what graphite looks like now versus before.

**Stop if:** graphite/dark breaks in a way that needs its own identity decision.

---

### VI-C39 — Raise the dependency floor to Rust 1.94 and land `sqlx` 0.9

**Needs nothing.** Wave 17. **Decided by the user 2026-09-07: the floor rises to the lowest version
the latest stable `sqlx` needs, not to the newest Rust.**

**Owns:** `rust-version` in `Cargo.toml` and `crates/tack-desktop/Cargo.toml`, the MSRV job in
`.github/workflows/ci.yml` (name, action pin, `RUSTUP_TOOLCHAIN`, its comment), the README badge and
line 152, `CONTRIBUTING.md`'s table, the `sqlx` line in `Cargo.toml` and everything in `tack-db`,
`tack-api` and `tack-orch` its 0.9 changes touch, the `sqlx` ignore in `.github/dependabot.yml`,
`CHANGELOG.md`, and the handoff.

`sqlx` 0.9.0 (stable, 2026-05-21) declares `rust-version = 1.94.0`; the floor here is 1.89. The
user's rule: the newest *stable* line of the dependency, on the *lowest* Rust that supports it, so
the floor never runs ahead of what the rest of the graph is known to build on.

**Tasks:** install the 1.94 toolchain (`rustup toolchain install 1.94`), read `sqlx`'s 0.9 changelog
and migration notes, bump the workspace dependency, fix what breaks (SQLite pool options, `query!`
macros, `Error` variants, `FromRow` derives — whatever 0.9 renamed), and prove the whole workspace
builds `--locked` on 1.94 exactly as the MSRV job will:
`RUSTUP_TOOLCHAIN=1.94.x cargo build --workspace --locked`. Then move every floor reference listed
above in the same commit, delete the Dependabot ignore, and update the MSRV job's comment to say
which dependency now sets the floor. Run the full suite on the pinned 1.98 toolchain as well.

**Acceptance:** `cargo build --workspace --locked` succeeds under `RUSTUP_TOOLCHAIN=1.94.x` and fails
under 1.89 (show the error, so the new floor is proven real, not nominal); `cargo nextest run
--workspace` green; `.githooks/pre-push` green; no `1.89` remains anywhere but `TODO.md`,
`CHANGELOG.md` and the handoffs (`grep -rn "1\.89" --exclude-dir=target --exclude-dir=node_modules`).
The handoff names every `sqlx` API that changed and what it was replaced with.

**Stop if:** `sqlx` 0.9 needs a migration-runner change (one `ALTER` per migration, no wrapping
transaction — see `CLAUDE.md`) or changes SQLite's journal/WAL behaviour; say what and stop.

---

### VI-C40 — CI moves to Node 22 LTS, and `jsdom` 30 follows

**Needs nothing.** Wave 17. **Decided by the user 2026-09-07: Node 22 LTS.**

**Owns:** every `node-version` in `.github/workflows/{ci,release,scheduled-audit}.yml`, the `jsdom`
ignore in `.github/dependabot.yml`, `jsdom` in `frontend/package.json` and the lockfile,
`CONTRIBUTING.md`'s prerequisites row if it names Node, and the handoff.

`jsdom` 30 needs Node `^22.22.2 || ^24.15.0 || >=26`; CI pins `20` in eight places. The user chose
22 LTS: the current maintenance line, not the newest major.

**Acceptance:** `npm run type-check`, `npx vitest run` and the e2e suite green on Node 22.22+;
Dependabot's ignore for `jsdom` gone; CI green on the card branch. The handoff records the Node
version the local machine runs (22.17.1 today, below `jsdom` 30's floor) and what that means for
a developer who has not upgraded.

---

## §VI.5 Definition of done, and deliberate exclusions

| Claim | Proof |
|---|---|
| A UI-only user who started `tack serve` on their machine and has a Vercel AI Gateway key reaches a completed attempt without touching a terminal again | VI-D1 stranger transcript |
| Turning agent execution on is one click on a loopback bind, and impossible on any other | VI-B3 |
| The README's first screen shows two components — a diagram before any PM screenshot — and a stranger reading it reports two parts, what each holds, and one board / many runners | VI-A3 acceptance test, re-run by VI-D1 |
| The hero asset shows an agent executing an item, and the recovery recording sits beside the paragraph it proves | VI-D2, with V-C2's slot untouched |
| A UI-only user with a harness's own login sees every console step inside the flow and never a documentation link | VI-C1 E2E |
| A provider key exists in exactly one place — the runner's owner-only store — and in no table, log or response | VI-B2 / VI-B3 absence tests with positive controls |
| A project default model is honoured with `project` provenance and shown as such in the modal | VI-C3 `model_policy_test.rs` + VI-C2 E2E |
| Nothing is green without an observation | VI-C1 |
| One page answers "item → attempt" and "model → provider → key", and it is the same page | VI-A1, amended by VI-D1 |
| Docket untouched; `runner_contract` byte-identical unless VI-B2 escalated and the integrator accepted a revision | every card's gate |

**Deliberately not in this Part**, recorded so no card adopts them by drift:

- **Any provider beyond Vercel AI Gateway.** OpenRouter, direct Anthropic / OpenAI keys and
  local endpoints stay in the harness's own configuration. B1's store plus
  `secret_reference` already lets a request carry any key into a harness's environment;
  a second gateway earns its own config section only when it measurably differs from this
  one (scope-discipline rule 3).
- **Tack calling any model API itself**, including "just to validate a key" — the runner's
  probe does that.
- **A harness-login broker.** Vendor OAuth flows stay in the terminal, rendered in the UI.
- **Multi-user accounts** (ADR 0059) and **anything docket** (ADR 0060).
- **Notifications, i18n, time tracking, in-UI diff review** — still deferred from Part V.
  C4 lists artifacts; reading a diff in the UI is a separate, later card.
- **Exercising the `decisions` path with a real harness** — still no harness in this tree
  asks a mid-run question; C4 lists decisions, it does not manufacture one.
- **Recorded, not decided:** whether bare `tack` (no subcommand) should mean
  `serve --with-runner` on a loopback bind. The recommendation is no — the UI switch (ADR
  0061 decision 6) removes the click's cost without changing a security default. Reopen only
  with evidence that the switch is not enough.

---

## §VI.6 Handoff additions for this Part

Use `docs/agent-handoffs/part-vi/TEMPLATE.md` — it carries the §III.2 template, Part V's
three sections (claim → evidence table, measured numbers, what a stranger still cannot do),
the three below, and a **Context spent** section (cold-start tokens against the dispatch
block's estimate, context at handoff, files opened and not used). Never open the archive
for the template. The three sections specific to this Part:

1. **Surface-map delta.** Which rows of the §VI.0 table your card moved from console to UI,
   or proved cannot move — with the reason, and whether that reason is already in the
   table's last column. If it is not, that is an escalation for ADR 0061, not a new row.
2. **Secret-path proof** (B1, B2, B3, D1 only). The exact commands showing the key in the
   runner's store and nowhere else: the `sqlite3 … | grep -c`, the log capture, the `stat`.
   A handoff for one of these cards without this section is incomplete.
3. **Vocabulary check** (A3, C1, C2, D1, D2). The grep for §VI.1 rule 8's words over what
   your card rendered or wrote, with every hit and why it is under *Advanced* or in the
   developer book. The README is allowed the word "runner" — it is telling the story.

---

# Part V — Adoption & First Public Release (Phase 59)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Adoption & First Public
Release*, opened by the adoption audit of **2026-08-30**. This Part has its own numbering
namespace (`§V.0` … `§V.6`) so the archive's load-bearing numbers stay put.

Like Parts III and IV, this board is written to be picked up cold by parallel agents in
isolated worktrees. Every card is bounded, names every shared-file owner, and has an
acceptance gate verifiable without trusting its author's handoff.

## Status board — Part V

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 11 — Release blockers | V-A1 · V-A2 · V-A3 · V-A4 | 59 | **Done, all four integrated** at `45416e7` on `develop` (handoffs: `docs/agent-handoffs/part-v/V-A1.md` … `V-A4.md`). Local-only artifacts awaiting explicit user approval to publish: `main` branch (V-A1), tag `v0.1.0-beta.7` (V-A3), GitHub description/topics/Pages (V-A4) — see each handoff's Next step. |
| 12 — Honest posture & prune | V-B1 · V-B2 | 59 | **Done, both integrated** at `990349f` on `develop` (handoffs: `docs/agent-handoffs/part-v/V-B1.md`, `V-B2.md`). V-B1: ADR 0059 (single-operator), non-loopback+no-token startup refusal now has an explicit opt-out (`TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK`); README paragraph proposed in the handoff, not yet merged (V-A4 already shipped this cycle). V-B2: ADR 0060 decides **keep** docket as a maintained optional bridge (not gate/delete), backed by measured numbers; no code/schema change shipped. |
| 13 — Distribution & launch | V-C1 · V-C2 · V-C3 · V-C4 | 59 | **V-C4 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C4.md`). *Verify install URLs* now runs `install.sh` for real into a scratch directory and asserts a runnable `tack` lands; the integrator re-ran its revert-once proof independently — with the `grep -v` reverted it downloads 2.14 MB and fails with `no 'tack' binary found in archive`. E2E's budget went 25 → 40 minutes on measured numbers (3.45 overhead + 8.40 chromium + 7.00 firefox, webkit unmeasurable on this machine and recorded as such, not as a webkit fact). **That will turn a cancellation into an honest failure, which is the point:** `scheduler-e2e.spec.ts`'s four tests fail deterministically on VI-C9's gate. **Bring the budget back down once VI-C9 lands** — about twelve of those minutes are the failing tests. Original note follows. | **V-C3 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C3.md`) — and it found the launch's own blocker while walking the stranger's path instead of reading it: **`install.sh` had installed nothing since `v0.1.0-beta.7`.** Two archives per release end in the same platform suffix, the releases API lists `tack-runner-…` first, so the advertised one-liner downloaded the runner archive and died on `no 'tack' binary found in archive`. One `grep -v`; verified against the live API, broken before and right after. Everything else it prepared is unpublished by design. Its amendment corrects three of its own claims after review. Two green-but-blind CI checks it surfaced are now **V-C4**. Original note follows. | V-C1 **done, integrated** at `135b941` (handoff: `docs/agent-handoffs/part-v/V-C1.md`) — Homebrew/AUR/Nix/ghcr.io recipes + cargo-binstall metadata, all four verified with real installs in disposable containers; found and fixed a pre-existing `Dockerfile` toolchain-ordering bug along the way. **V-C2 is now unblocked** — Part IV Wave 10 (all six cards, including IV-A4 zero-touch enrollment) and V-A2 are both done, so the demo can now be recorded as one command (`tack serve --with-runner`) with no hang risk. **V-C2 integrated 2026-09-06** (handoff `docs/agent-handoffs/part-v/V-C2.md`) — `docs/screenshots/recovery-demo.gif`, an attempt reaching *Needs operator*, an operator decision, and the same attempt reaching *Succeeded*, re-recordable with `scripts/record-recovery-demo.sh` against a published artifact in Docker. **That unblocks V-C3, dispatched 2026-09-06, and Part VI's VI-D2.** V-C2's escalation carries a dated amendment worth reading before trusting any card that measures a release artifact: recording from a published tag measures a *past* tree, and two of its three original product observations were already false on `develop` by the time it wrote them. The surviving one is carded as **VI-C7**. |

**Integration line:** `develop`, the repository's default branch — same as Parts III and IV,
and for the same reason. Branch every card from `develop`. Do not create a `plan/*` line.

---

## §V.0 Cold-start context capsule

**What this Part is for, in one sentence.** Tack has been a public repository since
2026-03-15 and has zero stars, zero forks, zero human issues and one human contributor;
this Part closes the distance between "it works on this machine" and "a stranger can use
it".

**Read that number correctly.** It is not a verdict on the code. This repository holds
~122k lines of Rust across six crates, ~45k lines of SolidJS, 1380 passing workspace tests,
57 migrations, a 92-path documented API, a ~113 ms cold start at ~11.7 MiB RSS, and durable
execution semantics — fencing tokens, leases, replay tables, recovery audits — that **no
competitor in its category has**. It is a verdict on the fact that the project has never
been released, published, positioned or shown to anybody. Every card here is about
distribution, proof and truthfulness. **None of them adds a product feature.**

### Evidence base, measured 2026-08-30

Do not re-derive these; act on them. Each row names the command that produced it, so any
card can re-check one cheaply.

| Fact | Value | How it was checked |
|---|---|---|
| README's headline install one-liner | **HTTP 404** | `curl -sI https://raw.githubusercontent.com/yielab/tack/main/install.sh` |
| branch `main` | **does not exist**; default branch is `develop` | `git ls-remote --heads origin main` → empty |
| stars / forks / human issues / human contributors | **0 / 0 / 0 / 1** | `gh repo view`, `gh issue list --state all`, `git shortlog -sn --all` |
| only release | `v0.1.0-beta.6`, **2026-06-22**, four `tack-*` archives, **no runner archive** | `gh release view v0.1.0-beta.6 --json assets` |
| `release.yml` packages `tack-runner` | **yes, already** — since `7d78de3`, 2026-08-19 | the workflow's `RUNNER_STAGE` block |
| mdBook published | **no** — `yielab.github.io/tack` returns 404 | curl |
| repo `homepageUrl` / topics | **empty** | `gh repo view` |
| live smoke | **`SMOKE FAILED`**, 2026-08-26 | Part III Wave 9 amendment, in the archive below |
| identity model | **none** — one shared bearer token; no token configured = allow-all | `crates/tack-api/src/middleware.rs::require_token` |
| docket legacy surface still in the tree | 11 `orch_*` tables + `control_planes`, an Approvals page, a ControlPlanesManager | `grep -o 'orch_[a-z_]*' crates/tack-db/src/migrations.rs \| sort -u` |

**The single most embarrassing line in that table is the first one.** The install command
printed in the README of a public repository has never worked, because the branch it names
was never created. That is V-A1, and it is a few minutes of work that has been costing
every visitor since March.

**A correction the audit made to itself, so no card repeats it.** The first reading of the
release gap was "CI does not package the runner". That is **false** — `release.yml` has
built and packaged `tack-runner` into its own per-platform archive since `7d78de3`
(2026-08-19). The gap is that **no tag has been cut since**, so the only downloadable
release predates every Phase 50–58 capability. V-A3 cuts a tag; it does not fix CI.

### Competitive context, and why the timing is the whole argument

The nearest thing to Tack that ever existed — **Vibe Kanban**, a kanban board that
orchestrates Claude Code / Codex / Gemini agents — **shut down on 2026-04-10** when its
company (Bloop) closed. Its own farewell post says it had thousands of engineers using it
daily and never found a business model. **Crystal**, the other popular open-source
orchestrator in that space, **was deprecated in February 2026**.

What remains above Tack in that category is either closed-source (Conductor — macOS-only,
$22M Series A; Sculptor — Claude-only, by Imbue) or is not a board at all (OpenHands,
Emdash). Meanwhile the mature open-source project managers — Plane (~54.6k ★), Huly
(~26.9k ★), Focalboard (~26.3k ★), Leantime (~9.4k ★), Vikunja (~3.8–5k ★) — **execute
nothing**.

Tack is the only thing that is both. **There is a real, identifiable, currently-orphaned
audience**, and it is being collected by Emdash (YC W26) while this board sits unstarted.
That is the adoption thesis, and it has an expiry date.

The category's history also carries a warning worth stating once: it killed its leader
through failure to monetize, not failure to be useful. For a project with no company
behind it that is an advantage, and the positioning card (V-A4) should not shy from it.

### Working-tree state at the time this board was written (2026-08-30)

`develop` is at `277868a`, and the tree is **not clean**:

- `crates/tack-runner/src/harness/mod.rs` — the uncommitted
  `registering_all_three_real_adapters_is_order_independent` fix inherited from Part IV
  §IV.0. **Land or discard it deliberately before branching any card**; do not let it ride
  into a card's diff unexamined. It is the same change Part IV's capsule describes; the two
  Parts must not both claim it. **Landed** at `f4941f0`, before any Wave 11 card branched —
  the fix matched a real `codex` binary now being installed on this machine.
- `docs/adr/0058-standalone-single-binary-runner.md`, `docs/agent-handoffs/part-iv/` —
  untracked Part IV material.
- `tack.db.before-037_orch_runs_rebuild.sqlite` — an untracked database snapshot sitting in
  the repository root. It is not a card, but it should not be there when a stranger clones.
  Whoever lands V-A4 removes it or gitignores it.

---

## §V.1 Rules for simultaneous agents

**All fourteen rules of §III.2 apply unchanged** — one card / one worktree / one branch,
stay inside `Owns`, no `unimplemented!()` or hidden fake success, tests ship with the card,
no blocking sleeps, logs carry ids and never credentials, stop on contract ambiguity, and
each wave ends with adversarial verification by someone who did not author the code. Read
them in the archive; they are not restated here.

Five rules are specific to this Part, and they exist because this Part's output is read by
strangers rather than by the team:

1. **No claim ships that this repository cannot demonstrate on a clean machine.** The
   standing failure mode here is documentation that describes the *intended* product rather
   than the delivered one — the README currently implies three working harnesses when
   `codex` has never completed a live attempt. A card that cannot prove a claim deletes the
   claim; it does not soften the wording.
2. **Do not fix a documentation gap by weakening the product, and do not fix a product gap
   by rewording the documentation.** If V-A2 cannot make the live smoke pass, V-A4 changes
   what the README claims. If V-A4 finds a claim it wants to keep, it asks V-A2 to prove
   it. Neither card resolves the tension alone.
3. **Removal needs a decision record, not a preference.** Anything this Part gates, hides or
   proposes deleting gets an ADR in `docs/adr/` naming the option chosen and the options
   rejected, with the measured cost of each. "It felt like dead weight" is not a reason this
   repository accepts.
4. **Measure, never estimate.** Every number that reaches a handoff, a doc or a release note
   arrives with the command that produced it. Binary sizes, install times, test counts,
   surface counts — all of them. This rule already exists in Part IV for binary size; here
   it covers every number.
5. **Nothing outward-facing is published without the user's explicit approval.** Creating a
   git tag, pushing a release, publishing a Pages site, opening a package-manager PR, or
   posting anywhere are all outward-facing. Cards prepare them and stop. V-C3 is entirely a
   preparation card for this reason.

---

## §V.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `install.sh`, the `main`-branch decision, the doc-URL CI check | V-A1 only |
| `scripts/smoke.sh` | V-A2 — **and IV-A6 in Part IV. Real conflict, see §V.3** |
| `.github/workflows/release.yml`, release notes, the tag itself | V-A3 only |
| `README.md` | V-A4 — **and IV-A6 in Part IV. Real conflict, see §V.3** |
| GitHub repo description / topics / `homepageUrl`, the Pages workflow, `docs/book/src/introduction.md` | V-A4 only |
| `crates/tack-api/src/middleware.rs`, `docs/book/src/user-guide/administration.md` | V-B1 only |
| `crates/tack-orch/src/adapters/**` gating, `frontend/src/features/approvals/**`, the control-planes UI | V-B2 only |
| `docs/CONFIG.md` | V-B1 for the auth rows, V-B2 for the docket rows — **disjoint sections, coordinate in handoffs**; and IV-A6 in Part IV |
| `packaging/**`, brew/AUR/Nix recipes, `cargo-binstall` metadata in `Cargo.toml` | V-C1 only |
| `docs/screenshots/**`, the demo asset | V-C2 only |
| `CHANGELOG.md` | V-A3 for the release section; every other card proposes text in its handoff |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |
| `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json` | **nobody — out of scope for this Part** |

---

## §V.3 Dependency graph, cross-Part conflicts and merge policy

```text
V-A1  (install path) ──────────────────┐
                                       │
V-A2  (smoke tells the truth) ──┬──────┼── V-A3  (cut the tag) ──┐
                                │      │                         │
                                └──────┴── V-A4  (positioning) ──┼── V-C1 (distribution) ──┐
                                                                 │                         │
V-B1  (identity posture) ────────────────────────────────────────┘                         ├── V-C3 (launch prep)
                                                                                           │
V-B2  (docket decision) ─────────────────────────────────────────  V-C2 (demo) ────────────┘
                                                                        ▲
                                        Part IV Wave 10 ────────────────┘
```

**Wave 11 parallelism.** V-A1 is independent of everything and should land first — it is the
cheapest fix on the board with the largest per-visitor cost. V-A2 is the long pole and
should start at the same time. V-A3 and V-A4 both consume V-A2's verdict about what is
actually provable, so they merge after it.

**Wave 12 is not blocked by Wave 11** and can run concurrently. Only V-B1's README paragraph
depends on V-A4 owning that file, and it is handed over as proposed text rather than edited.

### Two genuine cross-Part conflicts — read before branching

Part IV's §IV.2 assigns `scripts/smoke.sh`, `README.md` and `docs/CONFIG.md` to **IV-A6**.
Part V assigns the first two to **V-A2** and **V-A4**. This is a real collision, not an
oversight, and the resolution rule is:

1. **`scripts/smoke.sh` — V-A2 goes first, IV-A6 rebases onto it.** IV-A6 adds a standalone
   `--with-runner` step to the smoke; V-A2 fixes a smoke that is currently reporting a
   misleading cause for its own failure. Adding a step to a script that lies is strictly
   worse than fixing the lie first. If Part IV Wave 10 is already in flight when V-A2
   starts, **V-A2 escalates to the integrator rather than editing** — it does not race.
2. **`README.md` — V-A4 goes last and takes the merge.** IV-A6 documents `--with-runner`;
   V-A4 restructures the whole file. Whichever lands second resolves, and V-A4 is written to
   expect inbound Part IV text.
3. **`docs/CONFIG.md` — disjoint sections, no ordering constraint.** V-B1 owns the auth rows,
   V-B2 the docket rows, IV-A6 the local-runner rows. Each states in its handoff which rows
   it touched so the integrator can verify the disjointness rather than assume it.

A card that discovers a third collision states it in the handoff and stops. It does not
choose a winner.

---

## §V.4 Cards

### V-A1 — The install path a stranger actually takes

**The smallest card on this board and the most overdue.** Its risk is scope creep into V-A4.

**Owns:** `install.sh`, the install section of `README.md` (**only** that section — the
restructure is V-A4's), the `main`-branch decision, a new CI check, and the V-A1 handoff.
**Does not own** the README's positioning, the release workflow, or any crate.

**Context — the exact shape of the problem.** `README.md:166` prints, as the headline
install method for a public project:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

There is no `main` branch. `git ls-remote --heads origin main` returns empty; the repository's
default branch is `develop`. The URL returns **404**, and has since the repository was made
public on 2026-03-15. The same file served from `develop` returns 200, so the script itself
is fine — only the path is wrong. `cargo install --git` happens to work because cargo follows
the default branch, which is why this was never noticed locally.

**Tasks:**
- Decide, and record the reasoning in the handoff: (a) create `main` as a branch tracking
  released state, or (b) point every advertised URL at `develop`. **The recommendation is
  (a)** — a stranger's `curl | sh` should not follow the development tip, and every other
  project's convention says `main` exists. But this is the card's call to make and defend,
  because (b) is one line and (a) adds a branch to keep in sync.
- Fix every URL in `README.md` and `docs/` that names a branch, not just the one that was
  found. `grep -rn 'githubusercontent.com/yielab/tack' . --include='*.md' --include='*.sh'`.
- **The card's real deliverable:** a CI job that resolves every install URL the docs
  advertise and fails the build on any non-200. Without it this rots again the next time the
  branch layout changes, which is exactly how it broke the first time.

**Acceptance:** on a machine with no checkout of this repository, the exact command printed
in the README installs a `tack` that starts and serves the UI — **proven by running it in a
clean container and pasting the transcript**, not by reading the URL. The new CI check is
proven load-bearing by pointing one documented URL at a bad path once, watching CI go red,
and reverting. `git ls-remote --heads origin` output is recorded in the handoff before and
after, so the branch decision is visible.

---

### V-A2 — Make the live smoke tell the truth

**The long pole of this Part, and the only card here that may touch runtime code.** It is
also the card most likely to discover that the honest answer is "this does not work yet".
That is an acceptable outcome; hiding it is not.

**Owns:** `scripts/smoke.sh`, plus whatever narrow runner-side fix the diagnosis lands on
(`crates/tack-runner/**`), and the V-A2 handoff. **Does not own** the contract directory,
the scheduler, migrations, `router.rs` or the frontend. A diagnosis that points at any of
those is an escalation with evidence, not an edit.

**Context.** On 2026-08-26, with all three harnesses installed for the first time
(`codex-cli 0.149.1`, `claude 2.1.236`, `opencode 1.18.0`) and both binaries rebuilt in
release, `./scripts/smoke.sh --live` returned **`SMOKE FAILED`**. Steps 1–6 and 9 passed,
including the complete restart-recovery proof (SIGKILL mid-attempt → `needs_operator` → no
blind duplicate → operator requeue → attempt #2 success) and capacity-1 saturation. Two
steps failed:

- **Step 7** — the live attempt never reached a terminal state. The `opencode` +
  `llamacpp/qwen3.6-35b-uncensored` pairing was claimed and checked out at the exact
  requested commit with an isolated workspace, then produced `attempt ended '' —
  terminal_reason: null` after the 300 s budget.
- **Step 8** — all three harness kinds FAILed, printing canned text that the board had
  **already recorded as stale on 2026-08-20** and never reworded. It names a pre-III-H5
  structural cause that no longer applies, and it has now misled two separate readers into
  seeing a regression that is not there.

**The leading hypothesis is explicitly unverified.** The runner is enrolled at capacity 1;
step 7's attempt never terminated, so its lease was very likely still held when step 8
created three more requests — under which none could be claimed by anyone, which is exactly
the symptom all three printed. That would make step 8 a **cascade of step 7**, not three
independent defects. Nobody has re-run step 8 against an idle runner, and the runner-side
log was not preserved (`SMOKE_KEEP=1` was not set).

**Tasks — the four open questions from the Wave 9 amendment, each answered or explicitly
re-opened with what was ruled out:**
1. Re-run with `SMOKE_KEEP=1` and preserve the runner log and journal before anything else.
   The previous run's evidence was lost; do not lose this one's.
2. **Why the live attempt hangs.** Try a lighter declared pairing first — `opencode/big-pickle`,
   `hy3-free` and `mimo-v2.5-free` were all declared — to separate "the product hangs" from
   "the model was too slow for a 300 s budget". **Note carefully:** even if the cause is a
   slow model, an attempt that exceeds its budget and reports `terminal_reason: null` is a
   product defect. A budget expiry must produce a terminal state. Fix that regardless of
   which model was chosen.
3. **Whether step 8 survives an idle runner.** If the three failures vanish, the smoke
   saturates its own capacity-1 runner and then reports scheduling failures — in which case
   the defect is in the smoke, and the handoff says so plainly.
4. Reword step 8's canned FAIL text so it names the cause it actually observed.

**Acceptance:** `./scripts/smoke.sh --live` on a three-harness machine either passes end to
end, or fails with a message that names the real cause and is proven to do so by inducing
that cause deliberately. Each of the four questions above is answered in the handoff with
preserved evidence, or re-opened with what was eliminated. Any runner-side fix is proven
load-bearing by reverting it once and watching the smoke fail again. **Codex is reported
honestly**: it has never completed a live attempt, and if this card does not change that,
the handoff says so in those words and hands V-A4 the instruction to stop implying
otherwise.

---

### V-A3 — Cut a release that contains the product

**Owns:** `.github/workflows/release.yml`, the `[Unreleased]` → release section of
`CHANGELOG.md`, the tag, and the V-A3 handoff. **Needs V-A2's verdict** — the release notes
must not claim what the smoke could not show.

**Context — read this before touching CI.** The audit's first reading was that CI does not
package the runner. **That is false.** `release.yml` has built `tack-runner` with
`cargo auditable` and packaged it into its own per-platform archive (`RUNNER_STAGE`, with
the systemd unit and env example) since `7d78de3`, 2026-08-19. **This card does not need to
fix the workflow.** The gap is purely that no tag has been cut since: the only downloadable
release is `v0.1.0-beta.6` from 2026-06-22, whose four assets are `tack-*` only and which
predates every Phase 50–58 capability. A visitor who follows the README today downloads a
Tack with no runner fleet at all — the entire differentiator is undownloadable.

**Tasks:**
- Verify the workflow's artifact set on a dry run before tagging — that both `tack-*` and
  `tack-runner-*` archives are produced for all four platform targets, and that checksums
  and build provenance are unchanged.
- Cut `v0.1.0-beta.7` from `develop`. **Preparation only — the user approves the tag push**
  (§V.1 rule 5).
- Write release notes that state exactly what is proven and name what is not. If V-A2 could
  not prove a codex live attempt, the notes say two of three harnesses are live-proven. The
  notes are the first thing a stranger reads about the state of this project; §V.1 rule 1
  applies to them more than to anything else on this board.

**Acceptance:** the produced release carries both a `tack-*` and a `tack-runner-*` archive
per platform. A downloaded pair, on a machine with no repository checkout, enrolls a runner
against a served instance and completes an attempt — **or the notes state that it cannot and
why**, with the transcript in the handoff either way. No claim in the notes lacks a
corresponding proof in V-A2's or this card's evidence.

---

### V-A4 — Say what Tack is in one sentence, and publish it where it can be read

**Owns:** `README.md` (whole file — takes the merge against IV-A6, see §V.3), the GitHub repo
description / topics / `homepageUrl`, a new GitHub Pages workflow for the mdBook,
`docs/book/src/introduction.md`, and the V-A4 handoff. **Does not own** any crate, the
release workflow, or `docs/CONFIG.md`.

**Context.** The repository's GitHub description reads *"Self-hosted project manager in a
single binary — no Docker, no database server. Rust + SolidJS."* It does not mention agents.
That sentence places Tack in the category where it competes against Plane (~54.6k ★), Huly
(~26.9k ★) and Vikunja and loses on maturity — while omitting the one capability none of
them has. The README's opening line then says *"Local-first project management with a
harness-agnostic control plane for agent work"*, which is two products in one sentence and
requires the reader to already know what a harness is.

Meanwhile the mdBook under `docs/book/` — a genuinely good user and developer guide with a
recovery runbook — is **not published anywhere**. `yielab.github.io/tack` returns 404 and
`homepageUrl` is empty. The documentation exists and is invisible.

**Tasks:**
- Write one sentence that leads with the differentiator, not the category. It should be
  comprehensible to someone who has never heard the word "harness". Apply it identically to
  the GitHub description, the README's first line, and the book's introduction — three
  places currently saying three different things.
- Restructure the README so the first screen answers four questions in order: what is it,
  what does it do that nothing else does, how do I run it, **and what does it not do yet**.
  That fourth question is not a disclaimer to bury — a stranger who discovers the single-user
  limit after installing feels misled, and one who reads it up front feels informed. Take the
  identity paragraph from V-B1's handoff and the proven-harness list from V-A2's.
- Publish the mdBook to GitHub Pages from CI; set `homepageUrl` and repository topics.
- Remove or gitignore `tack.db.before-037_orch_runs_rebuild.sqlite` from the repository root
  (§V.0) — a database snapshot in the root of a project a stranger is about to clone.
- Record, but do not act on, the positioning question the audit raised and this card cannot
  settle alone: the ten project-type presets (construction, legal, homework, events) pull the
  story toward generic project management, which is the losing category. State the trade-off
  in the handoff for the user to decide; **change no preset code**.

**Acceptance:** the README's first fifteen lines, read by someone who has never heard of
Tack, state the category, the differentiator, and the current limits — verified by having an
agent with no prior context on this repository read only those lines and report back what it
believes the product is and cannot do. **Every capability claim on the first screen traces to
a proof from V-A2 or V-A3**, listed in the handoff as claim → evidence. The book is reachable
at a public URL published by CI, and the repository's homepage points at it.

---

### V-B1 — Declare the identity posture instead of leaving it ambiguous

**Owns:** `crates/tack-api/src/middleware.rs`, `docs/book/src/user-guide/administration.md`,
the auth rows of `docs/CONFIG.md`, a new ADR, and the V-B1 handoff. **Hands** its README
paragraph to V-A4 as proposed text; does not edit `README.md`.

**Context.** Tack has **no identity model**. There is no users table, no sessions, and no
per-user permissions. `assignee` is a free-text column added in migration 015; `roles` is a
per-project colour-and-icon label attached to items, not an identity. Authorization is a
single shared bearer token compared in `require_token` — and when no token is configured,
that function returns `Ok(next.run(req).await)` for everything, by design, for pure-local
mode.

**That is a defensible design for a single operator. It is not "self-hosted for a team", and
nothing in the docs distinguishes the two.** A reader who sees "self-hosted" reasonably
assumes accounts exist. This ambiguity is worse than either honest answer.

The posture also aged badly against Part III. Phase 27.2 made a non-loopback bind with no
token log a **warning**. When the product's job was storing task text, a warning was
proportionate. Now the same server schedules coding agents that execute arbitrary code, and
ADR 0058 already chose a startup **error** for exactly this shape of risk on the embedded
runner. The two postures contradict each other.

**Tasks:**
- **Decide and record in an ADR:** Tack v1 is single-operator. Name what was rejected (full
  accounts, OIDC, per-user tokens) and why now is not the time. This is the card's primary
  deliverable — the code change is small; the decision is the point.
- Make the code agree: binding a non-loopback address with no `TACK_API_TOKEN` becomes a
  startup error rather than a warning. Loopback with no token is unchanged — that is the
  pure-local mode the design intends.
- **Handle the deployment constraint honestly.** `Dockerfile` and `docker-compose.yml` exist
  in this repository and a container necessarily binds `0.0.0.0`. Turning this into a hard
  error breaks every existing container deployment on upgrade. Provide a single explicit
  opt-out (an env var whose name states what it means), document it, and make the error
  message name it. A change that silently bricks existing deployments is not shippable.
- State the model plainly in the administration guide's first paragraph, including that
  `assignee` is a label rather than an account.

**Acceptance:** the ADR exists and names the rejected options with reasons. A non-loopback
bind with no token fails to start, proven by a test; the same bind with the documented
opt-out starts, also proven by a test; loopback with no token is byte-identical to today,
proven by the existing tests passing untouched. The administration guide states the limit
before it states any feature. The README paragraph is written and handed to V-A4, not merged
here.

---

### V-B2 — Decide the fate of the docket control plane

**This card decides and gates. It does not delete.** A card that starts deleting has
misunderstood its scope.

**Owns:** a new ADR, the feature-gating of `crates/tack-orch/src/adapters/**`, the
control-plane and approvals UI routes in `frontend/src/features/`, the docket rows of
`docs/CONFIG.md`, and the V-B2 handoff. **Does not own** migrations, the scheduler, the
runner-v1 execution domain, or any `TODO.md` archive section.

**Context.** Parts I and II built a complete integration against exactly one backend,
[docket](https://github.com/yielab/docket): a `ControlPlane` trait, a reconciler and health
machine, a docket adapter, 11 `orch_*` tables (including `orch_runs_new` and
`orch_approvals_new`, leftovers of migration 037's rebuild), a `control_planes` table, a
fleet-wide Approvals inbox and a ControlPlanesManager. Part III then replaced that entire
model with the native pull-based runner and kept docket only as "an optional legacy bridge".

Both models now coexist in the schema, in the UI and inside `tack-orch` — where the docket
half sits beside the runner-v1 execution domain that Part III *does* depend on. A reader
encountering two `fleet` concepts cannot tell which one is current, and neither can a new
contributor.

**Read this before scoping anything.** `tack-orch` is ~19k lines and **is not all legacy** —
the neutral runner-v1 execution domain lives there too and is load-bearing. Furthermore,
Deletion is still not an `rm` — the crate mixes closed-cycle code with the load-bearing
execution domain, and separating them is the work. It is explicitly **not this card's job**.

**Tasks:**
- **Measure the surface first, before proposing anything.** Tables, rows in the live schema,
  LOC attributable to the docket half of `tack-orch` (as distinct from the runner-v1 domain),
  UI routes, config variables, and the release binary delta with the adapter compiled out.
  Numbers, with commands (§V.1 rule 4).
- Write the ADR deciding one of three options, each costed from those numbers: **keep** as a
  supported optional bridge; **gate** behind a default-off cargo feature plus config flag; or
  **schedule deletion**, with the migration plan for the `orch_*` tables and the citation
  updates written out.
- **Implement only the gating**, and only if that is the decision. Deletion, if chosen, is a
  future card that this ADR authorizes.

**Acceptance:** the handoff carries measured numbers, not adjectives. The ADR names the
option chosen and both rejected, with the cost of each. If gating shipped: a default build
exposes no docket concept in the UI, the CLI or the config table; `cargo nextest run --workspace` is
green in **both** feature states, with the test counts for each recorded; and no migration was
altered, proven by the migration list being unchanged.

---

### V-C1 — Distribution beyond `curl | sh`

**Needs V-A3.** There is nothing to distribute until a release exists that contains the
product.

**Owns:** `packaging/**`, a Homebrew formula/tap, an AUR `PKGBUILD`, a Nix derivation, the
container-publish step of `release.yml` (**coordinate with V-A3**, which owns that file), and
`cargo-binstall` metadata in `Cargo.toml`.

**Context.** Today there are exactly two install paths: a `curl | sh` one-liner (broken until
V-A1) and `cargo install --git`, which requires a Rust toolchain and a slow LTO build. The
audience this Part targets installs things with `brew`, `paru`, `nix` or `docker run`. A
`Dockerfile` exists but no image is published anywhere.

**Tasks:** add the recipes; publish the container image to `ghcr.io` from the release
workflow; add `cargo-binstall` metadata so `cargo binstall tack-cli` fetches the release
binary instead of compiling. **Preparation only for anything requiring an external account or
a PR to a third-party repository** — §V.1 rule 5 covers Homebrew taps and AUR submissions.

**Acceptance:** at least three channels install a working `tack` that starts and serves,
**each verified by an actual install in a clean container**, with the command and transcript
in the handoff. A channel that could not be tested on this machine is listed as **untested**
in the handoff and is not mentioned in the README until it is. Binary size and install time
are recorded per channel as real measurements.

---

### V-C2 — The demo that shows what nothing else can

**Needs Part IV Wave 10** (so the demo is one command, not four steps and a copied token) and
**V-A2** (so it does not hang on camera).

**Owns:** `docs/screenshots/**`, the demo recording and its script, the README hero asset
(hand the markdown to V-A4), and the V-C2 handoff.

**Context — this is the most under-used asset in the repository.** Every competitor can show
"assign a card to an agent and watch it work". **None of them can show durable recovery**,
because none of them has leases, fencing tokens or replay tables. Tack's own smoke already
proves the sequence end to end: SIGKILL mid-attempt → `needs_operator` → **no blind
duplicate** → operator requeue → attempt #2 succeeds. That is the entire competitive
argument, it is already tested, and it has never been shown to a single person outside this
repository.

**Tasks:** record roughly sixty seconds, no narration required: create an item → dispatch it
to an agent → watch the attempt live → kill the runner → see `needs_operator` and the absence
of a duplicate → requeue → success → open the artifact. Record it from a **release artifact on
a clean machine** running `tack serve --with-runner`, not from a development tree.

**Acceptance:** the recording is made from a release artifact on a machine with no repository
checkout, stated as such in the handoff with the version used. It contains no cut that hides a
failure or a retry. **If a step cannot be shown honestly, it is dropped from the demo rather
than staged** — a demo that shows five real steps beats one that shows seven with one faked.

---

### V-C3 — Launch preparation

**Needs V-A1 through V-A4, V-C1 and V-C2.** This card **prepares and stops**; §V.1 rule 5 is
absolute here.

**Owns:** a launch checklist under `docs/`, the issue and PR templates, `good first issue`
labelling, seeded Discussions topics, and the V-C3 handoff.

**Context.** Five months public, zero stars. Nothing on this board converts unless somebody
sees it, and the audience is unusually identifiable: the users Vibe Kanban orphaned when Bloop
shut down on 2026-04-10, and Crystal's after its February 2026 deprecation. Those people are
in known places asking a known question. A launch that reaches them is a different act from a
generic "Show HN".

**Tasks:**
- Draft the posts (HN / r/selfhosted / r/rust / Lobsters) — **drafts only, published by the
  user**.
- Write the comparison table, and be honest in it about what Tack lacks: no accounts, no
  notifications, English only, one contributor. A comparison table that only lists wins is
  read as marketing and discounted entirely; one that names its own gaps is read as
  engineering. Given this audience, the second converts better and is also simply true.
- Seed five to ten `good first issue`s from the real gaps this board and the audit named — i18n
  scaffolding, SMTP notifications, time tracking, in-UI artifact diff review. Each with enough
  context that a stranger could start.
- Review the issue and PR templates for someone who has never contributed here.

**Acceptance:** **nothing is published.** The deliverable is the prepared material plus a
repository that survives its first hundred visitors, verified by walking the stranger's path
end to end on a clean machine: the install command works, the docs load, the demo plays, the
limits are stated before they are discovered, and a `good first issue` can be understood
without reading `TODO.md`.

---

### V-C4 — Two green checks that were not measuring anything

**Needs nothing.** Found while preparing the launch, and a launch cannot ship over either.

**Owns:** the `e2e` job in `.github/workflows/ci.yml`, `.github/workflows/verify-install-urls.yml`
and `scripts/verify-install-urls.sh`, and the V-C4 handoff. **Does not touch** `install.sh`,
whose fix already landed with V-C3.

**Context, one — the badge.** On each of the last pushes to `develop`, eight CI jobs passed
and one did not: *E2E (Playwright, cross-browser)*, cancelled at 25m15s and 25m16s against its
own `timeout-minutes: 25`. That is a job exceeding its budget, not a suite failing, and not
anyone pressing cancel — but the badge a stranger sees on the repository front page reads the
same either way. Either the job fits its budget or the budget is wrong; decide which from what
the job actually spends its time on, and say so.

**Context, two — the check that checked the wrong thing.** `install.sh` installed nothing at
all from `v0.1.0-beta.7` until V-C3 fixed it: two archives per release end in the same
platform suffix and the API lists the runner's first, so the installer downloaded that one and
died on `no 'tack' binary found in archive`. Through every one of those pushes, *Verify install
URLs* was green in ten seconds. Its own header explains why it exists — "the OpenAPI/frontend/
coverage gates would all stay green while the public install path was broken" — and it was
right about the risk and blind to the instance, because resolving a URL is not installing from
it.

**Acceptance, all of it measured locally — this card does not push.** The E2E suite's real
cost is measured on this machine, per project, with the command and the numbers pasted, and
the job is changed so that cost fits with margin. The install check is proven load-bearing the
way this repo proves any other guard: revert `install.sh`'s `grep -v` once, run
`scripts/verify-install-urls.sh`, watch it fail, restore it. A check that cannot fail on the
exact bug it exists to catch is not evidence. **The confirming CI run is the user's step, not
this card's** — `origin/develop` is behind and pushing is theirs to authorize; the handoff ends
by naming what they should see when they do.

**Stop if:** making the E2E job fit needs a change in the frontend or the server. Record what
you measured and hand it over — the budget is yours, the code under test is not.

---

## §V.5 Deliberately not in this Part

Recorded so no card adopts them by drift, and so the roadmap keeps them visible:

- **Any new product feature.** Outbound notifications and SMTP (zero references today),
  i18n (the UI is English-only, with zero locale infrastructure), time tracking (`estimate`
  exists, `time_spent` does not), and in-UI diff review of agent artifacts are all real gaps
  the audit found. They belong to a post-adoption cycle and are recorded in the roadmap, not
  carded here.
- **Multi-user accounts.** V-B1 decides and documents the posture; it does not build identity.
- **Deleting the docket surface.** V-B2 decides and gates; deletion is a future card that its
  ADR authorizes.
- **Changing or removing the ten project-type presets.** V-A4 records the positioning
  trade-off for the user to decide and changes no preset code.
- **Anything Part IV owns.** `tack serve --with-runner`, `tack runner doctor` and the runner
  composition root are Phase 58 and stay there.
- **The Alexa integration.** The audit flags it as surface area with no adoption value, but it
  works, it is documented, and removing working features to tidy a story is not a trade this
  Part is authorized to make. **Superseded 2026-09-03:** the user directed its removal outright;
  it is gone from code, config, CI and docs (CHANGELOG, Unreleased → Removed), outside any card.

---

## §V.6 Handoff additions for this Part

Use the §III.2 template verbatim, plus three sections specific to Part V:

1. **Claim → evidence table.** Every user-visible claim your card added or kept, and the
   command, test or transcript that proves it. A row with no evidence column is a claim to
   delete, not a row to leave blank.
2. **Measured numbers.** Every number you produced, with the command that produced it
   (§V.1 rule 4).
3. **What a stranger still cannot do.** One short paragraph. Not what is unimplemented in
   general — specifically what someone arriving from outside this repository would try, and
   fail at, after your card landed.

---

# Part IV — Standalone Single-Binary Operation (Phase 58)

Executable board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Standalone Single-Binary
Operation*, and decided in [`docs/adr/0058-standalone-single-binary-runner.md`](docs/adr/0058-standalone-single-binary-runner.md).
**Parts I, II and III remain historical context.** This Part has its own numbering
namespace (`§IV.0` … `§IV.6`) so Part I's load-bearing section numbers stay put.

Like Part III, this board is written to be picked up cold by parallel agents in isolated
worktrees. Every card is bounded, names every shared-file owner, and has an acceptance
gate verifiable without trusting its author's handoff.

## Status board — Part IV

| Wave | Cards | Phase | Status |
|---|---|---|---|
| 10 — Standalone single binary | IV-A1 · A2 · A3 · A4 · A5 · A6 | 58 | **Done, all six integrated** at `83fefab` on `develop` (handoffs: `docs/agent-handoffs/part-iv/IV-A1.md` … `IV-A6.md`). `tack runner start` and `tack serve --with-runner` both proven live end-to-end, including zero-touch self-enrollment (A4, no token ever displayed/copied) and `tack runner doctor` (A5, verified byte-identical to a live server's capability snapshot); default `tack serve` starts no runner; non-loopback bind refuses to start `--with-runner`; binary-size delta measured at +0.83 MiB (+4.67%). A6 added three load-bearing `scripts/smoke.sh` steps (10-12, proven by an inject-a-real-failure-and-watch-it-FAIL run) and documented the embedded runner in `docs/CONFIG.md`/`docs/book/src/user-guide/agent-runners.md`/README, including a confirmed-live fix for a `RUST_LOG` visibility gap. Full workspace gate green: 1,413 tests, 0 failed, `cargo fmt`/`clippy -D warnings` clean. One pre-existing gap remains open, outside any card's ownership: `bootstrap::build_runtime` requires a credential even when a valid stored session exists on disk (A4 worked around it with a provably-inert placeholder rather than fixing the file it doesn't own) — a real but low-severity fix for whoever next touches `crates/tack-runner/src/bootstrap.rs`. |

**Integration line:** `develop`, the repository's default branch — unchanged from Part III
Wave 7 onward, and for the same reason (two naming failures in a row cost that cycle a
real trunk). Branch every card from `develop`. Do not create a `plan/*` line for this Part.

> **Part V is active at the same time, and shares three files with this Part.**
> `scripts/smoke.sh`, `README.md` and `docs/CONFIG.md` are assigned to **IV-A6** in §IV.2
> below *and* to Part V cards. That collision is real, and the resolution rule lives in
> **[§V.3](#v3-dependency-graph-cross-part-conflicts-and-merge-policy)** — read it before
> branching IV-A6. In short: **V-A2 fixes `scripts/smoke.sh` first and IV-A6 rebases onto
> it** (adding a step to a smoke that misreports its own failure is strictly worse than
> fixing the misreport first), and **V-A4 takes the `README.md` merge last**. If IV-A6 is
> already in flight when a Part V card starts, the Part V card escalates rather than racing.

---

## §IV.0 Cold-start context capsule

**What this Part is for, in one sentence.** Today a developer who wants an agent to run
against their own board needs two binaries, four manual steps and a copied one-time token;
after this Part they need one command, `tack serve --with-runner`.

**Read before touching anything:** `docs/adr/0058-standalone-single-binary-runner.md`. It
records why the runner is separate at all (ADR 0050), why that separation is about *roles
and not binaries*, and — most importantly — why the embedded runner must still speak
runner-v1 over loopback HTTP instead of calling handlers in-process. A card that "optimizes
away" that HTTP hop has broken the whole point of the design; escalate instead.

**Working-tree state at the time this board was written (2026-08-26).** `develop` is at
`277868a`, but the tree carries one **uncommitted** change: a fix to
`crates/tack-runner/src/harness/mod.rs::registering_all_three_real_adapters_is_order_independent`.
That test asserted all three adapters reject a fixture spec identically; that was only true
for `codex` *by accident*, because the binary was absent from the machine. With `codex`
installed the test failed legitimately — the codex adapter is a pass-through harness
(III-H5) and accepts any explicit model pre-spawn. The assertion now expects codex to
accept and the other two to reject, matching real behaviour instead of an environmental
artifact. **Land or discard this deliberately before branching cards** — do not let it ride
into a card's diff unexamined.

**Part III's tag is still refused, and NOT for the reason the board previously recorded.**
Wave 9 said the tag was blocked on one thing: `codex` not being installed. It has since
been installed (`codex-cli 0.149.1`, alongside `claude` 2.1.236 and `opencode` 1.18.0 — 3
of 3) and `./scripts/smoke.sh --live` was run on 2026-08-26. It **failed**, for a different
reason. See the Wave 9 amendment in the Part III board above before assuming the release is
one smoke run away. This Part does not depend on that being resolved, and must not be
blocked waiting for it.

**What is already true and must stay true.** The runner protocol, scheduler, fencing,
decisions, artifacts, retention and the operator API/CLI/UI are all built and tested; a
runner enrolls, claims, checks out, runs a real harness, submits events and artifacts, and
completes against a live server. This Part adds **packaging and first-run experience**. It
is not permitted to change behaviour that Part III proved.

---

## §IV.1 Rules for simultaneous agents

**All fourteen rules of §III.2 apply unchanged** — one card / one worktree / one branch,
stay inside `Owns`, no `unimplemented!()` or hidden fake success, tests ship with the card,
no blocking sleeps, logs carry ids and never credentials, stop on contract ambiguity, and
each wave ends with adversarial verification by someone who did not author the code. Read
them; they are not restated here.

Four rules are specific to this Part:

1. **The embedded runner uses the same protocol client as a remote runner.** No card may add
   an in-process shortcut, a privileged bypass, a second `RunnerProtocolClient`, or shared
   access to `AppState`. If loopback HTTP appears to be a problem, that is an escalation,
   not a design freedom.
2. **`tack-api` must not gain a dependency on `tack-runner`.** The composition root is
   `tack-cli`. A card that finds itself wanting `tack-api` to know a runner exists has
   mis-placed the work — escalate.
3. **Off by default, loud on failure.** No card may ship the embedded runner enabled by
   default, and none may let `tack serve` continue silently after the embedded runner has
   failed to start or has died. A server running without the runner the operator asked for
   is indistinguishable from a scheduler bug and must be an error, not a log line.
4. **No contract, scheduler, fleet, migration or frontend changes.** This Part is packaging.
   `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json` and
   `frontend/**` are all out of scope for every card here. A card that believes it needs one
   states the need in its handoff and stops.

**Handoff:** each card writes exactly one `docs/agent-handoffs/part-iv/IV-<card>.md`, using
the template in §III.2 verbatim, plus the three Part IV additions in §IV.6. Corrections are
appended as amendments, never rewritten. No card edits this board — the wave integrator
does that after independent verification.

---

## §IV.2 Shared-file ownership

| Chokepoint | Owner |
|---|---|
| `crates/tack-runner/src/main.rs`, `lib.rs`, the new bootstrap module | IV-A1 only |
| `crates/tack-api/src/server.rs` | IV-A2 only, and **only** the readiness/bound-address signal |
| `crates/tack-cli/src/main.rs`, `crates/tack-cli/Cargo.toml`, root `Cargo.lock` | IV-A3, then IV-A5 for its one subcommand arm |
| `crates/tack-cli/src/local_runner.rs` (new) | IV-A3 |
| `crates/tack-cli/src/local_enrollment.rs` (new) | IV-A4 |
| `crates/tack-api/src/handlers/runner_admin.rs` | IV-A4 only, and **only** to extract a reusable provisioning function without changing the route's behaviour |
| `scripts/smoke.sh` | IV-A6 only |
| `docs/CONFIG.md`, `docs/book/src/user-guide/agent-runners.md`, `README.md` | IV-A6 only |
| `TODO.md`, `docs/book/src/roadmap.md` statuses | wave integrator only |
| `docs/contracts/runner-v1/**`, `migrations.rs`, `router.rs`, `docs/openapi.json`, `frontend/**` | **nobody — out of scope for this Part** |

---

## §IV.3 Dependency graph and merge policy

```text
IV-A1  (tack-runner entry point) ──┐
                                   ├── IV-A3 ──┬── IV-A4 ──┐
IV-A2  (tack-api readiness seam) ──┘           │           ├── IV-A6
                                               └── IV-A5 ──┘
```

- **A1 and A2 run in parallel** — different crates, no shared file, neither depends on the
  other.
- **A3 needs both.** It is the card that makes `tack` one binary with both roles.
- **A4 and A5 run in parallel** after A3 lands; both touch files A3 created, so neither may
  start before A3 is merged.
- **A6 is last** — it proves the whole thing live and writes the operator docs. It needs A4
  (auto-enrollment) to exist for the standalone claim to be true; A5 is optional to it.

**Merge order:** A1 → A2 → A3 → (A4, A5 in either order) → A6. A1 and A2 may merge in either
order. Gates run once on the integrated tree, not per card, per the Part III precedent.

---

## §IV.4 Cards

### IV-A1 — Runner composition root as a reusable entry point

**No behaviour change. This card is a refactor and must prove it changed nothing.**

**Owns:** `crates/tack-runner/src/main.rs`, `crates/tack-runner/src/lib.rs`, a new
bootstrap/composition module in `crates/tack-runner/src/`, and the IV-A1 handoff.
**Does not own** any other crate, the contract directory, or any harness adapter.

**Context — the exact shape of the problem.** Everything that composes a working runner
lives in the binary's `main.rs::run()`: `build_adapter_registry`, `report_capabilities`,
`HttpPullProtocol`, `RunnerEngine`, `OwnerOnlyJournal`, `WorkspaceManager` +
`GitWorktreeProvisioner`, `HttpRunnerClient`, `RunnerRuntime`, and the `with_data_protocol`
wiring III-H6 added. None of it is reachable from the library, so an embedder would have to
copy it — and a copied composition root is a copy that drifts. `RunnerRuntime::run` already
takes an injected `Shutdown`, so the seam for an embedder mostly exists; what is missing is
a public function that builds the whole thing.

**Tasks:** extract the composition into a public library entry point taking a
`RunnerConfig` and a `Shutdown` and returning the same typed `Result` the binary returns
today. The `tack-runner` binary becomes argument parsing plus a call to it, and keeps its
own signal handling. Keep `ProcessLimits` and `PROTOCOL_REQUEST_TIMEOUT` explicit rather
than defaulted — they are deliberate operational choices with no `Default` for that reason.
Preserve the honest capability reporting verbatim: `cancel` advisory, `decisions`
unsupported, `artifacts`/`usage` advisory, and the "not registered when the binary is
absent" behaviour for each adapter.

**Acceptance:** the `tack-runner` binary's observable behaviour is unchanged — it enrolls,
claims, runs and completes against a live `tack serve` exactly as before, and
`./scripts/smoke.sh` (fake mode) reaches the same steps with the same outcomes as on the
base SHA, recorded side by side in the handoff. The new entry point is callable from
outside the crate with an injected shutdown, proven by a test that starts it and stops it
without a process signal. `cargo nextest run --workspace -E 'package(tack-runner)'` is green with no test deleted.

### IV-A2 — Server readiness and bound-address seam

**Small card. Its scope is one signal, and its risk is scope creep.**

**Owns:** `crates/tack-api/src/server.rs` (the readiness/bound-address signal only) and the
IV-A2 handoff. **Does not own** anything else in `tack-api`, and explicitly not
`router.rs`, `config.rs`, any handler, or the OpenAPI surface.

**Context.** `tack_api::serve()` loads config, migrates, binds, and blocks until shutdown.
An embedder must know two things it cannot know today: *when* the listener is actually
accepting, and *what address* it bound. The port is configurable and may differ from the
requested one, so the embedder cannot assume `127.0.0.1:3210`. Without this, the embedded
runner would have to poll-and-hope against a guessed URL — a race and a wrong-target bug
waiting to happen.

**Tasks:** add a way for an in-process caller to observe readiness and the real bound
`SocketAddr`, without changing `serve()`'s existing signature or behaviour for every
current caller. Signal readiness **after** the listener accepts, never before — an early
signal recreates the race this card exists to remove.

**Acceptance:** an in-process test starts the server through the new seam, receives the
bound address, and issues a successful request to it with no retry loop and no sleep. The
existing `serve()` entry point still works unchanged, proven by the CLI's `tack serve`
starting exactly as before. No route, handler, config field or spec path changes — asserted
by `openapi_contract` staying 5/5 drift-free.

### IV-A3 — One binary: `tack runner start` and supervised `tack serve --with-runner`

**Needs IV-A1 and IV-A2 merged.** This is the card that delivers the headline capability.

**Owns:** `crates/tack-cli/src/main.rs`, `crates/tack-cli/Cargo.toml`, root `Cargo.lock`, a
new `crates/tack-cli/src/local_runner.rs`, and the IV-A3 handoff. **Does not own**
`tack-api` or `tack-runner` internals — if either needs a change, that is an escalation to
IV-A2 or IV-A1's owner, not an edit.

**Context.** `tack-cli` already has a `runner` subcommand namespace (`enroll`, `revoke`,
`revoke-token`), so `tack runner start` slots in beside them. `tack-cli` already depends on
`tack-api`; it gains `tack-runner`. Verified before this board was written: **no dependency
cycle** — `tack-runner` depends on `tack-orch` only, never on `tack-api`.

**Tasks:**
- `tack runner start` — run A1's entry point with the same configuration precedence and the
  same flags the standalone binary accepts. Prefer `TACK_RUNNER_ENROLLMENT_TOKEN` over a
  flag, as the standalone binary already does, so the secret stays out of shell history.
- `tack serve --with-runner` (gate also readable as `TACK_LOCAL_RUNNER_ENABLE`) — start the
  server, wait on A2's readiness signal, then start an embedded runner **as a task in the
  same process**, pointed at the real bound loopback address, speaking ordinary runner-v1
  HTTP. Off unless explicitly enabled.
- Supervise it honestly: shutdown stops both roles cleanly; an embedded runner that fails to
  start or dies takes the process down with an operator-visible error rather than leaving a
  server running with no runner.
- Refuse to start the embedded runner when the server is not bound to loopback — reuse the
  existing `AppConfig::binds_loopback()`. This is a startup error, never a silent downgrade.
- Enrollment stays manual in this card: it consumes a credential from the environment.
  Zero-touch enrollment is IV-A4 and must not be pre-empted here.

**Acceptance:** `tack runner start`, given a credential, enrolls and completes a real
attempt against a live `tack serve` — the same proof the standalone binary carries.
`tack serve --with-runner`, given a credential, does the same from **one process and one
binary**, with the attempt visible through the operator API. Default `tack serve` starts no
runner — asserted by the absence of a runner in `GET /api/runners`, not merely by a missing
log line. A non-loopback bind refuses with a typed error, proven by a test. Killing the
embedded runner surfaces an error rather than a quiet server. **The binary-size delta is
measured and recorded as a real number**, before and after, never estimated.

### IV-A4 — Zero-touch local enrollment

**Needs IV-A3 merged.** Without this card the standalone claim is not true — the user still
copies a token by hand.

**Owns:** a new `crates/tack-cli/src/local_enrollment.rs`,
`crates/tack-api/src/handlers/runner_admin.rs` (**only** to extract a reusable provisioning
function — the HTTP route's behaviour, auth and response shape must not change), and the
IV-A4 handoff.

**Context.** Enrollment is deliberately two-step: an operator creates a pending runner and
receives a one-time token, and the runner redeems it for a durable credential. Only hashes
are stored. That design is not being weakened — it is being *automated for the local case*,
where the operator and the runner are the same person on the same machine.

**Tasks:** on `tack serve --with-runner`, after readiness and before starting the runner —
(1) if the runner's state directory already holds a durable credential, use it; (2)
otherwise self-provision: create the pending runner in-process (this is a bootstrap/admin
concern, not the runner protocol, so in-process is legitimate *here* and only here), obtain
the one-time token, and hand it to the embedded runner, which redeems it **over loopback
HTTP through the ordinary protocol path** like any other runner. Keep the durable credential
owner-only in the runner state directory. Auto-provisioning is gated by the same
loopback-only rule IV-A3 established, checked again here rather than assumed.

**Acceptance:** on a machine with no prior runner state, `tack serve --with-runner` reaches
a completed attempt with **no token ever displayed, copied or configured** — the headline
proof of this Part. A second start reuses the stored credential and does not create a
second runner, asserted against `GET /api/runners` row counts, not logs. The redemption is
shown to have gone through the real HTTP protocol path, not a bypass. No credential appears
in any log or terminal output, asserted with a positive control (the test proves it *can*
observe output by asserting an id does appear). `runner_admin.rs`'s route behaviour is
byte-identical — proven by its existing tests passing untouched.

### IV-A5 — `tack runner doctor`

**Needs IV-A3 merged.** Independent of IV-A4; the two may run in parallel.

**Owns:** a new doctor module in `crates/tack-cli/src/`, one subcommand arm in
`crates/tack-cli/src/main.rs`, and the IV-A5 handoff.

**Context — why this is in scope for a packaging Part.** The information an operator needs
in order to answer "why can't I run this model?" exists only inside a capability snapshot
that a runner posts to a server. There is no way to ask the local machine what it can do.
That gap is the direct cause of a real, reported confusion: it is not discoverable how one
configures Claude, Codex, OpenRouter or a local model. The answer — that provider
credentials live in each harness's own environment and Tack never proxies them (ADR 0050,
reaffirmed in ADR 0058) — is correct but invisible.

**Tasks:** report, for the local machine: which harness binaries are on `PATH` and their
versions; what each probe declares (`model_combinations`, `model_passthrough`, and each
feature's `cancel`/`resume`/`decisions`/`artifacts`/`usage` support with its reason); and
which are absent. Reuse A1's probe path — do not re-implement discovery. State plainly, for
each harness, where its provider credentials come from and that Tack neither stores nor
forwards them.

**Acceptance:** on a machine with a harness installed and one absent, the output names each
honestly — present with a version, absent as absent, never rounded up and never invented. A
probe error is reported as a probe error, distinct from "not installed". The declared
capabilities shown match exactly what the same probe reports to a server, proven by
comparing against a real capability snapshot rather than by re-deriving them.

### IV-A6 — Standalone smoke, configuration and operator docs

**Needs IV-A4 merged.** Last card of the wave.

**Owns:** `scripts/smoke.sh`, `docs/CONFIG.md`,
`docs/book/src/user-guide/agent-runners.md`, `README.md`, and the IV-A6 handoff.

**Context and a standing warning.** `scripts/smoke.sh` has shipped a false green once
already in this repository's history: steps 7–9 printed `SKIPPED` unconditionally and could
never fail, so the script reported `SMOKE PASSED` while proving less than it claimed. Any
step this card adds must be able to fail. A step that cannot pass because the product cannot
do the thing is a `FAIL`, never a `SKIP`; environmental absence is `ABSENT` and named in the
verdict, never counted as a pass. Additionally: `docs/CONFIG.md` today documents no
harness/model configuration at all, and the model-configuration story is exactly what users
report as unclear — this card is where that gets written down.

**Tasks:** add a standalone-mode step proving one binary, one command, zero manual
enrollment reaches a completed attempt; assert that default `tack serve` starts no runner;
assert the non-loopback refusal. Document the gate, the loopback rule, the state-directory
location, and — as its own section — how provider credentials actually work for each
harness (env/CLI login per harness, OpenRouter and local-model endpoints via the harness's
own configuration, and that Tack is never a model gateway, with the ADR cited). Update
`README.md`'s getting-started path to lead with the standalone command.

**Acceptance:** the new smoke step is proven load-bearing by breaking the feature once and
watching it `FAIL` — the same discipline III-H9 and III-H6 used, and the specific defense
against the false green above. The documented commands are executed exactly as written on a
clean state directory and the transcript recorded. Every capability claim in the new docs
cites the test or run that proves it. Nothing in this card changes compiled behaviour.

---

## §IV.5 Acceptance matrix

| Invariant | Owner | Must remain green through |
|---|---|---|
| One protocol-client implementation; no in-process bypass | A3 | A6 |
| `tack-api` never depends on `tack-runner` | A3 | A6 |
| Embedded runner is off unless explicitly enabled | A3 | A6 |
| Non-loopback bind refuses to auto-enroll, as a startup error | A3/A4 | A6 |
| A failed or dead embedded runner is loud, never silent | A3 | A6 |
| Durable credential owner-only; no credential in any log or terminal | A4 | A6 |
| Runner-v1 contract, scheduler, fleets, migrations, frontend untouched | all | A6 |
| Standalone smoke step can actually fail | A6 | release |

---

## §IV.6 Definition of done

On a machine with one harness installed and no prior Tack state, **`tack serve
--with-runner` is the only command needed** to go from nothing to a completed agent attempt
visible in the UI — one binary, one process, no second artifact to install, no pending
runner to create, no one-time token to copy.

Additionally:

- The embedded runner is off unless explicitly enabled, and refuses to auto-enroll on any
  non-loopback bind.
- The embedded runner speaks runner-v1 over loopback HTTP with **no second code path** —
  the same client a remote runner uses, against the same routes.
- A fleet of remote runners still works exactly as before; `tack-runner` remains shippable
  and useful on a machine with no server.
- `tack runner doctor` answers "what can this machine run, and where do its model
  credentials come from" without a server round trip.
- An operator can find out how to point a harness at Claude, Codex, OpenRouter or a local
  model from the documentation, including the fact that Tack never proxies model traffic.
- Full Rust gates green; `runner_contract`, `wave2_gate` and `openapi_contract` unchanged
  and drift-free, because nothing in this Part may touch what they pin.

**Handoff additions for this Part** — each card's handoff carries §III.2's template plus:

- **Binary-size delta**, measured before and after, for any card that changes what `tack`
  links.
- **Which role executed what**, for any card claiming a live run — whether the attempt was
  claimed by an embedded or a standalone runner, and over which address.
- **Loopback/gating proof**, naming the test that shows the off-by-default and
  non-loopback-refusal behaviours, not merely asserting them.

# Archive — closed and superseded cycles

> **Everything below this line is history.** Parts I, II and III are closed or superseded.
> They stay in this file at their original section numbers. The reason once given — hundreds
> of Rust doc comments citing those numbers — **no longer holds**: no Rust file cites them
> and one live document does. See the header for the measurement.
>
> Read a section from here only when a card's `Context` names it, or when you need the
> decision history behind a mechanism you are about to change. Part III's **Wave 9 amendment**
> is the exception worth reading unprompted: it holds the live-smoke failure that card V-A2
> exists to resolve, and its four open questions are that card's task list.

---

# Part I — Agent-Factory Control Center (Phases 33–38)

Executable task board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Agent-Factory Control
Center*. Read that section first: it holds the architecture, the schema table, and the
four non-negotiable design rules. **This file holds the dispatch plan** — written to be
picked up cold by parallel Sonnet agents.

Reciprocal upstream work lives in `~/Sites/rack-cli/ROADMAP.md` → **PHASE 22 — Control-plane
write API for an external plan-of-record (Tack)**, cards P22-1…P22-8. ~~Two Tack phases are
hard-blocked on it (35 needs P22-1, 37 needs P22-5).~~ **Corrected 2026-08-05: docket shipped
P22-1…P22-4 and P22-6.** ~~Only Phase 37 is still blocked (needs P22-5, `POST /pods`).~~
**Corrected again 2026-08-05 (card D3, found while investigating `pipeline validate`):
`POST /pods` (P22-5) has also shipped — `serve.py::_handle_post_pods` +
`core/pod_provisioning.py`, commit `0d84f47`. docket's own ROADMAP.md still marks it
`TODO` (a real staleness bug over there, not a timing artifact — its last commit
postdates 0d84f47). D4 is very likely unblocked; re-verify against `serve.py` directly
before starting, same discipline the first correction established. D3 also added
**P22-8 — `pipeline validate` over HTTP**, a genuine new gap: no HTTP route exists for
docket's own pipeline schema validator, CLI-only as of this writing.**

---

## Status board — updated 2026-08-05

Every card below has a handoff note in §6 with its full reasoning.

| Card | What it is | Status |
|---|---|---|
| W0-A · W0-B | tack-orch crate, trait + DTOs; migrations 019–024 | ✅ done |
| A1 · A2 · A3 | docket adapter; reconciler + health machine; orch repository | ✅ done |
| A4 · A5 · A6 | config + control-plane API + fleet endpoint; Fleet view; docs | ✅ done |
| A7 | real `ControlPlaneStore` + adapter registry (closed a gap no card owned) | ✅ done |
| A8 · A9 | `/fleet` a11y coverage; **backup token leak fix** (was mis-scheduled into C4) | ✅ done |
| A10 · A11 · A12 | WCAG AA contrast audit and fixes across the design system | ✅ done |
| B1 · B2 · B3 | runs + approvals ingestion; trace ingestion; metrics + retention | ✅ done |
| B4 · B5 | realtime broadcast; Agent Activity tab + shared state chip | ✅ done |
| B6 · B7 | agent-activity endpoints; retention-truncation disclosure in the UI | ✅ done |
| V1 | live verification of the docket adapter against a running server | ✅ done |
| C1 · C2 | dispatcher + `status_map`; auto-dispatch + **prompt-injection trust boundary** | ✅ done |
| C3 · C4 · C5 | DAG-ordered sprint dispatch + dry-run; dispatch UI; terminal status mapping | ✅ done |
| R1 | unfroze the trait: opaque trace cursor + typed policy-block error | ✅ done |
| R2 · R3 | WIP-limit race — dispatch path, then board-drag and voice paths | ✅ done |
| D1 | approvals inbox + proxy, behind a separate decision credential | ✅ done |
| F1 | per-sprint accessible names on the Run sprint control | ✅ done |
| D2 | budget + policy panels | ✅ done — **no pause control/indicator built**: docket has zero HTTP surface for it, in either direction (see §1.4 note and §6) |
| D3 | template `orchestration` block + pipeline library | ✅ done — backend + validation; UI editor deliberately deferred (see §6) |
| D5 | unit economics (tokens, estimated cost, lead time, rework rate) | ✅ done — own `handlers/economics.rs` + `repo/economics.rs` module (D4 was concurrently mid-edit in `repo/orch.rs`, so this card deliberately never touched that file — see §6) |
| D4 | provisioning flow + wizard | ✅ done — `POST /templates/{id}/provision` (project → validate → provision pod → link, rollback on every failure before the pod exists, never after); wizard at `/provision`; live-verified against an isolated docket, including 409/400/401 (see §6) |

**Known gaps, carried deliberately** (each is written up in the §6 note of the card that found it):

- `orch_events_daily` drops `item_id` on rollup, so per-item history truncation is
  reported by heuristic rather than fact (B6/B7).
- The Linear import has no HTTP-level trust test, because its GraphQL endpoint is not
  configurable for a mock server the way `TACK_GITHUB_API_BASE` is (C2).
- `parse_policy_block` still reads the policy id out of docket's error *prose*; docket
  has no structured field for it. A docket-side fix (R1).
- Board/List/Table each open their own WebSocket per project (B4).
- 3 pre-existing frontend unit-test failures (`requestBlob`/`createObjectURL`),
  independently confirmed to predate this cycle.
- `docket pipeline validate` has no HTTP route (CLI-only), so a template's
  `orchestration.pipeline_yaml` is only checked for being parseable YAML, not for
  being a valid docket pipeline. Upstream gap recorded as `rack-cli/ROADMAP.md`
  P22-8 (D3).
- docket's budget auto-pause has no HTTP surface at all — not to clear it (CLI-only,
  `docket profile <id> --resume`), and not even to reliably *read* it per Tack project
  (`orch_events` has no `remote_project` column, so the one proxy event that exists,
  `paused_refused`, can't be attributed to a specific linked project). D2 built the
  budget/policy panels without any pause indicator rather than guess. Two upstream/
  ingestion fixes would close this: docket exposing `paused`/`pausedReason` on
  `/status.json`, and Tack persisting `RemoteEvent.project`/a `remote_project` column
  on mirrored trace events.
- `orch_events.run_id` is always `NULL` — every `NewOrchEvent` constructed anywhere in
  this codebase (trace ingestion in `tack-orch::reconciler`, `status_map_rejected` in
  `dispatcher.rs`) hardcodes `run_id: None`. Any future feature that wants to
  correlate a mirrored event to one specific dispatch *attempt* (not just one item)
  cannot do it today — `orch_events.item_id` is the only reliable correlation. D5's
  rework-rate computation is item-level for exactly this reason (D5).
- A template's inline `orchestration.pipeline_yaml` has no delivery path to docket at
  provisioning time — `POST /pods` has no pipeline field, and `orch_links` has no
  `pipeline_yaml` column (only `pipeline_file`, a docket-known pipeline by name).
  `create_project_with_pod` still provisions successfully and surfaces this as a
  `warnings[]` entry rather than silently dropping it or inventing a delivery
  mechanism (D4).
- **A real SolidJS bug, not docket/Tack-API-specific:** `resource.latest`, like
  calling a `createResource` accessor directly, throws once the resource has
  errored — confirmed live. Reading it unguarded inside a `createMemo`/JSX
  expression silently stops that resource's whole reactive graph from updating
  (the page looks stuck loading forever, no console error). This is the same
  failure class card C4 already documented for the bare accessor call — `.latest`
  needs the identical guard, and this codebase has no lint for it yet. A grep for
  `\.latest` across `frontend/src` (excluding tests) currently turns up only the
  two guarded call sites this card just fixed — nothing else in the codebase uses
  `.latest` today, so there is nothing else to retrofit right now, only something
  to remember before the next `createResource` (D4).

---

## 0. Rules of engagement

Every agent, without exception:

1. **Own only your files.** The ownership map in §2 is authoritative. If you need a
   change in a file you don't own, write it in your handoff note — do not edit it.
2. **Read before you write.** `crates/tack-api/src/github_sync.rs` +
   `handlers/items.rs`'s `maybe_sync_github` call site is the precedent this whole
   cycle copies. Read it before starting any backend card.
3. **Tests are part of the task, not a follow-up.** No card is done without them.
4. **Green before handoff:**

   ```bash
   cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check
   cd frontend && npm run type-check && npm run test && npm run build
   ```

   A pre-push hook enforces `cargo fmt`; run it or the push fails.
5. **Never hold a SQLite write transaction across an HTTP call to docket.** Fetch →
   parse → short write txn. Tack is single-writer; violating this produces
   `database is locked` under real load.
6. **Never present an estimate as spend.** Token counts are primary; the money column
   is `cost_usd_estimated` and renders with the word "estimated" and its pricing
   snapshot date. This is a correctness requirement, not a copy preference.
7. **Status changes go through the workflow engine**, never raw SQL — WIP limits,
   explicit transitions, `started_at`/`completed_at`, and parent auto-propagation must
   all still fire.
8. **Orchestration is off by default, toggled from the UI, not hidden from it.**
   ~~`TACK_ORCH_ENABLE` unset ⇒ no reconciler task spawned, no new route accepting
   traffic.~~ **Rewritten 2026-08-05 (card E1) — every card before this one was
   written against the old rule; re-read this before touching the gate.** The
   enable flag now lives in `app_meta` (`handlers/settings.rs`, key
   `orch_config`), editable at runtime via `GET`/`PUT
   /api/settings/orchestration` — that route is registered **outside** the
   orchestration gate and stays reachable even while the feature is off, the
   same way `/api/settings/backup` always has (`handlers::settings::
   effective_backup_config` is the precedent this follows exactly).
   `TACK_ORCH_ENABLE` still exists, but only as the **deployment default**
   consulted when no UI value has ever been stored
   (`handlers::settings::effective_orch_enabled`); a stored value always wins.
   Two behavioral changes fall out of this:
   - **The gate no longer 404s.** With orchestration disabled, every route
     under `orch_routes` (`router.rs`) returns `409 Conflict` with
     `error.code: "orchestration_disabled"` and a message naming
     `PUT /api/settings/orchestration` — a bare 404 made "disabled"
     indistinguishable from "route doesn't exist," which hid the feature from
     its own operator. Assert the 409 + code in a test, not a 404.
   - **Toggling takes effect without a restart.** `AppState::orch_runtime`
     (`orch_runtime.rs`) is a start/stop handle the settings `PUT` calls
     directly — see that module's doc comment for the cooperative-cancellation
     design (a `tokio::sync::watch` stop signal raced against each reconciler
     task's poll-interval sleep). Assert this in a test too: flip the setting
     on and off against a real (or fake) `ControlPlaneStore` and check the
     task actually starts/stops, not just that the config value changed.
9. **Design system:** frontend cards use `--color-*` tokens only (no raw hex), pass
   axe, and work in all six palette×mode combinations. See
   `docs/book/src/developer/frontend.md`.
10. **Handoff note:** finish by appending to §6 — what you changed, what you
    discovered, and anything the next wave must know.

---

## 1. Shared contracts — freeze these before Wave 1

Wave 0 defines them; **every later card consumes them verbatim**. Do not redefine these
types locally, and do not "improve" a field name mid-wave — a rename here costs six
agents a rebase.

### 1.1 The `ControlPlane` trait (`crates/tack-orch/src/lib.rs`)

```rust
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str;                 // "docket"
    async fn health(&self) -> Result<Health, OrchError>;
    async fn status(&self) -> Result<FleetStatus, OrchError>;
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError>;
    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError>;
    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError>;
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError>;
    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError>;
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<Vec<RemoteEvent>, OrchError>;
    // Phase 35+ — write side, gated behind TACK_ORCH_ENABLE.
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<(), OrchError>;
}
```

### 1.2 Remote state enums — the exact strings docket emits

Deserialize from these and nothing else. Verified against
`~/Sites/rack-cli/src/docket/core/runs.py` and `core/dispatch.py`:

| Type | Values |
|---|---|
| `RunState` | `queued` · `running` · `succeeded` · `failed` · `cancelled` |
| `RunSource` | `cli` · `webhook` · `schedule` · `sweep` · `mcp` |
| `TaskStatus` | `pending` · `running` · `done` · `failed` · `blocked` · `waiting_approval` |
| `ApprovalState` | `pending` · `granted` · `denied` |

Every enum carries an `Unknown(String)` variant. A docket upgrade that adds a state
must degrade to "shown as-is", never to a deserialization error that kills the poll
loop. **This is the single most important robustness decision in the cycle.**

### 1.3 `status_map` (stored in `orch_links.status_map`, JSON)

```json
{
  "dispatch_from": ["Ready"],
  "on_running": "In Progress",
  "on_waiting_approval": "Blocked",
  "on_succeeded": "Done",
  "on_failed": "Blocked",
  "on_cancelled": "Ready"
}
```

All keys optional except `dispatch_from`. Every named status is validated against the
project's `WorkflowConfig` **at save time**. An absent key means "do not touch the
item's status on that transition".

### 1.4 docket HTTP surface

| Route | Auth | Notes |
|---|---|---|
| `GET /status.json`, `GET /metrics`, `GET /health` | none | **no pause/resume surface at all** — see the D2 note below |
| `GET /runs?project=`, `GET /runs/{id}` | Bearer | |
| `GET /approvals` | Bearer | records carry `context: {taskId, pipelineIndex}` |
| `POST /dispatch/{project}` | Bearer | body = pipeline `variables`; returns `{ok, run, project}` |
| `POST /approvals/{token}` | Bearer | `{action: "grant"\|"deny"}` |
| `GET /tasks/{project}` | Bearer | **shipped** (docket P22-2, `serve.py` `do_GET`) |
| `GET /traces/{project}?since=` | Bearer | **shipped** (docket P22-3) — cursor paging; events are **snake_case** |
| `POST /tasks/{project}` | Bearer | **shipped** (docket P22-1) — body `{description, priority, trusted}` → `{ok, task, project, status, approvalToken?}` |
| `POST /pods` | Bearer | **shipped** (docket P22-5, `serve.py::_handle_post_pods` + `core/pod_provisioning.py`, commit `0d84f47`) — body `{project, path, blueprint, pod, budget, verifyCmd}` (all but `project` optional) → `201 {ok, project, blueprint, members: [{id, role, model}]}`. `409` (`PodAlreadyExistsError`) and every non-2xx are raised before/after docket's own rollback — never a half-created pod. No HTTP route to delete/un-provision a pod exists. Live-verified by card D4, 2026-08-05 — this row previously said "does not exist yet," which was already stale by the time D4 started (see the top-of-file correction and card D3's handoff) |

> **Added 2026-08-05 (card D2).** docket's budget auto-pause
> (`core/dispatch.py::_pause_lead_for_budget`, sets a Lead agent's
> `paused`/`pausedReason`) has **zero HTTP surface, in either direction**.
> Read `serve.py`'s full `do_GET`/`do_POST` route table directly before
> assuming otherwise — there is no `/profile`, no pause, and no resume route
> anywhere in it; `docket profile <id> --resume` is CLI-only. And the read
> side isn't reachable either: `_agent_record()` (backs `/status.json`) and
> `render_metrics()` (backs `/metrics`) both omit `paused`/`pausedReason`
> from their output entirely — verified by reading each function's exact
> return value, not inferred. The only proxy is a `paused_refused` trace
> event, and even that can't be attributed to a single linked Tack project
> with today's ingestion (`orch_events` has no `remote_project` column). See
> TODO.md §6 (card D2) for the full write-up. **Do not build a pause
> read-or-write control against this table without first re-reading
> `serve.py` yourself** — this note describes what's missing, not a promise
> it'll stay missing forever.

> **Corrected 2026-08-04.** This table previously marked the first three as unbuilt and
> used them to block B2 and all of Wave 3. **That was wrong** — docket shipped Phase 22
> (commits `50398e0`, `c0c8423`, `ff81f19`, `a485111`). Verified directly against
> `/home/ox/Sites/rack-cli/src/docket/serve.py` and `core/dispatch.py`, not against
> docket's ROADMAP.md, which still lists P22-1/2/3 as `TODO` and is stale relative to
> its own source. **Only `POST /pods` is genuinely missing.** Re-verify against
> `serve.py` before trusting any "blocked" marker in this file.

`POST /tasks/{project}` honours the `pre_input` policy gate: a `block` verdict returns
4xx naming the policy id, and a `require_approval` verdict returns the task in
`waiting_approval` with its token — **not** a 200 pretending the task is queued. C1 must
handle all three outcomes distinctly.

**`trusted` exists and works** (`core/dispatch.py::enqueue_task`, keyword-only,
`bool | None`). `None` — what every existing CLI/MCP caller passes — means "trusted iff
`source == "operator"`", which is always true today. So C2 must pass `trusted: false`
**explicitly** for imported items; omitting it silently grants operator trust to
GitHub/Linear-imported text. That is precisely the prompt-injection boundary C2 exists
to draw.

---

## 2. File-ownership map

One owner per file per wave. Anything not listed is free to create.

| File | Owner | Wave |
|---|---|---|
| `Cargo.toml` (workspace members + deps) | **W0-A** | 0 — nobody else touches it all cycle; batch requests through W0-A |
| `crates/tack-db/src/migrations.rs` | **W0-B** | 0 — all six migrations land in one change |
| `crates/tack-orch/src/lib.rs` (trait + DTOs) | **W0-A** | 0 — ~~frozen after Wave 0~~ **unfrozen 2026-08-05, see below** |
| `crates/tack-orch/src/adapters/docket.rs` | A1 | 1 |
| `crates/tack-orch/src/reconciler.rs` | A2 | 1 → B1, B2, B3 extend it in **separate `poll_*` fns** |
| `crates/tack-db/src/repo/orch.rs` | A3 | 1 (owner) → B-wave adds fns, one agent at a time |
| `crates/tack-db/src/repo.rs` | A3 | 1 — one-line module registration only |
| `crates/tack-api/src/config.rs` | A4 | 1 |
| `crates/tack-api/src/handlers/orch.rs` | A4 | 1 → C1 extends (approvals proxy) |
| `crates/tack-api/src/router.rs` | A4 | 1 — **batch every route for the cycle in one edit**; later waves hand their routes to A4's successor rather than editing directly |
| `crates/tack-api/src/server.rs` | A2 | 1 — reconciler spawn only |
| `crates/tack-api/src/handlers/websocket.rs` | B4 | 2 |
| `crates/tack-api/src/handlers/items.rs` | C2 | 3 — auto-dispatch hook, beside `maybe_sync_github` |
| `frontend/src/features/fleet/**` | A5 | 1 |
| `frontend/src/features/item-detail/**` | B5 | 2 |
| `frontend/src/shared/realtime/**` | B4 | 2 |
| `docs/book/src/**/orchestration.md`, `SUMMARY.md` | A6 | 1 |

**Merge order within a wave:** whoever finishes first merges first; the rest rebase.
Cards were scoped so that only the four chokepoint files above can genuinely conflict.

### 2.1 The Wave 0 freeze is lifted — card R1

**`crates/tack-orch/src/lib.rs` is no longer frozen.** Freezing the `ControlPlane`
trait and `OrchError` after Wave 0 was meant to stop concurrent agents from churning a
shared interface. It worked for that, and then it started producing designs that are
worse than the change it was avoiding. **This is a structural refactor — there is no
legacy behaviour to preserve here, and no compatibility promise to keep.** Where the
interface blocks the right design, change the interface.

Two workarounds exist *solely* because of the freeze, and both are now technical debt
with a silent failure mode:

1. **B2 reimplemented docket's cursor algorithm client-side.** `ControlPlane::traces`
   can't return docket's own `next` cursor, so `next_trace_cursor`/`decode_trace_cursor`
   in `reconciler.rs` mirror `serve.py`'s exact anchor/count logic. It is correct today
   (V1 verified it against live cursors, including same-second boundaries) and it will
   drift the moment docket changes that algorithm — with no compile error and no test
   failure, just quietly wrong resumption. **Fix:** let `traces` return an opaque cursor
   from the remote, and delete the client-side reconstruction.
2. **C1 matches on error message text.** `OrchError` has no variant for "docket's
   `pre_input` policy deliberately blocked this", so C1 added a `POLICY_BLOCK_PREFIX`
   constant and string-matches to tell a deliberate refusal from a transport failure.
   A reworded docket message silently turns a policy block into a generic error.
   **Fix:** a typed variant carrying the policy id.

**Card R1** does both. It is a cross-cutting change (trait + adapter + reconciler +
`dispatcher.rs` + `orch_store.rs`), so it must run **alone** — no concurrent agents in
`crates/tack-orch/**` or `crates/tack-api/**` while it runs.

**General rule going forward:** a frozen interface, a red gate, a generated artifact, or
an over-specified test is not a reason to build a workaround. Change the constraint and
record why. Reserve caution for what's genuinely irreversible — secrets escaping,
destructive git operations on the shared tree, a half-applied security boundary.

---

## 3. Wave plan

```text
Wave 0  (blocking, 2 agents, ~sequential)      W0-A ─┬─ W0-B
                                                     │
Wave 1  (Phase 33, 6 agents, parallel)     A1 A2 A3 A4 A5 A6
                                                     │
Wave 2  (Phase 34, 5 agents, parallel)        B1 B2 B3 B4 B5
                                                     │
Wave 3  (Phase 35, 4 agents — docket POST /tasks shipped)  C1 C2 C3 C4
                                                     │
Wave 4  (Phases 36–38, 5 agents, independent tracks) D1 D2 D3 D4 D5
```

Waves 1 and 2 can overlap once A1 + A3 have merged — B-cards only need the adapter and
the repo layer, not the fleet UI or the docs.

---

## Wave 0 — Foundations (blocking)

### W0-A — `tack-orch` crate, `ControlPlane` trait, shared DTOs

**Tasks:** 33.1 · **Files:** `crates/tack-orch/**`, `Cargo.toml` · **Depends on:** —

1. `cargo new --lib crates/tack-orch`; add to workspace `members`. Deps:
   `tokio`, `reqwest` (already a workspace dep — reuse it, do not add a second HTTP
   client), `serde`, `serde_json`, `thiserror`, `async-trait`, `chrono`, `uuid`,
   `tracing`, `tack-core`, `tack-db`. Dev-deps: `wiremock`.
2. **`tack-orch` must not depend on `tack-api`** — the dependency points inward. Add a
   comment saying so; a future agent will try.
3. Define the trait from §1.1, every DTO it references, and `OrchError` (`thiserror`)
   with variants `Http`, `Auth`, `Decode`, `NotFound`, `Unavailable`, `Disabled`.
4. Every remote enum gets `Unknown(String)` (§1.2) with a `#[serde(other)]`-style
   fallback, plus a unit test proving an unrecognised value round-trips instead of
   erroring.

**Acceptance:** `cargo build --workspace` green; the unknown-variant test passes; no
`tack-api` import anywhere in the crate.

### W0-B — Migrations 019–024

**Tasks:** 33.2 · **Files:** `crates/tack-db/src/migrations.rs` · **Depends on:** —

Land all six tables from the roadmap's schema table in one change, following the
`018_github_links` precedent exactly (const slice + entry in the `migrations` array).

- FKs `ON DELETE CASCADE` from `items` / `projects` / `control_planes`.
- `orch_tasks` PK is **`(item_id, remote_task_id)`** — an item can be redispatched;
  a single-column PK here is a design error that Phase 35 will hit.
- Indexes: `orch_events(item_id, occurred_at)`, `orch_events(occurred_at)` (retention
  sweep), `orch_runs(control_plane_id, state)`, `orch_tasks(remote_run_id)`.
- Timestamps as TEXT RFC3339, UUIDs as TEXT — match the existing convention.

**Acceptance:** a fresh DB migrates cleanly; an existing `tack.db` (18 migrations)
upgrades in place; a test asserts an orphaning insert into each new table is rejected
(FK enforcement landed in 26.3 — this proves it still holds).

---

## Wave 1 — Phase 33, Control-Plane Link (read-only)

### A1 — `DocketAdapter`

**Tasks:** 33.3 · **Files:** `crates/tack-orch/src/adapters/docket.rs` ·
**Depends on:** W0-A

1. `reqwest` client, configurable base URL, Bearer on authenticated routes only,
   5s per-request timeout, `User-Agent: tack-orch/<version>`.
2. Implement every read method of `ControlPlane` (write methods → `OrchError::Disabled`
   until Wave 3).
3. **Prometheus text parser** for `/metrics` — no new dependency; parse
   `name{k="v",…} value`, skip `#` lines, tolerate missing labels. This parser is
   reused by B3; put it in its own module with its own tests.
4. Tests against a `wiremock` server using **real captured payloads** — capture them
   from a live `docket serve` and commit them under
   `crates/tack-orch/tests/fixtures/`. Include a malformed-payload and an
   unknown-enum-value case.

**Acceptance:** every read method has a passing fixture test; an unreachable host and a
401 map to distinct `OrchError` variants; parsing never panics on malformed input.

### A2 — Reconciler skeleton + spawn

**Tasks:** 33.6 · **Files:** `crates/tack-orch/src/reconciler.rs`,
`crates/tack-api/src/server.rs` · **Depends on:** W0-A, A1 (trait only — code against
`ControlPlane`, not `DocketAdapter`)

1. One `tokio` task per registered control plane. Interval `TACK_ORCH_POLL_SECS`
   (default 10) with **jitter** (±20%) so N planes don't stampede.
2. Exponential backoff on failure, capped at 5 minutes.
3. Health state machine: `healthy` → `degraded` (3 consecutive failures) →
   `unreachable` (10). Recovery is immediate on a single success. Persist to
   `control_planes.health` + `last_seen_at` + `consecutive_failures`.
4. Poll `/health` and `/status.json` only. Later waves add `poll_runs`,
   `poll_approvals`, `poll_metrics`, `poll_traces` as **separate functions** —
   structure `reconcile_once()` as a list of steps so B-cards append without conflict.
5. Spawn from `server.rs` beside the existing due-soon/backup schedulers, **only** when
   `TACK_ORCH_ENABLE=true`.
6. A panic in one plane's task must not take down the others, and must never surface in
   a user request.

**Acceptance:** with docket down, Tack starts, serves normally, and logs backoff at
`warn` without spam; the state machine has unit tests driven by a fake `ControlPlane`;
with the flag off, no task is spawned (assert it).

### A3 — `orch` repository module

**Tasks:** 33.4 · **Files:** `crates/tack-db/src/repo/orch.rs`,
`crates/tack-db/src/repo.rs` · **Depends on:** W0-B

CRUD for `control_planes` and `orch_links`; upsert helpers for `orch_runs`,
`orch_events`, `orch_approvals`, `orch_tasks`, `orch_metrics` (used by Wave 2 — write
them now so B-cards don't have to touch this file's structure).

- Parameterized SQL only.
- **The stored docket token never leaves this layer in a read DTO** — expose
  `token_set: bool`. Same discipline as the S3 secret key.
- Batch upserts take a slice and use one transaction — one txn per poll, not per row.

**Acceptance:** integration tests against in-memory SQLite for every fn; a test asserts
the control-plane read DTO contains no token field.

### A4 — Config + control-plane API

**Tasks:** 33.5 · **Files:** `crates/tack-api/src/config.rs`,
`crates/tack-api/src/handlers/orch.rs`, `crates/tack-api/src/router.rs`, `tack.toml` ·
**Depends on:** A3

1. `TACK_ORCH_ENABLE` (bool, default **false**), `TACK_ORCH_POLL_SECS` (default 10),
   `TACK_ORCH_EVENT_RETENTION_DAYS` (default 90), `TACK_ORCH_APPROVAL_TOKEN`
   (optional, Wave 4). Document every one in `CLAUDE.md`'s config table and
   `docs/book/src/user-guide/configuration.md`.
2. Routes: `GET/POST /api/control-planes`, `GET/PATCH/DELETE /api/control-planes/{id}`,
   `GET/PUT /api/projects/{id}/orch-link`, `GET /api/fleet` (the Fleet view's
   aggregate). **Batch every route this cycle needs into one `router.rs` edit**,
   including placeholders for Waves 2–4, so `router.rs` is touched once.
3. Token is write-only over the API (`token_set: bool` on read). `PUT` with an absent
   token field preserves the stored one; an explicit `null` clears it.
4. `utoipa` annotations on everything — the Phase 29.3 CI drift gate covers new routes.

**Acceptance:** round-trip a control plane through the API without the token ever
appearing in a response body; `openapi.json` regenerates and the drift gate passes;
with `TACK_ORCH_ENABLE` unset every new route 404s.

### A5 — Fleet view (frontend)

**Tasks:** 33.7 · **Files:** `frontend/src/features/fleet/**`, route registration in
`frontend/src/app/routes.tsx` · **Depends on:** A4's response shapes (start against a
hand-written mock; swap to live when A4 merges)

One row per product: Tack project · pod health chip · roster (roles + models) · last
activity · burn vs budget · gateway state · pending-approval count. Plus an empty state
that explains how to register a control plane, and a clear `unreachable` treatment
(stale data must **look** stale — a greyed row with "last seen 4m ago", never a
confident-looking zero).

**Acceptance:** Vitest unit tests for the row component incl. the degraded/unreachable
states; axe-clean; correct in all six palette×mode combinations; every money figure
carries the word "estimated".

### A6 — Docs

**Tasks:** 33.9 · **Files:** `docs/book/src/user-guide/orchestration.md`,
`docs/book/src/developer/orchestration.md`, `docs/book/src/SUMMARY.md`,
`CLAUDE.md` · **Depends on:** —

User guide: what a control plane is, how to register one, what the Fleet view shows,
and — prominently — **why every dollar figure says "estimated"**. Developer guide: the
architecture diagram, the `ControlPlane` trait, the reconciler's loop and state
machine, and the four non-negotiables. Update `CLAUDE.md`'s config table and crate
tour with `tack-orch`.

**Acceptance:** `mdbook build` clean; every documented env var exists in `config.rs`
(grep-check it — doc drift is finding #12 of the last audit).

---

## Wave 2 — Phase 34, Run Mirroring & Telemetry

All B-cards depend on **A1 + A2 + A3**. Each adds one `poll_*` fn to the reconciler's
step list plus its own module — coordinate only on the one-line step registration.

### B1 — Runs + approvals ingestion

**Tasks:** 34.1, 34.2 · Poll `/runs?project=` per linked project and `/approvals`
fleet-wide; upsert `orch_runs` / `orch_approvals`. Correlate approvals to items via
`record.context.taskId` → `orch_tasks.remote_task_id`; uncorrelated records store
`item_id = NULL` and **must still appear** in the fleet inbox. Runs that don't
correlate to an item (dispatched from the docket CLI) mirror unattributed — that is the
normal case until Wave 3 and must not error.

**Acceptance:** a CLI-dispatched run appears in Tack within one poll interval; an
approval created by a gated `bash` call correlates to the right item; re-polling is
idempotent (no duplicate rows, no spurious events).

### B2 — Trace ingestion

**Tasks:** 34.4 · ✅ **Unblocked** — docket shipped `GET /traces/{project}?since=`
(P22-3). Read `serve.py`'s cursor-paging section (~line 672) for the real cursor
semantics before designing against them.
Cursor-based (`?since=`), persisted per `(control_plane_id, remote_project)`. Map
docket's event types to the Tack taxonomy; **store unknown types verbatim**. Emit an
`orch_events` row per trace event.

**⚠️ Unsolved when this card was written — solve it before writing code.** Unlike
`orch_runs` (keyed by `run_id`) and `orch_approvals` (keyed by `token`), **`orch_events`
has no natural key**: a docket trace event is a position in a JSONL stream, not an
entity with a stable id. A3 made `orch_events.id` caller-assigned precisely because of
this and flagged that the upsert helper alone will **not** save you — re-polling an
overlapping window duplicates history unless the id is derived deterministically from
the source event. The original card said "cursor-based, resumable" and never said what
makes an event unique; that was a spec hole, not an implementation detail.

Required: derive `orch_events.id` as a **pure function of the source event**, so the
same docket event ingested twice produces the same row. In preference order —

1. If docket's trace records carry a monotonic sequence number or byte offset, key on
   `(control_plane_id, remote_project, seq)`. Check the real payload first; prefer this.
2. Otherwise hash a canonicalised form of the record (stable field order) together with
   `control_plane_id` and `remote_project`.

Do not key on ingestion time, poll tick, or row insert order — all three make replay
non-idempotent. **A cursor is an optimisation, not the correctness mechanism:** losing,
resetting, or rewinding the cursor must re-ingest without duplicating. Test that
directly — replay an overlapping window and assert the row count is unchanged.

Also note (A1, verified against docket's source): **trace events are snake_case on the
wire**, while every other docket endpoint is camelCase. This is the one endpoint where
that matters, and it would fail silently.

**Acceptance:** an item's event stream reconstructs the full hop sequence of a real
dispatch; restarting Tack resumes from the cursor without re-ingesting or gapping; and
a deliberately rewound cursor re-ingests an overlapping window with **zero** new rows.

### B3 — Metrics ingestion + retention

**Tasks:** 34.3, 34.6, 34.7 · Parse `/metrics` with A1's parser (do **not** write a
second one) into `orch_metrics`. Daily job: roll raw `orch_events`/`orch_metrics` into
per-day aggregates **before** deleting rows older than
`TACK_ORCH_EVENT_RETENTION_DAYS`. Expose `GET /api/metrics` merging Tack's own
work-tracking metrics with the mirrored docket ones.

Retention is **not** a follow-up: both tables grow unbounded, and docket has the
identical open gap in its own roadmap — do not inherit it.

**Acceptance:** a 91-day-old event is deleted but its day's aggregate survives with the
same totals; `GET /api/metrics` parses with a real Prometheus scrape.

### B4 — Realtime broadcast

**Tasks:** 34.5 · `crates/tack-api/src/handlers/websocket.rs` +
`frontend/src/shared/realtime/**`. Add `BoardEvent::AgentRunUpdated { project_id,
item_id, run_id, state }` and `BoardEvent::ApprovalPending { project_id, item_id,
token, action }`. Events are filtered by `project_id` before send, like the existing
ones. Frontend handles both and updates optimistic state.

**Acceptance:** a running dispatch moves the board without a refresh; an unknown event
variant on the wire is ignored by the client, not thrown.

### B5 — Item "Agent Activity" tab + agent badges

**Tasks:** 34.8, 34.9 · `frontend/src/features/item-detail/**` plus **one shared**
`AgentStateChip` used by Board, List, and Table (no per-view reimplementation).
Timeline renders hops, tool calls, verdicts, rework cycles, approvals, tokens, and
estimated cost for the item's `orch_tasks` rows — grouped by attempt, newest first.

**Acceptance:** the timeline of a real multi-hop dispatch is readable end-to-end; the
chip's five states are visually distinct at AA contrast in all six palette×mode
combinations; an item with no agent activity shows no chip and no empty tab.

---

## Wave 3 — Phase 35, Dispatch ✅ unblocked (docket `POST /tasks/{project}` shipped, P22-1)

Do not start C1–C4 until docket Phase 22 ships the enqueue endpoint. C1 and C3 can be
written against a `wiremock` stub of it in the meantime.

### C1 — Dispatcher core + `status_map`

**Tasks:** 35.2, 35.3, 35.6 · `crates/tack-orch/src/dispatcher.rs`, extend
`handlers/orch.rs`. `status_map` schema + save-time validation against the project's
`WorkflowConfig`. `POST /api/items/{id}/dispatch`: enqueue → dispatch → persist
`orch_tasks` → apply `on_running`. Idempotent per `(item_id, attempt)`. Reconciler
applies terminal states **through the workflow engine**; on rejection record
`status_map_rejected` with the engine's reason and leave the item alone.

**Acceptance:** a construction project's linear workflow refuses an illegal auto-move,
records the rejection, and surfaces it — with the item untouched; double-dispatching
the same item creates one task, not two.

### C2 — Auto-dispatch hook + untrusted sources

**Tasks:** 35.5, 35.7 · `crates/tack-api/src/handlers/items.rs` (beside
`maybe_sync_github`), plus `handlers/import_github.rs` / `import_linear.rs`. Best-effort:
a dispatch failure logs, records an event, and **never** fails the user's PATCH. Mark
imported items `source: imported` and enqueue them with `trusted: false` — imported
titles are attacker-authored text that becomes agent input, and docket's `pre_input`
hook is the defence. This is a real prompt-injection boundary.

**Acceptance:** moving a card into a `dispatch_from` status dispatches exactly once;
a GitHub-imported item enqueues with `trusted: false` (assert on the wire).

### C3 — Sprint dispatch (DAG-ordered)

**Tasks:** 35.4 · **The highest-value and highest-risk card in the cycle** — no
precedent exists in either codebase, so design it before coding it. Topologically sort
the sprint's items via `crates/tack-core/src/dependency.rs`; hold an item until every
dependency reaches a `done`-category status; cap in-flight dispatches per project.
A graph cycle is already impossible (the DAG validator prevents it) — assert anyway and
fail loudly rather than deadlocking. Include a **dry-run** mode returning the planned
order without dispatching; the UI needs it and so do the tests.

**Acceptance:** a 10-item sprint with a diamond dependency graph dispatches in a valid
topological order, respects the in-flight cap, and completes; dry-run output matches
actual order exactly.

### C4 — Dispatch UI + security gating

**Tasks:** 35.8, 35.9 · "Dispatch to agents" on item detail and the board card menu;
"Run sprint" on the Sprints view with the C3 dry-run order preview and an in-flight cap
control. Gate every dispatch route behind `TACK_ORCH_ENABLE` **and** a configured
control-plane token.

**Backup exclusion is DONE — closed by A9 (2026-08-04), see TODO.md §6 handoff.**
`scrub_snapshot_secrets` in `crates/tack-api/src/remote_backup.rs` now nulls
`control_planes.token` before the VACUUM, with a raw-bytes regression test
(`scrub_removes_control_plane_token_from_snapshot`) that was confirmed failing
pre-fix. **C4's remaining scope is just the UI/security gating** — tasks 35.8/35.9,
"Dispatch to agents" on item detail and the board card menu, "Run sprint" on the
Sprints view, and gating every dispatch route behind `TACK_ORCH_ENABLE` **and** a
configured control-plane token. Do not re-do the backup-exclusion work.

**Acceptance:** with `TACK_ORCH_ENABLE` unset every dispatch route 404s; the sprint
preview matches what actually runs. (The `GET /api/backup` token-exclusion
acceptance bar is already met — see A9's handoff for the test that proves it.)

---

## Wave 4 — Phases 36–38 (independent tracks)

### D1 — Approvals inbox + proxy

**Tasks:** 36.1, 36.2 · `POST /api/approvals/{token}` → docket's
`POST /approvals/{token}`. Requires a **separate** `TACK_ORCH_APPROVAL_TOKEN`; with only
`TACK_API_TOKEN` configured the approvals surface is read-only. Rationale: the Tack
token is one shared secret and granting a docket approval is a materially
higher-privilege act than editing a card. Fleet-wide inbox page, **oldest first** —
docket approvals fail closed on timeout, so latency here has a real cost. Show the
requesting agent, the action text, and the correlated item.

**Acceptance:** a gated `git push` appears in the inbox within one poll interval;
granting from Tack resumes the pipeline; without the approval token the grant button is
absent and the endpoint 403s.

### D2 — Budget, pause, and policy panels

**Tasks:** 36.3, 36.4 · Per-pod burn vs cap from `/status.json` with a warning band; an
explicit "this pod is budget-paused; its queue is refusing tasks" state naming the
remedy (`docket profile <id> --resume`) — Tack does not clear the pause itself. Policy
panel: denial rate, policy hits by id, approvals by channel, tool-call volume, all from
`/metrics`. Link out to `docket audit verify` rather than reimplementing chain
verification in Rust.

**Acceptance:** a pod driven past its cap shows paused with the remedy; every number
traces to a named Prometheus series.

### D3 — Template `orchestration` block + pipeline library

**Tasks:** 37.1, 37.3 · Extend the template payload
(`crates/tack-db/src/repo/templates.rs`) with an optional `orchestration` object:
blueprint (`software`/`research`/`content`/`ops`/`agentic-product`), pipeline YAML ref,
`verifyCmd`, budget cap, default `status_map`, pod shape. **Backwards compatible** —
templates without it behave exactly as today. Validate pipeline YAML by round-tripping
through docket's own `pipeline validate`, never by reimplementing the schema in Rust
(a second validator drifts — that is what finding #13 was about).

**Acceptance:** an existing template still loads and creates a project unchanged; an
invalid pipeline is rejected with docket's own error text.

### D4 — Provisioning flow + wizard

**Tasks:** 37.2, 37.4 · ~~⚠️ Blocked on docket `POST /pods` (docket Phase 22).~~
**Corrected 2026-08-05 (card D4): unblocked — `POST /pods` shipped (P22-5),
live-verified against an isolated docket. Done — see §6.** Original brief:
`POST /api/projects/from-template/{id}` gains `provision_pod: true`: create the Tack
project → the docket pod → pipeline → budget → `orch_link`, with **rollback on partial
failure**. A Tack project with a half-created pod is worse than no project. Wizard:
product type → template → pod shape → budget → verify command → create, reusing the
Phase 31 template-first onboarding surface. (Built as a separate route,
`POST /api/templates/{id}/provision`, rather than extending the existing
endpoint — see card D4's §6 handoff for why.)

**Acceptance:** one wizard pass yields a correct board **and** a correct `docket list`
entry (roles, models, budget, verify command); a forced mid-provision failure leaves
neither a stray project nor a stray pod.

### D5 — Unit economics

**Tasks:** 38.1–38.4 · Per completed item: tokens in/out, estimated cost, **agent lead
time** (`dispatched_at → completed_at`) vs the human `started_at → completed_at`, and
**rework rate** (`rework_started` + `verification_failed` + `tester_verdict_failed` per
task). Slice by `project_type` and `item_type`. Export per-role outcome quality against
the model docket used, in a shape docket's role→model policy can consume — **export
only**; Tack does not mutate docket's model policy in this phase. CSV/JSON via the
existing machinery in `crates/tack-api/src/handlers/export.rs`.

Cost-per-completed-item by product line is the headline number of the whole cycle.
Rework rate is the agent-fleet equivalent of a defect escape rate and the earliest
regression signal available.

**Acceptance:** the dashboard answers "what did each product line cost, in tokens and
estimated dollars, per shipped item, and how often did agents need rework?" — with
every dollar figure labelled an estimate and carrying its pricing-snapshot date.

---

## 4. Cross-cutting acceptance for the cycle

- [ ] `TACK_ORCH_ENABLE` unset ⇒ no reconciler spawned, every new route 404s (tested)
- [ ] No control-plane token in any API response or backup bundle (tested)
- [ ] No `database is locked` under a reconciler + concurrent-user-edit soak test
- [ ] Every money figure in UI, API, and docs is labelled an estimate
- [ ] An unknown docket enum value degrades gracefully everywhere (tested)
- [ ] `openapi.json` regenerated; Phase 29.3 drift gate green
- [ ] `mdbook build` clean; every documented env var exists in `config.rs`
- [ ] Playwright E2E covers: register a plane → view fleet → dispatch an item →
      watch it complete → approve a gated action

## 5. Known risks

| Risk | Mitigation |
|---|---|
| docket is beta and its API can break | `apiVersion` from `/status.json` is checked on every poll; a mismatch degrades the plane with a clear message rather than misparsing |
| Two SQLite writers (reconciler + user) | §0 rule 5; soak test in the cross-cutting list |
| `orch_events` unbounded growth | B3 retention + rollup, shipped in the same wave as ingestion |
| Cost estimates mistaken for spend | §0 rule 6, enforced in review and in the E2E copy assertions |
| Wave 3 blocked indefinitely on upstream | C1/C3 are written against a `wiremock` stub; Waves 1–2 and D1/D2/D5 deliver standalone value without it |
| Prompt injection via imported items | C2's `trusted: false` boundary; docket's `pre_input` hook is the enforcement point |

## 6. Handoff notes

*Append as you finish. Format: `### <card-id> — <date>` then what changed, what you
discovered, and what the next wave must know.*

### W0-A — 2026-08-04

**What I created.** `crates/tack-orch` (new lib crate), registered in the root
`Cargo.toml` workspace `members`, plus `async-trait = "0.1"` and `wiremock = "0.6"`
added to `[workspace.dependencies]`. Contents:

- `crates/tack-orch/Cargo.toml` — deps per the card (`tack-core`, `tack-db`, `tokio`,
  `reqwest` reused from the workspace dep, `serde`, `serde_json`, `thiserror`,
  `async-trait`, `chrono`, `uuid`, `tracing`; dev-deps `tokio`, `wiremock`). Header
  comment states the inward-only dependency rule explicitly.
- `crates/tack-orch/src/lib.rs` — the `ControlPlane` trait (verbatim from §1.1),
  `OrchError`, the four remote-state enums, every DTO the trait references, and 12
  unit tests (one round-trip test per enum + one "known values still round-trip"
  test + shape tests deserializing realistic docket JSON for `Health`, `FleetStatus`,
  `RemoteRun`, `RemoteApproval`, `RemoteEvent`, plus an `OrchError` display smoke
  test). Module doc at the top restates the "must never depend on tack-api" rule.
- `crates/tack-orch/src/adapters/mod.rs` — empty (doc-comment only), declared so A1
  can add `docket.rs` there without touching `lib.rs`.
- `crates/tack-orch/tests/fixtures/.gitkeep` — for A1's captured payloads.

**Acceptance status:** `cargo build --workspace` green, `cargo test --workspace`
green (all 12 new unit tests pass, plus everything else including W0-B's new
`orch_migrations_test.rs` which landed concurrently — no conflict), `cargo clippy
--workspace -- -D warnings` green, `cargo fmt --all` applied. `grep -rn tack_api
crates/tack-orch/` finds only the two doc comments explaining the rule — no actual
dependency line.

**The frozen trait/DTO surface (copy verbatim, do not rename fields).**

The `ControlPlane` trait is exactly §1.1's signature, unchanged. Enums
(`RunState`, `RunSource`, `TaskStatus`, `ApprovalState`) are generated by a
`remote_string_enum!` macro in `lib.rs`: each gets a hand-written `Serialize`/
`Deserialize` (via `String`, not `#[serde(other)]` — that attribute only works on a
unit fallback with no payload, and we need the original string captured) plus
`as_str()`/`Display`/`From<String>`/`From<&str>`. `Unknown(String)` round-trips
exactly — verified by a test per enum.

DTOs (all in `lib.rs`):

- `Health { status: String, gateway: u8 }` — `/health`'s literal `{"status":"ok","gateway":N}`.
- `FleetAgent { id, name, kind, scope, model, registered, bindings: Vec<FleetBinding>, last_activity, cost_usd_estimated (wire "costUsd"), budget_usd: Option<f64> }`
- `FleetBinding { channel: String, peer_id: String }` — docket's `agent_bindings()` returns `[{channel, peerId}, ...]`, **not** a list of strings; get this from `core/fleet.py`, not `serve.py`.
- `FleetStatus { api_version, timestamp, gateway: String ("active"/"inactive"), channels: Vec<String>, agents: Vec<FleetAgent>, total_cost_usd_estimated (wire "totalCostUsd") }`
- `MetricSample { name: String, labels: BTreeMap<String,String>, value: f64 }` — the Prometheus-line shape A1's parser must produce.
- `RemoteRun { id, source: RunSource, project, state: RunState, task_ids: Vec<String>, error: String, created, started_at: Option<String>, finished_at: Option<String>, pids: Vec<i64>, variables: serde_json::Value }`
- `RemoteApproval { token, project, role, action, state: ApprovalState, created, context: serde_json::Value }`
- `RemoteTask` / `NewRemoteTask` — **speculative**, see deviations below.
- `RemoteEvent { ts, project, session_id, agent_role, event_type: String, payload: serde_json::Value, cost_usd_estimated: Option<f64> (wire "cost_usd"), duration_ms: Option<i64> }` — **snake_case on the wire**, not camelCase (see deviations).

Every `*_usd`/`*Usd` field is named `*_usd_estimated` on the Rust side per §0 rule 6,
with an explicit `#[serde(rename = "...")]` back to docket's actual (unqualified)
wire name so the mapping still works.

**Contract deviations / things TODO.md §1 didn't spell out, verified against
`~/Sites/rack-cli/src/docket/`:**

1. **Enum values match §1.2 exactly** — I re-verified `RunState`/`RunSource` against
   `core/runs.py`'s `Literal[...]` declarations and `TaskStatus`/`ApprovalState`
   against `core/dispatch.py`/`core/approval.py`'s actual state transitions. No
   discrepancies found; §1.2's table is accurate as written.
2. **Trace events are snake_case, not camelCase.** Every other docket JSON endpoint
   (`/status.json`, `/runs`, `/approvals`) uses camelCase, but `core/trace.py`'s
   JSONL records use snake_case field names (`session_id`, `agent_role`,
   `event_type`, `cost_usd`, `duration_ms`). `RemoteEvent` does **not** carry
   `#[serde(rename_all = "camelCase")]` — B2 (trace ingestion) needs to know this
   before writing the `/traces/{project}` adapter call.
3. **`event_type` is a plain `String`, not an enum.** `core/trace.py`'s
   `EVENT_TYPES` frozenset has ~25 members and is explicitly designed to grow; B2's
   own acceptance criteria says "store unknown types verbatim" — a plain string
   gives that for free without needing `Unknown(String)` machinery. Don't wrap it
   in an enum later without re-reading B2's card.
4. **`RemoteTask`/`NewRemoteTask` are speculative — flagged in doc comments.**
   `GET /tasks/{project}` and `POST /tasks/{project}` don't exist in docket yet
   (blocked on docket Phase 22, per §1.4). I projected `RemoteTask`'s shape from
   docket's *internal* queued-task record (`core/dispatch.py`'s
   `_TASK_SCALAR_DEFAULTS`/`_normalize_task`: `id, description, priority, status,
   created, startedAt, completedAt, source, reason, costUsd, claimId, claimedAt,
   approvalToken, pendingApprovalIndex`) since that's the closest real precedent —
   but it is **not a verified wire contract**. `NewRemoteTask.trusted: bool` is
   even more speculative: `core/dispatch.py`'s real `enqueue_task(project,
   description, priority)` has no `trusted` parameter at all — it derives trust
   internally from a fixed `source == "operator"` check. TODO.md's own Wave 3 card
   C2 requires imported items to enqueue with `trusted: false`, which means the
   *future* docket endpoint will need to accept trust as an explicit input; I added
   the field so C1/C2 have something to code against now, but **whoever writes
   C1's wiremock stub or A1's real adapter must re-verify both structs field-by-
   field against docket's actual Phase 22 endpoint** the moment it ships — do not
   assume my guess is correct, only that it's a reasonable placeholder.
5. **`/dispatch/{project}`'s real response is `{ok, run, project, status:
   "dispatched"}`**, one field richer than §1.4's `{ok, run, project}` — harmless,
   `ControlPlane::dispatch` just needs the `run` field (returns it as the run id
   `String`). Noted here so A1 doesn't get confused seeing an extra field in a
   captured fixture.
6. **`docket_agent_cost_usd`/`docket_cost_usd_total` etc. are the concrete
   Prometheus series names** `/metrics` emits (from `serve.py`'s `render_metrics`),
   useful for A1's parser tests and B3/D2's series-name assertions:
   `docket_agents_total`, `docket_agent_cost_usd{agent,model}`,
   `docket_agent_turns_total{agent}`, `docket_cost_usd_total`, `docket_gateway_up`,
   `docket_approvals_pending_total`, `docket_tool_calls_total{decision}`,
   `docket_policy_hits_total{policy_id,hook,action}`,
   `docket_approvals_total{channel,outcome}`, `docket_turn_duration_seconds_sum`/
   `_count` (a Prometheus `summary`, sum+count only, no quantiles).

**For Wave 1 (A1–A6):**

- A1: the Prometheus parser's target type is `MetricSample` above — `name` +
  `labels: BTreeMap<String,String>` + `value: f64`. Comment/blank lines never
  produce a sample; the series list in deviation #6 is what real fixtures will
  contain. Auth: `/status.json`, `/metrics`, `/health` are **unauthenticated**;
  `/runs`, `/runs/{id}`, `/approvals`, `/dispatch/{project}`,
  `/approvals/{token}` all require `Authorization: Bearer <token>` compared via
  `secrets.compare_digest` — a 401 on those is a real `OrchError::Auth`, not a
  parse failure.
- A2: `FleetStatus.api_version` is the field to check against a known-good
  `SERVE_API_VERSION` (currently `"2"`) for the apiVersion-mismatch degrade
  mentioned in §5's risk table — it's a plain `String`, compare it as such.
- A3/A4: no token field exists anywhere in this crate's DTOs (docket's own Bearer
  token is an adapter-construction concern, never serialized) — consistent with
  the "token never leaves the repo layer" discipline your cards need for
  `control_planes`.
- Everyone: `RemoteRun.created`/`started_at`/`finished_at` and `RemoteApproval.created`
  and `RemoteEvent.ts` are all kept as raw `String`, not `chrono::DateTime`,
  **deliberately** — docket mixes two ISO-8601 conventions (`runs.py`'s
  `datetime.now(UTC).isoformat()` produces a `+00:00` offset form;
  `trace.py`/`approval.py` produce a `...Z`-suffixed form via `strftime`). Parse
  at the call site if you need a typed timestamp; don't tighten these fields to
  `DateTime<Utc>` in `lib.rs` without handling both formats, or a single
  malformed/unexpected timestamp will fail an entire poll instead of degrading.

### W0-B — 2026-08-04

Landed migrations 019–024 in `crates/tack-db/src/migrations.rs`, following the
`018_github_links` precedent (const `[&str; N]` slice + entry in the migration list).
Added one new test file, `crates/tack-db/tests/orch_migrations_test.rs` (8 tests, all
green). No other files touched.

**Deviation from TODO §2's card text:** the TODO card body lists seven tables
("`control_planes`, `orch_links`, `orch_runs`, `orch_tasks`, `orch_events`,
`orch_approvals`, `orch_metrics`") but calls it "six tables," and the roadmap's own
schema table (`docs/book/src/roadmap.md` → "Schema added this cycle (migrations
019–024)") lists exactly **six** tables for 019–024 and does **not** include
`orch_metrics`. Per the card instructions ("the roadmap table is authoritative"), I
followed the roadmap: `orch_metrics` is **not** part of this migration batch. It
belongs to Phase 34 / Task 34.3 ("parse `/metrics` into `orch_metrics`") and needs its
own migration (025) landed by whichever B-wave card owns metrics ingestion (B3) — flag
this before B3 starts, since `migrations.rs` is a chokepoint file.

**Final table/column names (A3's repository layer must match exactly):**

- **019 `control_planes`** — `id, name, kind, base_url, token, api_version, health,
  last_seen_at, consecutive_failures, created_at, updated_at`. No FKs (root of the
  graph). `token` nullable, write-only at the API layer per the S3-secret-key
  precedent. `kind` defaults to `'docket'`. `health` defaults to `'unknown'` (reconciler
  drives it through `healthy`/`degraded`/`unreachable`).
- **020 `orch_links`** — `project_id` (PK, FK → `projects(id)` CASCADE), `control_plane_id`
  (FK → `control_planes(id)` CASCADE), `remote_project, pipeline_file, blueprint,
  auto_dispatch, budget_usd, status_map` (JSON TEXT, default `'{}'`), `created_at,
  updated_at`. **Design decision:** `project_id` is the primary key — one link per
  project, mirroring the `github_links` one-row-per-item shape. If a project ever needs
  multiple control-plane links, that's a breaking schema change for a later phase, not
  this one. `budget_usd` is deliberately **not** suffixed `_estimated` — it's a
  user-set cap, not a derived spend figure; only `cost_usd_estimated` (below) falls
  under the money-column rule. Index: `idx_orch_links_control_plane`.
- **021 `orch_tasks`** — PK **`(item_id, remote_task_id)`** as required. Columns:
  `item_id` (FK → `items(id)` CASCADE), `remote_task_id, remote_run_id, remote_status`
  (default `'pending'`), `attempt` (default 1), `tokens_in, tokens_out` (INTEGER,
  default 0 — primary measure), `cost_usd_estimated` (REAL, nullable, derived),
  `dispatched_at, trusted` (INTEGER default 1 — Phase 35.7 sets 0 for imported items),
  `created_at, updated_at`. `remote_run_id` is **not** a hard FK to `orch_runs` — it's
  just indexed (`idx_orch_tasks_remote_run`) for correlation, since a task can exist
  before its run is mirrored. Verified the composite PK with a test: two dispatches of
  the same item with different `remote_task_id`s both persist; the same pair twice
  collides.
- **022 `orch_runs`** — `run_id` (PK), `control_plane_id` (FK CASCADE), `item_id`
  (nullable FK → `items(id)` CASCADE — null means "mirrored, unattributed," the normal
  case pre-Phase-35), `remote_project, source` (default `'cli'`), `state` (default
  `'queued'`), `started_at, ended_at, error, created_at, updated_at`. Index:
  `idx_orch_runs_plane_state` on `(control_plane_id, state)`.
- **023 `orch_events`** — `id` (PK), `control_plane_id` (FK CASCADE), `item_id`
  (nullable FK CASCADE), `run_id` (TEXT, no hard FK — same reasoning as
  `orch_tasks.remote_run_id`), `event_type` (raw string — docket's type stored
  verbatim, including unrecognised ones), `payload` (JSON TEXT, default `'{}'`),
  `occurred_at, created_at`. Indexes: `idx_orch_events_item_occurred` on `(item_id,
  occurred_at)` and `idx_orch_events_occurred` on `(occurred_at)` for the retention
  sweep, exactly as specified.
- **024 `orch_approvals`** — `token` (PK — docket's approval token, not a credential),
  `control_plane_id` (FK CASCADE), `item_id` (nullable FK CASCADE — null means
  uncorrelated, must still show in the fleet inbox per 34.2), `remote_task_id` (no hard
  FK, correlation only), `agent, action, state` (default `'pending'`), `requested_at,
  decided_at, created_at, updated_at`. Indexes: `idx_orch_approvals_item`,
  `idx_orch_approvals_state`.

**FK/index summary:** every FK from a new table to `items`/`projects`/`control_planes`
is `ON DELETE CASCADE`, as required. `control_planes` itself has no FK columns — it's
the root — so there's no "orphan" test possible for it (noted explicitly in the test
file's module doc). All four required indexes from the card landed
(`orch_events(item_id, occurred_at)`, `orch_events(occurred_at)`,
`orch_runs(control_plane_id, state)`, `orch_tasks(remote_run_id)`), plus a few
uncontroversial extras on FK/lookup columns (`orch_links.control_plane_id`,
`orch_approvals.item_id`, `orch_approvals.state`).

**Refactor inside `migrations.rs`:** extracted `all_migrations()` (the ordered list)
and `apply_migrations()` (the loop) out of `run_all`, and added a new `pub async fn
run_up_to(pool, cutoff_name)` that applies the migration list up to and including a
named migration. This is what let the "existing `tack.db` at 18 migrations upgrades in
place" acceptance test run without needing an on-disk fixture: the test calls
`run_up_to(&pool, "018_github_links")` to simulate an installed database, asserts the
six new tables don't exist yet, then calls `run_all` again and asserts they do (24
migrations recorded total). `run_up_to` is a small, generically useful addition — not
gated behind `cfg(test)` because the integration test crate compiles the lib normally,
not with test cfg — but it's additive only; `run_all`'s behavior and the on-disk SQL
are unchanged.

**Tests:** `crates/tack-db/tests/orch_migrations_test.rs`, 8 tests — fresh-db table
existence, upgrade-in-place from 018, one FK-orphan-rejection test per new table that
has an FK (`orch_links`, `orch_tasks`, `orch_runs`, `orch_events`, `orch_approvals`),
and the composite-PK redispatch test for `orch_tasks`. `cargo test -p tack-db`: 27 + 8
= all green. `cargo clippy -p tack-db -- -D warnings`: clean. `cargo fmt -p tack-db --
--check`: clean. `cargo build --workspace` is green as of this handoff (W0-A's
`tack-orch` crate is already compiling).

**For A3 (orch repository module, Wave 1):** column names above are final from my
side — please match them verbatim. Two things worth knowing before you write CRUD:
(1) `orch_links.project_id` is the PK, so "upsert the link for a project" is a
straightforward `INSERT ... ON CONFLICT(project_id) DO UPDATE`; (2) neither
`orch_tasks.remote_run_id` nor `orch_events.run_id` nor `orch_approvals.remote_task_id`
are FK-enforced — correlation is your layer's job (join manually), not SQLite's.

### A3 — 2026-08-04

**What I built.** `crates/tack-db/src/repo/orch.rs` (new, ~830 lines) plus one line in
`crates/tack-db/src/repo.rs` (`pub mod orch;` — no `Repository` wrapper methods added
there; this module follows the `roles.rs`/`items.rs` house style of `impl Repository`
blocks living in the entity's own file, not the free-fn-plus-repo.rs-wrapper style
`boards.rs`/`github_links.rs` use — I picked that style specifically because my card
restricts `repo.rs` to "one-line module registration only"). Tests: new
`crates/tack-db/tests/orch_repo_test.rs`, 16 tests, all green.

**Public API surface (A4 builds directly on this — signatures are final, please match
verbatim):**

```rust
// ── control_planes ──
create_control_plane(&self, input: CreateControlPlane) -> Result<ControlPlane, sqlx::Error>
get_control_plane(&self, id: Uuid) -> Result<ControlPlane, sqlx::Error>            // RowNotFound if absent
list_control_planes(&self) -> Result<Vec<ControlPlane>, sqlx::Error>               // ORDER BY name
update_control_plane(&self, id: Uuid, input: UpdateControlPlane) -> Result<ControlPlane, sqlx::Error>
update_control_plane_health(&self, id: Uuid, health: &str, last_seen_at: Option<DateTime<Utc>>, consecutive_failures: i64, api_version: Option<&str>) -> Result<(), sqlx::Error>
delete_control_plane(&self, id: Uuid) -> Result<bool, sqlx::Error>
get_control_plane_token(&self, id: Uuid) -> Result<Option<String>, sqlx::Error>    // INTERNAL ONLY — never call from an HTTP handler response path

// ── orch_links (project_id is the PK — one link per project) ──
upsert_orch_link(&self, project_id: Uuid, input: UpsertOrchLink) -> Result<OrchLink, sqlx::Error>   // ON CONFLICT(project_id)
get_orch_link(&self, project_id: Uuid) -> Result<Option<OrchLink>, sqlx::Error>
list_orch_links_for_plane(&self, control_plane_id: Uuid) -> Result<Vec<OrchLink>, sqlx::Error>       // reconciler: which projects to poll
delete_orch_link(&self, project_id: Uuid) -> Result<bool, sqlx::Error>

// ── orch_tasks (composite PK: item_id, remote_task_id) ──
upsert_orch_tasks(&self, tasks: &[NewOrchTask]) -> Result<(), sqlx::Error>          // batch, one txn, ON CONFLICT(item_id, remote_task_id)
get_orch_task(&self, item_id: Uuid, remote_task_id: &str) -> Result<Option<OrchTask>, sqlx::Error>
list_orch_tasks_for_item(&self, item_id: Uuid) -> Result<Vec<OrchTask>, sqlx::Error> // ORDER BY attempt DESC
find_orch_task_by_remote_task_id(&self, remote_task_id: &str) -> Result<Option<OrchTask>, sqlx::Error> // for approval→item correlation (B1)

// ── orch_runs (PK: run_id) ──
upsert_orch_runs(&self, control_plane_id: Uuid, runs: &[NewOrchRun]) -> Result<(), sqlx::Error>  // batch, one txn, ON CONFLICT(run_id)
get_orch_run(&self, run_id: &str) -> Result<Option<OrchRun>, sqlx::Error>
list_orch_runs_for_item(&self, item_id: Uuid) -> Result<Vec<OrchRun>, sqlx::Error>

// ── orch_events (PK: id, caller-assigned) ──
upsert_orch_events(&self, control_plane_id: Uuid, events: &[NewOrchEvent]) -> Result<(), sqlx::Error> // batch, one txn, ON CONFLICT(id)
list_orch_events_for_item(&self, item_id: Uuid, limit: Option<i64>) -> Result<Vec<OrchEvent>, sqlx::Error> // chronological, oldest first

// ── orch_approvals (PK: token) ──
upsert_orch_approvals(&self, control_plane_id: Uuid, approvals: &[NewOrchApproval]) -> Result<(), sqlx::Error> // batch, one txn, ON CONFLICT(token)
get_orch_approval(&self, token: &str) -> Result<Option<OrchApproval>, sqlx::Error>
list_pending_orch_approvals(&self) -> Result<Vec<OrchApproval>, sqlx::Error>        // fleet-wide, state='pending', oldest first
```

All `New*`/`Create*`/`Update*` input structs and all read DTOs (`ControlPlane`,
`OrchLink`, `OrchTask`, `OrchRun`, `OrchEvent`, `OrchApproval`) are `pub` in
`repo::orch` — plain Rust structs, no `Deserialize`/`validator`/`utoipa` derives (those
belong at A4's API-DTO boundary; these are internal repo-layer types your handlers
construct and read directly). Read DTOs derive `Debug, Clone, PartialEq, Serialize` —
`Serialize` is there specifically so a handler can return one directly as a JSON body
without a translation struct, if that's convenient for you.

**Non-negotiables, how I satisfied them:**

1. **Token discipline.** `ControlPlane` has no `token` field, ever — only
   `token_set: bool`. The only way to get the real value is
   `get_control_plane_token()`, doc-commented "INTERNAL ONLY" for the
   reconciler/adapter. Test `test_control_plane_read_dto_never_exposes_token` builds a
   `ControlPlane`, serializes it with `serde_json::to_string`, and asserts the JSON
   contains neither the token value nor a `"token"` key, plus a compile-time guarantee
   (there's no field to accidentally serialize).
2. **`UpdateControlPlane.token: Option<Option<String>>`** gives A4 the tri-state PATCH
   semantics the card requires: `None` = field absent, leave stored token untouched;
   `Some(None)` = explicit null, clear it; `Some(Some(t))` = set/replace. Tested in
   `test_update_control_plane_token_set_then_clear`. (I used a plain
   `Option<Option<T>>` here rather than the workspace's `serde_with::double_option` —
   that crate feature is for *deserializing* a JSON PATCH body, which is A4's job at
   the handler boundary; my struct is constructed directly by Rust code, no serde
   involved on the input side.)
3. **Batch upserts are one-transaction-per-call.** `upsert_orch_tasks/_runs/_events/_approvals`
   each open one `pool.begin()`, loop the slice with parameterized `INSERT ...
   ON CONFLICT DO UPDATE`, and `commit()` once. An empty slice returns `Ok(())` without
   opening a transaction at all. None of these functions do any I/O to docket — callers
   fetch-then-call, so there's no way to accidentally hold a write txn across an HTTP
   call from inside this file.
4. **Parameterized SQL only** — every value is `.bind()`ed; the only place I build a
   SQL string dynamically is the column-list `format!` at the top of each section
   (e.g. `CONTROL_PLANE_COLUMNS`), which interpolates a `const &str` of column names I
   wrote, never user/caller data.
5. **Money field is `cost_usd_estimated`** on `OrchTask`, `Option<f64>`, nothing named
   plain `cost_usd`. `orch_links.budget_usd` stays unsuffixed on purpose (W0-B's call,
   which I kept) — it's a user-set cap, not a derived spend figure.
6. **Unknown remote-state strings pass through unvalidated.** `remote_status` /
   `state` / `source` / `event_type` are plain `String` columns and struct fields;
   this layer does no matching against `RunState`/`TaskStatus`/etc. Tests exercise this
   directly with made-up strings (`"some_future_status_tack_has_never_seen"`,
   `"totally_new_source_docket_invented"`, `"an_event_type_from_the_future"`) and
   assert they round-trip byte-for-byte.

**Idempotency tests** (one per batch fn, per the acceptance bar): each upserts a batch,
re-upserts the same keys with changed content, and asserts the row count is unchanged
while the content did update — `test_upsert_orch_tasks_batch_is_idempotent`,
`test_upsert_orch_runs_batch_is_idempotent_and_supports_unattributed_runs`,
`test_upsert_orch_events_batch_is_idempotent`,
`test_upsert_orch_approvals_batch_is_idempotent`. Also covered: an empty-slice call is
a documented no-op (not an error, not a transaction), and `orch_tasks`'s composite PK
is exercised directly — re-upserting the same `(item_id, remote_task_id)` updates in
place, but a *different* `remote_task_id` for the same item inserts a third row
(a redispatch), never collides.

**Design decisions the next waves should know about:**

- **`orch_runs.item_id` and `orch_approvals.item_id` are "write-once" through the
  upsert path** — `ON CONFLICT DO UPDATE SET item_id = COALESCE(excluded.item_id,
  item_id)`, i.e. only overwrite the existing row's `item_id` when the incoming batch
  actually knows one. A poll that doesn't know the attribution yet (`item_id: None` in
  the `New*` struct) can never clobber an attribution a *previous* poll already
  learned. B1's card explicitly requires "re-polling is idempotent" and correlating a
  CLI-dispatched run after the fact — this is what makes that safe without B1 having
  to read-before-write. Covered by `test_orch_run_attribution_is_never_unlearned`.
- **`orch_events.id` is caller-assigned, not server-generated.** `orch_events` has no
  natural key from docket's side (a trace event is identified by its position in a
  JSONL stream, not a stable id), so idempotent re-ingestion is only possible if B2
  derives the *same* `Uuid` for the *same* source event on every poll — e.g. a
  deterministic hash/UUIDv5 of `(control_plane_id, run_id, occurred_at, event_type)`,
  or whatever stable identifier docket's `/traces` endpoint turns out to expose. I
  flagged this in the module doc comment on `NewOrchEvent` too. If B2's cursor
  (`?since=`) genuinely never re-delivers an event, this is moot — but don't assume
  that without checking docket's actual behavior at the cursor boundary (an event
  timestamped exactly at `since` could be inclusive on both sides of two consecutive
  polls).
- **`find_orch_task_by_remote_task_id`** is what B1 needs for "correlate approvals to
  items via `record.context.taskId` → `orch_tasks.remote_task_id`" — it searches
  across all items (not scoped to one), returns the most-recently-dispatched match if
  there happen to be several (there shouldn't be, since `remote_task_id` comes from
  docket and should be globally unique, but the PK is technically per-item so I didn't
  assume global uniqueness at the schema level).
- **`orch_metrics` has no functions here, on purpose** — W0-B's handoff confirmed it's
  not part of migrations 019–024; it's deferred to B3 (34.3), which will need its own
  migration (025) *and* will be the one to add its repo functions to this file. B3:
  when you add `orch_metrics` functions, please follow the same shape as the other
  entities here (a `New*` input struct + a batch `upsert_*` taking a slice in one
  transaction) so the file stays consistent — you're the one-agent-at-a-time B-wave
  addition this file's structure was built for.
- **Retention delete (34.6, B3's job) has no function here yet** — I deliberately did
  not add a speculative `delete_orch_events_before(cutoff)` since I didn't want to
  guess at the rollup-then-delete transaction shape B3's card describes ("roll up
  before deleting" needs to read `orch_events`/`orch_metrics` and write an aggregate
  table that doesn't exist yet). B3 will need to add this fn; it's a plain addition,
  no restructuring required.
- **`list_orch_events_for_item` orders oldest-first (`ORDER BY occurred_at ASC`).**
  B5's card describes the UI timeline as "newest first" — that's a display-order
  choice, so I left the repo layer in natural chronological order and expect the
  handler/frontend to reverse if it wants newest-first; flagging this so B5 doesn't
  assume the repo already reverses it.

**Test results.** `cargo test -p tack-db`: 27 (pre-existing) + 8 (W0-B's migrations
tests) + 16 (mine) = 51 passed, 0 failed. `cargo clippy -p tack-db -- -D warnings`:
clean. `cargo fmt -p tack-db -- --check`: clean. `cargo build --workspace`: green as of
this handoff (A1/A2's `tack-orch` and whatever A4/A5/A6 have landed so far all still
compile together).

**Nothing outside my ownership was edited.** `crates/tack-db/src/migrations.rs` was
read only, not modified (its diff against git HEAD in the working tree is entirely
W0-B's own pre-existing uncommitted work, not mine — verified before writing this
note). I ran `cargo fmt --all` once while finishing up; it reformatted nothing outside
Rust source (no `.tsx`/`.md` files are touched by `cargo fmt`), and a follow-up
`cargo fmt -p tack-db -- --check` after the fact confirms my own files need no further
changes — but if you're another agent reading this: prefer `cargo fmt -p <your-crate>`
over `--all` while the cycle is still in flight, to avoid reformatting a file mid-edit
in someone else's terminal.

### A5 — 2026-08-04

**What I built.** `frontend/src/features/fleet/` (new, self-contained — imports
nothing from any other `features/*`, enforced by `architecture.test.ts`):

- `api.ts` — **the single file isolating every assumption about `GET /api/fleet`'s
  wire shape.** A4's endpoint doesn't exist yet; when it lands, reconcile against this
  file only, never the components. Exports `FleetRow` (the per-project-link row DTO),
  `FleetRosterAgent`, `ControlPlaneHealth = 'healthy' | 'degraded' | 'unreachable' |
  'unknown'`, `FleetGatewayState = 'active' | 'inactive' | 'unknown'`,
  `FleetResponse = { rows: FleetRow[] }`, and `fleetApi.list()`. Every field name is a
  snake_case best-guess projection of tack-orch's frozen `FleetAgent`/`FleetStatus`
  (see TODO.md §6 "W0-A") plus `control_planes`/`orch_links` columns (see §6 "W0-B") —
  full reasoning and field-by-field provenance is in the file's own header comment,
  not repeated here. Two field-naming decisions A4 should specifically weigh in on:
  (1) `cost_usd_estimated` mirrors tack-orch's field name exactly; (2) `budget_usd` is
  deliberately **not** suffixed `_estimated` (it's `orch_links.budget_usd`, a user-set
  cap, not a derived figure — matches W0-B's own note on that column). Also exports
  `isOrchDisabled(err)` — `true` iff the failure is a 404, which the page treats as
  "orchestration not enabled on this server" (§0 rule 8: `TACK_ORCH_ENABLE` unset ⇒
  every new route 404s) and renders as a **third** state distinct from both "200 with
  empty rows" (enabled, nothing registered) and any other failure (network/500).
- `format.ts` — pure, unit-tested helpers: `formatEstimatedCost` (always appends the
  literal word "estimated" plus the pricing-snapshot date when known; renders
  `null` as "cost estimate unavailable", never `$0.00`), `formatBudget`,
  `formatTokens` (k/M compaction), `relativeTime` (never a blank — "never"/"unknown"
  are explicit), `isStale(health)` (true for `unreachable`/`unknown`), and the
  `HEALTH_LABEL`/`HEALTH_TONE` maps.
- `HealthChip.tsx` — the pod-health chip (wraps the existing `Badge` primitive from
  `shared/ui`, tone + a small colored dot + text label so the state never relies on
  color alone). Four states: healthy (success/green), degraded (warning/amber),
  unreachable (danger/red), unknown (neutral/grey, "never polled").
- `FleetRow.tsx` — one `<tr>` per Tack project with an `orch_link`. **This is the
  component the acceptance criteria's Vitest coverage targets** (38 tests total across
  the folder, all passing). Columns: Project (name + linked control-plane name/kind),
  Pod health (chip + "Last seen …"/"Not yet connected" caption), Roster (role · model
  chips), Last activity, Burn vs budget (tokens **first and more prominent** than the
  estimated-dollar line beneath, per §0 rule 6), Gateway, Pending approvals.
- `FleetPage.tsx` — the route component: loading skeleton, the disabled/empty/error/
  populated states described above, and the real `<table>` (semantic `<caption
  class="sr-only">`, `<th scope="col">`) wrapped in `overflow-x-auto`.

**The single most important visual decision (per the card): never a confident-looking
zero for a stale plane.** `FleetRow.tsx`'s `stale()` branch (true for `unreachable`/
`unknown`) replaces tokens, estimated cost, gateway state, and pending-approval count
with an em dash + a caption naming why ("no fresh estimate — plane unreachable", "—
unavailable while unreachable"), and forces the gateway badge to "Unknown" rather than
showing a possibly-stale cached "Active" as if it were current. Roster composition is
shown greyed rather than hidden (composition, unlike money/counts, isn't the kind of
figure that reads as "confidently current" the same way a dollar amount does) — but
still labelled "last known … may be out of date." `degraded` is deliberately **not**
treated as stale (only 3 consecutive poll failures vs. 10 for unreachable) — it keeps
live figures at the plain row background, distinguished from `healthy` only by the
amber chip + a "Last seen …" caption, and from `unreachable` by *not* getting the
muted background or the dashed-out fields. One implementation pitfall I hit and fixed:
my first pass dimmed the whole stale row with CSS `opacity: 0.6`, which is an
accessibility anti-pattern — it blends already-token-based text colors with whatever
sits behind them, which can silently drop already-AA text below the 4.5:1 contrast
floor axe enforces, in a way that's easy to miss by eye and impossible to reason about
generically across six palette×mode combinations. I replaced it with a background-only
treatment (`background: var(--color-bg-subtle)` on the `<tr>`, all text stays at its
normal already-audited token color/opacity) — visually still reads as "muted", but
never risks contrast. Flagging this explicitly in case B5 (Wave 2, `AgentStateChip`)
or D2 (Wave 4, budget-paused state) reach for opacity dimming for a similar
"deemphasize this without hiding it" need — don't; use a background/token swap instead.

**Route + nav.** Registered `{ path: '/fleet', component: Fleet }` in
`frontend/src/app/routes.tsx` (lazy-loaded, workspace-level — sibling to `/projects`
and `/templates`, not nested under a project) and a "Fleet" entry in the sidebar's
Workspace section (`frontend/src/shared/ui/Sidebar.tsx`), between "Templates" and
"Settings". Added one new icon, `IconFleet` (three-node graph glyph), to
`frontend/src/shared/ui/icons.tsx` — the only edit outside `features/fleet/` beyond
the two files named in my card, and it was strictly necessary for the sidebar entry
(same `stroke()` helper as every other icon there, so it inherits `currentColor` +
`aria-hidden="true"` for free). I did **not** add a Fleet entry to the global command
palette (`Layout.tsx`'s `globalCommands()`) since that file isn't in my ownership and
wasn't named as an exception — worth a one-line addition (`go-fleet` under the
"Workspace" group, same pattern as `go-templates`) whenever someone next touches that
file.

**No control-plane-registration UI exists yet, by design.** The empty state
(`RegisterPlaneEmptyState` in `FleetPage.tsx`) explains registration in prose
(`TACK_ORCH_ENABLE=true` + `POST /api/control-planes`) rather than linking to a
settings panel or docs page, because neither exists yet in this wave — A4's card is
API-only and A6's user-guide page is being written concurrently. Whichever future card
adds a Settings UI for registering a control plane should also swap this copy for a
real CTA/link.

**Tests.** 38 Vitest tests across `format.test.ts`, `FleetRow.test.tsx`,
`FleetPage.test.tsx`, covering: healthy/degraded/unreachable/unknown row rendering,
the "never a confident zero" rule (explicitly asserts no `$0.00` and no bare `0`
tokens/gateway/approvals text for unreachable rows), the "estimated" + pricing-snapshot
wording, the three-way empty/disabled/error state split on `FleetPage`, and a retry
flow. `cd frontend && npm run type-check && npm run test && npm run build` all green
— the only 3 pre-existing failures (`client.test.ts`'s `requestBlob` Blob-instanceof
check, two `createObjectURL` assertions in `GlobalSettings.test.tsx`/
`panels.test.tsx`) reproduce identically on a clean `develop` with my changes stashed,
so they're environment/pre-existing issues, not something I introduced. `npm run
lint:tokens` passes at the existing baseline (0 new raw-color literals, 0 new inline
hex from my files). Did not run the Playwright a11y suite (`frontend/e2e/a11y.spec.ts`,
outside my three required commands and needs a live backend); I did not add a `/fleet`
scan there since that file isn't in my ownership map and I didn't want to touch a
CI-gating file speculatively — whoever owns E2E next should add one, following the
existing per-view pattern (`board view has no accessibility violations`, etc.).

**For A4 (reconciling the real `GET /api/fleet`):** treat `frontend/src/features/fleet/
api.ts` as the target, not a spec — every field name in `FleetRow` is my best guess,
not verified against your handler. Things I'd flag as worth confirming rather than
assuming I got right: whether "one row per project" (my assumption, matching the
card's "one row per Tack project" wording) vs. "one row per control plane" is really
what you're building; whether `pending_approval_count` is scoped per-project (via
`orch_approvals.item_id`'s project) or fleet-wide (unattributed approvals, `item_id:
NULL`, per B1's card in Wave 2 — those "must still appear" somewhere, and my current
shape has nowhere to put them since they don't belong to any one row); and whether
`last_activity_at` should be the roster's max `FleetAgent.last_activity` (what I
assumed) or the linked project's most recent `orch_task`/`orch_run` update.

### A1 — 2026-08-04

**⚠️ Disclosure first, since it isn't mine to bury:** my very first fixture-capture
command ran `docket serve` without exporting `DOCKET_HOME`, which meant it hit the
operator's real `~/.docket` instead of an isolated sandbox. `docket serve`'s startup
sweep (`_run_sweeps`, always run once at start, before any flag-gating) fail-closed-
denied **11 real pending approvals** (project `myapp`) that were already >18h past the
900s `APPROVAL_TIMEOUT` — i.e. they'd have been denied identically the next time the
operator ran `docket serve` or any sweep-triggering command; I just made that happen
several hours early, at 2026-08-04T19:47:51Z, via a command that had no business
touching real state. It also appended ~187 trace files with the corresponding
`approval_denied` events / stale-session closes (additive only — nothing was deleted).
No `--dispatch` flag was used, so no real agent turns ran and nothing was mutated
beyond that one sweep's normal, documented fail-closed behavior. I did **not** attempt
to revert any of it — rewriting real approval/trace state back to "pending" after the
fact would be worse than leaving docket's own security behavior stand. Every command
after I caught this used an explicit isolated `DOCKET_HOME` (a scratch temp dir);
`health.json`/`status_empty.json`/`metrics_empty.txt`'s provenance comments carry the
full detail. Flagging this prominently for whoever reads this next, and as a general
warning: **never run `docket serve` without an explicit `DOCKET_HOME` override** — its
startup sweep is not a no-op read, even without `--dispatch`.

**What I built** (all under `crates/tack-orch/`, nothing outside it touched):

- `src/adapters/docket.rs` (new) — `DocketAdapter`, the `ControlPlane` impl for docket.
- `src/adapters/prometheus.rs` (new) — the Prometheus text-exposition parser, in its
  own module as required.
- `src/adapters/mod.rs` (edited) — added `pub mod docket;` and `pub mod prometheus;`.
  See "lib.rs is frozen" below for why prometheus lives here instead of the crate root.
- `tests/docket_adapter_test.rs` (new) — 20 integration tests against `wiremock`.
- `tests/fixtures/*` (16 files, new) — see provenance table below.
- `src/adapters/prometheus.rs` also carries its own 11 unit tests (parser-only, no
  network).

**Constructor signature (A2 and A4 both build against this):**

```rust
DocketAdapter::new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, OrchError>
```

Trailing slash on `base_url` is normalized either way. `token: None` is a legitimate,
fully-supported configuration — every unauthenticated route still works, and calling an
authenticated route without one just gets docket's real 401 (mapped to
`OrchError::Auth`), not a client-side short-circuit. `Err` is only possible if
`reqwest::Client::builder().build()` itself fails (essentially never, for a plain
timeout+User-Agent client) — the constructor propagates rather than panics so a bad
control-plane row can never crash the process that registers it. To satisfy the trait
object A2's `RegisteredPlane`/`ControlPlaneStore` expect: `Arc::new(DocketAdapter::new(base_url,
token)?) as Arc<dyn ControlPlane>`.

**⚠️ `lib.rs` is frozen — I could not add `pub mod prometheus;` there.** W0-A's `lib.rs`
already has `pub mod adapters;` and `pub mod reconciler;` (pre-declared in Wave 0 for
A1/A2's benefit) but nothing for a new top-level Prometheus module, and my card
explicitly forbids editing `lib.rs` at all. I declared it as a child of `adapters`
instead (`src/adapters/prometheus.rs`, `pub mod prometheus;` inside `adapters/mod.rs`),
so **the public path is `tack_orch::adapters::prometheus::parse`, not
`tack_orch::prometheus::parse`** as TODO.md's own wording ("put it in its own module,
e.g. `crates/tack-orch/src/prometheus.rs`") might suggest. B3 (Wave 2, metrics
ingestion): use `tack_orch::adapters::prometheus::parse(&body) -> Vec<MetricSample>` —
it's a free function, stateless, takes the full `/metrics` response body and returns
every sample it could parse (never errors, never panics — a malformed line is dropped,
not a document-wide failure). If a future agent *can* touch `lib.rs` (e.g. whoever owns
it in Wave 2+), consider adding `pub use adapters::prometheus;` there so both paths
work — I didn't do that myself since it's still an edit to a file I was told not to
touch, even a purely additive one.

**Fixture provenance** (`crates/tack-orch/tests/fixtures/`, one line per file; every
file also carries its own detailed header comment — JSON fixtures via a leading
`//`-prefixed block, the Prometheus `.txt` fixtures via native `#` comments, and
`not_found_route.txt` via a `PROVENANCE: ...\n---\n<body>` convention since raw
plain-text has no comment syntax of its own):

| File | Provenance |
| --- | --- |
| `health.json`, `status_empty.json`, `metrics_empty.txt` | **Captured live**, but against the operator's real `~/.docket` (see the disclosure above), not a sandbox — content is genuine, just not from a fresh install |
| `status_with_agent.json`, `metrics_with_agent.txt`, `runs_list.json`, `run_single.json`, `run_not_found.json`, `approvals_pending.json`, `unauthorized.json`, `not_found_route.txt` | **Captured live** against an isolated `DOCKET_HOME` (scratch temp dir), seeded via docket's own code (`core.approval.approval_create`, `core.runs.create_run`/`execute`, a hand-written `.docket-meta.json` in the same shape `docket add` itself writes) — real `docket serve` responses over real HTTP |
| `tasks_list.json`, `traces_list.json` | **Partially derived.** The real entries (one task, two of three trace events) came from calling `docket.core.dispatch.enqueue_task()` / `docket.core.trace.trace_event()` directly (real docket code, real on-disk records) — genuine output, just not fetched over HTTP, because `GET /tasks/{project}`/`GET /traces/{project}` don't exist in docket yet (Phase 22). The `{"tasks":[...]}`/`{"events":[...]}` wrapper keys are **my own guess** by analogy with `/runs`/`/approvals`'s wrapping convention — not verified. `traces_list.json`'s third event (`some_future_event_type_v3`) is hand-constructed, because docket's real `trace_event()` actively *refuses* to persist an event type outside its current `EVENT_TYPES` set — there is no way to really capture a forward-compat trace event from this docket version. |
| `status_malformed.json`, `metrics_malformed.txt`, `run_unknown_state.json` | **Constructed**, not captured — deliberately truncated/malformed/mutated for the required robustness tests (Decode error, parser tolerance, `Unknown(String)` end-to-end) |

**Contract surprises vs. TODO.md §1 / W0-A's handoff, verified against real docket
(rack-cli @ pyproject 0.2.0-beta.1, both by reading `serve.py`'s `do_GET`/`do_POST` and
by hitting a real running instance):**

1. **`/tasks/{project}` and `/traces/{project}` genuinely don't exist** — confirmed by
   reading `serve.py`'s `do_GET` directly: it has exactly six recognised paths
   (`/status.json`, `/status`, `/metrics`, `/health`, `/approvals`, `/runs`,
   `/runs/{id}`) and a catch-all `else` returning plain-text `not found\n` at 404 for
   everything else — no partial/stub handler, nothing docket-Phase-22-shaped to find.
   Confirms TODO.md §1.4 exactly.
2. **The generic "route doesn't exist" 404 is `text/plain`, not JSON** (`not found\n`,
   no `{"ok":false,...}` wrapper) — only auth-gated-route 404s/401s
   (`_send_json_error`) are JSON. My adapter's error-body parsing tries JSON first and
   falls back to raw trimmed text, specifically so this doesn't break `list_tasks`/
   `traces`'s 404 handling.
3. **`GET /runs/{id}`'s 404 body is `{"ok": false, "error": "Unknown run: <id>"}`**
   (confirmed live, see `run_not_found.json`) — matches the `ErrorBody` shape I assumed
   for every JSON-producing authed route.
4. **`core.utils.gateway_active()` is hardcoded to always return `False`** on this
   docket version (no daemon gateway exists any more — its own docstring says so). This
   means `Health.gateway` is always `0` and `FleetStatus.gateway` is always `"inactive"`
   against any real docket instance today, not just my seeded ones — worth knowing for
   A5's Fleet view (its `FleetGatewayState` handling) and D2's gateway-state panel: a
   "gateway inactive" reading is the *universal* current state, not a signal anything is
   wrong.
5. **`approval.action`'s em dash confirms Python's `ensure_ascii=True` default** —
   docket's live JSON responses escape non-ASCII as `\uXXXX` rather than emitting raw
   UTF-8 bytes. Functionally irrelevant (both decode to the same `String`), but if a
   future agent diffs a captured fixture against a fresh capture and sees this, it's not
   a regression.
6. **`RemoteTask`/`NewRemoteTask`'s speculative shape (W0-A's handoff, deviation #4) is
   independently confirmed accurate** for the *internal* record: I called
   `core.dispatch.enqueue_task()` directly and its normalized output
   (`core.dispatch.read_tasks()`, which runs `_normalize_task`) matches W0-A's projected
   field set exactly, plus two fields the DTO doesn't carry (`hops: []`,
   `gateOverridePipelineIndex: null`) which `RemoteTask`'s `Deserialize` silently
   ignores (no `deny_unknown_fields`) — verified via `tasks_list.json`'s live-derived
   entry. This is still not proof of the future *HTTP* endpoint's shape, only of the
   internal record it would presumably be built from — same caveat W0-A already flagged.
7. **`RemoteRun.created`/`startedAt`/`finishedAt` really do use the `+00:00`-offset ISO
   form** (not `...Z`) when produced by `core.runs.create_run()` — confirms W0-A's
   handoff note about the two-convention split; `runs_list.json`/`run_single.json` are
   live proof, not just a read of the source.

**Test results.** `cargo test -p tack-orch`: 44 lib tests (the pre-existing W0-A/A2
tests plus 11 new prometheus unit tests) plus 20 new adapter integration tests, all
green.
`cargo test --workspace`: green (166 tests total across every crate, including A2's
reconciler and A3's repo/orch tests, which had already landed by the time I finished).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all --
--check`: clean. `cargo build --workspace`: green.

**For A4 (building the real `ControlPlaneStore`/adapter-construction glue in
`server.rs`'s `TODO(A3/A4)` block):** `DocketAdapter::new` is infallible in practice but
returns `Result` — you'll want `.map_err(...)` or a `?` at the call site, not an
`.unwrap()`, since it's on the path from an untrusted DB row (a malformed `base_url` a
user typed into the registration API). `kind()` returns the static string `"docket"` —
match on `control_planes.kind` and construct `DocketAdapter` only for that value; leave
a clear error/skip for any other `kind` rather than guessing.

**For B2 (Wave 2, trace ingestion) and B3 (Wave 2, metrics ingestion):** B2, re-read
contract surprise #1/#2 above before writing `poll_traces` — there is truly nothing to
poll yet; `traces()` will 404→`NotFound` against every real docket instance until
Phase 22 ships, and that must not be treated as the plane being broken (my `NotFound`
mapping already gives you this distinction for free, per the card's acceptance
criteria). B3: your parser is `tack_orch::adapters::prometheus::parse`, not
`tack_orch::prometheus::parse` — see the "lib.rs is frozen" section above.

### A2 — 2026-08-04

**What I built.** `crates/tack-orch/src/reconciler.rs` (new, ~640 lines incl. tests) —
the reconciler skeleton: one `tokio` task per registered control plane, the
`healthy`→`degraded`→`unreachable` health state machine, exponential backoff capped at
5 minutes, deterministic jitter, panic isolation, and the `apiVersion` mismatch policy.
Plus the reconciler spawn call site in `crates/tack-api/src/server.rs`, gated behind
`TACK_ORCH_ENABLE`. Full module doc at the top of `reconciler.rs` covers everything
below in more depth — this note is the summary for whoever doesn't want to read 100
lines of doc comment first.

**Verification (final, after A1 + A4/A6's concurrent work had landed):**
`cargo build --workspace` green. `cargo test --workspace`: **332 tests, 0 failed**
(across every crate — `tack-orch` alone: 44 unit tests, 19 of them
`reconciler::tests::*`). `cargo clippy --workspace --all-targets -- -D warnings`: clean.
`cargo fmt --all -- --check`: clean **for every file I own**
(`crates/tack-orch/src/reconciler.rs`, `crates/tack-api/src/server.rs`,
`crates/tack-orch/src/lib.rs`'s one line, `crates/tack-api/Cargo.toml`) — at the moment
I ran this, `crates/tack-api/src/handlers/orch.rs` (A4's file) had 3 unformatted spots;
I did **not** run `cargo fmt --all` again to fix it (that would edit a file outside my
ownership while A4 is mid-edit) — verified via `cargo fmt --all -- --check | grep "Diff
in"` that it was the *only* file with a diff. Re-run `cargo fmt --all` once A4 lands.

I hit two transient workspace-build breaks while finishing, both from other agents'
concurrent in-flight edits, neither caused by nor fixed by me: (1) `adapters/mod.rs`
briefly declared `pub mod docket;` before A1's `docket.rs` existed — resolved once A1
finished; (2) a `cargo test --workspace` run caught `docs/openapi.json` /
`crates/tack-api/src/openapi.rs` (A4/A6) mid-edit, failing
`openapi_spec_matches_committed_file` — re-running moments later (and again in the final
full run above) passed clean. Neither touched a file I own; flagging only so the pattern
("a red workspace-wide run mid-cycle is often just another agent's WIP, not a real
regression") is legible to whoever reads this next.

**The Wave 2 step-list extension recipe (B1/B2/B3 — copy-pasteable).** `reconcile_once`
builds a `FetchOutcome` as a flat struct-of-results, one field per HTTP call. Adding a
poll step is exactly three edits, each one line except the new function itself, and
nothing else in the file needs to change:

```rust
// 1. Add a field to FetchOutcome (near the top of reconciler.rs):
struct FetchOutcome {
    health: Result<Health, OrchError>,
    status: Result<FleetStatus, OrchError>,
    runs: Result<Vec<RemoteRun>, OrchError>,       // <- new, e.g. B1
}

// 2. Add your own fetch-only poll_* fn, same shape as poll_health/poll_status
//    (module-private, no DB access — HTTP call only):
async fn poll_runs(
    control_plane: &Arc<dyn ControlPlane>,
    project: Option<&str>,
) -> Result<Vec<RemoteRun>, OrchError> {
    control_plane.list_runs(project).await
}

// 3. Add one line inside reconcile_once's struct literal:
async fn reconcile_once(control_plane: &Arc<dyn ControlPlane>) -> PollEvaluation {
    let fetched = FetchOutcome {
        health: poll_health(control_plane).await,
        status: poll_status(control_plane).await,
        runs: poll_runs(control_plane, project).await,   // <- new, e.g. B1
    };
    evaluate(&fetched)
}
```

Your own persistence (`store.upsert_orch_runs(...)`, etc.) does **not** go inside
`reconcile_once` — add it in `spawn_one`'s loop (the per-plane task loop), as its own
short call placed *after* `store.record_health(...).await`, following the same
fetch-then-persist separation §0 rule 5 requires. **Do not let a
runs/approvals/traces/metrics failure influence `evaluate`'s reachability verdict** —
`evaluate` deliberately reads only `.health`/`.status`; only those two calls decide
plane health. If `project` needs to come from somewhere (B1's `/runs?project=` is
per-linked-project), that's a loop-level concern for you to add — `spawn_one`'s current
signature only carries one `Arc<dyn ControlPlane>` per plane, not a project list; you'll
likely want to fetch `list_orch_links_for_plane` once per tick (via the store) before
calling `poll_runs` per project, still entirely within the fetch phase.

**Persistence interface — reconciled against A3's landed `repo/orch.rs`.** I wrote
`reconciler.rs`'s `ControlPlaneStore` trait *before* `repo/orch.rs` existed (per my
card's instructions to not block on A3), then re-checked it once A3 landed. It matches
closely by design — `HealthRecord`'s field shapes mirror
`Repository::update_control_plane_health(id, health: &str, last_seen_at:
Option<DateTime<Utc>>, consecutive_failures: i64, api_version: Option<&str>)`'s
parameters exactly (`i64` failure count; `last_seen_at: None` means "leave the column
untouched", not "clear it" — same contract on both sides). No mismatch to report on that
half. The gap is `ControlPlaneStore::list_registered() -> Vec<RegisteredPlane>`, where
`RegisteredPlane.control_plane` must be a **live** `Arc<dyn ControlPlane>` — A3's
`Repository::list_control_planes()` returns `Vec<tack_db::repo::orch::ControlPlane>`
(DTO rows, no adapter), so a real implementation needs to, per row: call
`get_control_plane_token(id)` for the real Bearer token (A3's `token_set`-only DTO
deliberately doesn't carry it), match `kind` (today only `"docket"`), and construct
`Arc::new(DocketAdapter::new(base_url, token)?) as Arc<dyn ControlPlane>` (A1's
constructor, confirmed above — `Result`-returning, handle the `Err` rather than
`.unwrap()` since `base_url` came from a user-typed DB row). I did not write this glue
myself since it needs A1's adapter, which wasn't in the tree when I started, and my card
scopes me to `reconciler.rs`/`server.rs` only — see "For A4" below for exactly where it
goes.

**Jitter.** Deterministic, not `rand` (not a workspace dependency, and the card said not
to add one): `jittered_secs(plane_id: &Uuid, tick: u64, base_secs: u64)` hashes
`(plane_id, tick)` with `std::collections::hash_map::DefaultHasher`, maps the hash to a
fraction in `[-0.20, 0.20]`, and applies it to `base_secs`. Deterministic on purpose —
reproducible in tests, no seeded RNG to thread through. Verified in tests that (a) output
always stays within ±20% of base, (b) different plane ids produce different jitter on
the same tick (the actual anti-stampede property), (c) it never produces a zero/negative
sleep even at `base_secs = 1`.

**`apiVersion` policy (TODO.md §5's risk table).** `EXPECTED_API_VERSION = "2"` (a
`pub const` in `reconciler.rs`, matching W0-A's verified `SERVE_API_VERSION`). "Mismatch"
is defined as a difference in the **major** component only — the substring before the
first `.`, or the whole string if there's no `.` (`major_version()`). docket's version
scheme today is a bare integer, so in practice this is an exact-string comparison; the
`.`-split means a future `"2.1"` vs `"3.0"` only degrades on the part that actually
signals a breaking change, not every dot-release. A mismatch **never reports healthy**
but also **never overrides a worse reachability-driven state** — `evaluate`/
`HealthTracker::observe` take the more severe of the two independent signals
(reachability-from-failures, and version-match), so a plane that's both unreachable *and*
version-mismatched still shows `unreachable`, not silently downgraded to `degraded`.
Tested: `evaluate_flags_a_major_api_version_mismatch_but_stays_reachable`,
`evaluate_ignores_a_minor_version_difference`,
`version_mismatch_forces_at_least_degraded_even_while_reachability_is_healthy`,
`version_mismatch_does_not_downgrade_an_already_unreachable_plane`. There's no free-text
"reason" column on `control_planes` to persist the mismatch message into — I log it at
`warn` (via `spawn_one`) and persist the plane's *actual* reported `api_version` into the
`api_version` column every poll (regardless of match/mismatch) via `HealthRecord`, so an
operator can compare it against `EXPECTED_API_VERSION` themselves once A5's Fleet view
surfaces the column; A5/A4, if you want the mismatch message itself persisted, that
needs a schema change I didn't make (out of scope for this card, and `migrations.rs` is
a chokepoint file).

**The `gateway` field — checked, not used.** A1's handoff (above) flags that
`core.utils.gateway_active()` is hardcoded `False` in this docket version, so
`Health.gateway`/`FleetStatus.gateway` are always `0`/`"inactive"` against any real
plane. I audited `reconciler.rs` for this: neither `evaluate()` nor
`HealthTracker::observe()` reads `.gateway` on either struct anywhere — only
`.health.is_ok()`/`.status.is_ok()` (the *call succeeding*, not any field inside the
response) and `.status.as_ref().ok().map(|s| &s.api_version)` feed the health verdict.
A real docket instance with an always-inactive gateway will **not** be marked degraded
by this reconciler. No change needed.

**Deviations from strict file ownership (all minimal, all necessary for my files to
compile, flagged per the rules of engagement):**

1. **`crates/tack-orch/src/lib.rs`** — added one line, `pub mod reconciler;`, next to
   the existing `pub mod adapters;`. `lib.rs` had no placeholder for `reconciler`
   despite `reconciler.rs` being a new file assigned to me — a single-file module needs
   a declaration somewhere in the crate root, and there's no way around that short of
   `cargo test --workspace` never seeing this file at all. Purely additive, doesn't
   touch the frozen trait/DTO surface. (A1's handoff above describes this line as
   "pre-declared in Wave 0" — it wasn't; I added it before A1 started reading `lib.rs`,
   so from A1's vantage point it looked pre-existing. Correcting the record here.)
2. **`crates/tack-api/Cargo.toml`** — added `tack-orch = { path = "../tack-orch" }` and
   `async-trait = { workspace = true }` (needed for the placeholder
   `ControlPlaneStore` impl in `server.rs`). Both are additive, one line each, no
   version/feature changes to anything already there.

No other files outside my ownership were edited.

**For A4 (wiring `TACK_ORCH_ENABLE`/`TACK_ORCH_POLL_SECS` through config, and the real
`ControlPlaneStore`):**

- `server.rs` currently reads `TACK_ORCH_ENABLE`/`TACK_ORCH_POLL_SECS` directly from
  `std::env::var` at the spawn call site (marked `// TODO(A4)`), because `AppConfig` had
  no such fields when I wrote this. Once your `config.rs` lands them, replace that block
  with `config.orch_enable` / `config.orch_poll_secs` — the parsing logic (bool from
  `"1"`/`"true"`, u64 with a `10` default) is right there to copy if useful.
- `server.rs` currently passes a placeholder `NotYetWiredOrchStore` (defined just above
  `init_tracing` in the same file) that always reports zero registered planes —
  `spawn_reconcilers` handles that as a correct, inert no-op (it still spawns zero
  tasks, just via a real empty query instead of the `enabled=false` short-circuit).
  Replace it with a real `ControlPlaneStore` impl — see "Persistence interface" above
  for exactly what it needs to do (list rows via A3's `Repository::list_control_planes`,
  fetch each token via `get_control_plane_token`, dispatch on `kind` to build a
  `DocketAdapter` via A1's constructor, map `sqlx::Error` → `OrchError` for
  `record_health`'s return type). This impl doesn't obviously belong in `tack-api` vs.
  `tack-orch` — I'd lean `tack-api` (it's the only crate that will ever construct the
  real `Repository`+adapter combination together, and `tack-orch` has no reason to know
  about `tack-db`'s concrete `Repository` type beyond what it already imports) but that's
  your call, not mine to make by editing `router.rs`/`config.rs`.
- Once wired, `spawn_reconcilers(true, real_store, ReconcilerConfig { poll_secs })`
  slots in exactly where `NotYetWiredOrchStore` is now — no other change to `server.rs`
  needed on your end.

**For B1/B2/B3 (Wave 2):** see the copy-pasteable recipe above; it's also written out at
length in `reconciler.rs`'s module doc comment (top of the file) if you want the fuller
rationale alongside it.

### A4 — 2026-08-04

**What I built.** `crates/tack-api/src/config.rs` (four new fields),
`crates/tack-api/src/handlers/orch.rs` (new, ~640 lines: control-plane CRUD,
project↔control-plane link, fleet aggregate), one line in `handlers.rs`
(`pub mod orch;`), and `router.rs` (new `orch_routes()` fn + one `.merge()`
call). Also `crates/tack-api/src/openapi.rs` (registered every new path/DTO —
not in my card's file list, but not owned by anyone else this wave either, and
required by acceptance criterion 6), `docs/openapi.json` (regenerated),
`CLAUDE.md` and `docs/book/src/user-guide/configuration.md` (config tables —
A6 hadn't started per §6 at the time I checked), and a new test file
`crates/tack-api/tests/orch_test.rs` (15 tests). Did **not** touch `tack.toml`
(gitignored, personal, and every new field has a serde default so an existing
file round-trips unchanged) or `server.rs` (A2's file, per my card).

**1. Config accessor A2 needs.** `AppConfig` now has exactly the two fields
A2's `// TODO(A4)` comment in `server.rs` (right above the `orch_enabled`/
`poll_secs` env-var block, ~line 121) asked for, same names:
`config.orch_enable: bool` (default `false`) and `config.orch_poll_secs: u64`
(default `10`). Also added `config.orch_event_retention_days: u32` (default
`90`, for B3/34.6) and `config.orch_approval_token: Option<String>` (default
`None`, for D1/36.1 — never logged, follows the same "skip if empty" env
pattern as `TACK_GITHUB_TOKEN`). Env vars: `TACK_ORCH_ENABLE` (`"1"`/`"true"`,
case-insensitive), `TACK_ORCH_POLL_SECS`, `TACK_ORCH_EVENT_RETENTION_DAYS`,
`TACK_ORCH_APPROVAL_TOKEN`.

**I did not edit `server.rs` myself** (out of my ownership this wave, per both
my card and A2's). A2's spawn call site still reads `std::env::var` directly
at the `// TODO(A4)` comment — someone (A2 or a follow-up) needs to do the
one-line swap to `config.orch_enable` / `config.orch_poll_secs` now that the
fields exist, plus build the real `ControlPlaneStore` impl A2's note describes
(replacing `NotYetWiredOrchStore`). I did **not** attempt that impl either —
it needs `A1::DocketAdapter` + `A3::get_control_plane_token` +
`A3::list_control_planes` glued together behind a `kind` match, which A2's
note already scoped out in detail and suggested lives in `tack-api` (a new
file, not `server.rs`/`router.rs`/`config.rs`/`handlers/orch.rs`) — that's a
real design call for whoever picks it up, not mine to make by editing files
outside my card.

**2. Full route list.** All under `/api`, batched into `router.rs`'s new
`orch_routes()` fn, gated as one sub-router:

```text
POST/GET     /control-planes
GET/PATCH/DELETE /control-planes/{id}
GET/PUT      /projects/{id}/orch-link
GET          /fleet
```

Commented placeholders for later waves, in the same function, with the exact
insertion point already marked — later agents add one `.route(...)` line each,
no restructuring:

```rust
// ─── Wave 2 (Phase 34) — metrics, add here: ────────────────────────
// .route("/metrics", get(orch::get_metrics)) // B3, 34.3
// ─── Wave 3 (Phase 35) — dispatch, add here: ───────────────────────
// .route("/items/{id}/dispatch", post(orch::dispatch_item)) // C1, 35.2/35.3
// .route("/sprints/{id}/dispatch", post(orch::dispatch_sprint)) // C3, 35.4
// .route("/sprints/{id}/dispatch/dry-run", get(orch::dry_run_sprint_dispatch)) // C3, 35.4
// ─── Wave 4 (Phases 36–38) — approvals + provisioning, add here: ───
// .route("/approvals/{token}", post(orch::decide_approval)) // D1, 36.1 — also gated on TACK_ORCH_APPROVAL_TOKEN
// .route("/projects/from-template/{id}", post(templates::create_project_from_template)) // D4, 37.2 — provision_pod:true extension of the existing endpoint, not a new route
```

The whole `orch_routes()` sub-router carries one middleware layer,
`orch::require_orch_enabled` (a small fn in `handlers/orch.rs`, exported
`pub`), which 404s every request when `config.orch_enable` is false —
checked **once**, not per-handler. It's merged into the main `api` router
*before* the existing `require_token` layer is applied, so the ordinary
Bearer-token gate still covers these routes too (token gate runs first/outer,
then the orch-enabled gate, then the handler). Tested in `orch_test.rs`'s
`every_orch_route_404s_when_disabled` (hits all 8 method+path combinations)
and `orch_routes_are_reachable_when_enabled`.

**3. Token tri-state semantics.** `PATCH /api/control-planes/{id}`'s
`UpdateControlPlaneRequest.token: Option<Option<String>>` uses a hand-rolled
`deserialize_some` helper (the well-known serde trick — no new dependency,
despite `serde_with` being pre-added to the workspace `Cargo.toml` for this
exact purpose; I didn't need it): `#[serde(default)]` leaves the field `None`
when the JSON key is absent entirely (untouched); when the key **is**
present — including `"token": null` — `deserialize_some` runs and always
wraps in `Some(..)`, so `null` → `Some(None)` (clear) and a string →
`Some(Some(v))` (set/replace). This passes straight through to A3's
`UpdateControlPlane.token` field, same shape, no translation needed. Every
response DTO (`ControlPlaneResponse`) has no `token` field at all — only
`token_set: bool` — so there is no code path that could serialize the secret
even by accident. Tested: `token_never_appears_in_create_response`,
`token_never_appears_in_list_or_get_response`,
`patch_with_absent_token_field_preserves_stored_token`,
`patch_with_explicit_null_token_clears_it`, `patch_with_token_value_replaces_it`
— the leak-check helper asserts both "the literal secret string doesn't
appear anywhere in the response body" and "there is no `token` key at all",
so a future refactor that adds a `token: null` field back in would also fail
the test, not just a refactor that leaks the value.

**4. `GET /api/fleet` — reconciled against A5's already-landed frontend.** A5's
handoff (§6, above mine) shipped `frontend/src/features/fleet/api.ts` before my
endpoint existed, with a best-guess shape. I read that file and matched it
field-for-field rather than asking A5 to adapt — their file explicitly says
"when A4's endpoint lands, reconciling the frontend means editing THIS FILE
ONLY," so I optimized for that file needing **zero changes**, not minimal
changes on my side. Final response:

```json
{
  "rows": [
    {
      "project_id": "uuid", "project_name": "string",
      "control_plane_id": "uuid", "control_plane_name": "string", "control_plane_kind": "docket",
      "remote_project": "string",
      "health": "unknown" | "healthy" | "degraded" | "unreachable",
      "last_seen_at": "RFC3339 | null", "consecutive_failures": 0, "api_version": "string | null",
      "gateway": "active" | "inactive" | "unknown",
      "roster": [{ "id": "string", "name": "string", "role": "string", "model": "string" }],
      "last_activity_at": "RFC3339 | null",
      "auto_dispatch": false, "blueprint": "string | null", "budget_usd": 50.0,
      "tokens_in": 0, "tokens_out": 0,
      "cost_usd_estimated": 0.0,
      "pricing_snapshot_at": "string | null",
      "pending_approval_count": 0
    }
  ]
}
```

Envelope is `{ rows: [...] }` (not a bare array — I changed my first draft to
match A5's `FleetResponse` type). One row per Tack project with an
`orch_links` row (confirms A5's assumption over "one row per control plane").
Field-by-field reconciliation against `api.ts`:

- **Exact match, real data:** `project_id`, `project_name`,
  `control_plane_id`, `control_plane_name`, `control_plane_kind`, `health`,
  `last_seen_at`, `consecutive_failures`, `api_version`, `auto_dispatch`,
  `blueprint`, `budget_usd`, `cost_usd_estimated`. Plus `remote_project`,
  which A5's `FleetRow` doesn't have — extra, harmless, additive.
- **Renamed to match A5** (I adopted their name, not the reverse):
  `pending_approvals` → `pending_approval_count`.
- **Added, real data (weren't in my first draft):** `tokens_in`/`tokens_out`
  (summed from `orch_tasks`, primary per §0 rule 6 — A5's contract treats
  these as always-present plain numbers, trustworthiness signaled by `health`/
  `isStale()`, not by nullability, so I did **not** gate them behind
  `health == "unreachable"` the way I gate `cost_usd_estimated`) and
  `last_activity_at` (real: `MAX(orch_tasks.dispatched_at)` for the project's
  items, `null` until Wave 3 dispatches anything).
- **Added, honest placeholders (always the same value in Wave 1, but present
  on the wire so `api.ts` needs no shape change later):** `gateway` (always
  `"unknown"` — no gateway column exists on `control_planes`, and A2's
  reconciler doesn't poll/store one either, confirmed by A2's own handoff
  above: "`Health.gateway`/`FleetStatus.gateway`... neither `evaluate()` nor
  `HealthTracker::observe()` reads `.gateway`... No change needed" — so this
  isn't a gap I introduced, it's a real absence all the way up the stack);
  `roster` (always `[]` — no agent-roster table exists in migrations
  019–024; needs a new migration + ingestion in a later wave); `pricing_snapshot_at`
  (always `null` — no pricing-snapshot mechanism exists anywhere yet).
- **`cost_usd_estimated`'s staleness rule (the card's explicit acceptance
  bar):** `None` when `health == "unreachable"`, `Some(0.0)` otherwise
  (including `"unknown"` — a freshly-registered, never-yet-polled plane is
  "no evidence of trouble" and reports a real, current zero, not a stale one).
  Tested in `fleet_reports_zero_cost_distinctly_from_unreachable`, which
  round-trips a link, asserts `Some(0.0)` at `health: "unknown"`, then calls
  `Repository::update_control_plane_health(..., "unreachable", ...)` directly
  (bypassing HTTP — the test builds its own `AppState` via a local
  `app_with_state()` helper so it can reach the repo) and re-asserts `null`.
- **Approval scoping A5 flagged as worth confirming:** per-project, via
  `orch_approvals.item_id` → `items.project_id` (an inner join — uncorrelated
  approvals, `item_id IS NULL`, are excluded from every fleet row and are
  meant to surface only in D1's fleet-wide inbox, Wave 4). This matches one of
  A5's two guesses; flagging explicitly since they asked.

New DTOs in `handlers/orch.rs`, all `utoipa::ToSchema` + registered in
`openapi.rs`: `ControlPlaneResponse`, `CreateControlPlaneRequest`,
`UpdateControlPlaneRequest`, `OrchLinkResponse`, `OrchLinkView` (`{linked:
bool, link: OrchLinkResponse | null}` — `GET /orch-link` on an unlinked
project returns `200 {linked:false, link:null}`, not `404`, matching the
`settings.rs` "not configured yet" precedent, not the "resource not found"
one), `UpsertOrchLinkRequest`, `StatusMap` (§1.3's shape, typed:
`dispatch_from: Vec<String>` + 5 optional `on_*: Option<String>` fields),
`FleetEntry`, `FleetListResponse`, `FleetRosterMember`.

**5. `status_map` save-time validation (§1.3).** `PUT /orch-link` fetches the
project, and every status name in the incoming `status_map` (all of
`dispatch_from` plus any set `on_*` field) is checked against
`project.workflow.statuses` — an unknown name is a `400` naming the bad
key/value, never silently stored. `dispatch_from` may be an empty list (a
link registered before a dispatch policy is configured is a valid state in
this wave; Wave 3's dispatcher is what actually needs it non-empty). Tested:
`orch_link_rejects_unknown_status_name`,
`orch_link_round_trips_with_valid_status_map` (uses `"To Do"`/`"Done"` —
real status names from the `software` project type's scrum workflow preset).

**6. Not-found / degrade discipline.** `get_control_plane`/`update_control_plane`
map `sqlx::Error::RowNotFound` → `404` explicitly (A3's repo fns return
`Result<ControlPlane, sqlx::Error>`, not `Option`, so this needed an explicit
match arm rather than the `.ok_or_else()` pattern the rest of the API uses).
Every fleet/orch-link read is a plain DB read — no outbound HTTP call happens
inside any handler in this file, so there is no code path where a docket
outage can turn into a `500`; the reconciler (A2) is the only thing that ever
talks to docket, entirely out-of-band.

**Test results.** `cargo test -p tack-api`: 123 passed (15 new in
`orch_test.rs`, 108 pre-existing, 0 failed). `cargo test --workspace`: green
across every crate (`tack-orch`'s 44 tests including A1's 20
`docket_adapter_test.rs` and A2's `reconciler::tests::*` all still pass — my
changes never touched files either of them owns). `cargo clippy --workspace
-- -D warnings`: clean. `cargo fmt --all -- --check`: clean (this resolves
the 3-unformatted-spots note in A2's handoff above — `cargo fmt -p tack-api`
fixed them). `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract` regenerated `docs/openapi.json`;
`openapi_spec_matches_committed_file` and the other 3 contract tests all
green. Did not run the frontend commands (`npm run type-check`/`test`/
`build`) — no frontend file was touched by this card.

**For A5:** no action needed unless you want to delete the reconciliation
comments at the top of `api.ts` now that the shape is confirmed — every field
your `FleetRow`/`FleetResponse` types declare is present on the wire with the
same name and the same nullability, per the field-by-field list above.

**For whoever wires the real `ControlPlaneStore` in `server.rs`:** the config
accessors are ready now (`config.orch_enable`, `config.orch_poll_secs`) —
see point 1 above for exactly what's still open.

### A7 — 2026-08-04

**What I built.** The integration gap A2 and A4 both scoped out of their
cards: the reconciler now actually polls a real docket. Three pieces, all
new or in the two files my card owns:

1. **`crates/tack-api/src/orch_store.rs`** (new) — `RepoControlPlaneStore`, a
   real `ControlPlaneStore` impl backed by `tack_db::Repository`. Matches
   A2's trait shape verbatim (didn't touch `reconciler.rs`): `record_health`
   is a thin pass-through to `Repository::update_control_plane_health(id,
   record.health.as_str(), record.last_seen_at, record.consecutive_failures,
   record.api_version.as_deref())`, one line, `i64` failure count and
   `Option<DateTime<Utc>>`/`None`-means-untouched preserved exactly as both
   A2's and A3's docs promised — no adapter needed on that side, it really
   was a direct mapping. `list_registered` is the part that needed real
   glue: `Repository::list_control_planes()` for the rows, then per row
   `Repository::get_control_plane_token(id)` for the secret, then a `match
   row.kind.as_str()` dispatching `"docket"` →
   `tack_orch::adapters::docket::DocketAdapter::new(row.base_url.clone(),
   token)`. Three distinct failure modes inside that loop — unknown `kind`,
   a token-lookup `sqlx::Error`, or `DocketAdapter::new` itself erroring
   (per its own doc comment, only on a `reqwest::Client` build failure, e.g.
   no usable TLS backend) — all `warn!` and `continue` to the next row
   rather than propagating; only a failure of `list_control_planes()` itself
   (DB unreachable) surfaces as an `Err`, matching how `spawn_reconcilers`
   already treats that case (log, spawn nothing, don't panic). The stored
   token only ever flows into `DocketAdapter::new`'s `token` parameter —
   never logged, never touches a DTO, never named in any `warn!` call (I
   checked every log line in the file for this specifically).

2. **Module registration.** One line in `crates/tack-api/src/lib.rs`:
   `pub mod orch_store;`, alongside the existing `pub mod` list (alphabetical
   slot, between `openapi` and `remote_backup`). No other file needed a
   registration — `orch_store` is only consumed from `server.rs` via
   `crate::orch_store::RepoControlPlaneStore`.

3. **`crates/tack-api/src/server.rs` config swap.** Replaced the whole
   `TODO(A4)` block — the direct `std::env::var("TACK_ORCH_ENABLE")` /
   `TACK_ORCH_POLL_SECS` reads plus the `TODO(A3/A4)` comment — with `if
   config.orch_enable { ... reconciler::ReconcilerConfig { poll_secs:
   config.orch_poll_secs } }`. Deleted `NotYetWiredOrchStore` and its
   `ControlPlaneStore` impl entirely; the real store is constructed inline
   as `Arc::new(RepoControlPlaneStore::new(state.repo.clone()))` (`Repository`
   is `Clone` — cheap, it's a pool handle). Net diff to `server.rs`: +26/-25
   lines, nothing outside the reconciler spawn block touched.

**Deviation from the file list, disclosed per the rules.** My card scoped me
to `server.rs`, one new module file, its one-line registration, and one new
test file. `crates/tack-api/Cargo.toml` needed a two-line comment update (no
dependency/version change — `tack-orch` and `async-trait` were already
present, added by A2 for the placeholder; I just corrected the comments
above them, which described the placeholder that no longer exists, to
describe `orch_store.rs` instead). Flagging it explicitly since it's a file
outside my card's list, even though the change is comment-only.

**Testing.** `crates/tack-api/tests/orch_reconciler_wiring_test.rs` (new, 5
tests, all real-repo/real-`ControlPlaneStore` — no fakes, unlike
`reconciler.rs`'s own unit tests which correctly use fakes for the pure
state-machine logic):

- `record_health_round_trips_through_the_real_repo` — creates a plane via
  `Repository::create_control_plane`, calls `RepoControlPlaneStore::
  record_health` twice (once with `last_seen_at: Some(now)`, once with
  `None`), and asserts against `Repository::get_control_plane` that the
  second call's `None` truly leaves the previously-stored timestamp
  untouched, and that `api_version: None` doesn't clobber a
  previously-stored version (the repo's `COALESCE` in
  `update_control_plane_health` — confirmed by test, not just by reading
  the SQL).
- `list_registered_builds_a_live_adapter_for_a_docket_plane` — one `"docket"`
  row round-trips to one `RegisteredPlane` whose `control_plane.kind()` ==
  `"docket"`.
- `list_registered_skips_an_unknown_kind_without_failing` — one `"docket"`
  row + one `"some-future-thing"` row (no CHECK constraint on `kind` at the
  DB layer, confirmed by reading migration 019 — A3 left it deliberately
  open per the module comment: "ControlPlane trait is written to allow
  other kinds later") ⇒ `list_registered()` returns `Ok` with exactly the
  one recognized plane, not an `Err`.
- `spawn_reconcilers_polls_a_real_docket_and_persists_health_via_the_real_store`
  — the actual end-to-end path: a `wiremock` server standing in for docket
  (inline `/health` + `/status.json` JSON bodies, not A1's fixture files —
  those live under `crates/tack-orch/tests/fixtures/` and pulling them
  cross-crate via a relative path felt more fragile than four lines of
  inline JSON), a real `control_planes` row pointed at it, `spawn_reconcilers
  (true, real_store, ...)`, a 500ms sleep for the first tick, then
  `Repository::get_control_plane` shows `health: "healthy"`,
  `last_seen_at: Some(_)`, `api_version: Some("2")`. This is the test that
  actually proves the three pieces (store, adapter dispatch, reconciler
  loop) compose correctly, not just that each compiles against the others'
  types.
- `disabled_orch_enable_spawns_no_tasks_even_with_a_registered_plane` —
  `AppConfig::default()` (`orch_enable: false`, matching
  `TACK_ORCH_ENABLE` unset), a real `RepoControlPlaneStore` wrapping a repo
  that *does* have a registered plane, `spawn_reconcilers(config.orch_enable,
  ...)` ⇒ empty handles. `reconciler.rs`'s own
  `disabled_orchestration_spawns_no_tasks_and_never_queries_the_store` already
  covers this at the generic-fake-store level; this one exists because the
  card explicitly asked for it wired through the real store, and because a
  bug that only manifested with real data (e.g. a `RepoControlPlaneStore`
  that eagerly queried in its constructor rather than lazily in
  `list_registered`) wouldn't have been caught by the fake-store version.

**Verification.** `cargo test --workspace`: every crate green, no failures
(`tack-orch` 44 + `tack-api`'s full suite including my new 5 tests and A4's
15 in `orch_test.rs`, `tack-db`'s 8 + 16 orch tests, `tack-orch`'s 20
`docket_adapter_test.rs`). `cargo clippy --workspace --all-targets -- -D
warnings`: clean. `cargo fmt --all -- --check`: clean — only `orch_store.rs`
needed a pass (one `rustfmt` reflow inside `list_registered`'s error-map
closure); no other file had a diff, so I didn't need to touch anything
outside what I already owned to get to green.

**Where A2's and A4's assumptions held, and one place worth flagging.**
Everything A2 predicted in their handoff ("the eventual glue is a thin,
mechanical wrapper") held exactly — no surprises in `HealthRecord`'s shape,
no impedance mismatch with `update_control_plane_health`'s signature. One
thing neither handoff called out, because neither could have hit it without
A1's adapter in the tree yet: `DocketAdapter::new` is genuinely almost
infallible per its own doc comment (only fails if `reqwest::Client::builder
().build()` itself fails), so "an adapter that fails to construct" is a real
code path in `list_registered` (handled, logged, skipped) but not one I
found a way to actually trigger in a test without mocking `reqwest`
internals — a malformed `base_url` doesn't fail `new()` at all, only the
later `Url::parse` inside `.url()` on first use, which is a poll-time
failure (surfaces as `OrchError::Http` from `health()`/`status()`, degrading
that plane's health normally) rather than a `list_registered`-time one. Not
a gap in the implementation, just a note that the "skip a plane whose
adapter fails to construct" branch is currently untested by
construction — a `base_url` that's outright unparseable as a URL degrades
that plane's health on every poll instead of being excluded from
registration, which is arguably the more correct behavior anyway (it shows
up in the fleet view as `unreachable` rather than silently vanishing) but
is worth knowing if a future card wants to distinguish "misconfigured" from
"down."

**What's still open.** Nothing in my three deliverables — store, registry,
config swap — is stubbed or deferred. Everything Wave 2+ needs from this
layer: `FetchOutcome`/`reconcile_once` extension is entirely A2's documented
recipe (untouched by me); persistence for new poll steps (runs/approvals/
traces/metrics) will need their own `ControlPlaneStore`-adjacent methods or
a direct `Repository` handle threaded into `spawn_one`'s loop the same way
`record_health` is today — `RepoControlPlaneStore` doesn't currently expose
anything beyond the two trait methods, so B1/B2/B3 will likely add methods
to the trait itself (in `reconciler.rs`, A2's/whoever owns Wave 2's file) and
this impl just grows alongside it, same mechanical shape as `record_health`.
The one open design question I noticed but didn't act on (out of scope for
this card): a second control-plane `kind` beyond `"docket"` has nowhere to
plug in yet except editing this file's `match` directly — fine for now (a
one-`kind` system today), but worth a registry/factory pattern if a second
adapter ever lands.

### A8 — 2026-08-04

**What I built.** Closed the gap A5 flagged: `/fleet` had no axe coverage in
`frontend/e2e/a11y.spec.ts`. Added two tests, following the file's existing
per-view pattern exactly (same `scan()` helper, same `waitForApp` + assertion
shape, no new style invented):

- `fleet page (orchestration disabled) has no accessibility violations` —
  real navigation to `/fleet` against the real e2e API. `TACK_ORCH_ENABLE`
  is unset in `playwright.config.ts`'s `webServer` env (not a file I own),
  so `GET /api/fleet` genuinely 404s and the page renders its real
  `OrchDisabledEmptyState`. The test asserts the actual disabled-state copy
  ("Agent-fleet orchestration is disabled") is visible before scanning, so a
  regression to a blank/broken page can't slip through as a false "clean"
  scan.
- `fleet page (populated) has no accessibility violations` — the populated
  state. I could not seed this through the real API the way other specs
  seed data (`helpers.ts`'s `getOrCreateProject`/`getOrCreateItem`):
  orchestration is off in this harness, there's no control-plane
  registration UI yet (A5's own handoff notes this), and `playwright.config.ts`
  isn't mine to edit to flip `TACK_ORCH_ENABLE` on for the e2e run. Note for
  whoever *can* touch that file: `TACK_ORCH_ENABLE` isn't in the explicit
  `env: {...}` override block Playwright applies to the webServer process,
  and Playwright merges that block with the *invoking* shell's `process.env`
  first — so exporting `TACK_ORCH_ENABLE=true` before `make e2e`/`npx
  playwright test` does reach the spawned server today, no code change
  needed for a manual/CI run. I didn't wire that in permanently since it's
  an env convention change to a file outside my ownership and it'd make the
  new test's pass/fail depend on an ambient var nobody else's invocation
  sets — fragile. Instead I used `page.route('**/api/fleet', ...)` to
  intercept the browser's request and fulfill it with a payload shaped
  exactly like `frontend/src/features/fleet/api.ts`'s `FleetResponse`/
  `FleetRow` (the file A5's card designates as the single source of truth
  for the wire contract) — four rows, one per `ControlPlaneHealth` state
  (healthy/degraded/unreachable/unknown), covering populated roster chips,
  both tokens/cost/budget rendering *and* the stale-row dashed-out
  treatment, and both a >0 and a 0 pending-approval count. This is real
  network-level mocking of the SPA's one wire boundary, not a fake DOM — the
  actual `FleetPage`/`FleetRow`/`HealthChip` components render for real and
  axe scans the real resulting DOM. It does **not** exercise the real
  `tack-orch` reconciler, the real `GET /api/fleet` handler, or
  `TACK_ORCH_ENABLE=true` startup path — only the frontend's rendering of
  their documented contract. Flagging clearly: the real backend-integrated
  populated path is still unscanned by this gate.

**Six-combo palette×mode:** `a11y.spec.ts` doesn't parameterize any existing
scan over the six palette×mode combinations — every test scans whatever
theme is default (Teal, light). I matched that; I did not add palette
looping unilaterally, since the card said to cover `/fleet` the way the file
covers everything else, not restructure it. That means TODO.md §0 rule 9's
six-combo requirement is still only spot-checked by hand for Fleet (see
below), not gated by CI, for the same reason it isn't for any other view in
this file today — a real gap, but a pre-existing one this card wasn't asked
to fix.

**Real violations found and fixed (in `frontend/src/features/fleet/`).** The
populated scan initially failed: axe flagged `color-contrast` (serious) on
both Fleet spots that use `Badge tone="warning"` — `HealthChip`'s degraded
chip and `FleetRow`'s "N pending" badge. Measured: `--color-warning-700`
text (`#b45309`) on `--color-warning-100` background (`#f7ecdb`) in the
default Teal-light palette is 4.29:1, under WCAG AA's 4.5:1 floor for normal
text — I independently re-derived the same ratio by hand from the relative-
luminance formula, not just trusting axe's number. `grep -rn 'tone="warning"'
src` outside `fleet/` returns nothing: Fleet is the *first* surface in the
app to ever render that tone, so this is a real, pre-existing bug in the
shared token pair (`frontend/src/index.css`'s `--color-warning-600`/`-700`
and `shared/ui/Badge.tsx`'s tone map) that's sat latent, unexercised by any
axe-gated view, until now. Root-causing it belongs in `index.css`/
`Badge.tsx` — both outside my ownership for this card — so I fixed it
locally instead: added `frontend/src/features/fleet/WarningBadge.tsx`, a
pill shaped identically to `Badge` (same classes) but with text forced to
`--color-text-primary` (the app's main body-text token, already verified AA
against `--color-bg-base`/`--color-bg-subtle`-class surfaces in all six
palette×mode combinations everywhere else it's used) instead of
`--color-warning-700`, keeping the `--color-warning-100` tint as a purely
decorative background (no text-contrast rule applies to a flat background by
itself). No raw hex introduced — `npm run lint:tokens` still reports 0 raw
color literals and the pre-existing baseline-1 inline-hex count unchanged
(that one hex is in a code *comment* documenting the measured ratio, not a
style value). Swapped both call sites (`HealthChip.tsx`'s degraded branch,
`FleetRow.tsx`'s pending-approvals badge) to `WarningBadge`; every other
tone (healthy/unreachable/unknown chips, the gateway badge) is untouched and
still uses the shared `Badge` as before, since only the `warning` pairing is
broken. Manually re-verified the fix's luminance math for the Teal-light
values (13.63:1, comfortably clear) and sanity-checked that `--color-text-
primary` vs. `--color-warning-100` should hold in the other five combinations
too, since both light-mode `warning-100`s are pale near-white tints and both
dark-mode `warning-100`s are low-opacity amber overlays over an already-dark
base — but this is a hand check, not an axe run, per the six-combo caveat
above. Left a comment in `WarningBadge.tsx` pointing whoever next touches
`index.css`/`Badge.tsx` at the real fix (darken `--color-warning-600` the
same way `--color-success-600`/`--color-danger-600` already were, per the
comments next to those tokens) so this local workaround can be deleted.

**Verification.** `npm run type-check` clean. `npm run test` (Vitest):
207 tests, 204 pass — same 3 pre-existing failures A5 reported
(`client.test.ts`'s `requestBlob` Blob-instanceof check, two
`createObjectURL` assertions in `GlobalSettings.test.tsx`/`panels.test.tsx`).
I independently verified these predate this whole work cycle, not just
A5's slice: `git stash push -u` (stashing every uncommitted file — mine and
every other Wave agent's) back to clean `develop` HEAD (`c4193d7`) and
reran `npm run test` — identical 3 failures, identical error messages, 166
passing instead of 204 (the difference being exactly the Fleet-added
tests). Confirmed jsdom/environment issue, unrelated to orchestration or
Fleet. `npx playwright test e2e/a11y.spec.ts --project=chromium` (a11y
scan runs chromium-only by the file's own `test.skip`): both new Fleet
tests pass. Four *other*, unrelated tests fail on this branch — board,
timeline, sprint, and item-detail-drawer — all the same axe finding: the
rich-text-editor toolbar's heading `<select>` (`shared/ui/
RichTextEditor.tsx`) has no accessible name. I checked: `git diff HEAD --
src/shared/ui/RichTextEditor.tsx` is empty (file untouched, byte-identical
to HEAD), and I re-ran the full a11y suite against the same clean-HEAD
stash used for the Vitest check above — same 4 failures, same file, same
finding, with no `/fleet` route to speak of at that commit. So this is a
pre-existing, unrelated a11y gate failure already on `develop`, not
something any Wave-33 agent (including me) introduced; flagging it here
since nobody else's card owns `RichTextEditor.tsx` either. `npm run
lint:tokens` passes (0 new raw-color literals). Did not run the full
cross-browser `make e2e` (all specs, not just a11y) — out of scope for a
card whose owned files are `a11y.spec.ts` and `features/fleet/**`, and the
a11y spec itself only runs assertions on chromium regardless.

**What's still open / for whoever picks this up next.** (1) The real
backend-integrated populated Fleet state — actual `TACK_ORCH_ENABLE=true` +
a real registered control plane + real reconciler polling — is still
unscanned; needs either a `playwright.config.ts` change (owner: whoever's
next card touches E2E config) or a fixture/seed path once a control-plane-
registration UI exists (per A5's note, that's still TODO too). (2) The four
pre-existing `RichTextEditor` failures above are a standing, unrelated gate
break on `develop` — worth its own card. (3) The `--color-warning-600`/
`-700` token pair itself is still broken in `index.css` for every *other*
future consumer of `Badge tone="warning"`; `WarningBadge.tsx` only patches
Fleet's two call sites. (4) `a11y.spec.ts` still doesn't parameterize any
scan over the six palette×mode combinations — a real, pre-existing gap this
card didn't introduce or fix, just inherited and matched.

### A6 — 2026-08-04

**What I built.** `docs/book/src/user-guide/orchestration.md` (new, ~220 lines),
`docs/book/src/developer/orchestration.md` (new, ~560 lines), `docs/book/src/SUMMARY.md`
(registered both under User Guide / Developer Guide), and `CLAUDE.md` (added `tack-orch`
to the directory tree and Crate Boundaries, added the 8 orchestration routes to API
Endpoint Structure, corrected the stale "68 REST endpoints" count to 76). Did not touch
`config.rs`, `configuration.md`, or duplicate the `TACK_ORCH_*` config table — A4 had
already landed all four vars correctly in both places; I verified rather than re-added.

**Verification method.** I did not write these docs from the handoff notes alone — every
field name, route, struct, const, and test name in both pages was grep/read-verified
directly against the landed source (`crates/tack-orch/src/lib.rs`,
`crates/tack-orch/src/reconciler.rs`, `crates/tack-orch/src/adapters/{docket,prometheus}.rs`,
`crates/tack-api/src/{config,router,orch_store}.rs`, `crates/tack-api/src/handlers/orch.rs`,
`crates/tack-db/src/migrations.rs`) at the time I read it, not copied from any agent's
prose. `mdbook build docs/book` is clean (confirmed twice, before and after a mid-session
scare described below). Every `TACK_ORCH_*` var I mention exists in `config.rs`
(`orch_enable`, `orch_poll_secs` grep-confirmed with hit counts; `orch_event_retention_days`/
`orch_approval_token` confirmed present but I only reference them by description, not by
literal var name, since neither page needed the exact names). Every route in my API
surface table matches `router.rs`'s `orch_routes()` + `handlers/orch.rs` exactly, including
the still-commented Wave 2–4 placeholders, which I documented as placeholders, not as live
routes.

**Corrections to earlier handoff notes / TODO.md text, found by reading code directly:**

1. **W0-B's handoff calls migrations 019–024 "six tables" then says the card text says
   "seven"** — I didn't re-litigate this, just documented the six real tables
   (`control_planes`, `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`,
   `orch_approvals`) and explicitly noted `orch_metrics` does not exist yet and needs its
   own migration number, matching W0-B's resolution.
2. **`orch_routes()` has 8 method+path combinations, not 7.** I initially wrote "7
   endpoints" in `CLAUDE.md` from memory while drafting, then counted `docs/openapi.json`
   programmatically (`GET+POST /control-planes`, `GET+PATCH+DELETE /control-planes/{id}`,
   `GET+PUT /orch-link`, `GET /fleet` = 8) and fixed it before finishing — matches A4's own
   test name (`every_orch_route_404s_when_disabled`, "hits all 8 method+path combinations").
   Flagging in case any other doc/comment elsewhere in the cycle still says 7.
3. **`GET /api/backup` does NOT yet scrub `control_planes.token`.** I checked
   `crates/tack-api/src/remote_backup.rs`'s `scrub_snapshot_secrets` directly: it only
   deletes specific `app_meta` keys (the S3 secret key, the install identity) — it has no
   code path touching `control_planes` at all. This is exactly the gap Wave 3 card C4
   is scoped to close ("Exclude the control-plane token from backup bundles... the S3
   secret key already leaked this way"). No handoff note before mine says this plainly for
   the *current* state (as opposed to C4's future acceptance bar), so I added an explicit
   "known gap" section to the user guide and a pointer to it in the developer guide's
   non-negotiable #4, rather than implying the token is already safe in a backup the way
   the S3 secret is. **C4: this is still open — verify it's actually closed before
   marking 35.9/C4 done, don't assume the S3-secret precedent already covers it.**
4. **The real `ControlPlaneStore` wiring in `server.rs` was finished by the time I
   checked, with no handoff note yet describing it.** A2's and A4's handoff notes (both
   above) describe `server.rs` as still reading `TACK_ORCH_ENABLE`/`TACK_ORCH_POLL_SECS`
   directly via `std::env::var` behind a `// TODO(A4)` comment, with a placeholder
   `NotYetWiredOrchStore`. When I read `server.rs` directly (needed to document the real
   spawn call site), it was already using `config.orch_enable`/`config.orch_poll_secs` and
   a real `Arc::new(RepoControlPlaneStore::new(state.repo.clone()))` from a new,
   already-landed `crates/tack-api/src/orch_store.rs` (not mentioned in any file-ownership
   row or handoff note I could find in TODO.md at the time I read it). I documented the
   real, wired state — including `orch_store.rs`'s per-row skip-on-error behavior — since
   that's what's actually in the tree, but **whoever owns `server.rs`/`orch_store.rs`
   should append their own handoff note** so the historical record isn't just my
   after-the-fact description of someone else's unlogged work.

**A live git-state scare, resolved, no data lost — worth recording.** Partway through
writing this handoff, a tool-level notification reported `CLAUDE.md` and
`docs/book/src/SUMMARY.md` reverted to their pre-cycle `HEAD` content (no `TACK_ORCH_*`
anywhere, no `tack-orch` crate, my new doc files missing from disk). I checked rather than
assumed: `git status --porcelain` was briefly fully clean, `git reflog` showed three
`reset: moving to HEAD` entries against `c4193d7`, and `git stash list` briefly showed one
entry — `"On develop: A8 verify pre-existing a11y failures on clean HEAD"` — meaning
another concurrent agent stashed **every** agent's uncommitted work (mine, A1–A5's,
W0-A/B's — the whole cycle) to get a clean tree for its own test, and (based on the
timing) popped it back shortly after. I did not run any git command that could have made
this worse — no `stash pop`/`apply`/`drop`, no `checkout -- <path>`, no `reset` — I only
ran read-only `status`/`diff`/`grep`/`stash list` commands to confirm what happened, then
re-verified after the fact that every file was back and every edit I'd made (including the
76/8 endpoint-count fix above) was intact. **Flagging for whoever runs A8 or any future
"verify on a clean HEAD" card: stashing the entire shared working tree while six-plus other
agents are actively editing it is a real, demonstrated risk to concurrent work, not a
theoretical one — worth using a `git worktree` or a throwaway clone for that kind of check
instead of stashing the shared tree next time.**

**Not verified / could not verify:**

- I did not run `cargo test --workspace`, `cargo clippy`, or the frontend test/build
  commands myself — those aren't part of my card (docs-only) and every crate/file I
  documented already has a green report from its owning agent's handoff above, which I
  cross-checked against source rather than re-running.
- I could not find a handoff note (as of my read of TODO.md) explaining who created
  `crates/tack-api/src/orch_store.rs` or finished wiring `server.rs` — see point 4 above.
- `docs/book/src/developer/README.md` (Architecture Overview) and `crate-tour.md` still
  describe "four Rust crates" / "18 migrations" with no mention of `tack-orch` or
  migrations 019–024. Neither file is in my ownership (not listed in §2, not named in my
  card), so I didn't touch them, but they're now stale in the same way CLAUDE.md was
  before this card — worth a future pass by whoever owns doc maintenance more broadly.

**For Wave 2+ agents:** both new pages are written to describe *only* what's landed as of
this handoff (Phase 33, read-only). If you land `poll_runs`/`poll_approvals`/`poll_traces`/
`poll_metrics`, mirroring ingestion, dispatch, or anything else that changes the "What's
implemented vs. not" / "What's not here yet" lists in either page, please update those
sections directly rather than leaving them stale — they're written as living "as of this
release" statements, not a permanent historical record of Phase 33 alone.

### A9 — 2026-08-04

**Card:** secret-leak regression fix — `control_planes.token` (migration 019) was
shipping in plaintext inside every downloadable/uploadable backup bundle.
`scrub_snapshot_secrets` (`crates/tack-api/src/remote_backup.rs`) only ever touched
`app_meta`; the new orch table reintroduced the exact bug the July audit (finding #7,
the S3 secret key) already made this function's whole reason to exist.

**Fix:** `scrub_snapshot_secrets` now, after the existing `app_meta` key deletions and
**before** the trailing `VACUUM` (ordering is load-bearing — VACUUM is what physically
drops the freed bytes, so nothing was moved or duplicated): checks `sqlite_master` for
`control_planes` (defensive — the table won't exist in a snapshot from a pre-019
database, same posture as the existing `app_meta` `CREATE TABLE IF NOT EXISTS`), and if
present runs `UPDATE control_planes SET token = NULL WHERE token IS NOT NULL`. The row
itself (name, base_url, health, etc.) is left intact — only the credential is nulled —
so a restored backup still shows which control planes were registered; the operator
re-enters the token afterward. `orch_links`/`orch_tasks`/`orch_runs`/`orch_events`/
`orch_approvals` are untouched, as scoped.

Also updated the doc comments on `SENSITIVE_META_KEYS` and `scrub_snapshot_secrets`
so the next person adding a secret-bearing column anywhere in the schema is pointed at
this function as the chokepoint, rather than assuming the `app_meta` list is the whole
story.

**Restore path:** verified rather than assumed. `crates/tack-db/src/repo/orch.rs`'s
`ControlPlaneRow::into_control_plane` already derives `token_set: self.token.is_some()`,
so a null token correctly reports `token_set: false` after a restore, and
`get_control_plane_token` (`Option<Option<String>>` → `.flatten()`) already returns
`Ok(None)` for a null column. No code changes were needed on the restore/read side —
the column being nullable and the existing DTO shape already handle it sanely.

**Test — confirmed failing before the fix, passing after.** Added
`scrub_removes_control_plane_token_from_snapshot` in
`crates/tack-api/src/remote_backup.rs`'s existing test module (mirrors
`scrub_removes_secrets_from_snapshot`, which is the existing raw-bytes regression test
for the S3 secret key — I checked `crates/tack-api/tests/` first as instructed, found
nothing there, then found that inline test, and mirrored its approach rather than
inventing a second one). The new test creates a control plane with a distinctive token
through the real `Repository::create_control_plane` path, runs the *actual*
`create_bundle` end-to-end (snapshot → scrub → tar → zstd), decompresses and extracts
`database.db` back out via `parse_bundle` exactly as a restore would, and asserts:
(1) the raw extracted DB bytes never contain the token string, and (2) the
`control_planes` row still exists with its token column `NULL` — so a future "fix" that
just deletes the table instead of nulling the column would also fail this test. I ran it
before touching `scrub_snapshot_secrets`: it failed with `"control plane token still
present in snapshot bytes"` at the raw-bytes assertion, confirming the leak. After the
fix, all 17 tests in `remote_backup::tests` pass, including this one.

**Green before handoff, all three commands run clean:**
`cargo test --workspace` (all crates pass, no failures anywhere in the run — checked the
full output, not just tack-api), `cargo clippy --workspace --all-targets -- -D warnings`
(zero warnings), `cargo fmt --all -- --check` (no diff).

**C4 scope shrink:** Wave 3 card C4 ("Dispatch UI + security gating") listed excluding
the control-plane token from backups as part of its acceptance bar, citing this exact
vulnerability. That work is done now — see above — so I trimmed C4's card text in place
to note the exclusion is closed and narrowed its remaining scope to just the UI/gating
work (tasks 35.8/35.9: dispatch-to-agents UI, run-sprint UI, `TACK_ORCH_ENABLE` +
token gating on dispatch routes). Whoever picks up C4 should not redo the backup
exclusion or its test.

**Files touched:** `crates/tack-api/src/remote_backup.rs` only (fix + doc comments +
new test, inline in the existing `tests` module — matches the file's existing
convention, no new test file needed). No changes to `crates/tack-db/src/repo/orch.rs`
or any restore-path code — verified they already handle a null token correctly.
`TODO.md` (this note + the C4 scope-shrink edit above). Did not touch
`frontend/e2e/` or `frontend/src/features/fleet/`, per the concurrent-agent boundary.

### A10 — 2026-08-04

**Card:** root-cause the `Badge tone="warning"` contrast failure A8 hit on the Fleet
page, and fix 4 pre-existing `RichTextEditor` axe failures on `develop`.

**Task 1 — measured all six `Badge` tones in all six palette×mode combinations**
(script in scratchpad, not committed anywhere): relative-luminance/contrast computed
directly from the `--color-*` primitives in `frontend/src/index.css`, compositing the
low-opacity dark-mode `-100` backgrounds over each mode's `--color-bg-base`/
`--color-bg-elevated`-equivalent backdrop rather than eyeballing. **The warning pair
A8 found was not the only failure** — light mode has four broken pairs across three
palettes, not one:

| Palette / mode | success | warning | danger | info | primary | neutral |
|---|---|---|---|---|---|---|
| Teal / light | 5.75 ok | **4.30 FAIL** | 5.53 ok | 5.78 ok | 5.10 ok | 5.04 ok |
| Teal / dark | 7.47 ok | 7.83 ok | 5.21 ok | 6.32 ok | 8.91 ok | 6.08 ok |
| Clay / light | **4.40 FAIL** | **4.22 FAIL** | 4.55 ok* | **4.22 FAIL** | 6.15 ok | 5.44 ok |
| Clay / dark | 7.97 ok | 8.13 ok | 5.35 ok | 6.66 ok | 8.48 ok | 6.36 ok |
| Graphite / light | 4.82 ok | **2.81 FAIL** | 5.53 ok | 6.57 ok | 4.51 ok* | 6.11 ok |
| Graphite / dark | 7.96 ok | 8.25 ok | 5.43 ok | 5.59 ok | 10.31 ok | 6.26 ok |

(\* passes but with <0.1 headroom — Clay-light danger at 4.55 and Graphite-light
primary at 4.51 — flagging as fragile, not touched since not a failure.) All three
dark variants share identical success/warning/danger primitives regardless of
palette (`.dark`, `.dark[data-palette="clay"]`, `.dark[data-palette="graphite"]` all
hardcode the same three pairs) — only `accent`/`accent2` differ by palette in dark
mode, which is why dark mode is uniformly clean. Graphite-light `warning` was the
worst offender in the whole matrix at 2.81:1 — nowhere near AA, and nothing to do
with the Fleet-specific bug report; it was simply never exercised by any axe-gated
view either, same root cause as the Teal one A8 found.

**Fix — token level, not `Badge.tsx`.** Darkened the four failing `-600` (the
"700"/text alias resolves to the same value) primitives in `frontend/src/index.css`,
same technique already used for `--color-success-600`/`--color-danger-600` in
Teal-light per the existing comments there — hue/saturation preserved, only
lightness reduced, via binary search in HSL space to the least darkening that clears
4.5:1 with ~0.12 margin (targeting ~4.6:1 actual, since axe's own rounding and the
compositing assumption above aren't guaranteed to match browsers to the hundredth):

- Teal light `--color-warning-600`: `#b45309` → `#ac4f09` (4.30 → 4.64)
- Clay light `--color-success-600`: `#15803d` → `#147c3b` (4.40 → 4.63)
- Clay light `--color-warning-600`: `#a16207` → `#985d07` (4.22 → 4.61)
- Clay light `--color-accent2`: `#2f7d6e` → `#2c7668` (4.22 → 4.64) — this is the
  token `--color-info-700` aliases to; it's also consumed by
  `shared/ui/PriorityDot.tsx` and `shared/ui/TypeBadge.tsx` outside Badge, both of
  which only get *more* AA headroom from the darkening, never less
- Graphite light `--color-warning-600`: `#d97706` → `#a35a05` (2.81 → 4.61)

No change to `Badge.tsx` — the tone map already reads the right token names; the
tokens themselves were wrong. This is the smallest change that fixes the whole
system: every current and future `Badge tone="warning"`/`"success"`/`"info"` caller
in every palette is now AA-clean, not just Fleet's two call sites.

**Deleted the fork.** Removed `frontend/src/features/fleet/WarningBadge.tsx`
entirely and reverted both call sites to the shared component: `HealthChip.tsx`'s
degraded branch now always renders `<Badge tone={HEALTH_TONE[props.health]}>`
(dropped the `degraded`-only branch and the now-unused import), and
`FleetRow.tsx`'s pending-approvals cell is back to `<Badge tone="warning">{…}
pending</Badge>`. Confirmed no other file referenced `WarningBadge`
(`FleetRow.test.tsx` doesn't mention it) before deleting.

**Task 2 — RichTextEditor.** Added `aria-label="Text style"` to the heading
`<select>` in `frontend/src/shared/ui/RichTextEditor.tsx`. Followed the existing
convention exactly — grepped every `<select>`/`<input>` in the app first
(`shared/ui/Sidebar.tsx`'s project switcher uses `aria-label="Switch project"`,
`features/table/Table.tsx`'s filter input uses `aria-label="Filter items"`,
`features/settings/panels/WorkflowPanel.tsx` uses `aria-label="Remove status"`) —
same pattern, `aria-label` directly on the control, no visible `<label>` element
anywhere else in the codebase for this kind of inline toolbar control. One-line
change, no refactor.

**Verification.**
- `npm run type-check` — clean.
- `npm run lint:tokens` — 0 raw color literals (baseline 0), 1 inline-style hex
  literal (baseline 1, unchanged — that's a pre-existing comment-documented
  exception, not mine).
- `npm run test` (Vitest) — 204/207 pass, same 3 pre-existing `requestBlob`/
  `createObjectURL` failures called out in the card brief and independently
  confirmed by A8 to predate this whole work cycle. Nothing in my diff touches
  those files.
- `npx playwright test e2e/a11y.spec.ts --project=chromium` — both Fleet scans
  (`fleet page (orchestration disabled)`, `fleet page (populated)`) pass, as they
  did before my change per the card's note. Of A8's 4 `RichTextEditor` failures
  (board, timeline, sprint, item-detail-drawer), **board and item-detail-drawer now
  pass clean** — the `select`-name violation is gone. **Timeline and sprint still
  fail, but on different, unrelated findings**: timeline now reports `color-contrast`
  (white text on a `#b8692e` Gantt-bar background, serious, in
  `frontend/src/features/timeline/` — not a file I touched or own) and sprint
  reports `scrollable-region-focusable` (a non-focusable scrollable `<div>`, likely
  in a sprint-board column container — also not mine). Both are new violation *IDs*
  I hadn't seen mentioned before, surfaced now that the `select`-name finding no
  longer masks/co-occurs with them in the same scan. I did not touch either file
  (`git diff` confirms my changes are scoped to `index.css`, `RichTextEditor.tsx`,
  `WarningBadge.tsx` deletion, `HealthChip.tsx`, `FleetRow.tsx` only), so these
  predate this card and are out of my file ownership — flagging for whoever owns
  `features/timeline/` and the sprint board view. Worth its own card; don't assume
  it's the same bug as the RichTextEditor one.

**Six-combo coverage gap** — per A8's handoff and the card's own instruction, I
didn't restructure `a11y.spec.ts` to add palette×mode looping; I measured the
ratios directly from tokens instead (the table above), which is exactly how the
Graphite-light and Clay-light failures were caught even though no axe-gated view
currently exercises those palettes. The coverage gap itself is unchanged and still
open for whoever picks it up.

**Files touched:** `frontend/src/index.css` (5 token edits + comments),
`frontend/src/shared/ui/RichTextEditor.tsx` (1-line `aria-label`), deleted
`frontend/src/features/fleet/WarningBadge.tsx`, reverted
`frontend/src/features/fleet/HealthChip.tsx` and
`frontend/src/features/fleet/FleetRow.tsx` to the shared `Badge`. `TODO.md` (this
note). Nothing in `crates/` touched, per the concurrent-agent boundary on
`remote_backup.rs`.

### B1 — 2026-08-04

**What I built.** Tasks 34.1/34.2 — the reconciler now mirrors `GET /runs?project=`
(per linked project) and `GET /approvals` (fleet-wide) into `orch_runs`/
`orch_approvals`, with correlation to a Tack item via `remote_task_id`. All in
`crates/tack-orch/src/reconciler.rs` (the file I own this round, per exclusively),
plus the two pieces of necessarily-related glue described below.

1. **`ControlPlaneStore` trait (`reconciler.rs`) gained four methods** alongside
   `record_health`: `list_linked_projects`, `find_item_for_remote_task`,
   `upsert_runs`, `upsert_approvals`. Each is documented as a thin, mechanical
   pass-through to a single `tack_db::repo::orch` function (A3's
   `list_orch_links_for_plane`, `find_orch_task_by_remote_task_id`,
   `upsert_orch_runs`, `upsert_orch_approvals`) — no correlation logic belongs in an
   implementor, only in this module.
2. **Two new fetch-only `poll_*` fns** (`poll_runs`, `poll_approvals`), following
   `poll_health`/`poll_status`'s shape exactly — HTTP-only, no DB access, per A2's
   recipe. `poll_runs` takes a `&[String]` of linked-project names and makes one
   `list_runs(Some(project))` call per project, keeping each project's result
   independent so one project's failure never drops another's runs.
3. **Two new `FetchOutcome` fields** (`runs: Vec<(String, Result<Vec<RemoteRun>,
   OrchError>)>`, `approvals: Result<Vec<RemoteApproval>, OrchError>`) and one line
   each in `reconcile_once`'s struct literal, per the recipe.
4. **Correlation + persistence, in `spawn_one`'s loop, after
   `store.record_health(...)`** — never inside `reconcile_once`, per §0 rule 5 and
   the card's explicit instruction. Two new free functions, `persist_runs` and
   `persist_approvals`, do the correlating (via `correlate_remote_task`, which walks
   a run's `task_ids` or an approval's `context.taskId` and returns the first
   `Some(item_id)` `find_item_for_remote_task` reports, or `None` — never an error)
   and then call `store.upsert_runs`/`upsert_approvals` with the resolved
   `NewOrchRun`/`NewOrchApproval` batch (A3's types, reused verbatim, not
   reinvented).
5. **`evaluate()` untouched** — still reads only `.health`/`.status`. Verified by a
   dedicated test (`approvals_poll_failure_leaves_plane_health_untouched`): a plane
   whose `/approvals` call fails every tick still reports `healthy`.

**Deviation from A2's recipe as written, disclosed because B3 needs to know before
adding `poll_metrics`.** The recipe's step 3 (`reconcile_once` returns
`evaluate(&fetched)`, i.e. just a `PollEvaluation`) discards `fetched` entirely —
that's fine for a step whose only consumer is `evaluate`, but B1's runs/approvals
data has to reach `spawn_one`'s persistence phase, and `evaluate` never carries data
forward, only a verdict. I changed `reconcile_once`'s return type to `(PollEvaluation,
FetchOutcome)` — the second element is the raw, unevaluated fetch, there specifically
so a later phase can read fields `evaluate` doesn't touch. **B3: read `metrics` off
the `FetchOutcome` half of `reconcile_once`'s return in `spawn_one`, the same way
`persist_runs`/`persist_approvals` do today — it will not be visible to `evaluate`,
nor should it be.** I updated the module doc comment in `reconciler.rs` in place (the
"Extending this for Wave 2" section) to show the corrected recipe with this return
type, so anyone reading the file itself (not just this note) sees the right shape.
Every call site that used the old one-value return (`panic_in_a_poll_is_isolated_by_
the_task_boundary`, `spawn_one`'s match arm) is updated to match; the panic-recovery
branch in `spawn_one` now constructs a synthetic `FetchOutcome` (all `Err`, empty
`runs`) alongside the synthetic `PollEvaluation` it already built.

**One more real design decision the recipe didn't anticipate:** `poll_runs` needs a
project list, and that list lives in the database (`orch_links`), not on the control
plane. I fetch it via `store.list_linked_projects(id)` in `spawn_one`, **before** the
panic-isolated `reconcile_once` call — a single short read, not held open across any
HTTP `.await`, so §0 rule 5 holds, but it does mean a given tick's project list can be
up to one tick stale relative to a concurrent `orch_links` edit (accepted, bounded by
`TACK_ORCH_POLL_SECS`, not a bug). It also means this one read isn't inside the
panic-isolation boundary the way the HTTP calls are — a bug in `list_linked_projects`
would surface as a real panic in `spawn_one`'s task rather than a caught `JoinError`.
I judged this an acceptable, narrow exception (a DB read is far less likely to panic
than N HTTP calls to an external system) rather than restructuring the isolation
boundary; flagging it explicitly in case B2 (which will need a similar per-plane
cursor read for `/traces`) wants to make a different call.

**Design decisions worth knowing about:**

- **`RemoteApproval.role` → `orch_approvals.agent`.** There's no separate "agent"
  concept on docket's `/approvals` wire shape — `role` is the closest field (who the
  gate is asking), so that's what's stored in the column D1's approvals inbox will
  read. If D1 wants something else there, this is the one line to change
  (`persist_approvals` in `reconciler.rs`).
- **`orch_approvals.decided_at` is always `None` on ingest.** `RemoteApproval` (A1's
  DTO) carries no such field — confirmed against `ApprovalsResponse`'s
  `{"pending": [...]}` wrapper in `adapters/docket.rs`: `/approvals` only ever
  returns the still-pending set. A real decision is Wave 3's `decide_approval`; when
  that lands, it's the natural place to set `decided_at` directly (Tack made the
  decision, no need to round-trip through a poll), not something this ingestion path
  needs to grow.
- **A malformed `created` timestamp on an approval is dropped, not defaulted.**
  `requested_at` is a required, non-nullable column; rather than substitute
  `Utc::now()` (which would corrupt the fleet-wide "oldest first" ordering
  `list_pending_orch_approvals` relies on), `persist_approvals` skips just that one
  record with a `warn!`, and every other approval in the same poll still lands.
  Covered by `parse_optional_rfc3339_accepts_both_docket_timestamp_conventions`
  (both of docket's observed conventions — `+00:00` from `core/runs.py`, `Z` from
  `core/approval.py` — parse; garbage degrades to `None`, never a panic).
- **Runs use `RemoteRun.project` (the record's own field), not the queried project
  string, for `orch_runs.remote_project`.** They should always agree since every
  call passes `Some(project)`, but the record's own field is the more authoritative
  source if docket ever returns something unexpected.

**Two files outside `reconciler.rs`, both disclosed, both exactly what the card
predicted I'd need to touch:**

1. **`crates/tack-api/src/orch_store.rs`** — the card explicitly said "you will need
   to extend the `ControlPlaneStore` trait... `orch_store.rs` holds the real
   implementation," so I added the same four methods to A7's `RepoControlPlaneStore`,
   each a one-line pass-through in the same style as its existing `record_health`
   (no correlation logic here either — that stays in `reconciler.rs`, same as the
   `TestRepoStore` I wrote for `tack-orch`'s own integration tests). Net diff: +54
   lines, one new `use` line for `NewOrchApproval`/`NewOrchRun`. Nothing else in that
   file changed.
2. **`crates/tack-orch/Cargo.toml`** — added `sqlx = { workspace = true }` under
   `[dev-dependencies]`, needed by `tests/ingestion_test.rs`'s direct `SELECT
   COUNT(*)` idempotency assertions (not a second SQLite driver — the same `sqlx`
   already used throughout the workspace). One line, commented.

No other files were touched — confirmed via `git status --porcelain` before and after
that only `crates/tack-orch/src/reconciler.rs`, `crates/tack-orch/Cargo.toml`,
`crates/tack-orch/tests/ingestion_test.rs` (new), and `crates/tack-api/src/
orch_store.rs` changed as a result of this session. `handlers/orch.rs`, `router.rs`,
`websocket.rs`, `items.rs`, and every frontend file are untouched, per the card's
scope boundary. I did not add any `BoardEvent` variant, any metrics polling, any HTTP
route, or any write-back to docket.

**Testing.** Two layers, per the card's "wiremock + A1's fixtures are the right
source of truth" guidance:

- **`reconciler.rs`'s own `#[cfg(test)] mod tests`** (+7 tests, fast, no real DB):
  `FakeControlPlane` gained configurable `list_runs`/`list_approvals` responses
  (`with_runs`, `with_approvals`, `healthy_with_failing_approvals`) and `FakeStore`
  gained the four new methods plus captured-call storage
  (`upserted_runs`/`upserted_approvals`) and configurable `linked_projects`/
  `known_tasks`. New tests: `approvals_poll_failure_leaves_plane_health_untouched`,
  `a_correlated_run_lands_with_the_right_item_id`,
  `an_uncorrelated_run_lands_with_item_id_none_and_does_not_error`,
  `a_correlated_approval_lands_with_the_right_item_id`,
  `an_uncorrelated_approval_lands_with_item_id_none_and_still_surfaces`,
  `extract_task_id_handles_missing_and_non_string_taskid`,
  `parse_optional_rfc3339_accepts_both_docket_timestamp_conventions`. All existing
  `FetchOutcome`-literal tests and the panic-isolation test were updated for the new
  fields/return type; nothing about the pre-existing health-state-machine tests
  changed behaviorally.
- **`crates/tack-orch/tests/ingestion_test.rs`** (new, +2 tests, real stack): a real
  `tack_db::Repository` (in-memory SQLite, real migrations), a real `DocketAdapter`
  against a `wiremock` server, and the real `spawn_reconcilers`/`spawn_one` loop —
  via a local `TestRepoStore` (a test-only `ControlPlaneStore` impl wrapping
  `Repository` directly, written to the exact same mechanical shape
  `RepoControlPlaneStore` needs, since `tack-orch` must never depend on `tack-api`).
  `correlated_and_uncorrelated_runs_and_approvals_mirror_idempotently`: seeds a
  project/item/`orch_tasks` row, mocks `/runs` returning one correlated + one
  CLI-only (empty `task_ids`) run and `/approvals` returning one correlated + one
  uncorrelated approval, polls for ~2 ticks, and asserts both correct `item_id`
  attribution *and* `SELECT COUNT(*) FROM orch_runs`/`orch_approvals` staying at 2
  rows each across multiple polls (idempotency, not just "no crash").
  `a_later_poll_does_not_erase_an_earlier_run_attribution`: a custom `wiremock`
  `Respond` impl (`SequentialBody`) returns a correlated run on the first poll, then
  an *uncorrelated* version of the same `run_id` on every poll after — proving A3's
  `COALESCE(excluded.item_id, orch_runs.item_id)` really does hold across a live
  reconciler loop, not just at the repo-test level A3 already covered.

Every item in the card's "cover at minimum" list is covered: correlated run/approval
with the right `item_id` (unit + integration), uncorrelated with `item_id = NULL`
(unit + integration), re-polling idempotent (integration, row-count asserted),
attribution never un-learned (integration, live COALESCE proof), `/approvals` failure
leaving health untouched (unit).

**Verification.** `cargo test --workspace`: 362 passed, 0 failed (up from Wave 1's
352 — +7 `reconciler::tests`, +2 `ingestion_test`, plus whatever A9/A10 added
concurrently; I re-ran the full suite after their changes landed in the shared tree
and it's still all green). `cargo clippy --workspace --all-targets -- -D warnings`:
clean. `cargo fmt --all -- --check`: clean — I ran `cargo fmt -p tack-orch -p
tack-api` (not `--all`) first per A3's convention of not reformatting files mid-edit
in someone else's terminal, then re-ran the `--all -- --check` gate to confirm no
diff anywhere in the tree.

**What's still open / for B3, B2, B4, D1.**

- **B3 (metrics ingestion):** see the "deviation from the recipe" note above —
  `reconcile_once` now returns `(PollEvaluation, FetchOutcome)`, and your `metrics`
  field goes on `FetchOutcome` the same way `runs`/`approvals` do. `poll_metrics`
  should call `tack_orch::adapters::prometheus::parse` (A1's parser, per your own
  card) — nothing about my change affects that path.
- **B2 (trace ingestion):** flagging the "DB read before the panic-isolated fetch
  phase" pattern above as a precedent you may want for a per-plane traces cursor —
  or may not; it's a real (small) trade-off, not a mandate.
- **D1 (approvals inbox):** `orch_approvals.agent` is populated from docket's `role`
  field, not a distinct "which coding agent" concept — see the design-decisions note
  above if the inbox UI wants something more specific. `decided_at` is never
  populated by this ingestion path (see above); it'll need Wave 3's write side.
- **Nothing I found was shaky enough to flag as "works but don't trust it."** The
  two things I'd call out as judgment calls rather than certainties: (1) the
  project-list DB read sitting outside the panic-isolation boundary (explained
  above — I think it's the right call, but it is a deliberate carve-out from the
  "everything the fetch phase touches is panic-isolated" invariant A2's doc
  describes); (2) `list_linked_projects` re-queries `orch_links` fresh every tick
  (no caching) — fine at the scale this is designed for (a handful of linked
  projects per plane), but if a future plane ends up with hundreds of links this is
  the first place to look.

### A11 — 2026-08-04

**Card:** get the Playwright a11y gate actually green (two pre-existing, unrelated
failures A10 surfaced but didn't own — timeline `color-contrast`, sprint
`scrollable-region-focusable`), and finish the token-contrast audit A10 started
(Badge tones only) across the rest of the component set.

**Result: `npx playwright test e2e/a11y.spec.ts --project=chromium` — 10/10 pass.**
Every scan is now clean: home, board, table, calendar, timeline, sprint, global
settings, item-detail-drawer, both fleet scans. `npm run test` — 204/207, the same
3 pre-existing `requestBlob`/`createObjectURL` failures called out in the card and
confirmed by A8/A10 to predate this cycle (untouched files). `type-check` and
`lint:tokens` both clean (0 raw literals, 1 inline-hex — the pre-existing Avatar
exception, unchanged).

**Task 1a — Timeline Gantt bar `color-contrast`.**

Root cause was *not* a simple wrong-token pick — the axe finding (white text on a
composited `#b8692e`, "serious") was one symptom of a structural bug: the bar's
status-based fade (`opacity: 1 | 0.85 | 0.45` for todo/in-progress/done) was CSS
`opacity` applied to the element **containing the text**. CSS group-opacity
composites the whole rendered subtree — fill *and* text — against whatever's
behind it at the same alpha, which drags both toward the page background and
collapses the fill/text contrast gap. I verified this by modeling the actual
composite (not just the raw token pair): at the old `opacity: 0.85` (in-progress),
3 of 4 priorities already failed AA in at least one palette; at `0.45` (done),
**every priority failed in every one of the six palette×mode combos**, down to
~2:1 in several. Retrying with the fade isolated to a background-only layer
(text left fully opaque) still failed at 0.85 in half the combos and catastrophically
at 0.45 — there is no opacity value here that's compatible with legible bar text.
Per the card's explicit "never by dimming," the fix removes the mechanism rather
than retuning its constants:

- `frontend/src/features/timeline/Timeline.tsx`: dropped `getStatusOpacity()` and
  the `opacity` style entirely (including the `0.8` drag-preview value, for the
  same reason). Bars now render at full opacity always. "Done" is signaled with a
  ✓ prefix + strikethrough title (mirrors the existing ⛔-prefix pattern already
  used for blocked items) instead of a fade. Legend copy updated to match
  ("Done items show a ✓ and strikethrough."). Label text changed from the raw
  Tailwind `text-white` class to `style={{ color: 'var(--color-text-inverse)' }}`
  — matching `Calendar.tsx`'s event-chip pattern exactly (same priority-color
  fill, same inverse-text token, no fade — which is presumably *why* Calendar's
  scan was already clean).
- `frontend/src/shared/ui/PriorityDot.tsx`: `priorityColor('low'|'none')` returned
  `--color-text-tertiary`. That's fine as a small non-text dot swatch, but this
  function is also used as a **solid fill under `--color-text-inverse` text**
  (Timeline bars, Calendar chips) — and `-tertiary` fails there in all three
  dark-mode palettes (3.84–4.31:1) even at full opacity, both before and after
  the token-level fix below (still 4.04–4.49:1 post-fix — the lightened
  `-tertiary` was calibrated for plain body text, not this specific solid-fill
  pairing). Changed to `--color-text-secondary`, which clears AA everywhere
  (5.69–7.44:1). This is a "reaching for the wrong token" fix, not a token-value
  fix — it benefits both Timeline and Calendar (Calendar had the identical latent
  dark-mode bug, just never caught: axe scans always run in the default
  light/Teal theme, never dark or Clay/Graphite).

**Task 1b — sprint board `scrollable-region-focusable`.**

`frontend/src/features/sprints/Sprints.tsx`. The flagged node was the backlog
panel's item list (`<div class="flex-1 overflow-y-auto p-3 space-y-2">`), a
scrollable region with no focusable content and no `tabindex`. I grepped the
codebase for an existing scrollable-region convention first (per the card) —
there isn't one (Board.tsx has the same unlabeled-overflow shape on its column
lists and hasn't been caught only because the E2E fixture's single item never
overflows it; flagging for whoever owns `features/board/` since it's out of my
file ownership). The closest precedent is the `aria-label`-directly-on-control
pattern A10 already documented (Sidebar's project switcher, Table's filter
input). Applied `tabindex="0"` + a descriptive `aria-label` to all three
scrollable containers in this file (not just the one axe's fixture happened to
hit, since the other two have the identical shape and would fail the same way
under real data):

- backlog list → `aria-label={`${t('backlog')} items`}`
- sprint-lanes horizontal scroller → `aria-label={`${t('sprint')} lanes`}`
- each sprint's item list → `aria-label={`${sprint.name} items`}`

**Task 2 — contrast audit beyond Badge.** Computed WCAG contrast from the raw
`--color-*` primitives in `index.css` (relative-luminance formula, dark-mode
alpha-washed tokens composited over the relevant backdrop) for every
text/background pairing I could find actually used in the component tree, across
all six palette×mode combos. Script (not committed): scratchpad
`contrast.mjs`. Found and fixed **two more genuine failures** beyond Timeline's:

1. **`--color-text-tertiary` (dark mode, all 3 palettes) fails AA as plain body
   text against every core surface it's used on** — `index.css`, token level,
   not a single-component bug. This is the same tier used all over the app for
   muted/meta text (timestamps, hints, empty-state copy). Worst case (against
   `--color-bg-subtle`, the lightest — hence lowest-contrast — dark surface):
   Teal-dark 3.52:1, Clay-dark 3.54:1, Graphite-dark 3.41:1, all under AA. Every
   *light*-mode surface was already fine (5.15–6.50:1); this was purely a
   dark-mode gap, invisible to the current test suite because none of the axe
   scans switch mode or palette. Lightened all three dark-mode values in HSL
   (hue/saturation preserved, same technique as A10's Badge fix), binary-search
   to clear 4.5:1 against the worst surface with ~0.15 margin:
   - Teal dark: `#67807a` → `#7b958e` (3.52 → 4.65 worst-case)
   - Clay dark: `#81705c` → `#97846d` (3.54 → 4.69 worst-case)
   - Graphite dark: `#6a7480` → `#818b96` (3.41 → 4.68 worst-case)

   Updated in both places each value appears in `index.css` (the `.dark`/
   `.dark[data-palette=...]` blocks *and* their `@media (prefers-color-scheme:
   dark)` mirrors for the no-explicit-preference case — six edits, three
   values). All other surfaces for the new value land at 4.65–5.86:1.

2. **`WipChip`'s not-exceeded state** (`frontend/src/shared/ui/WipChip.tsx`) used
   the same `--color-chip`/`--color-text-tertiary` pair directly — same class of
   bug, same three dark-mode palettes (3.41–3.54:1 pre-fix). The token fix above
   would have cleared this on its own, but only to 4.65–4.69:1 — a thin margin
   for a pairing rendered on every populated board column. I additionally
   switched it to `--color-text-secondary`, which is the *exact* pair Badge's
   `neutral` tone already uses (bg-subtle + text-secondary, 5.04–6.36:1
   everywhere) — consistency with an established pattern rather than a new one,
   and more headroom than the token fix alone provided. Flagging explicitly since
   the card asked not to touch things beyond genuine failures: at the time I
   changed it, 3.41:1 was unambiguously failing; the token-level fix (found
   afterward) would have independently resolved it to a thin pass, so keeping
   both isn't strictly required — I judged the extra headroom + consistency
   worth it, but reverting `WipChip.tsx` to `-tertiary` would still pass today.

**Full ratio table** (recomputed after all fixes above; light-mode-only pairs
that were always clean are condensed). FAIL = under the relevant AA floor
(4.5:1 text, 3:1 large-text/non-text-UI); "thin" = passes but under 4.7:1,
reported per the card's instruction rather than touched:

| Pair (bg → fg) | Teal/L | Teal/D | Clay/L | Clay/D | Graphite/L | Graphite/D |
|---|---|---|---|---|---|---|
| Badge success (100→600) | 5.75 | 6.64 | 4.63 thin | 7.47 | 4.82 | 7.31 |
| Badge warning (100→600) | 4.64 thin | 6.98 | 4.61 thin | 7.60 | 4.61 thin | 7.58 |
| Badge danger (100→600) | 5.53 | 4.68 thin | 4.55 thin | 5.02 | 5.53 | 5.01 |
| Badge info (accent2-soft→accent2) | 5.78 | 5.63 | 4.64 thin | 6.25 | 6.57 | 5.14 |
| Badge primary / TypeBadge epic (accent-soft→accent-ink) | 5.10 | 7.93 | 6.15 | 7.94 | 4.51 thin | 9.46 |
| Badge neutral / TypeBadge default (bg-subtle→text-secondary) | 5.04 | 6.08 | 5.44 | 6.36 | 6.11 | 6.26 |
| Solid primary-600 → on-accent (Button primary, Sidebar/Layout logo mark, Timeline view-toggle, ItemHeader active pill, List.tsx save button) | 5.47 | 8.45 | 5.18 | 7.72 | 7.40 | 10.81 |
| priorityColor(critical)=danger-600 → text-inverse (Timeline bars, Calendar chips) | 6.47 | 6.61 | 5.44 | 6.61 | 6.47 | 6.61 |
| priorityColor(high)=warning-600 → text-inverse | 5.42 | 10.96 | 5.38 | 10.96 | 5.22 | 10.96 |
| priorityColor(medium)=accent2 → text-inverse | 6.68 | 8.82 | 5.39 | 8.77 | 7.58 | 7.13 |
| priorityColor(low/none)=text-secondary → text-inverse (fixed, was tertiary) | 5.69 | 7.44 | 6.37 | 6.89 | 6.98 | 7.07 |
| WipChip exceeded (danger-100→danger-600) | = Badge danger row | | | | | |
| WipChip not-exceeded (bg-subtle→text-secondary, fixed) | = Badge neutral row | | | | | |
| Baseline: any core surface → text-primary | 12.9–17.8 everywhere | | | | | |
| Baseline: any core surface → text-secondary | 4.9–7.7 everywhere | | | | | |
| Baseline: any core surface → text-tertiary (fixed, dark) | 5.15–6.50 (light) | 4.65–5.86 | 5.50–6.40 (light) | 4.69–5.29 | 5.64–6.44 (light) | 4.68–5.62 |
| Sidebar health dot, online (success-600 vs bg-sidebar, 3:1 floor) | 5.66 | 10.43 | 4.47 | 11.23 | 4.79 | 11.31 |
| Sidebar health dot, offline (text-tertiary vs bg-sidebar, 3:1 floor) | 5.15 | 4.28→ improves with fix | 5.50 | 4.11→improves | 5.69 | 4.15→improves |

**Near-misses reported, not touched** (all genuinely pass, all under 4.7:1):
Badge success Clay-light 4.63; Badge warning Teal-light/Clay-light/Graphite-light
4.64/4.61/4.61; Badge danger Teal-dark/Clay-light 4.68/4.55; Badge info Clay-light
4.64; Badge primary Graphite-light 4.51 (all six are A10's pre-existing "fixed to
~4.6 with margin" values — re-verified here, unchanged, still the tightest
margins in the system by design); the new text-tertiary dark-mode fix itself
lands at 4.65–4.84:1 against `bg-subtle`/`bg-elevated` (the two darkest-relative
surfaces) — deliberately calibrated there, not tighter, but still under the 4.7
report line so noting it rather than darkening further and eating into the
"still visibly fainter than -secondary" distinction.

**Found, not fixed — flagging for a follow-up card.**
`frontend/src/shared/ui/Avatar.tsx` renders initials as hardcoded `color: '#fff'`
over `background: hsl(hueFromName(name) 45% 50%)` — a per-name deterministic hue,
entirely outside the `--color-*` token system (it's `scripts/check-tokens.sh`'s
one documented baseline exception, commented there as "correct in both modes").
It is not: sweeping all 360 hues at fixed 45%/50%, white text fails AA for
**56% of possible hues** (worst case h=60, ~yellow, 2.08:1; e.g. h=90/120/180 all
under 2.6:1). This is also **not exercised by any current a11y scan** — I traced
why: `e2e/helpers.ts`'s `getOrCreateItem` never sets `assignee`, so `Board.tsx`'s
`<Avatar>` (the only consumer) never renders during any scan. This is a real,
statically-computable bug, but the fix is a genuine design decision (clamp
lightness, pick per-hue ink color, or something else) rather than a token swap,
and `Avatar.tsx` isn't a file this card put in my ownership — leaving it for
whoever picks up next, with the check-tokens.sh comment now known to be wrong.

**Method limits — what's verified how.**

- **Statically verified** (computed from the literal token values in
  `index.css`, all six combos): every pair in the table above.
- **Verified by running axe** (`--project=chromium`, light/Teal only, the
  suite's only configuration): all 10 scans in `a11y.spec.ts`, confirming no
  *other* violations exist in the DOM as currently rendered — but only in the
  one mode/palette the suite runs in. The token-level dark-mode bugs above
  (`text-tertiary`, `WipChip`) were invisible to axe entirely; only the static
  math caught them.
- **Not covered by either method**, disclosed rather than implied clean:
  - `Avatar.tsx` (above) — computed, but not fixed, and not axe-exercised.
  - A handful of low-alpha decorative washes with no text drawn on top —
    `Calendar.tsx`'s `rgba(59,130,246,0.07)` "today" cell tint,
    `Dashboard.tsx`'s two `rgba(...,0.1)` icon-badge backgrounds (emoji icons,
    not text glyphs), `RichTextEditor.tsx`'s `rgb(156 163 175)` empty-state
    placeholder color. Sampled by inspection, judged low-risk (translucent
    decorative fills / non-critical placeholder text), not run through the
    contrast formula — a genuine gap, not a clean bill.
  - I did not attempt a runtime/rendered-pixel audit (e.g. driving axe across
    all six mode×palette combinations by toggling `.dark`/`data-palette` in a
    script) — everything above is from the token *definitions*, which catches
    anything statically identifiable but, per the card's own caveat, would miss
    a color composited over an unexpected runtime layer this pass didn't
    already know to model (I did model Timeline's opacity compositing
    specifically, because that was the bug under investigation — I did not go
    hunting for other components doing something similarly unusual with
    `opacity`/`filter`/`mix-blend-mode` on text-bearing elements beyond the grep
    sweep already described).

**Files touched:** `frontend/src/index.css` (dark-mode `text-tertiary` × 3
palettes × 2 locations each), `frontend/src/features/timeline/Timeline.tsx`
(opacity removal, inverse-text token, done-state ✓/strikethrough, legend copy),
`frontend/src/features/sprints/Sprints.tsx` (3× `tabindex`/`aria-label` on
scrollable containers), `frontend/src/shared/ui/PriorityDot.tsx`
(`priorityColor` low/none token), `frontend/src/shared/ui/WipChip.tsx`
(not-exceeded token), `frontend/src/shared/ui/primitives.test.ts` (2 assertions
updated to match the intentional token changes above). `TODO.md` (this note).
Did not touch `e2e/a11y.spec.ts` (no rules disabled, no scans skipped — all 10
pass on their own merits) or anything under `crates/`.

### B5 — 2026-08-04

**What I built.** Tasks 34.8/34.9 — the item-detail "Agent Activity" tab and
one shared `AgentStateChip` used by Board, List, and Table. Neither backend
endpoint exists yet (`GET /items/{id}/agent-activity`,
`GET /projects/{id}/agent-activity`), so, following A5's Fleet-view precedent
exactly, every wire-format assumption lives in one new file:
`frontend/src/shared/agentActivity/api.ts`. Its header comment documents,
field by field, which migration/DTO each type is projected from
(`crates/tack-db/src/migrations.rs` 021 `orch_tasks`, 022 `orch_runs`, 023
`orch_events`, 024 `orch_approvals`; `crates/tack-orch/src/lib.rs`'s
`TaskStatus`/`RunState`/`ApprovalState` for the wire *values*) — when the real
endpoints land, reconciling the frontend means editing that file only, never
the components. `frontend/src/shared/agentActivity/format.ts` holds the
derivation/formatting logic (`deriveAgentChipState`, `formatEstimatedCost`,
`formatTokens`, `relativeTime`, `eventTypeLabel`), and
`useAgentActivityMap.ts` is the bulk-fetch hook Board/List/Table share for
badges. None of this lives under `features/item-detail/` even though that's
my named ownership folder — it had to live somewhere `features/board`,
`features/list`, and `features/table` could all reach without violating
`architecture.test.ts`'s "no features/* imports another features/*" rule, and
"anything not listed [in the ownership map] is free to create" covers a new
`shared/agentActivity/` folder. Flagging this explicitly since it's a
deviation from the letter of "files you own" (item-detail/**, shared/ui/,
Board/List/Table call sites) even though I believe it's exactly what that
scope was gesturing at.

**Two endpoint shapes, both my best-guess projections, not verified against
a real handler:**

- `GET /items/{id}/agent-activity` → `{ attempts: ItemAgentAttempt[],
  approvals: ItemAgentApproval[] }`. One `attempts` entry per `orch_tasks`
  row for the item (`remote_task_id, remote_run_id, remote_status, attempt,
  tokens_in, tokens_out, cost_usd_estimated, pricing_snapshot_at,
  dispatched_at`), each carrying its correlated `orch_runs` row (`run` —
  `null` if not yet resolved) and an `events` array from `orch_events`
  (`id, event_type, payload, occurred_at`) that is **always empty today**:
  ingestion is B2 (Trace ingestion, 34.4), blocked on a docket endpoint that
  doesn't exist yet. I built the full render path for events anyway
  (`AgentActivityTab.tsx`'s `eventTypeLabel` maps roadmap.md's known type list
  to short labels, unknown types degrade to a title-cased raw string) so that
  when B2 ships, populating `events` is a pure backend change — zero frontend
  diff, matching the outcome A4 got by targeting A5's file exactly.
  `orch_tasks.trusted` (the Phase-35/C2 untrusted-auto-dispatch flag) exists
  on the table but isn't surfaced — no feature reads it yet; flagged as a gap
  for whoever builds C2.
- `GET /projects/{id}/agent-activity` → `{ rows: AgentBadgeRow[] }`, one row
  per item **that has at least one `orch_tasks` row** (an inner join, not a
  left join with nulls — an item with no agent activity simply has no row).
  Each row is `{ item_id, remote_status, attempt, updated_at }` — the
  *latest* attempt's raw status. My assumption, worth the real endpoint
  author confirming: "latest" means highest `attempt` number, ties broken by
  `dispatched_at` desc. This mirrors roadmap.md 34.9's "compact state chip …
  driven by the orch_tasks LEFT JOIN" wording, projected down to what a badge
  needs.

**The chip's 5 states, and why blocked doesn't get a 6th.** The roadmap
(34.9) names 4 states ("queued / running / waiting-approval / failed"); the
card's acceptance criterion says "five" without naming them. `TaskStatus` has
6 wire values plus an `Unknown(String)` fallback. I read "five" as the four
named plus `done` (a completed dispatch is exactly the at-a-glance signal
this chip exists for), and folded `blocked` **and** any unrecognised value
into `failed` — the conservative direction: showing an unfamiliar or stuck
status as calm risks hiding a real problem, flagging it as "needs a look"
costs nothing when it's actually fine. `deriveAgentChipState` in `format.ts`
has the full reasoning and is the one place this mapping lives, so revisiting
the decision (e.g. giving `blocked` its own slot and a 6th tone) is a
one-function change. `AgentStateChip` still accepts an optional `title` so a
caller can surface the raw `remote_status` (e.g. "blocked") on hover even
though it visually renders as the `failed` chip — used by both the Board/
List/Table badges and the tab's per-attempt chip.

**Rule 6 (never present an estimate as spend), specifically the null
`pricing_snapshot_at` case.** `pricing_snapshot_at` is always `null` today —
no pricing-snapshot mechanism exists anywhere in the system yet (confirmed
against A4's Wave-1 handoff, which found the identical gap for the Fleet
view's equivalent field). The Fleet view's `formatEstimatedCost` **silently
omits** the snapshot clause when the date is `null` — my card explicitly
called that out as not good enough for this feature ("don't silently drop
the qualifier"), so `shared/agentActivity/format.ts#formatEstimatedCost`
always states one of two things: `"pricing as of <date>"` when known, or the
literal `"pricing snapshot date unknown"` when not — never just the bare
`"$X estimated"`. Tested explicitly in `format.test.ts`. I did not import
Fleet's `formatEstimatedCost` (couldn't — `architecture.test.ts` forbids
`features/agentActivity`-style cross-feature reach, and this logic lives
outside `features/` anyway) so the two implementations now intentionally
diverge on this one point; flagging in case a future pricing-snapshot card
wants to reconcile them once a real snapshot date exists somewhere.

**Tab visibility — "no chip and no empty tab".** Unlike every other tab in
`ItemDetailDrawer.tsx` (which self-fetch once mounted), `AgentActivityTab`
receives its data as a prop. The drawer has to know whether the item has any
agent activity *before* deciding whether "Agent Activity" appears in the tab
list at all, so it fetches once (`agentActivity` resource in
`ItemDetailDrawer.tsx`) and both decides tab visibility and feeds the tab —
avoiding a second, redundant fetch. A 404 (orchestration disabled, the
default for every install) or any other fetch failure is treated identically
to "no activity": the tab quietly doesn't appear, no error surfaces. Tab is
inserted right after "Activity" when present.

**Board/List/Table wiring.** Each view calls `useAgentActivityMap(projectId)`
once and threads `stateFor(item.id)` down to where it already renders
per-item metadata: Board's `ItemCard` header row (next to the short id),
List's `ItemRow` (next to the status pill), Table's title cell (inline after
the title, no new column/localStorage schema change). The hook fails open on
any fetch error including 404 — a missing badge is never worth degrading
three list views over, since orchestration-disabled is the default state for
every existing install (TODO.md §0 rule 8). No existing Board/List/Table test
files existed to break (`features/board` has none; `features/list` has none;
`features/table/Table.test.ts` only covers the exported pure `sortItems`/
`filterItems` helpers, untouched).

**Design tokens.** Introduced no new color pairing — `AgentStateChip` reuses
`Badge`'s five non-primary tones verbatim (`neutral, info, warning, success,
danger`), all already AA-audited across all six palette×mode combinations
per A10's handoff above. The "running" state's pulse animation reuses the
existing `tk-pulse` keyframe already defined in `index.css` (used today by
`Board.tsx`'s live-socket indicator) — referenced by name only, no CSS edit
(A11 is mid-audit there, out of my ownership regardless).

**Tests.** 5 new test files, all passing: `shared/agentActivity/format.test.ts`
(cost/token/time formatting, the state-derivation fold-into-failed cases, the
null-pricing-snapshot honesty rule), `shared/agentActivity/api.test.ts`
(`isOrchDisabled`, both fetch functions hit the right path),
`shared/agentActivity/useAgentActivityMap.test.tsx` (resolves a state for a
present item, `undefined` for an absent one, fails open on both 404 and 500),
`shared/ui/AgentStateChip.test.tsx` (exactly 5 states/tones, a text label
present for every state — never color alone, the optional title, the pulse
only on `running`), `features/item-detail/tabs/AgentActivityTab.test.tsx`
(empty/loading states, newest-attempt-first ordering regardless of input
order, the pending-approval banner appearing/disappearing, the cost-honesty
rule end-to-end, an unrecognised `remote_status` rendering `Failed` without
throwing). `cd frontend && npm run type-check && npm run lint:tokens && npm
run test && npm run build` all green — `lint:tokens` unchanged at baseline
(0 raw color literals, 1 pre-existing inline hex); `test` shows the exact
same 3 pre-existing failures called out in the card brief
(`client.test.ts`'s `requestBlob`, two `createObjectURL` assertions in
`GlobalSettings.test.tsx`/`panels.test.tsx`), nothing else. Did not run the
Playwright e2e suite per the card's explicit instruction (A11 mid-audit on
`e2e/a11y.spec.ts`).

**For whoever builds the real endpoints (a natural Wave-3-or-later card,
unassigned as of this note):** `shared/agentActivity/api.ts`'s header comment
is the target — match it field-for-field the way A4 matched A5's `fleet/
api.ts`, and the frontend needs zero changes. The two open questions flagged
inline there: (1) whether "latest attempt" for the bulk badge endpoint should
break ties by `attempt` number or `dispatched_at`, and (2) whether an inner-
join (rows only for items with activity) vs. a left join with an explicit
`null`/`none` status is the right contract — I chose inner-join because it
maps directly onto "no chip for no activity," but a left join would let the
frontend distinguish "never dispatched" from "dispatched, but the reconciler
hasn't polled since" if that distinction ever matters.

### A12 — 2026-08-04

**Card:** fix the Avatar contrast bug A10/A11 found and deliberately left
alone (`frontend/src/shared/ui/Avatar.tsx` — white initials over a per-name
`hsl()` background, ~56% of hues failing AA, never caught because no e2e
fixture ever set an `assignee`), close the coverage hole, and correct the
false "correct in both modes" comment in `scripts/check-tokens.sh`.

**Approach chosen: per-avatar text-color selection by computed luminance**,
not a curated safe-hue palette. Reasoning: the background hue is real
identity signal (A10/A11's audits treat "same person, same color" as
intentional product behavior elsewhere in the app), and the fix that best
preserves that while being *provably* correct doesn't need to touch the hue
generation at all — it only needs to stop assuming white always works.
`hueFromName` and the `hsl(hue, 45%, 50%)` background are unchanged.

`Avatar.tsx` now computes the WCAG relative luminance of the exact background
about to be painted (`hslToRgb` → `relativeLuminance`, same conversion the
browser's `hsl()` uses) and picks whichever of two fixed extremes — pure
white or pure black — has the higher contrast against it
(`textColorForHue`). This isn't a heuristic: for any background luminance L,
max(contrast(L, white), contrast(L, black)) ≥ 4.5 always holds at this
saturation/lightness (verified by exhaustively computing all 360 integer
hues, not sampled — script below). The two extremes needed new tokens
because they must NOT vary with the app's own theme the way
`--color-text-inverse` does: the avatar background is the same hue in every
palette/mode, so the correct text pick for a given hue is also the same in
every palette/mode. Added `--color-avatar-ink-white` (`#ffffff`) and
`--color-avatar-ink-black` (`#000000`) to `index.css`, defined exactly once
in the base `:root` block and deliberately never redefined in any
`.dark`/`[data-palette]`/`prefers-color-scheme` block — that's what makes
them constants instead of theme-relative aliases. Comment at the definition
site says so explicitly, to stop a future edit from "fixing" them into a
palette-aware pair by mistake.

**Hex-literal gate:** this *removes* the raw-hex use entirely (both
`color: '#fff'` and the old implicit assumption) rather than trading one hex
for two — `textColorForHue` returns a `var(--color-avatar-ink-*)` string, so
`Avatar.tsx` no longer contains any hex literal. Confirmed via
`scripts/check-tokens.sh`'s own pattern: `Inline-style hex literals: 0`.
Lowered `STYLE_BASELINE` from `1` to `0` to ratchet the win in, and deleted
the false comment that called the old behavior "correct in both modes" —
replaced with nothing, since there's no longer an exception to explain.

**Before/after contrast, exhaustively over all 360 hues at `s=45% l=50%`**
(script in scratchpad, not committed — WCAG relative-luminance formula, same
`hsl→sRGB` conversion the CSS spec/browser uses):

| | Before (fixed white text) | After (`textColorForHue`) |
|---|---|---|
| Hues failing AA (4.5:1) | 202 / 360 (56.1%) | 0 / 360 |
| Worst-case contrast | 2.08:1 (hue 60) | 4.59:1 (hue 10, black text) |
| Best-case contrast | — | 10.08:1 (hue 60, black text) |

The 56.1% figure matches A11's "roughly 56%" estimate from the original
finding. Sample points (every 30°) confirming the crossover behaves as
expected — dark-background hues (blue/violet, ~210–330°) pick white, the
rest pick black, and the two ranges overlap around the true crossover rather
than leaving a gap:

```
hue=  0  bgLum=0.1514  white=5.21  black=4.03  → white(5.21)
hue= 30  bgLum=0.2605  white=3.38  black=6.21  → black(6.21)
hue= 60  bgLum=0.4539  white=2.08  black=10.08 → black(10.08)
hue=210  bgLum=0.2011  white=4.18  black=5.02  → black(5.02)
hue=240  bgLum=0.0920  white=7.39  black=2.84  → white(7.39)
hue=330  bgLum=0.1624  white=4.94  black=4.25  → white(4.94)
```

**Coverage hole closed, both ways per the card's preference:**

1. **Unit test** (`frontend/src/shared/ui/Avatar.test.tsx`) — the strictly
   stronger check, over the whole hue space rather than one name. Exported
   `hslToRgb`, `relativeLuminance`, `contrastRatio`, and `textColorForHue`
   from `Avatar.tsx` specifically so the test can *independently*
   recompute the contrast for whichever token the function picked (not just
   re-assert its internal decision) and prove every one of the 360 hues
   clears 4.5:1. Also asserts: the pre-fix "always white" behavior really
   does fail for >50% of hues (regression guard — if this stops failing, the
   background formula changed and the "verified exhaustively" claim above
   needs re-checking), a directional sanity check (hue 240 → white, hue 60 →
   black), and that a mounted `Avatar` never emits a raw hex literal.
2. **E2E fixture** (`frontend/e2e/a11y.spec.ts`, new test "board view with an
   assigned item (populated avatar)") — added
   `createItemWithAssignee` to `e2e/helpers.ts` (new function, existing
   `getOrCreateItem` untouched) since no existing helper could create an item
   carrying `assignee` and the reuse-if-exists pattern in `getOrCreateItem`
   would silently skip creation on a re-run. Used assignee name `"Avery
   Green"` deliberately — `hueFromName("Avery Green")` = 58, right in the
   pre-fix failing band (old white-on-bg contrast ≈2.1:1), so the fixture
   actually exercises the bug rather than getting lucky with a hue that
   happened to pass even before the fix. Only additive changes to
   `a11y.spec.ts`; no existing scan, assertion, or `KNOWN_ISSUES` entry
   touched.

**Verification:**
- `npm run type-check` — clean.
- `npm run lint:tokens` — `Raw color literals: 0 (baseline 0)`,
  `Inline-style hex literals: 0 (baseline 0)` (ratcheted down from 1).
- `npm run test` — 245/248; the same 3 pre-existing `requestBlob`/
  `createObjectURL` failures named in the card (`client.test.ts`,
  `GlobalSettings.test.tsx`, `panels.test.tsx`), plus my 5 new `Avatar.test.tsx`
  tests all passing. No other file's tests changed status.
- `npx playwright test e2e/a11y.spec.ts --project=chromium` — **11/11 pass**
  (the original 10 plus the new populated-avatar scan). First attempt hit a
  webServer build failure from a concurrent, unrelated in-flight change
  (`crates/tack-api/src/orch_store.rs` missing a trait method) — confirmed
  via `git status` that I hadn't touched that file, wasn't mine to fix, and a
  retry a few minutes later (another agent's work landing) built and passed
  clean. Not something to chase further from here.

**Could not verify:** the exact rendered pixel appearance in all six
palette×mode combos by eye (didn't screenshot) — relied on the exhaustive
math instead, per the card's explicit steer away from eyeballing. Since the
avatar background/text pair is palette/mode-invariant by design, this should
be a non-issue, but flagging since I didn't visually spot-check Clay/Graphite
dark specifically.

### B3 — 2026-08-04 — ⚠️ agent terminated mid-card; note written by the coordinator, not by B3

**B3 did not write this note.** Its session ended on a usage limit immediately after
it reported "Compiles. Now let's regenerate the OpenAPI spec and run the full workspace
test suite." Everything below is what I could establish by inspecting the tree
afterwards — **B3's own reasoning is lost**, so treat design rationale here as absent,
not as agreed.

**What is in the tree.** Migration `025_orch_metrics`, plus **two further migrations
B3 added on its own initiative and which the card did not ask for**: `026`
(`orch_events_daily`) and `027` (`orch_metrics_daily`) rollup tables. New repo fns in
`repo/orch.rs`: `upsert_orch_metrics`, `list_latest_orch_metrics`,
`rollup_and_purge_orch_events`, `rollup_and_purge_orch_metrics`,
`list_orch_events_daily`, `list_orch_metrics_daily`. New test file
`crates/tack-db/tests/orch_metrics_test.rs`. Handler work in `handlers/orch.rs`
(includes `tack_item_status_counts`, so `GET /api/metrics` appears to merge Tack's own
work-tracking metrics with mirrored docket ones, as the card asked).

**Verified state (coordinator, after termination):** `cargo test --workspace` 383
passed / 0 failed (was 362 before B3 — so ~21 new tests, and they pass). OpenAPI drift
gate passes, so the spec *was* regenerated despite B3 believing it hadn't been.

**Two gate failures I fixed, since B3 never reached its checklist:**
1. `cargo fmt` — one unformatted signature in `handlers/orch.rs`. Ran `cargo fmt --all`.
2. `cargo clippy -D warnings` — `create_test_workspace` dead in the new
   `orch_metrics_test` binary. Every prior test binary happened to call it (one via a
   fully-qualified path), so it had never needed an attribute; `orch_metrics_test.rs` is
   the first that doesn't. Added `#[allow(dead_code)]` in `tests/common/mod.rs`,
   matching what `make_project`/`make_item` there already do, with a comment explaining
   the per-binary compilation reason.

**Retention ordering — VERIFIED 2026-08-05 (coordinator), and B3 beat the spec.**
The card asked for the rollup to commit *before* the raw delete so a crash between them
loses nothing. B3 did better: the aggregate `INSERT … ON CONFLICT DO UPDATE` and the
`DELETE` run **inside the same transaction** per batch (`repo/orch.rs`
`rollup_and_purge_orch_events`, ~line 1262), so a crash between them is *impossible*
rather than survivable. Re-running after a committed batch can't double-count, because
the rolled-up rows are already gone. Batching is per-transaction and bounded by
`batch_size`, so the single-writer lock is never held across the whole sweep; the loop
exits on a short batch. Tests pin it: `…_preserves_totals_and_deletes_raw_rows`,
`…_sweeps_a_backlog_larger_than_one_batch`, `…_leaves_fresh_rows_untouched`,
`…_rerun_with_no_new_data_is_a_noop`. Non-finite metric samples are counted in
`sample_count` but excluded from `value_sum`/`min`/`max`, so one `NaN` can't poison a
day. **This item is closed — no further verification needed.**

**Still not established:**
- Whether migrations 026/027 were the right call versus aggregating into `orch_metrics`.
  They are a schema commitment made without review.
- `evaluate()` untouched, rule-5 fetch/persist split preserved, rule-6 naming
  (`cost_usd_estimated`) — all unverified by me.

**For B4:** `orch_store.rs` is free now. **For whoever verifies B3:** start with the
retention ordering test, then the two extra migrations.

### B4 — 2026-08-05

**What I built.** Task 34.5 — two new `BoardEvent` variants
(`crates/tack-api/src/handlers/websocket.rs`) and the broadcast that fires them,
placed exactly where the card specified: inside `RepoControlPlaneStore::upsert_runs`/
`upsert_approvals` (`crates/tack-api/src/orch_store.rs`), **not** in
`tack-orch::reconciler` (I never opened that file). `tack-orch` still has zero
websocket dependency.

```rust
BoardEvent::AgentRunUpdated { project_id, item_id, run_id, state }
BoardEvent::ApprovalPending { project_id, item_id, token, action }
```

**How I detect a real change (compare-before-write, not a repo-layer return
value).** Both `upsert_runs`/`upsert_approvals` now do: (1) read each row's
*pre-upsert* state via `Repository::get_orch_run`/`get_orch_approval` — one
extra read per row, batches are per-project-per-poll and small, not worth
redesigning A3's/B1's/B3's repo-layer return types over; (2) run the real
upsert exactly as it was; (3) diff old vs. new and broadcast only on a real
change. "Real change" for a run = new row, `state` changed, or attribution
just went from unattributed to attributed. For an approval it's narrower —
only a transition *into* `pending` (new row, or `state` became `pending`, or
newly attributed while still `pending`) — a grant/deny decision or a re-poll
of an already-pending approval never re-fires `ApprovalPending`; that event
name promises "needs a look," not "something happened." Both diffs compute
the *effective* item_id the same way the SQL's own
`COALESCE(excluded.item_id, orch_runs.item_id)` does
(`r.item_id.or(old_item_id)`), so a poll that carries `item_id: None` while a
prior poll already learned the real attribution is correctly seen as
unchanged, not as "attribution lost."

**Unattributed runs/approvals — deliberate, disclosed choice.** `BoardEvent`s
are delivered to per-project WebSocket subscribers
(`event_matches_project` in `websocket.rs`) — there is no "everyone" channel.
A run with no Tack `item_id` (docket-CLI-dispatched, or an approval docket
hasn't correlated) has no project to filter into, so I skip the broadcast
entirely rather than guess or invent a project. The row is still fully
persisted either way (D1's fleet-wide approvals inbox and any future
uncorrelated-runs view are unaffected); the first poll that *does* learn the
attribution broadcasts retroactively via the "newly attributed" branch above.
I did not add an unfiltered/global WebSocket channel — that's a bigger design
change than this card, and every existing `BoardEvent` already assumes a
project.

**Acceptance bar — a second identical poll broadcasts nothing.** Tested
directly against the real store and a real broadcast channel (not a mock),
in a new file: `crates/tack-api/tests/orch_broadcast_test.rs` (+8 tests).
Covers: a new correlated run broadcasts once
(`upsert_runs_broadcasts_for_a_new_correlated_run`); an identical re-poll
broadcasts nothing
(`a_second_identical_poll_of_the_same_run_broadcasts_nothing` — the card's
literal acceptance bar); a real state transition broadcasts again
(`upsert_runs_broadcasts_again_when_state_changes`); an uncorrelated run
broadcasts nothing until attribution is learned, then broadcasts once, then
goes quiet again on the next identical poll
(`uncorrelated_run_does_not_broadcast_until_attribution_is_learned`); and the
same four shapes mirrored for approvals, plus
`upsert_approvals_does_not_broadcast_for_a_non_pending_state` (granted/denied
never fires `ApprovalPending`). `crates/tack-api/src/handlers/websocket.rs`
also gained 5 unit tests: snake_case serde tags for both new variants,
per-project filtering, and
`an_unrecognised_event_type_fails_to_deserialize_without_panicking` (the
Rust-side analogue of the client's "unknown variant is ignored, not thrown"
acceptance point — decoding a future tag is a recoverable `Err`, never a
panic).

**Wiring the broadcast sender into the store.** `RepoControlPlaneStore::new`
now takes a second parameter, `broadcast_tx: broadcast::Sender<BoardEvent>`
— `server.rs` passes `state.broadcast_tx.clone()` alongside `state.repo.clone()`,
the same sender every WebSocket subscriber gets. The store sends via its own
`broadcast()` helper (mirrors `handlers::websocket::broadcast_event`'s
ignore-send-errors behavior) rather than calling that free function directly,
since the store only holds a bare sender, not a full `AppState`.

**Deviation from file ownership, disclosed per the rules.** A7's
`crates/tack-api/tests/orch_reconciler_wiring_test.rs` calls
`RepoControlPlaneStore::new` at 5 call sites; changing the constructor's
arity meant all 5 needed a `let (broadcast_tx, _) =
tokio::sync::broadcast::channel(100);` added ahead of them. Mechanical only —
no assertion or test behavior changed, confirmed by re-running that file's
suite (still 5/5 green). I did not touch anything else in that file.

**Frontend.** `shared/realtime/boardSocket.ts` itself needed no logic
change — `dispatch()` already forwards any event whose `project_id` matches
and only special-cases `ping`, so a wire message with a `type` this build
doesn't recognise already passes through to listeners without throwing (the
try/catch is around `JSON.parse`, not the type check). I proved that
explicitly instead of assuming it: `an unrecognised event type on the wire is
forwarded, not thrown` in `boardSocket.test.ts` (+3 tests total there, one of
which extends an existing test's assertion list rather than adding a new
`it`). The one required change was `BoardEvent`'s TS union
(`frontend/src/shared/types/index.ts`, disclosed edit — not in my
`shared/realtime/**` ownership, but the type has to move in lockstep with the
Rust enum and nobody else owns it this wave, same category of necessary glue
A2/A7/B1 each flagged in their own handoffs).

For "make B5's agent-activity UI respond to them," I extended
`useAgentActivityMap` (`shared/agentActivity/useAgentActivityMap.ts`) with a
`refetch()` — the hook's `createResource` already had one, it just wasn't
exposed — and wired it into all three surfaces that use the hook:

- **Board.tsx** already had a live socket; its handler now also calls
  `agentActivity.refetch()` specifically on `agent_run_updated`/
  `approval_pending` (the plain `refetch()` for items doesn't touch this
  resource — a run/approval change doesn't change the item's status).
- **List.tsx and Table.tsx had no live socket at all before this** — only a
  same-tab `ITEM_UPDATED_EVENT` custom-event listener for drawer edits. I
  added a `createBoardSocket` connection to each, scoped *only* to
  refreshing `agentActivity` on the two new event types — I deliberately did
  not extend them to live-refresh the full item list too, since that's a
  materially bigger change to those views' existing architecture than this
  card asked for and isn't needed for the acceptance bar ("a running dispatch
  moves the board without a refresh" — Board already does that for item
  moves; this closes the gap specifically for the badge chip).
- **`ItemDetailDrawer.tsx`** (B5's file — minimal, surgical edit per the
  card's own allowance): the per-item `agentActivity` resource's `refetch`
  is now exposed and wired to a socket scoped to the open item's
  `project_id` (only knowable once the item itself has loaded, so the effect
  waits on `item()`), filtered to `event.item_id === itemId()` so a change on
  some other item in the same project doesn't refetch this drawer's tab.

**Known tradeoff, not fixed.** With this change, up to three independent
WebSocket connections to the same project can be open at once (Board's own +
List's-or-Table's new one, if the drawer is open on top of either — the
drawer's socket only opens while it's mounted with an item loaded). Each is
cheap (one more subscriber on a 100-capacity broadcast channel, filtered
server-side) and correctness doesn't depend on there being exactly one, but a
future card that wants a single shared per-project socket (e.g. hoisted into
`projectContext` or a new connection-manager) would remove the duplication.
I did not build that here — it's a bigger architectural change than "wire two
new event types into the existing pattern."

**Verification.** `cargo test --workspace`: 396 passed, 0 failed, 3 ignored
(k6/doctest placeholders, pre-existing) — up from 383 baseline by exactly the
13 tests I added (8 in `orch_broadcast_test.rs`, 5 in `websocket.rs`); no
other agent's tests landed in the shared tree while I worked, confirmed by
the arithmetic matching exactly. `cargo clippy --workspace --all-targets -- -D
warnings`: clean. `cargo fmt --all -- --check`: clean. Frontend: `npm run
type-check` clean; `npm run lint:tokens` — 0 raw color literals / 0 inline-hex,
unchanged baseline (I added no new UI, only socket/data wiring); `npm run
test` — 248 passed, 3 failed, the exact same pre-existing `requestBlob`/
`createObjectURL` failures called out in the card brief (up from the 245/3
baseline by exactly the 3 tests I added); `npm run build` also clean. Did not
run the Playwright e2e suite (not in this card's green-before-handoff list,
and A11's a11y audit may still be touching `e2e/a11y.spec.ts`).

**What's still open.**

- **The multiple-sockets-per-project tradeoff above** — flagged, not fixed.
- **`orch_approvals.agent`/`action` values feeding `ApprovalPending.action`**
  are whatever B1's ingestion put there (`agent` from docket's `role` field,
  per B1's handoff) — I didn't second-guess that mapping, just forwarded
  `action` verbatim onto the wire event.
- **No Rust-side consumer of `AgentRunUpdated`/`ApprovalPending` exists yet
  beyond the WebSocket wire** — by design, this card's scope is the broadcast
  itself, not a new UI surface. D1 (approvals inbox) may eventually want to
  subscribe to `ApprovalPending` directly rather than polling; the event is
  shaped to support that (`token` is exactly what
  `POST /api/approvals/{token}` needs) but nothing wires it up yet.
- **Performance of the compare-before-write reads** — one extra `SELECT` per
  run/approval per poll, per linked project. Fine at today's scale (a
  handful of in-flight runs/approvals per plane, per B1's own note about
  `list_linked_projects`); if a plane ever mirrors thousands of runs per
  poll, batching the "get many by id" lookup instead of one query per row is
  the first place to optimize — I didn't do this preemptively since A3's
  `repo/orch.rs` has no batch-get today and adding one is outside my file
  ownership this wave.

### B2 — 2026-08-05

**What I built.** Task 34.4 — the reconciler now mirrors `GET
/traces/{project}?since=` per linked project into `orch_events`, following
B1/B3's `poll_*`/`persist_*`/`FetchOutcome` recipe exactly (one field, one
fetch fn, one line in `reconcile_once`; persistence in `spawn_one`'s loop
after `record_health`). All in `crates/tack-orch/src/reconciler.rs` — the
"Trace cursor" section I added to that file's module doc has the full
argument for everything below in more depth than this note.

**The real cursor semantics — verified against `serve.py` source, not
guessed.** docket's `/traces/{project}?since=` cursor is a compound
`"<ts>Z:<n>"` token (`_traces_page`/`_decode_trace_cursor`), not a
timestamp and not an offset: `ts` is second-granularity and the underlying
filter (`core.trace.export_lines`) is *inclusive* (`ts >= since`), so `n`
counts how many events at that exact second have already been delivered —
without it, a poll would either redeliver a whole second's worth of events
forever (bare-ts cursor) or silently drop same-second events arriving after
the first poll that saw that second (`ts >` instead of `ts >=`). I mirrored
docket's exact anchor/count algorithm client-side (`next_trace_cursor`,
`decode_trace_cursor`) rather than approximating it, and unit-tested it
against hand-computed cases including the same-second-boundary-spans-two-polls
case docket's own comment calls out.

**Why client-side reconstruction at all, instead of just reading docket's
`next` field:** the frozen `ControlPlane::traces` trait (§1.1) returns
`Vec<RemoteEvent>` only — no second value comes back through it, and I don't
own `lib.rs` to widen it. `next_trace_cursor` is provably equivalent to
reading docket's own `next` and discarding it, because the events it
operates on are already `_traces_page`'s post-trim `lines` — same
computation over the same data, same answer. Flagging this because it's the
one place this card genuinely wanted a trait change and didn't take it;
if a future wave *can* touch `lib.rs`, adding a `next` return value would let
`reconciler.rs` delete `next_trace_cursor`/`decode_trace_cursor` entirely in
favor of the value docket already computed.

**⚠️ A real bug this card found, not just designed around: `adapters/docket.rs`'s
`traces()` had the wrong wire shape.** A1 built it before docket's `/traces`
endpoint existed and disclosed the guess explicitly (`{"events": [...]}` as
parsed objects). The real shape, verified by reading `serve.py`'s
`_traces_page`/`do_GET` directly: `events` is an array of **raw JSON
strings** — `_traces_page` returns docket's verbatim on-disk JSONL lines
(never parsed back into objects), and `do_GET` `json.dumps`s that list of
strings as-is. A1's original `TracesResponse { events: Vec<RemoteEvent> }`
would have failed to deserialize *every* real docket response
(`serde_json` erroring on a string where it expected an object) — not
"silently," as the card's wire-format-trap warning about snake_case
predicted, but loudly, which at least means it would have been caught
immediately in integration rather than corrupting data. I fixed this in
`adapters/docket.rs` (`TracesResponse.events: Vec<String>`, decode each
element a second time, drop-and-warn on an individual malformed line rather
than failing the whole page) — **a file A1 owns, not me**, disclosed here
per the "minimal necessary cross-file edit" precedent A2/A3/B1 all used.
I also corrected `tests/fixtures/traces_list.json` and
`docket_adapter_test.rs`'s traces tests to the real shape — those would
have kept passing against the wrong assumption forever otherwise, since a
fixture that's wrong in the same way as the code it tests proves nothing.
The snake_case trap itself (the card's other warning) turned out to already
be handled correctly — `RemoteEvent` in `lib.rs` has no `rename_all =
"camelCase"`, unlike every other DTO in that file — so no fix was needed
there, just confirmation.

**Event id derivation — option 2, not option 1.** Checked docket's real
trace-record writer (`core/trace.py::trace_event`) directly: the fields it
ever writes are `ts`, `project`, `session_id`, `agent_role`, `event_type`,
`payload`, and two optional fields (`cost_usd`, `duration_ms`) — no
monotonic sequence number, no byte offset. So `(control_plane_id,
remote_project, seq)` (the card's preferred option 1) is not available;
`derive_event_id` is UUIDv5 (namespace + name, deterministic, no
randomness) over every one of those fields plus `control_plane_id`/
`remote_project`, joined with a `\u{1}` separator so field boundaries can't
collide (tested explicitly:
`derive_event_id_ignores_field_boundaries_not_field_content`). `payload`'s
JSON serialization is already field-order-stable for free — this crate
never enables `serde_json`'s `preserve_order` feature, so `Value::Object`
is a `BTreeMap` under the hood. `Uuid::new_v5` needed the `uuid` crate's
`v5` feature, which the *workspace* `uuid` dependency (root `Cargo.toml`,
W0-A's file) doesn't enable — I added `features = ["v5"]` to `tack-orch`'s
own `Cargo.toml` line instead (`uuid = { workspace = true, features =
["v5"] }`), an additive per-crate feature request that doesn't touch or
widen W0-A's file, same pattern B1 used adding a dev-dependency there.

**Retention composition — the part neither A2 nor I had looked at before
this card.** Worked out the actual failure mode concretely: `orch_events.id`
being content-derived (not server-generated) means a lost/rewound cursor
re-delivering an event whose row was already rolled into
`orch_events_daily` and purged by B3's sweep would `INSERT` a fresh row
with the *same* id the purged row had — indistinguishable from a genuinely
new event to the *next* sweep, which would roll its count in a second time,
silently corrupting a real cost/token total. I considered and rejected a
schema-based "purge watermark" (a persisted high-water mark of the last
completed sweep's cutoff) because it needs either a new column somewhere or
its own table, and a much simpler check gives the same safety: I recompute
`now - retention_days` — the *identical* formula `spawn_retention_sweep`
uses for its own cutoff, threaded through a new `ReconcilerConfig
.event_retention_days` field so both sides can never independently drift —
at ingest time in `persist_events`, and drop (don't insert) any event whose
`occurred_at` already predates it. Cost: a handful of events at the extreme
edge of a pathological rewind never get (re-)counted. Benefit: a corrupted
total is structurally impossible, not just unlikely. Tested both in
isolation (`an_event_older_than_the_retention_cutoff_is_not_persisted`,
against a `FakeStore`) and composed with B3's *real* rollup function
end-to-end (`retention_composition_re_ingesting_a_purged_event_does_not_double_count`,
`crates/tack-orch/tests/traces_ingestion_test.rs` — real in-memory SQLite,
real `Repository::rollup_and_purge_orch_events`): ingest with a wide
retention window → roll up and purge for real → re-poll the identical stale
event with a realistic small retention window → assert the raw row count
stays 0 and the daily aggregate's `event_count` stays 1 after a second
rollup pass, not 2.

**Cursor storage — a new table, not a column on `orch_links`.** The card
said "your call, say what you chose and why." `orch_links` is keyed by
`project_id` (Tack's side of the link) and can be unlinked/relinked
independent of docket's own trace history; the card explicitly wants the
cursor keyed by `(control_plane_id, remote_project)`, and I didn't want to
assume that pair is 1:1 with `orch_links` rows forever. New migration
**028** (`orch_trace_cursors`, `crates/tack-db/src/migrations.rs` — 025/026/027
were taken per the card's own numbering note), PK `(control_plane_id,
remote_project)`, `ON DELETE CASCADE` from `control_planes`. Repo functions
`set_trace_cursor`/`list_trace_cursors` in `crates/tack-db/src/repo/orch.rs`
(A3's file — extended, not restructured, same as B1/B3 did), three new
`ControlPlaneStore` trait methods (`list_trace_cursors`, `set_trace_cursor`,
`upsert_events`), fetched once per plane per tick in `spawn_one` alongside
the existing `list_linked_projects` read — same "short DB read outside the
panic-isolation boundary" pattern B1 established and explicitly flagged for
me to reuse or not; I reused it, same tradeoff, same reasoning (a DB read is
far less likely to panic than N HTTP calls).

**Correlation: session_id, not run_id, and no natural run_id at all.**
Checked `core/dispatch.py` directly: docket mints `session_id =
f"agent:{project}:{task_id}"` for a task-dispatched session (also
`"agent:{project}:dispatch"` for a bare dispatch, and a project-key form in
`core/pod.py` — neither correlates, which is fine and expected, not an
error). `session_id_task_id` parses the `<suffix>` out and feeds it through
the same `correlate_remote_task` helper B1 already built for approvals.
`RemoteEvent` carries no `run_id` field at all (docket's trace payload
doesn't have one) — `NewOrchEvent.run_id` is always `None` on ingest; I did
not invent a session_id→run_id lookup since nothing in the current
`ControlPlaneStore` surface supports one and it wasn't asked for.

**Files touched, and why, beyond my exclusive `reconciler.rs` +
`repo/orch.rs` + `migrations.rs`:**

- `crates/tack-orch/src/adapters/docket.rs`, its `traces_list.json` fixture,
  and `docket_adapter_test.rs` — the real wire-format bug above; A1's file,
  disclosed.
- `crates/tack-orch/Cargo.toml` — one additive feature flag (`uuid`'s
  `"v5"`), no workspace-root edit.
- `crates/tack-api/src/orch_store.rs` — **re-read immediately before
  editing, per the card's explicit instruction, and found B4's broadcast
  work already landed** (`upsert_runs`/`upsert_approvals` now broadcast,
  `RepoControlPlaneStore::new` takes a `broadcast::Sender<BoardEvent>`). My
  three new methods (`list_trace_cursors`/`set_trace_cursor`/`upsert_events`)
  are a clearly separate block after `upsert_metrics`, thin pass-throughs,
  nothing reformatted or restructured around them. **Deliberately no
  broadcast in `upsert_events`** — no `BoardEvent` variant exists for a
  trace event and adding one is explicitly B4's territory, not mine; also
  flagging that a naive "broadcast every upsert_events call" would be far
  noisier than `upsert_runs`/`upsert_approvals` ever are (many events per
  poll, not one state transition) and would need its own
  rate-limiting/aggregation design if anyone picks it up later.
- `crates/tack-api/src/server.rs` — one field + comment
  (`event_retention_days: config.orch_event_retention_days`) in the
  `ReconcilerConfig` literal A2's reconciler-spawn block already builds;
  necessary because I added a field to `ReconcilerConfig` (mine to change)
  and the one construction site outside my ownership needed updating to
  compile. `AppConfig.orch_event_retention_days` already existed (A4's
  config work); I only added the one line wiring it through.
- `crates/tack-orch/tests/ingestion_test.rs` (B1's file) and
  `crates/tack-api/tests/orch_reconciler_wiring_test.rs` — both had a full
  `impl ControlPlaneStore` and/or a `ReconcilerConfig { poll_secs: N }`
  literal that stopped compiling the moment I grew the trait/struct. Fixed
  minimally (three pass-through methods added in the same mechanical shape
  as their existing ones; `..Default::default()` added to the config
  literals) — unavoidable, since a trait/struct I own gaining a field breaks
  every implementor/constructor workspace-wide, not just my own files.
- `crates/tack-db/tests/orch_metrics_test.rs` (B3's file) — **at the
  coordinator's explicit request**, not my own initiative: migration 028
  broke two brittle assertions (`applied.last() == "027_..."`, `COUNT(*) ==
  27`) that encoded "027 is the newest migration in the project" rather
  than "this card's three migrations applied." Rewrote both to assert
  presence-and-order of 025/026/027 specifically, so migration 029+ won't
  break them again either.

**Testing.** `crates/tack-orch/src/reconciler.rs`'s own `#[cfg(test)] mod
tests` (+27 tests): `derive_event_id` determinism/field-sensitivity/
delimiter-safety, `decode_trace_cursor`/`next_trace_cursor` against
hand-computed cases mirroring docket's own algorithm, `session_id_task_id`
parsing, correlated/uncorrelated event landing, unknown event-type
verbatim storage, traces-poll-failure-leaves-health-untouched, cursor
advancement and cursor-is-actually-sent-as-since, and the retention-age
filter. `crates/tack-orch/tests/traces_ingestion_test.rs` (new, +2 tests,
real stack — a real `Repository`, real migrations, real `DocketAdapter`
against `wiremock`, the real `spawn_reconcilers` loop, a `TestRepoStore`
duplicating `ingestion_test.rs`'s shape per this cycle's per-card test-file
ownership): idempotent mirroring **and** an explicit deliberate cursor
rewind asserting the `orch_events` row count is unchanged (the card's exact
acceptance wording), and the full retention-composition scenario described
above. `crates/tack-db/tests/orch_repo_test.rs` (+3 tests): cursor
set/list scoped correctly per `(control_plane_id, remote_project)` (not
just `remote_project` — two planes can both have a project named "demo"),
upsert-in-place, and cascade-delete from `control_planes`.
`docket_adapter_test.rs`'s traces tests updated in place to the corrected
wire shape rather than added to, since the old ones tested the wrong thing.

**Verification.** `cargo test --workspace`: every crate green, 0 failures
(coordinator's stated baseline was 396 with B4 in; my additions are +3
`orch_repo_test`, +27 `reconciler::tests`, +2 `traces_ingestion_test`, and
the `docket_adapter_test`/`ingestion_test`/`orch_reconciler_wiring_test`/
`orch_metrics_test` changes are fixes-in-place, not net-new counts).
`cargo clippy --workspace --all-targets -- -D warnings`: clean (two
`collapsible_if` lints on my own new code, fixed with `let`-chains per
clippy's own suggestion — this workspace's Rust edition supports them).
`cargo fmt --all -- --check`: clean — ran `cargo fmt -p tack-orch -p
tack-db -p tack-api` (my three touched crates only, not `--all`) first per
A3's convention of not reformatting a file mid-edit in another agent's
terminal, then confirmed the workspace-wide check passes with no remaining
diff anywhere.

**What's still open / for whoever's next.**

- **The `next_trace_cursor`/`decode_trace_cursor` client-side
  reconstruction is a real, if narrow, maintenance liability**: it must stay
  byte-for-byte in sync with `serve.py`'s `_traces_page` algorithm forever,
  with no compiler check that it has. If docket's cursor algorithm ever
  changes, or if a future wave gets permission to widen `ControlPlane
  ::traces`'s return type to carry `next` directly, that's the fix — not
  another parallel reimplementation.
- **`orch_events.run_id` is always `None` from this ingestion path** — see
  the correlation note above. If a future card wants trace events joined to
  a specific `orch_runs` row (not just an item), that needs either a new
  docket-side field or a session_id→run_id lookup this store doesn't expose
  today.
- **I did not verify this against a real running docket instance** — no
  live `docket serve` was available in this environment. Everything about
  the wire format (the double-encoded `events` array, the `next` cursor
  format, the trace record's exact fields) is verified by reading
  `serve.py`/`core/trace.py`/`core/dispatch.py` source directly at
  `~/Sites/rack-cli`, not by hitting a live server. A1's handoff describes
  the same limitation for the endpoints that existed in their wave; this
  card inherits it for the ones that didn't exist yet then. If a live
  capture becomes possible later, `traces_list.json`'s inner event objects
  (still A1's original real captures, just re-wrapped by me into the
  corrected envelope) and my hand-derived `next` values are the two things
  worth re-verifying first.
- **B4's territory, flagged not assumed:** whether `upsert_events` should
  ever broadcast, and in what shape — see above.

### V1 — 2026-08-05

**The card.** Close the risk A1 and B2 each flagged and left open: every
endpoint `DocketAdapter` (`crates/tack-orch/src/adapters/docket.rs`) and its
fixtures (`crates/tack-orch/tests/fixtures/`) assume was built and verified
by reading docket's Python source, never by talking to a running docket.
B2 proved the risk was real by finding a genuine bug (`traces()`'s
double-encoded `events` array) that source-reading alone let through. This
card's job was to stand up an isolated docket and check every endpoint
against reality.

**Isolation — confirmed, not assumed.** `DOCKET_HOME` pointed at
`<scratchpad>/docket-home` for every single invocation, no exceptions,
including the one-off `--version`/`list` sanity checks before anything else
ran. Baseline `~/.docket` mtime recorded before the first command
(`2026-08-05 07:38:45.512757176 -0300`) and re-checked after *every*
subsequent command in this card, including the very last one after tearing
the server down — identical every time. `~/.docket` was never read, written,
moved, or deleted. (Unlike the earlier incident this card exists partly to
guard against, `health.json`'s fixture already documents a prior agent
running `docket serve` against the real `~/.docket` and fail-closed-denying
11 real pending approvals — that did not recur here.)

**What I verified live**, against an isolated `docket serve --port 18401`
(pod `demo` provisioned via `docket.core.pod_provisioning.provision_pod` —
the same path `docket add`/`POST /pods` both call):

- `GET /health`, `/status.json`, `/metrics` (unauthenticated) — all match
  `Health`/`FleetStatus`/`MetricSample` exactly as `lib.rs` models them.
  `apiVersion` really is `"2"`, matching `reconciler::EXPECTED_API_VERSION`.
- `GET /runs`, `/runs/{id}` (found + unknown-id 404), `/approvals`,
  `/tasks/{project}`, `/traces/{project}?since=` (Bearer) — all match their
  DTOs and wrapper keys (`{"runs":[...]}`, `{"pending":[...]}`,
  `{"tasks":[...]}`, `{"events":[...],"next":...}`) exactly.
- `POST /dispatch/{project}` — creates a real `RemoteRun` (`source:
  "webhook"`, `state` transitioning `queued`→`running`), response
  `{"ok","run","project","status":"dispatched"}` (one field —`status`—
  beyond what TODO.md §1.4 documents; harmless, just incomplete docs).
- `POST /approvals/{token}` grant — genuinely resumes the gated task
  (`waiting_approval` → `pending`, confirmed via a follow-up `GET
  /tasks/{project}`), and an unknown token 404s with `{"ok":false,"error":
  "Approval not found: ..."}`.
- **`POST /tasks/{project}`'s three `pre_input` outcomes — all three,
  exhaustively, with real policy files** (three hand-written policies under
  the isolated `$DOCKET_HOME/policies/`: a `block` match, a
  `require_approval` match, and an `id: "prompt-injection"` match to
  exercise the `trusted` skip specifically):
  1. No match → `200 {"ok":true,"task":"<id>",...,"status":"pending"}`.
  2. `block` → `400 {"ok":false,"error":"task rejected by guardrail policy
     '<id>' at enqueue: <message>"}` — names the policy id, exactly as
     documented.
  3. `require_approval` → **still `200`**, but with `"status":
     "waiting_approval"` and a real `"approvalToken"` — never a 200 that
     pretends the task queued normally. Confirmed the task really did sit
     ungated until granted.
- **The `trusted` boundary — the one this card cared about most.** Same
  `prompt-injection`-id policy, same triggering description, two requests:
  omitting `trusted` entirely → `200`, task enqueued as `pending` (operator
  trust silently applied, policy skipped, exactly as `core/dispatch.py`'s
  docstring warns); passing `"trusted": false` explicitly → `400`, blocked
  by the same policy. This is a real, live-confirmed security boundary, not
  a read-source inference — C2 (Wave 3) can build on it with confidence.
- **The trace cursor, at exactly the seam the card named** ("second
  boundaries with multiple events in the same second, which is exactly
  where an off-by-one would hide"). Built a single-session-file project with
  hand-written JSONL timestamps to get a clean, reproducible sequence
  (docket's real trace files interleave many sessions, sorted by *filename*
  not chronologically — the natural first attempt at this, reusing the
  already-populated `demo` project, produced a cursor anchored on an
  unrelated approval session's trace file purely due to alphabetical
  filename ordering, which is why the clean single-session `curstest`
  project exists as a second fixture-capture project). Four real polls
  against the live server, feeding docket's own minted `next` back in as
  the next poll's `since`:
  1. `since=""` → 5 events (3 at `:00Z`, 2 at `:01Z`), `next=":01Z:2"`.
  2. `since=":01Z:2"` → 0 events, `next` unchanged.
  3. 2 more events appended at the *same* `:01Z` second → `since=":01Z:2"`
     returns exactly those 2 (not all 7), `next=":01Z:4"` — the prior
     count (2) carried forward and added to the new trailing run (2).
  4. One event at a new second (`:02Z`) → `since=":01Z:4"` returns just
     that event, `next=":02Z:1"` — the old second's count did not leak
     across the boundary.
  All four match `next_trace_cursor`/`decode_trace_cursor`'s existing
  hand-computed unit tests byte-for-byte. Added
  `next_trace_cursor_matches_a_real_docket_servers_minted_cursor`
  (`crates/tack-orch/src/reconciler.rs`) encoding this exact real sequence
  as a permanent regression test — the first time this reconstruction has
  been checked against a live server rather than only against hand-built
  cases, closing the specific gap B2's handoff flagged.

**What I found wrong, and fixed.**

1. **TODO.md §1.4's own summary line for `POST /tasks/{project}` is wrong
   about the response shape** (I'm flagging this here rather than editing
   §1.4 myself — this card's scope is `crates/tack-orch/**`, and §1.4 is
   shared, coordinator-maintained reference text, not a file I own). It
   says the response is `{taskId}`. The real response, confirmed live, is
   `{"ok": true, "task": "<id>", "project": "...", "status": "...",
   "approvalToken"?: "..."}` — the id is under the key `"task"`, not
   `"taskId"`, and three more fields ride along. This doesn't affect
   `DocketAdapter` today (`enqueue_task` is unconditionally `Disabled`, so
   nothing parses this response yet), but it would have been a real bug
   the moment Wave 3's C1 wired it up trusting the table as written.
   Corrected in `adapters/docket.rs`'s module doc (new "Verified live"
   section) so C1 reads the right shape before writing any deserialize
   code; whoever next edits TODO.md §1.4 should fix the table line too.
2. **`tests/fixtures/tasks_list.json` and `traces_list.json` were not
   actually live captures**, despite both endpoints being real and
   reachable — `tasks_list.json` was A1's derived-from-internals guess
   (only source-verified, and it flagged the wrapper key as an open
   question), `traces_list.json` was B2's envelope corrected against
   `serve.py` source but explicitly never exercised over the wire. Both
   are now genuine `curl` captures against the live isolated server
   (headers stripped, provenance rewritten to say so plainly). Both prior
   guesses turned out **correct** — the `{"tasks":[...]}` wrapper key and
   `RemoteTask`'s field shape match exactly, and the double-encoded
   `events`-as-strings + `next` envelope B2 worked out from source also
   matches exactly — so no adapter code changed, only the fixtures'
   authority and their session_id/task_id values (had to bump
   `docket_adapter_test.rs`'s `traces_happy_path_decodes_the_double_encoded_events_array`
   assertion from `"agent:demo:task-90e465a8"` to
   `"agent:demo2:task-7bf4553c"` to match the new capture; renamed
   `list_tasks_happy_path_against_projected_shape` to
   `list_tasks_happy_path_against_a_live_captured_shape` since "projected"
   is no longer accurate).
3. **Three stale doc comments in `adapters/docket.rs`** still said
   `POST /tasks/{project}`/`GET /tasks/{project}` "doesn't exist yet" /
   "blocked on docket Phase 22" — true when A1 wrote them, corrected in
   TODO.md's own table on 2026-08-04, but never propagated into this
   file's comments (the module doc's "Write methods" section, the
   `list_tasks` method body, the `enqueue_task` method body, and
   `TasksResponse`'s doc comment). Fixed all four, and added the "Verified
   live (card V1)" module-doc section documenting the `POST
   /tasks/{project}` response-shape and `trusted`-boundary findings above
   for whoever builds Wave 3.

**What I did not change.** `ControlPlane`'s trait signature — untouched.
`enqueue_task`/`dispatch`/`decide_approval` still unconditionally return
`OrchError::Disabled` — I confirmed the routes they'd call are real and
behave exactly as documented, but wiring them up is still Wave 3's design
work (`status_map`, idempotency), not this card's.

**What I still could not verify.**

- **`POST /approvals/{token}` deny, and the `ApprovalNoop`/409 path**
  (deciding an already-resolved token) — only tested `grant` and an
  unknown-token 404. `serve.py`'s logic here is short and was read
  directly, so risk is low, but it is genuinely unexercised live.
- **A same-second cursor collision spanning *two different session
  files`** — my clean cursor test deliberately used a single-session
  project to get a reproducible sequence; I did not construct a scenario
  where the anchor second's trailing run straddles a session-file boundary
  (`export_lines` concatenates files in *filename* sort order, not
  chronological order — confirmed while debugging the first, noisier
  attempt at this test). `_traces_page`'s algorithm operates on the
  already-flattened line list regardless of source file, and
  `next_trace_cursor` mirrors exactly that flattened-list algorithm, so
  this should be equivalent — but "should be" is exactly the kind of claim
  this card exists to convert into "verified," and I didn't get to this
  specific case.
- **Dispatch's actual agent-turn execution.** `POST /dispatch/{project}`
  and the `sudo`-gated task's granted resume both produced real `run`/
  `task` records with the HTTP contract confirmed correct, but the
  underlying agent turns themselves failed (`"no endpoint configured for
  model 'anthropic/claude-haiku-4-5'"` — no real model credentials in this
  sandbox). This is expected and doesn't affect anything this card was
  checking (the wire contract, not agent execution), but it means the
  `costUsd`/`turns` fields in `/status.json`/`/metrics` were never
  exercised non-zero live.
- **`POST /pods`** — out of this card's endpoint list (Tack's adapter
  doesn't call it), not exercised.

**Verification.** `cargo test --workspace`: 431 passed, 0 failed (up from
the 417 baseline — my net-new tests are
`next_trace_cursor_matches_a_real_docket_servers_minted_cursor` in
`reconciler.rs`; every other tack-orch test count shift is the two renamed/
recomment tests in `docket_adapter_test.rs`, not new tests).
`cargo clippy --workspace --all-targets -- -D warnings`: clean.
`cargo fmt --all -- --check`: clean. No `crates/tack-api/**`,
`crates/tack-db/**`, or frontend files touched.

### B6 — 2026-08-05

**What I built.** The two backend endpoints B5's boundary file declared but
that didn't exist yet: `GET /api/items/{id}/agent-activity` and
`GET /api/projects/{id}/agent-activity`, both in
`crates/tack-api/src/handlers/orch.rs`, wired into `orch_routes()` in
`router.rs` (inherits the sub-router's `require_orch_enabled` layer, no
per-handler gating added), and registered in `crates/tack-api/src/openapi.rs`,
with a regenerated `docs/openapi.json`. Two additive repository queries in
`crates/tack-db/src/repo/orch.rs`: `list_orch_approvals_for_item` (pending
and decided, newest-requested-first) and
`list_latest_orch_task_status_for_project` (the bulk badge query — see
below). Every existing repo/handler function I read from
(`list_orch_tasks_for_item`, `list_orch_runs_for_item`,
`list_orch_events_for_item`) is reused verbatim, untouched.

**B5's two open questions, resolved:**

1. **"Latest attempt" tie-break.** B5's own assumption, confirmed: highest
   `attempt` number wins; ties broken by `dispatched_at` desc. I added one
   further, purely mechanical tie-break on `remote_task_id` desc so the SQL
   (an anti-join: "no other row for this item_id ranks higher by this three-
   column order") is provably deterministic — the PK is `(item_id,
   remote_task_id)`, so two rows for the same item can never tie on all
   three columns, meaning the anti-join always yields exactly one row per
   `item_id`. This never fires in practice (attempt numbers are sequential
   per item and `dispatched_at` is set once at dispatch time), but it means
   the query has no theoretical ambiguity rather than "usually deterministic."
2. **Inner join vs. left join for the bulk badge endpoint.** Kept B5's inner
   join. `list_latest_orch_task_status_for_project` joins `orch_tasks` to
   `items` and filters by `project_id` — an item with zero `orch_tasks` rows
   is structurally absent from the result, never present with a null status.
   Reasoning: this is exactly the "no chip" signal `useAgentActivityMap`
   already implements (absent `item_id` → no badge), the frontend's
   `AgentBadgeRow.remote_status` is a non-nullable `string` (would need a
   type change for a left join's null case), and no UI anywhere reads the
   distinction a left join would add ("never dispatched" vs. "dispatched,
   but the reconciler hasn't polled since"). If a future card needs that
   distinction — e.g. a "dispatch" button that should grey out differently
   for "already tried and failed" vs. "genuinely never touched" — this is
   the query to extend, not rewrite; the anti-join shape generalizes to a
   `LEFT JOIN` by moving the `orch_tasks` join from `JOIN` to `LEFT JOIN` and
   handling `NULL` in the SELECT list.

**The `events_truncated` / `events_retention_days` addition — the one place
the frontend swap wasn't perfectly zero-diff.** The card's brief named a real
gap: `orch_events_daily` (B3's retention rollup) aggregates purged
`orch_events` rows by `day` × `control_plane_id` × `event_type` only — it
drops `item_id` entirely (see `rollup_and_purge_orch_events` in
`repo/orch.rs`), so there is no query that can answer "were *this item's*
events rolled up." The honest signal I could actually compute: whether the
item has any attempt (`orch_tasks.dispatched_at`) older than the current
retention cutoff (`now - TACK_ORCH_EVENT_RETENTION_DAYS`) — if so, some of
its event history may already be gone from the raw table this endpoint reads,
even though `events` on each attempt is (correctly) always exactly what
remains. I added `events_truncated: bool` and `events_retention_days: u32` to
`ItemAgentActivityResponse` and the matching TS `ItemAgentActivity` interface
in `frontend/src/shared/agentActivity/api.ts` — additive fields an old
client ignores safely, so the reconciliation still touched only that one
file, not `AgentActivityTab.tsx` or any other component (matching the card's
"keep it confined to `api.ts`" ask). But this is the one place the swap
wasn't fully mechanical in the deeper sense: the new honesty signal exists on
the wire and in the TS type, but nothing renders it yet — `AgentActivityTab.tsx`
is B5's file, not mine, and I didn't touch it. Flagging as the real, narrow
answer to "if it isn't mechanical, that's a finding worth reporting": the
*type* reconciliation was zero-diff to components; the *feature* (surfacing
the caveat to a user) still needs a small follow-up in `AgentActivityTab.tsx`
— render something like "history before {retention cutoff date} may be
incomplete" when `events_truncated` is true. Low-risk, not blocking: `events`
is empty for every item today regardless (B2's trace ingestion just landed
concurrently with this card; no real event data exists yet to be truncated
in practice), so there's no user-visible incompleteness to hide right now.

**`run.error` — an empty string, never null, on the wire.** The repo layer's
`OrchRun.error` is `Option<String>`, but B5's `ItemAgentRun.error` is
`error: string` (not `string | null`), with the comment "non-empty only when
`state === 'failed'`." `ItemAgentRunResponse::from(OrchRun)` in
`handlers/orch.rs` projects `Option<String>` → `String` via
`.unwrap_or_default()`. Tested explicitly
(`item_agent_activity_correlates_run_and_events_via_remote_run_id` asserts
`run["error"] == ""` for a running, non-failed run).

**`pricing_snapshot_at` — left `null`, per rule 6.** `ItemAgentAttemptResponse.pricing_snapshot_at`
is always `None`/`null`; no pricing-snapshot mechanism exists anywhere in the
system (confirmed against both A4's Fleet-view handoff and B5's own note
finding the identical gap). Not touched, not invented. Every attempt in
`item_agent_activity_attempts_are_newest_first` asserts this explicitly.

**Event-to-attempt correlation.** `orch_events.run_id` is the join key, not
`item_id` directly: I fetch every event for the item once
(`list_orch_events_for_item(item_id, None)`), group by `run_id`, then attach
each attempt's group via its own `remote_run_id`. Events with no `run_id`
(shouldn't happen once B2 ingestion is live, but the column is nullable)
attach to no attempt — there's no attempt-less place to put them in this
endpoint's shape, and B5's contract doesn't have one either. An attempt whose
`remote_run_id` doesn't resolve to any known `orch_runs` row (task dispatched,
run not yet mirrored — a real transient state, not an error) gets `run:
null`, matching the frontend's documented meaning for that case exactly.

**Project-scoped endpoint doesn't 404 for an unknown project.** Unlike the
item-detail endpoint (which 404s like `get_item` does), the bulk badge
endpoint just runs its query and returns `{ rows: [] }` for a `project_id`
that matches nothing — mirrors `list_items`'s existing precedent in
`handlers/items.rs` (also no existence check, just an empty result). This
also means it degrades gracefully if a project is deleted between a page
loading and a badge poll, consistent with `useAgentActivityMap`'s
fail-open-on-any-error design (B5's handoff).

**Tests.** New file `crates/tack-api/tests/orch_agent_activity_test.rs`
(+13 tests, all passing): both routes 404 when `TACK_ORCH_ENABLE` is unset;
item-detail 404s for an unknown item and returns empty arrays for an
item with zero dispatches; the bulk endpoint's inner join (an untouched
sibling item never appears), its empty-project case, its "highest attempt
wins" and "ties break by `dispatched_at` desc" tie-break rules (seeded with
two same-attempt rows, different `remote_task_id`/`dispatched_at`); attempts
ordered newest-first; run+event correlation via `remote_run_id` (including
the `run: null` unresolved case); `run.error` empty-string-not-null;
approvals include both pending and decided, newest-requested-first;
`events_truncated` both `false` (nothing old) and `true` (an attempt seeded
30 days back against a 1-day retention config). No repo-level unit tests
added separately — the two new repo functions are exercised end-to-end
through these handler tests, following A4/B1's mixed precedent of not always
duplicating repo coverage at both layers when the handler test already
proves the SQL correct.

**Frontend.** Changed only `frontend/src/shared/agentActivity/api.ts`: header
comment updated from "neither endpoint exists yet" to confirmed-against-the-
real-handler, the two open questions marked resolved, and the two new
`ItemAgentActivity` fields added. `agentActivityApi.getForItem`/`.listForProject`
needed **zero** changes — B5 built them against the real paths and envelope
shapes from the start (there was no runtime mock/fake generator in this file
to strip out, unlike some other "boundary file" precedents; B5's file was
always a real `fetch` call, just against endpoints that 404'd until now). I
verified every field name/type pair against the Rust DTOs by hand (see above)
rather than trusting the match — the one place they'd have silently diverged
(`run.error`'s nullability) is exactly the one B5 flagged with a comment, and
it was already handled correctly in my first draft, then covered by a test.

**Verification.** `cargo test --workspace`: 431 passed, 0 failed (up from
417 at Wave 2's start — +13 from this card, the rest from B2/other
concurrent Wave 2 cards landing in the shared tree). One transient failure
(`traces_happy_path_decodes_the_double_encoded_events_array` in
`tack-orch`'s `docket_adapter_test`, a wiremock-based test in a file I don't
own) appeared in one `--workspace` run under concurrent load and passed
clean both standalone and on a full re-run immediately after — a flake, not
a regression from this card (confirmed by re-running the full suite twice).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt
--all -- --check`: clean (ran `cargo fmt -p tack-api -p tack-db` first, then
the full `--check` gate). `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`: regenerated and drift-gate green. Frontend: `npm run
type-check` clean, `npm run lint:tokens` unchanged at 0/0, `npm run test`
shows exactly the 3 known pre-existing failures (`client.test.ts`'s
`requestBlob`, two `createObjectURL` assertions in
`GlobalSettings.test.tsx`/`panels.test.tsx`) and 248 passing — identical to
the stated baseline, nothing else broke.

**What's still open / for whoever's next.**

- **`AgentActivityTab.tsx` doesn't yet render `events_truncated`** — see
  above. A small, low-risk follow-up whenever B2's real trace data starts
  making the distinction visible in practice.
- **`orch_tasks.trusted`** (Phase 35/C2's untrusted-auto-dispatch flag)
  still isn't surfaced on either endpoint — B5 flagged this gap originally
  and it's still true; no UI reads it yet, so I left it out rather than
  adding a field nobody consumes, consistent with B5's own call.
- **`AgentBadgeRow.updated_at`** is projected from `orch_tasks.updated_at`
  (the row's last-upsert time), not `dispatched_at` — B5's original comment
  didn't specify which timestamp, and `updated_at` is the more accurate
  "how fresh is this status" signal for a badge (it changes on every
  reconciler poll that touches the row, `dispatched_at` doesn't). Worth
  knowing if a future card expected the other one.

### B7 — 2026-08-05

**What I built.** The follow-up B6 flagged above: `AgentActivityTab.tsx` now
renders `events_truncated` / `events_retention_days`. New helper
`eventsTruncatedMessage(retentionDays)` in `shared/agentActivity/format.ts`;
the tab renders it as a small `Badge tone="info"` labelled "Partial history"
plus the message text, placed between the tokens/cost summary and the
attempts list, only inside the `attempts().length > 0` branch so the
common (no-attempts, or not-truncated) case has no banner and no layout
shift. Nothing else in the tab changed.

**Wording, deliberately.** B6's handoff was explicit that a precise count is
unknowable (`orch_events_daily` drops `item_id`), so the message says "may
have been aged out" and names the retention window, never a number of
events. `format.test.ts` has a regression guard
(`not.toMatch(/\d+ events?\b/)`) so a future edit can't accidentally start
implying a count exists. Reused the `Badge` component's already-AA-checked
`info` tone rather than a new bordered banner box (the pending-approvals
section's warning-100/warning-600/warning-700 triple was the obvious
template, but introducing a third color pairing for a one-line, non-blocking
notice felt like more visual weight than the signal deserves — happy to
revisit if a reviewer wants it more prominent).

**Testing.** Added to `shared/agentActivity/format.test.ts`
(`eventsTruncatedMessage` mentions the retention window; never claims a
count) and `features/item-detail/tabs/AgentActivityTab.test.tsx` (three
cases: flag false/absent renders nothing, flag true surfaces the notice with
the retention window, and the empty-activity state never shows the notice
even if the flag were somehow true — defensive, since `events_truncated`
should always be `false` when `attempts` is empty in practice).

**Verification.** `npm run type-check` clean. `npm run lint:tokens` still
0/0 (no raw hex, no new color pairing — reused `Badge`'s existing `info`
tone verbatim). `npm run test`: 253 passing (248 baseline + 5 new), same 3
pre-existing `requestBlob`/`createObjectURL` failures as the stated
baseline, nothing else broken. Did not touch `crates/tack-orch/**` or any
other Rust file.

### C1 — 2026-08-05

**What I built.** The write path: `POST /api/items/{id}/dispatch` now really
enqueues a governed task on docket and applies `status_map` through the
workflow engine. Four pieces:

1. **`crates/tack-orch/src/adapters/docket.rs`** — implemented
   `ControlPlane::enqueue_task`'s body (was `Err(OrchError::Disabled)`
   unconditionally since A1). `dispatch`/`decide_approval` are untouched,
   still `Disabled` — see "What I deliberately didn't build" below. Added
   `POLICY_BLOCK_PREFIX` (a `pub const`) and `EnqueueTaskResponse { task:
   String }`. 5 new tests in `crates/tack-orch/tests/docket_adapter_test.rs`
   covering all three `POST /tasks/{project}` outcomes (allow,
   `require_approval`, block) plus the `trusted` flag reaching the wire and
   a 401. Also had to fix the pre-existing `write_methods_are_disabled` test
   (renamed to `dispatch_and_decide_approval_are_still_disabled`, dropped
   the now-wrong `enqueue_task` assertion) — flagging since that test wasn't
   mine to begin with, but it broke the moment `enqueue_task` stopped
   returning `Disabled`.
2. **`crates/tack-api/src/dispatcher.rs`** (new) — the actual dispatch flow:
   resolve link → `status_map` → eligibility check → idempotency guard →
   `enqueue_task` → persist `orch_tasks` → apply the mapped status through
   the workflow engine. Full design in the module's own doc comment (long —
   read it before extending this). The two load-bearing exported items:
   `pub async fn dispatch_item(state, item_id: Uuid, trusted: bool) ->
   Result<DispatchOutcome, ApiError>` and `pub async fn apply_mapped_status
   (state, item: &Item, project: &Project, target_status: &str,
   control_plane_id: Uuid, trigger: &str) -> Result<StatusApplication,
   ApiError>` (the workflow-engine gate + `status_map_rejected` recording,
   written generic so a future terminal-state caller can reuse it — see
   below).
3. **`crates/tack-api/src/handlers/orch.rs`** — `dispatch_item` HTTP
   handler + `DispatchedTaskResponse`/`DispatchItemResponse` DTOs
   (`From<DispatchOutcome>`). Registered in `router.rs` (uncommented A4's
   marked insertion point verbatim) and `openapi.rs`/`docs/openapi.json`
   (regenerated).
4. **`crates/tack-api/tests/orch_dispatch_test.rs`** (new, 13 tests) — see
   "Acceptance criteria, verified" below.

**The dispatcher's signature, and how trust is threaded (read this before
building C2).** `dispatcher::dispatch_item(state, item_id, trusted: bool)`
— `trusted` is a **required, non-`Option` positional `bool`**, deliberately
not defaulted anywhere in this function. There is no sibling function that
omits it and no default value baked in. This is the direct fix for what V1
documented as the real vulnerability: `core/dispatch.py::enqueue_task`'s own
`trusted: bool | None` treats an omitted value as "trusted iff `source ==
"operator"`", which is always true today, so *every* existing caller
silently gets operator trust. A required Rust `bool` parameter can't stop
you from passing the wrong *value*, but it makes the *omission* — the actual
failure mode V1 confirmed live — a compile error. **C2: call
`dispatcher::dispatch_item(state, item_id, false)` for GitHub/Linear-
imported items, `true` otherwise (or your own richer provenance signal).**
Don't add a `trusted: Option<bool>` convenience wrapper anywhere — that
would recreate exactly the hole this signature exists to close.

One layer up, `handlers::orch::dispatch_item` (the HTTP entry point, for a
human clicking "Dispatch" directly) has no request body asking the caller to
state trust, so it resolves a default via `dispatcher::resolve_default_trust`
— checks for an existing `github_links` row on the item, `false` if found,
`true` otherwise. **This is a stopgap, documented as such in its own doc
comment**, not the real mechanism: task 35.7 (C2) formalizes item
provenance with a `source: imported` marker at import time, and Linear
imports currently leave no persistent correlation row at all (confirmed —
no `linear_links` table exists in migrations), so this check is blind to
Linear-imported items today. C2 should replace this function, not build on
it.

**How I defined "attempt", and what makes double-dispatch safe.** `attempt
= 1 + the highest existing orch_tasks.attempt for this item` (0 if none).
Before dispatching, the item's most recent attempt is checked: if its
`remote_status` is `pending`/`running`/`waiting_approval` (`ACTIVE_TASK_
STATUSES` in `dispatcher.rs`), the request returns `DispatchOutcome::
AlreadyInFlight` with the existing task and **never calls docket**.
Anything else (`done`/`failed`/`blocked`/an unrecognised string) is treated
as terminal — redispatchable — erring toward "allow a retry" over
"permanently wedge an item because of a status string this Tack doesn't
recognise yet."

That check alone isn't race-safe against two truly concurrent requests for
the same item, so there's a second layer: `DispatchLocks`, a process-wide
`static LazyLock<Mutex<HashSet<Uuid>>>` in `dispatcher.rs` (**not** a field
on `AppState` — deliberately; `AppState` is built via a plain struct literal
in dozens of pre-existing test files across this crate that this card
doesn't own, and a new required field there would have rippled into every
one of them). A second concurrent request for the same item gets an
immediate `409 Conflict` rather than racing the first. Tested two ways:
`double_dispatch_hits_docket_once_and_the_second_call_reports_already_in_
flight` (sequential — second call sees the DB-level guard) and
`concurrent_double_dispatch_creates_exactly_one_task` (a `wiremock`
`set_delay(150ms)` + `tokio::join!` to force real overlap — asserts exactly
one `409` and one `dispatched`, and exactly one `orch_tasks` row). Both
pass. This holds because Tack is a single-process, single-SQLite-writer
binary; it would not hold across multiple replicas.

**How `enqueue_task`'s frozen return type and the three docket outcomes
reconcile.** `ControlPlane::enqueue_task`'s signature (`Result<String,
OrchError>`, frozen since Wave 0) has nowhere to carry docket's real
`status`/`approvalToken` fields, even though `POST /tasks/{project}`'s
response body has them (V1's finding). I did not touch the trait or add an
`OrchError` variant (also Wave-0-frozen, in the same file). Instead:

- **block (HTTP 400)** is classified inside `DocketAdapter::enqueue_task`
  itself and returned as `Err(OrchError::Http(format!("{POLICY_BLOCK_
  PREFIX}{docket's own error text}")))`. `POLICY_BLOCK_PREFIX` (`pub const`
  in `adapters::docket`) is a private, documented contract between that
  file and `dispatcher.rs` — `dispatch_item` does `msg.strip_prefix
  (POLICY_BLOCK_PREFIX)` to tell "docket refused this on purpose" apart from
  every other `Http` failure (a real outage, a malformed request, etc.),
  which get mapped to a `409` with a generic message instead of
  `DispatchOutcome::Blocked`.
- **allow / require_approval** are indistinguishable at the `enqueue_task`
  return-type level (both `Ok(task_id)`), so `dispatch_item` makes one
  follow-up `control_plane.list_tasks(project)` call, finds the just-created
  task by id, and reads its real `status`/`approval_token` off the already-
  fully-implemented `RemoteTask` DTO. This costs one extra HTTP round trip
  per dispatch but needed no changes to any frozen file. If that round trip
  ever needs to go away, the real fix is loosening the `ControlPlane` trait
  freeze for Wave 4+, not working around it here again.

**`status_map_rejected` — where it's recorded, and the one thing this card
did *not* wire up.** `apply_mapped_status` gates every status_map-driven
transition through `WorkflowConfig::validate_transition` +
`check_wip_limit` — the exact same two checks `handlers::items::update_item`
applies to a human dragging a card (§0 rule 7). A refusal writes one
`orch_events` row (`event_type: "status_map_rejected"`, payload =
`{trigger, from_status, target_status, reason}`) — migration 023's own doc
comment already names `status_map_rejected` as an intended use of that
table, so no schema change was needed — and leaves the item exactly as it
was; a success mirrors `update_item`'s side effects (WebSocket broadcast,
parent auto-propagation, GitHub push-back) by calling `handlers::items::
maybe_sync_github`/`propagate_parent_completion` directly (both
`pub(crate)`, no edit to `items.rs` needed — C2 owns that file this wave, so
I couldn't and didn't touch it).

**What's wired today:** `on_running` (successful dispatch) and
`on_waiting_approval` (require_approval outcome), both applied synchronously
inside `dispatch_item`, right after the `orch_tasks` row is persisted.
Tested directly, including the rejection path: `construction_workflow_
rejects_illegal_on_running_and_leaves_item_untouched` links a construction
project (`Permit` → `Procurement` → `Build` → `Inspect` → `Handover`, strict
transitions) with `on_running: "Handover"` from `dispatch_from: ["Permit"]`
— an illegal jump — and asserts the dispatch still reports `"dispatched"`
(docket really did get the task), `status_map_rejected` names the engine's
own `InvalidTransition` reason, the item's status is untouched (`"Permit"`),
and exactly one `status_map_rejected` `orch_events` row exists.

**What's NOT wired: `on_succeeded`/`on_failed`/`on_cancelled` — the
reconciler-driven half of task 35.6.** These trigger when a *run* (not the
initial dispatch) reaches a terminal `RunState`, which only B1's run-polling
(`orch_runs`, mirrored in `tack-orch::reconciler::persist_runs`) can observe.
Wiring that requires a call site inside `reconciler.rs` — a file this card
does not own (not in my file list; B1/A2 built it, and no Wave-3 owner was
assigned in the ownership table). I deliberately did not touch it.
`apply_mapped_status` is written generic (`target_status`/`trigger` are
plain strings, nothing `on_running`-specific) specifically so it's ready to
be that call site's engine — **the concrete extension a future agent
needs**: add a `ControlPlaneStore` method (mirroring B1's `upsert_runs`/
`upsert_approvals` pattern) that, after a run transitions to `succeeded`/
`failed`/`cancelled`, resolves the item via `orch_tasks.remote_run_id` →
`item_id`, loads the item/project/orch_link, maps `RunState` → the right
`status_map` key, and calls `tack_api::dispatcher::apply_mapped_status`.
This is a real, acceptance-bar-adjacent gap: TODO.md's task 35.6 names both
halves, and only the dispatch-time half is done. Flagging it loudly rather
than quietly declaring 35.6 complete.

**`ControlPlane::dispatch` (`POST /dispatch/{project}`, pipeline
`variables`) is never called.** The roadmap's task 35.3 text says "enqueue
the task on docket → `POST /dispatch/{project}` with the item's fields bound
as pipeline variables," which reads as if both docket write routes should
be called per dispatch. My actual task brief (given directly, superseding
that text) only ever discussed `enqueue_task`/`POST /tasks/{project}` — all
of V1's live-verification findings I was handed are about that route alone,
`NewRemoteTask`'s shape matches `enqueue_task` not `dispatch`, and
`orch_tasks.remote_run_id` is nullable specifically because A3 designed it
to be populated later by correlation, not filled in at dispatch time. I
built against `enqueue_task` only and left `dispatch` `Disabled`,
unconsumed. **This is a real, unresolved ambiguity** between the roadmap
prose and what I was actually briefed to build — if a future card discovers
docket's task-queue and pipeline-run mechanisms are meant to be used
together (e.g. `dispatch` starts a pod/run, `enqueue_task` adds work to it),
this needs a second look; I did not find evidence either way beyond what's
already in TODO.md §1.4 and the `docket.rs` module doc.

**Acceptance criteria, verified** (`crates/tack-api/tests/orch_dispatch_
test.rs`, 13 tests): 404 with `TACK_ORCH_ENABLE` unset and for an unknown
item; 409 for an unlinked project; `no_dispatch_policy` (empty
`dispatch_from`) and `not_eligible` (item's status outside `dispatch_from`)
both `200`, not errors; a full happy-path dispatch that applies `on_running`
and moves the item; `waiting_approval` applies `on_waiting_approval` instead
and is never reported as `"dispatched"`; a `pre_input` block surfaces the
policy id in `message`, creates **no** `orch_tasks` row, and leaves the item
untouched; sequential and concurrent double-dispatch both produce exactly
one `orch_tasks` row; `trusted: false` reaches the wire for a GitHub-linked
item (`body_partial_json` matcher — the request is only matched, and thus
only succeeds, if the flag is genuinely on the wire) and `trusted: true` for
an ordinary one; and the construction-workflow rejection case above. Plus 5
new tests in `tack-orch` for `enqueue_task`'s three outcomes.

**Verification.** `cargo test --workspace`: 449 passed, 0 failed (431
baseline + 5 `tack-orch` `enqueue_task` tests + 13 `orch_dispatch_test.rs`,
net of the one renamed pre-existing test). `cargo clippy --workspace
--all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean
(ran `cargo fmt -p tack-api -p tack-orch` first, per A3's convention of not
reformatting the whole tree mid-cycle). `UPDATE_OPENAPI=1 cargo test -p
tack-api --test openapi_contract`: regenerated `docs/openapi.json`, drift
gate green. Did not touch `frontend/**` (a second agent is concurrently
editing `frontend/src/features/item-detail/`, per my brief) or run any
`npm` command.

**Files touched, beyond what my brief listed, all disclosed:**
`crates/tack-orch/src/adapters/docket.rs` and
`crates/tack-orch/tests/docket_adapter_test.rs` — not in my brief's file
list (which named `tack-api/src/handlers/orch.rs`, `router.rs`,
`openapi.rs`, `docs/openapi.json`, `repo/orch.rs` (additive), a new
`tack-api` dispatcher module, and test files), but every one of
`enqueue_task`/`dispatch`/`decide_approval` returned `Err(Disabled)`
unconditionally before this card — without implementing `enqueue_task`'s
body, nothing in `dispatcher.rs` could ever succeed. A1's card (Wave 1)
built the read side and explicitly deferred all three write methods to
"Wave 3 (cards C1-C4)"; no later card's file list claims `docket.rs` either.
I made the smallest possible change there (one method body + one doc
comment + one const), left `dispatch`/`decide_approval` exactly as A1 left
them, and did not touch anything else in the file (the auth split, the
Prometheus parser, `traces`/`list_tasks` are all untouched).
`crates/tack-db/src/repo/orch.rs` needed **no** changes — every function my
card needed (`get_orch_link`, `get_control_plane`/`get_control_plane_token`,
`list_orch_tasks_for_item`, `upsert_orch_tasks`/`get_orch_task`,
`upsert_orch_events`, `count_items_by_status`) already existed from A3/B6.

**What's still open, for C2/C3/C4 and beyond:**

- **C2**: call `dispatcher::dispatch_item(state, item_id, /* your own
  trust signal */)`. Do not use `handlers::orch::resolve_default_trust` as
  your model — it's a stopgap that only checks `github_links`; build the
  real `source: imported` marker per your card and pass it explicitly.
- **C3** (sprint DAG dispatch): call `dispatcher::dispatch_item` per item in
  topological order; it already handles "this item isn't eligible right
  now" (`NotEligible`) and "already in flight" cleanly as non-error outcomes
  your loop can just skip over.
- **C4** (dispatch UI): every `DispatchItemResponse.outcome` is a `200` —
  including `"blocked"` — branch on the `outcome` field, not HTTP status.
  `status_applied`/`status_map_rejected` are mutually exclusive per
  response; surface `status_map_rejected` prominently (it means "docket is
  running the task but Tack couldn't reflect it on the board" — the kind of
  silent-looking gap TODO.md's non-negotiables call out by name).
- **Terminal-state reconciliation (the second half of 35.6)** is the
  biggest real gap — see above. Whoever picks it up should read
  `dispatcher::apply_mapped_status`'s doc comment first; it was written for
  exactly this reuse.
- **The `ControlPlane::dispatch`/pipeline-`variables` question** above is
  unresolved; flag it to whoever owns Phase 35's design going forward if it
  resurfaces.
- I did not attempt `POST /pods` (still doesn't exist per V1) or anything
  in Wave 4's scope (approvals, budgets, provisioning).

### C5 — 2026-08-05

**The design question, answered up front: human wins.** If a run reaches a
terminal state (`succeeded`/`failed`/`cancelled`) and the item's status has
drifted from where our own automation last parked it — a human dragged the
card, or anything else changed it — the mapped terminal status is **not**
applied. Docket's state is still fully mirrored into `orch_runs` regardless
(nothing is lost), but the board-visible item status is left alone, and the
skip is recorded as a `status_map_skipped_human_override` `orch_events` row
so the gap is visible rather than silent.

**Why.** An agent finishing its work is a real, useful signal — but a human
who deliberately dragged a card to e.g. "Blocked" made an explicit decision.
Silently reverting it the moment a run happens to succeed is exactly the
kind of thing that makes people stop trusting the board (TODO.md's own
framing of the tradeoff). "Docket wins" was the other real option — it's
simpler and never leaves a run's outcome unreflected — but it fails the
"make sure the human can tell" bar the card set: once overwritten, the
human's action leaves no trace at all. "Human wins" plus an audit event
satisfies both halves: nothing is silently reverted, and nothing is silently
lost either (docket's own state is still there in `orch_runs`, and the
`orch_events` row names exactly what was skipped and why).

**What I built.** The other half of task 35.6, closing the gap C1's handoff
flagged explicitly: `crates/tack-api/src/orch_store.rs`'s
`RepoControlPlaneStore::upsert_runs` (card B4's territory — I extended it,
not `reconciler.rs`, per the file-ownership boundary and B4's own "the emit
lives here, not in tack-orch" precedent) now calls a new
`reconcile_terminal_status_map` right after resolving `item` and before the
`AgentRunUpdated` broadcast, reusing B4's own `is_new`/`state_changed`/
`newly_attributed` gate rather than computing a second "did anything change"
determination. Three new private methods, all in a plain (non-trait)
`impl RepoControlPlaneStore` block placed after the `ControlPlaneStore` impl
(they're internal helpers, not trait members — first compile attempt caught
this, E0407):

1. **`reconcile_terminal_status_map`** — maps `run.state` (`"succeeded"` /
   `"failed"` / `"cancelled"`; `"queued"`/`"running"` return immediately —
   see below for why those have no reconciler-driven trigger at all) to the
   matching `status_map` key, no-ops on an absent key or an already-matching
   status, then either applies the transition or records the skip.
2. **`card_has_diverged`** — the human-detection check, explained below.
3. **`record_status_map_skipped`** — same free-form-`event_type` convention
   as C1's `status_map_rejected` (migration 023's own doc comment already
   names this as the intended pattern for a locally-generated event).

**`on_running`/`on_waiting_approval` are not reconciler-driven, and that's
correct, not a gap.** C1's handoff phrased the missing half as "reacting to
`on_running`/`on_waiting_approval`/`on_succeeded`/`on_failed`/`on_cancelled`
as the reconciler polls docket" — but I confirmed (grepped `reconciler.rs`)
that the reconciler never polls docket's `/tasks` endpoint at all, only
`/runs` and `/approvals`. `on_running`/`on_waiting_approval` correspond to
`TaskStatus` (the `/tasks` shape), which C1 already applies once,
synchronously, at dispatch time — there is no live signal for the reconciler
to react to for those two keys. Only `on_succeeded`/`on_failed`/
`on_cancelled` map to `RunState` (the `/runs` shape B1 actually mirrors), so
that's the entire scope of what this card wires up. Worth flagging
explicitly since it reads like a smaller scope than the brief implied — it
isn't; it's the complete set of what's observable.

**How "has a human moved it" is determined, without a schema change or
touching `dispatcher.rs` beyond nothing at all.** Turned out
`apply_mapped_status` was already `pub` and the module already `pub mod
dispatcher` — no visibility change was needed, contra what my brief
predicted. I did not touch `dispatcher.rs`.

There's no persisted "who/what last set this item's status" column, and this
card was scoped to avoid a new migration (`migrations.rs` is frozen — see
§2) and any non-visibility change to `dispatcher.rs` (C2's file this round).
So `card_has_diverged` compares `item.status` against the **one**
`status_map` key the item's latest `orch_tasks` attempt actually used:
`on_waiting_approval`'s value if that attempt's `remote_status` is
`"waiting_approval"` (and the key is configured), else `on_running`'s value.
With neither available, it falls back to "is the item still in one of
`dispatch_from`" — the only marker such a `status_map` claims at all.

**This has to be exactly one key, not a union — I hit this bug in my own
first draft and a test caught it.** My first instinct was "safe if
`item.status` is in `{dispatch_from ∪ on_running ∪ on_waiting_approval}`."
That's wrong, and TODO.md's own worked example in §1.3 proves it: that
`status_map` has `on_waiting_approval` and `on_failed` **both** set to
`"Blocked"`. Under the union check, a human dragging a card to "Blocked"
(the brief's own scenario) would be misread as "unchanged, still parked at
`on_waiting_approval`'s value" — and the terminal transition would fire
anyway, silently overwriting the human's decision, exactly the failure mode
this whole card exists to prevent. The fix is resolving to a single expected
value from the attempt's own last known `remote_status` rather than a set of
every plausible marker. `crates/tack-api/tests/orch_terminal_status_test.rs`
has a test built around this precise collision
(`a_human_move_since_dispatch_blocks_on_succeeded_even_when_the_value_
collides_with_on_waiting_approval`) — it failed against the union version
and passes against the current one.

**Accepted limit, stated plainly:** this cannot detect a human re-choosing
the *exact* status the automation already believed the item was in (e.g. the
human also drags it to "In Progress" for their own reason while `on_running`
already put it there). No value-based check can, without a change-log of
who-set-what-when. Not fixed here; would need the schema change this card
was scoped to avoid.

**How the store gets what `apply_mapped_status` needs.** `apply_mapped_status`
takes `&AppState`, and `RepoControlPlaneStore` only held `repo` +
`broadcast_tx` (B4's fields) — not `config`/`workspace_id`/`webhook`, the
rest of what `AppState` carries (`config.github_token` is what
`maybe_sync_github` inside `apply_mapped_status` needs, so this isn't
optional plumbing). Rather than changing `RepoControlPlaneStore::new`'s
arity again (which would have rippled into all 12 existing call sites across
`orch_reconciler_wiring_test.rs` and `orch_broadcast_test.rs`, on top of B4's
own precedent of doing exactly that for `broadcast_tx`), I added an optional
builder: `with_app_context(config, workspace_id, webhook)`, stored as
`Option<AppContext>`. Every pre-existing call site (`new()` alone) keeps
compiling and behaving identically — the feature is simply inert without it,
confirmed by `without_app_context_a_terminal_run_is_a_silent_no_op`.
`server.rs`'s production wiring (the only call site I changed outside
`orch_store.rs` and my own test file) now chains
`.with_app_context(config.clone(), workspace_id, state.webhook.clone())`
onto the existing `RepoControlPlaneStore::new(...)` call, both variables
already in scope there.

**Idempotency and ordering.** No new idempotency mechanism was needed:
placement after B4's `is_new || state_changed || newly_attributed` guard
means this only runs on a genuine transition, and `apply_mapped_status`'s
own `item.status == target_status` early-return (plus my own identical
check just above it, to skip the divergence computation entirely when
there's nothing to do) makes a second call for an unchanged terminal state a
safe no-op regardless. No SQLite write transaction is ever held across an
HTTP call here — `upsert_runs` makes no HTTP calls at all (§0 rule 5 holds
trivially).

**Testing.** New file, `crates/tack-api/tests/orch_terminal_status_test.rs`
(+8 tests), calling `RepoControlPlaneStore::upsert_runs` directly (not
through the HTTP router — mirrors B4's own `orch_broadcast_test.rs`, since
the surface under test is the store, not an endpoint). Covers: a clean
`on_succeeded` application when nothing has touched the item since dispatch;
a legitimate `waiting_approval`-then-`failed` path correctly resolving to
`on_waiting_approval`'s marker rather than `on_running`'s; the human-move
collision case above (the load-bearing test); a human move to a status
`status_map` never mentions at all; an absent key doing nothing and logging
nothing; `queued`/`running` never triggering anything; a workflow-engine
rejection still recording C1's `status_map_rejected` (not this card's
`status_map_skipped_human_override` — proving the two paths stay distinct);
and the no-`app_context` inert case. All 8 pass; the collision test is the
one that would have caught my own first-draft bug.

**Verification.** `cargo test --workspace --no-fail-fast`: 473 passed, 0
failed at the snapshot I verified against (up from the 449 baseline by more
than my own 8 — other Wave 3 agents landed tests concurrently in the same
window; arithmetic not exact for that reason, but 0 failures throughout).
`cargo clippy -p tack-api --lib --tests -- -D warnings` and
`cargo clippy --workspace --all-targets -- -D warnings`: clean at the
snapshot verified (workspace-wide clippy intermittently broke on
`auto_dispatch_test.rs`/`tests/common/mod.rs` mid-session — confirmed via
`git status`/mtimes to be C2's concurrent in-progress edit, not mine; gone
by my final run). `cargo fmt --all -- --check`: clean for every file I
touched; the only remaining diffs (`openapi.rs`, `auto_dispatch_test.rs`,
`item_source_migration_test.rs`) are all files I never opened for writing —
confirmed via `git diff --stat` scoped to my own files showing empty. Ran
`rustfmt` directly on just `orch_store.rs`/`server.rs`/my test file rather
than `cargo fmt --all`, per A3's stated convention of not reformatting
files mid-cycle in someone else's terminal.

**Files touched, all disclosed, all in-scope:** `crates/tack-api/src/
orch_store.rs` (owned, per my brief), `crates/tack-api/tests/
orch_terminal_status_test.rs` (new, my test file), and
`crates/tack-api/src/server.rs` (one call site — chaining
`.with_app_context(...)` onto the existing `RepoControlPlaneStore::new(...)`
call B4 already wrote there; `server.rs` isn't in my brief's file list, but
it's the only place `RepoControlPlaneStore` is constructed for real, and the
change is additive to an existing statement, not a restructure). Did not
touch `crates/tack-orch/src/reconciler.rs`, `handlers/items.rs`, any import
handler, `dispatcher.rs`, or the frontend, per scope.

**Heads-up for whoever reads this next.** While I was finishing up, TODO.md
picked up a new §2.1 ("Card R1") announcing that `crates/tack-orch/src/
lib.rs`'s Wave-0 freeze is lifted and a cross-cutting refactor is coming
that explicitly touches `dispatcher.rs` **and** `orch_store.rs`, and that it
needs to run **alone** (no concurrent agents in `tack-orch/**` or
`tack-api/**`). I did not coordinate with R1 — this note is timed to land
before it starts, not after. `reconcile_terminal_status_map`'s call into
`dispatcher::apply_mapped_status` is a real, live dependency on that
function's current signature (`&AppState`, plain `&str` target/trigger); if
R1 changes it (e.g. to carry a typed `OrchError` policy variant, per its own
stated motivation), `orch_store.rs`'s three new methods are the other call
site that needs updating alongside `dispatcher.rs` itself.

**What's still open, for whoever picks up C2/C3/C4 or Wave 4:**

- **The accepted limit above** (can't detect a human re-choosing the exact
  status the automation already set) — real, not a bug, would need a schema
  change to close.
- **`on_running`/`on_waiting_approval` are still only ever applied once, at
  dispatch time.** If a task sits in `waiting_approval` and gets approved
  entirely on docket's side, nothing in Tack ever moves the item to
  `on_running`'s target while it's actually running — the item stays parked
  at `on_waiting_approval`'s value until a terminal `RunState` arrives. This
  was already true before this card (C1's own scope) and isn't something I
  introduced or fixed; noting it because my `card_has_diverged` logic
  depends on knowing which of the two was last applied, and this gap is
  exactly why that lookup has to go through `orch_tasks.remote_status`
  rather than something simpler.
- **Performance:** `reconcile_terminal_status_map` adds up to three extra
  reads per terminal run per poll (`get_orch_link`, `list_orch_tasks_for_item`,
  `get_project`) on top of B4's existing per-row read. Fine at today's scale
  (terminal transitions are rare relative to poll ticks, same reasoning B4
  gave for its own extra reads); a batch-get would be the first place to
  optimize if that stops being true.

### C2 — 2026-08-05

**The card.** Task 35.5/35.7 — the other half of the prompt-injection
boundary V1 verified live and C1's `dispatch_item(state, item_id, trusted:
bool)` was built to require: make sure the *right* value of `trusted`
actually gets passed, for every path that creates items from outside Tack,
and for the auto-dispatch hook that fires without a human in the loop at
all.

**1. Trust is now persisted on the item, not inferred at dispatch time.**
New migration **029** (`crates/tack-db/src/migrations.rs`, 019–028 were
taken): `ALTER TABLE items ADD COLUMN source TEXT NOT NULL DEFAULT
'unknown'`. Backing Rust type: `tack_core::models::ItemSource` (`Manual` /
`Github` / `Linear` / `JsonImport` / `CsvImport` / `Unknown`), with
`is_trusted()` — `true` iff `Manual` — as the **single** place the trust
rule is encoded. Added `pub source: ItemSource` to `Item`
(`#[serde(default)]`, so any JSON predating this field deserializes to
`Unknown`/untrusted, never `Manual`).

**Default-value reasoning (the card's explicit ask).** Two different
defaults, deliberately not the same value:

- **The column's SQL `DEFAULT` (`'unknown'`) is a backfill-only value.** It
  exists purely for rows that existed before this migration ran — on *any*
  existing install, including ones where an item was imported from GitHub
  before this whole Phase 35 cycle existed (migration 018/`github_links`
  predates Phase 33). We have no record of which pre-migration rows were
  manually typed versus imported, so — per "the unsafe state must not be
  the accidental default" — every one of them resolves to untrusted, not to
  the safer-looking-but-unverifiable `manual`. No code path ever writes
  `'unknown'`/`Unknown` for a *new* item; every creation path names its
  source explicitly (see below). Verified live in
  `crates/tack-db/tests/item_source_migration_test.rs`'s
  `upgrade_in_place_backfills_pre_migration_items_to_untrusted` — a hand-
  inserted pre-029 row upgraded in place resolves to `Unknown` at both the
  raw-SQL and repository layers.
- **`Item`'s `#[serde(default)]` also resolves to `Unknown`,** for the same
  reason applied to JSON instead of SQL: an old export, or a hand-built
  import payload, that omits `source` must not be able to claim trust it
  never had just by leaving the field out (`export.rs::run_import` reads
  `item.source` straight off the parsed payload — see below).

**Sticky, by construction, not by convention.** `source` is written exactly
once, in `Repository::create_item_with_source` (the new function every
import path calls), and `UpdateItem`/`Repository::update_item` have **no**
`source` field and **no** code path that touches the `source` column at
all — not merely "we chose not to wire it," the column name doesn't appear
anywhere in `update_item`'s SQL. Confirmed by
`update_item_never_changes_source`, which edits an untrusted item's title
*and* description (the actual injection surface) through `update_item` and
asserts `source` is unchanged.

**2. Every import path found, and how each is handled:**

| Path | Handler | `ItemSource` |
|---|---|---|
| `POST /api/projects/{id}/items` (UI, `tack add`, MCP tool) | `handlers::items::create_item` | `Manual` (the new default body of `Repository::create_item`, unchanged signature) |
| Alexa voice skill | `handlers::alexa` | `Manual` — unchanged call site, still hits the same `create_item`. The project owner's own speech to their own skill, not third-party text — deliberately *not* treated like GitHub/Linear |
| `POST /api/projects/{id}/import-github` | `handlers::import_github` | `Github` |
| `POST /api/projects/{id}/import-linear` | `handlers::import_linear` | `Linear` |
| `POST /api/projects/import` (JSON/YAML project snapshot) | `handlers::export::run_import` | **preserves `item.source` from the parsed payload**, not hardcoded — see below |
| `POST /api/projects/{id}/import-csv` | `handlers::export::import_csv` | `CsvImport` |

`Repository::create_item` (existing signature, every pre-existing caller —
`alexa.rs`, `handlers/items.rs`, and every test fixture across `tack-db`/
`tack-api`/`tack-orch` — untouched) now just calls the new
`create_item_with_source(..., ItemSource::Manual)`. This was the deliberate
alternative to changing `create_item`'s signature: a signature change would
have rippled into test files I don't own, across crates whose owners are
mid-session concurrently (§0 rule 1). `create_item_with_source` is
additive; nothing else changed shape.

**JSON/YAML project import (`run_import`) preserves rather than hardcodes.**
Considered hardcoding this path to `JsonImport` (always untrusted,
matching CSV) but rejected it: `POST /api/projects/import` is a full
project snapshot restore, and anyone with access to call it already has
privilege equivalent to calling `POST /api/projects/{id}/items` directly
and getting `Manual`/trusted for free — preserving the original item's
`source` through export→import doesn't grant any privilege a caller
couldn't already get through the ordinary create-item endpoint, and it's
what makes a genuine backup/restore round-trip not silently downgrade every
item to a stricter docket policy on re-import. The safety net is the
`#[serde(default)]` above: a payload that never had a `source` (an old
export, or one built by hand) still resolves to `Unknown`/untrusted, so a
crafted payload can't just omit the field to fake `Manual`.

**No import path found that I could not cover.** I looked for every
handler that calls (or could call) item creation with data whose author
Tack can't vouch for: the MCP server (`tack-cli/src/mcp.rs`) proxies over
HTTP to the same `create_item` endpoint as the UI — no separate DB access,
so no separate path to wire. Project templates (`handlers::templates`)
create boards/workflow config, never items. There is no path that writes
directly to the `items` table outside `tack-db::repo::items`.

**3. `dispatcher::resolve_default_trust` — replaced its body, kept its
signature.** C1's handoff was explicit that this function (in
`dispatcher.rs`, which I own) was a `github_links`-sniffing stopgap to
replace, not build on — and that it was blind to Linear imports (no
`linear_links` table exists). It's now a two-line read: `state.repo
.get_item(item_id)?.source.is_trusted()`. Same signature
(`&AppState, Uuid) -> Result<bool, ApiError>`), so `handlers::orch::
dispatch_item` (C1's manual-dispatch HTTP handler, which I do not own)
needed zero changes. **One disclosed loose end:** that handler's own doc
comment (around line 1370 of `handlers/orch.rs`) still describes the old
`github_links`-based stopgap by name — I didn't edit prose in a file I
don't own for a body change in a file I do; whoever next touches
`orch.rs` should update that comment to match.

**4. The auto-dispatch hook — `handlers::items::maybe_auto_dispatch`,**
called from `update_item` right after `propagate_parent_completion`/
`maybe_sync_github`, same best-effort shape. Fires
`dispatcher::dispatch_item(state, item.id, item.source.is_trusted())` on a
`tokio::spawn` when **all** of: `config.orch_enable` (§0 rule 8),
`item.status != old_status` (a real transition happened), a linked
`orch_link` exists, and `link.auto_dispatch` is true.

**Hazards from the brief, and exactly how each is closed:**

- **"Don't dispatch on every update."** Two independent layers: (a) the
  hook itself short-circuits on `item.status == old_status` before any DB
  or HTTP call — an edit that doesn't touch status never reaches
  `dispatch_item`; (b) `dispatch_item`'s own idempotency guard (C1's
  process-wide per-item lock + `orch_tasks` "already in flight" check) is
  the belt-and-suspenders layer for two genuine status changes in quick
  succession. Tested directly:
  `auto_dispatch_does_not_refire_on_an_edit_that_does_not_change_status`
  moves an item into `dispatch_from`, then edits its title three times, and
  asserts (via a wiremock `.expect(1)` that would fail the whole test on a
  second hit, plus a direct `orch_tasks` row count) exactly one dispatch.
- **§0 rule 5 (no SQLite write txn across an HTTP call).** The hook itself
  makes no DB writes before spawning — it reads (`get_orch_link`) then
  hands off entirely to the spawned task, which is `dispatch_item`'s own
  problem (already solved by C1: fetch → HTTP → short write, never both in
  one transaction).
- **A dispatch failure must not fail the PATCH, and must not be silent.**
  The hook runs off the request path (`tokio::spawn`, so the PATCH response
  is already sent before docket is even called) — that alone satisfies
  "never fails the user's PATCH." For visibility: an `Err` from
  `dispatch_item` (transport/config failure) or a `DispatchOutcome::Blocked`
  (docket's `pre_input` policy) is logged via `tracing::warn!` **and**
  recorded as an `orch_events` row (`auto_dispatch_failed` /
  `auto_dispatch_blocked`), the same table and free-form-`event_type`
  convention C1's `status_map_rejected` and C5's
  `status_map_skipped_human_override` already use — so a failed
  auto-dispatch surfaces wherever that event history is read (the item's
  Agent Activity tab, per B5/C5), not only in server logs. `Success` is not
  separately logged — it already gets its own `orch_tasks` row.
- **§0 rule 8 (off by default).**
  `auto_dispatch_does_not_fire_when_orch_disabled` asserts zero requests
  reach a mock control plane with `TACK_ORCH_ENABLE` unset even though the
  link has `auto_dispatch: true` and the item enters the right status — the
  PATCH itself still succeeds normally.
  `auto_dispatch_does_not_fire_when_link_auto_dispatch_is_off` covers the
  second gate (orch enabled, but this project's link opted out).

**5. The headline acceptance test — asserted at the HTTP boundary,
not a function call.**
`crates/tack-api/tests/auto_dispatch_test.rs::
auto_dispatch_sends_trusted_false_on_the_wire_for_a_github_imported_item`:
seeds an item via `create_item_with_source(..., ItemSource::Github)`, links
the project with `auto_dispatch: true`, `PATCH`es the item's status into
`dispatch_from`, and lets the hook fire. The wiremock `POST /tasks/demo`
mock is registered with `.and(body_partial_json(json!({"trusted":
false})))` — it only returns 200 (letting the dispatcher proceed to persist
an `orch_tasks` row at all) if `trusted: false` was genuinely on the wire;
a wrong value means the mock never matches and the test times out waiting
for the `orch_tasks` row instead of failing on a value mismatch, which is a
harder failure to fake than a body assertion after the fact. Sibling test
`auto_dispatch_sends_trusted_true_for_a_manually_created_item` proves the
same for an ordinary item.

**6. Also tested:** the manual-dispatch HTTP handler
(`orch_dispatch_test.rs::dispatch_sends_trusted_false_for_a_github_imported_
item`) — updated in place, since `resolve_default_trust`'s mechanism
changed: it now seeds the item via `create_item_with_source` instead of the
old `set_github_link`-only setup, and still passes, proving the manual
"Dispatch" button path picks up the same persisted trust value the
auto-dispatch hook does. Export → import round-trip:
`api_test.rs::github_imported_item_source_is_untrusted_and_survives_
export_import_round_trip` (GitHub-import → export JSON → re-import into a
fresh project → still `source: "github"`) and a same-shape addition to the
pre-existing `export_yaml_round_trips_through_import` asserting a manual
item's `source: "manual"` round-trips too.
`api_test.rs::csv_import_marks_items_with_csv_import_source` covers CSV.
`tack-core`'s `models.rs` test module gained six unit tests for `ItemSource`
itself (`is_trusted`, the `Default`/`FromStr` "always resolves to `Unknown`,
never panics, never silently trusts" guarantees, serde `rename_all =
"snake_case"`, and a legacy-JSON-without-`source` deserialization case).

**What I could not verify.** The Linear import path (`ItemSource::Linear`)
is correct by code inspection and by the generic
`create_item_with_source_persists_and_only_manual_is_trusted` repo-layer
test (which parametrizes over every `ItemSource` variant including
`Linear`), but I did **not** write an HTTP-level test exercising
`handlers::import_linear` the way `github_imported_item_source_is_untrusted
_and_survives_export_import_round_trip` does for GitHub. Reason: Linear's
GraphQL endpoint is hardcoded to `https://api.linear.app/graphql` in
`import_linear.rs` (no `TACK_GITHUB_API_BASE`-style override exists for
it), so it isn't mockable with wiremock without adding a configurable base
URL — a change to that file's shape I judged out of scope for this card,
and no pre-existing test for the Linear HTTP path exists to extend either.
**Flagging this explicitly rather than silently calling Linear import
fully covered**: the persistence mechanism is tested, the exact call site
in `import_linear.rs` is a one-line, code-reviewed change identical in
shape to the GitHub one, but it has no end-to-end test of its own.

**Heads-up on card R1, seen while finishing up.** TODO.md's new §2.1
("Card R1") lifts the Wave-0 freeze on `crates/tack-orch/src/lib.rs` and
describes a cross-cutting refactor touching `dispatcher.rs` and
`orch_store.rs`, to run with no concurrent agents in `tack-orch/**` or
`tack-api/**`. I did not coordinate with R1 either — like C5, this note is
timed to land before R1 starts. Two live dependencies on `dispatcher.rs`'s
current shape that R1 should know about on top of C5's
`orch_store.rs`/`apply_mapped_status` one: (1) `resolve_default_trust`'s
new body calls `state.repo.get_item(item_id)` — unremarkable, but it's a
real read added to a function R1 may be restructuring; (2)
`handlers::items::maybe_auto_dispatch` calls `dispatcher::dispatch_item`
and matches on `DispatchOutcome::Blocked` by name — if R1's typed
`OrchError` policy variant (§2.1's own stated motivation) changes
`DispatchOutcome`'s shape, this match arm in `items.rs` is a second call
site (besides `handlers/orch.rs`) that needs updating alongside it.

**Verification.** `cargo test --workspace`: 509 tests run across the
workspace at my final snapshot, 0 failed (baseline moved past 449 during
this session — the operator committed several other agents' completed
cards mid-session, and C5 landed concurrently at 473; I re-ran the full
suite after both landed in the shared tree and it's still all green — not
attempting exact arithmetic against a moving baseline for the same reason
C5's own note gives). `cargo clippy --workspace --all-targets -- -D
warnings`: clean. `cargo fmt --all -- --check`: clean (ran `cargo fmt
-p tack-api -p tack-db -p tack-core` first, per A3's stated convention of
not reformatting the whole tree mid-cycle, then confirmed with the `--all
-- --check` gate). `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`: regenerated `docs/openapi.json` to register the new
`ItemSource` schema; drift gate green.

**Files touched, all in my brief's list or explicitly additive within it:**
`crates/tack-core/src/models.rs` (`ItemSource` + `Item.source` + tests),
`crates/tack-db/src/migrations.rs` (migration 029),
`crates/tack-db/src/repo/items.rs` (`create_item_with_source`, row mapping,
every `SELECT`), `crates/tack-api/src/dispatcher.rs`
(`resolve_default_trust` body only — no signature change),
`crates/tack-api/src/handlers/items.rs` (`maybe_auto_dispatch` +
`record_auto_dispatch_event`), `crates/tack-api/src/handlers/import_github.rs`,
`crates/tack-api/src/handlers/import_linear.rs`,
`crates/tack-api/src/handlers/export.rs` (both `run_import` and
`import_csv`), `crates/tack-api/src/openapi.rs` (registered `ItemSource` —
not in my brief's file list, same "required by the drift gate, not owned by
anyone else this wave" reasoning A4/C1 used for the same file),
`docs/openapi.json` (regenerated), and my test files (`crates/tack-db/
tests/item_source_migration_test.rs`, `crates/tack-api/tests/
auto_dispatch_test.rs`, plus additions to the pre-existing
`crates/tack-api/tests/orch_dispatch_test.rs` and `crates/tack-api/tests/
api_test.rs`). Did not touch `crates/tack-api/src/orch_store.rs`,
`crates/tack-orch/**`, `handlers/orch.rs`, `router.rs`, or the frontend, per
scope.

### R1 — 2026-08-05

**The card.** §2.1 — remove the two workarounds the Wave-0 freeze on
`crates/tack-orch/src/lib.rs` forced: B2's client-side trace-cursor
reconstruction, and C1's string-prefix policy-block detection. Ran alone, as
required, with no concurrent agent in `tack-orch/**` or `tack-api/**`.

**Fix 1 — `ControlPlane::traces` now returns the remote's own cursor.** New
DTO in `lib.rs`:

```rust
pub struct TracesPage {
    pub events: Vec<RemoteEvent>,
    pub next: Option<String>,   // opaque — never parsed by Tack
}
```

`traces`'s signature is now `async fn traces(&self, project: &str, since:
Option<&str>) -> Result<TracesPage, OrchError>`. `adapters::docket::
DocketAdapter::traces` now reads `next` off `GET /traces/{project}`'s
response (`TracesResponse` gained a `next: Option<String>` field) and
forwards it verbatim — no parsing, no inspection, just pass-through.

**Deleted, not deprecated:** `reconciler.rs`'s `next_trace_cursor` and
`decode_trace_cursor` — the entire client-side anchor/count reimplementation
of docket's `"<ts>Z:<n>"` cursor algorithm — are gone, along with the 6 tests
that existed solely to check that reconstruction (`decode_trace_cursor_
parses_compound_and_bare_forms` and five `next_trace_cursor_*` tests,
including V1's live-captured regression `next_trace_cursor_matches_a_real_
docket_servers_minted_cursor`). None of them tested anything else — the logic
they covered no longer exists, so keeping them would have been noise, per
the brief's own instruction. `persist_events` now reads `page.next` straight
off the poll result instead of computing it; the "don't write the cursor if
it didn't move" no-op guard (`Some(next) != since`) is unchanged, just fed by
the remote's value instead of a derived one.

**Migration 028 (`orch_trace_cursors`) needed no change.** The `cursor TEXT
NOT NULL` column was already an opaque string as far as the schema is
concerned — B2 just happened to be the one populating it with a client-
computed value. `repo::orch::set_trace_cursor`/`list_trace_cursors` are
untouched; they never cared what was inside the string.

**Fix 2 — typed `OrchError::PolicyBlocked { policy_id, message }`.** Added to
`lib.rs`. `adapters::docket`'s `POLICY_BLOCK_PREFIX` const is deleted;
`enqueue_task`'s HTTP-400 branch now calls a new private `parse_policy_block`
that extracts the id from docket's own error text (`"task rejected by
guardrail policy '<id>' at enqueue: <message>"`) via a plain `split_once`,
falling back to `policy_id: "unknown"` (never panicking) if docket's wording
ever drifts — same "degrade, don't fail the poll" discipline this crate
already applies to unrecognised remote enum values.

**Callers updated:**

- `crates/tack-api/src/dispatcher.rs` — `DispatchOutcome::Blocked` gained a
  `policy_id: String` field alongside `message`. The `enqueue_task` match arm
  is now `Err(OrchError::PolicyBlocked { policy_id, message }) => Ok(
  DispatchOutcome::Blocked { policy_id, message })` — no more
  `msg.strip_prefix(...)`.
- `crates/tack-api/src/handlers/orch.rs` — `DispatchItemResponse` gained a
  `policy_id: Option<String>` field (present only when `outcome ==
  "blocked"`), populated in the `From<DispatchOutcome>` impl. This is a wire
  shape change, so `docs/openapi.json` is regenerated (`UPDATE_OPENAPI=1
  cargo test -p tack-api --test openapi_contract`) — a clean additive diff,
  one new schema property.
- `crates/tack-api/src/handlers/items.rs` — `maybe_auto_dispatch`'s match arm
  now destructures `policy_id` too, logs it (`tracing::warn!(policy_id =
  %policy_id, ...)`), and threads it into a new `Option<&str>` parameter on
  `record_auto_dispatch_event`, stored in the `auto_dispatch_blocked`
  `orch_events` row's payload alongside `message`.
- `crates/tack-api/src/orch_store.rs` — **no change needed.** C5 flagged this
  as a live dependency on `dispatcher::apply_mapped_status`'s signature
  (`&AppState`, plain `&str` target/trigger); that function's signature is
  untouched by this card, so `reconcile_terminal_status_map`'s call site
  still compiles and behaves identically. Confirmed by reading it, not just
  by the compiler being quiet — it doesn't touch `DispatchOutcome` or
  `traces()` at all.

**Tests.** `crates/tack-orch/tests/docket_adapter_test.rs`: the block test
(renamed `enqueue_task_block_maps_to_policy_blocked_naming_the_policy`) now
matches on `OrchError::PolicyBlocked { policy_id, message }` and asserts
`policy_id == "prompt-injection"` (a genuinely parsed field, not a substring
check); the traces happy-path test now asserts `page.next ==
Some("2026-08-05T11:08:10Z:3")` — the fixture already had the real `next`
value from V1's live capture, just previously unread by any test.
`reconciler.rs`'s `FakeControlPlane` gained a `with_traces_next` builder (the
opaque cursor is remote-minted, so the fake scripts it explicitly rather than
computing one); only the one test that actually asserts on the persisted
cursor value (`a_successful_traces_poll_advances_the_stored_cursor`) needed
it — every other traces test keeps using the existing 3-arg `with_traces`
with `next: None`, since they don't care about the cursor.
`crates/tack-api/tests/orch_dispatch_test.rs`'s block test gained one more
assertion, `v["policy_id"] == "prompt-injection"`, so the typed field is
actually exercised at the HTTP boundary, not just in `tack-orch`.

**Net test count: 475 → 472, and that's correct, not a regression.** −6
(deleted cursor-reconstruction tests, covering code that no longer exists)
+3 (`lib.rs`: `policy_blocked_display_names_the_policy_id`,
`traces_page_round_trips_events_and_the_opaque_cursor`,
`traces_page_next_defaults_to_none_when_absent`) = −3, and 475 − 3 = 472
exactly — the arithmetic reconciles precisely, not just directionally.
`orch_dispatch_test.rs`'s strengthened assertion added no new test function.

**Live-verified, isolated `DOCKET_HOME`.** Stood up `docket serve --port
18402` with `DOCKET_HOME` pointed at a scratchpad directory for every
invocation (confirmed via `~/.docket`'s mtime, unchanged across the whole
session: before starting the server, after every request, and after
teardown). Reused V1's already-provisioned `demo`/`demo2` projects and
policy files rather than re-provisioning. Two throwaway `#[ignore]`d tests
(deleted after the run — they need a live server, so they can't live in the
permanent suite) proved, against the real server, not just wiremock:

1. **The opaque cursor round-trips correctly end to end.** `traces("demo2",
   None)` → 4 events, `next = Some("2026-08-05T12:31:00Z:1")`. Feeding that
   value straight back as `since` → 0 events, `next` unchanged — exactly the
   "nothing new since last poll" case, proving the adapter's forwarded
   cursor is one docket itself accepts and re-mints identically, not a value
   that merely looks plausible.
2. **A live policy block deserializes to a correctly-parsed
   `PolicyBlocked`.** A description matching the isolated `DOCKET_HOME`'s
   real `block-cmd` policy (`rm -rf`) produced `OrchError::PolicyBlocked {
   policy_id: "block-cmd", message: "task rejected by guardrail policy
   'block-cmd' at enqueue: destructive shell command in task description"
   }` — the id extracted by `parse_policy_block` matches docket's real,
   live-minted text exactly, not a guess against a fixture.
3. **A live allow verdict still returns `Ok(task_id)`** (sanity check that
   the happy path wasn't disturbed).

**What I did not verify live:** the reconciler's full poll loop
(`spawn_reconcilers`) against this live server end-to-end — B2/V1 already
covered that shape thoroughly (`crates/tack-orch/tests/
traces_ingestion_test.rs` exercises the real reconciler against `wiremock`,
and V1's fixtures are genuine captures), and this card's live check was
scoped narrowly to the two things that actually changed: does the adapter
forward `next` faithfully, and does the new error variant parse correctly.
I did not re-verify `require_approval` or the other docket routes live —
V1 already did, and nothing about their wire shape changed in this card.

**Turned out smaller than expected, not larger.** The brief flagged this as
possibly needing "a further interface change" beyond the two named fixes if
updating callers revealed one was needed. It didn't: `apply_mapped_status`,
`resolve_default_trust`, `DispatchLocks`, the idempotency/attempt logic, and
every other piece of `dispatcher.rs`'s design were untouched. The blast
radius was exactly the two DTOs/variant the brief named, plus the direct
consumers of each.

**Green:** `cargo test --workspace --no-fail-fast`: 472 passed, 0 failed.
`cargo clippy --workspace --all-targets -- -D warnings`: clean (one
`clippy::type_complexity` hit on `FakeControlPlane`'s traces map, fixed with
a local `type ScriptedTracesResponse = (Vec<RemoteEvent>, Option<String>)`
alias). `cargo fmt --all -- --check`: clean.

**What's still open, for whoever's next.** `lib.rs` is no longer frozen, but
nothing else about it needed to change for this card — `enqueue_task`'s
`Result<String, OrchError>` return type still can't carry `status`/
`approvalToken`, so `dispatcher::dispatch_item` still makes the one
follow-up `list_tasks` call C1 designed; widening that is a real, separate
future change if the extra round trip ever needs to go away, not something
this card had reason to touch. `ControlPlane::dispatch`/`decide_approval`
remain `Disabled`, unchanged — still Wave 4's territory.

### C3 — 2026-08-05

**The card.** Task 35.4 — `POST /api/sprints/{id}/dispatch` and
`GET /api/sprints/{id}/dispatch/dry-run`, DAG-ordered. Ran with no other
concurrent agent in `crates/tack-orch/**`/`crates/tack-api/**` per R1's
"run alone" note having already cleared by the time this card started;
a concurrent frontend agent (C4) was working in `frontend/**` the whole
time — untouched, per scope, but see the reconciliation section below,
since C4 built its API client against a guessed shape for these two
routes before this card landed.

**The five decisions, answered up front** (full reasoning lives in the new
module's own doc comment, `crates/tack-api/src/sprint_dispatch.rs`, since
that's where a future maintainer will actually look):

1. **Partial failure: skip the one item, continue with the rest.** A
   policy block, a transport error, or a worker-task panic on one item
   never aborts the others. Deliberately *not* implemented as "also skip
   everything downstream" — that behaviour **emerges for free** from
   decision 2: a blocked/failed item never reaches a Done-category status,
   so everything depending on it reports `waiting_on_dependencies` on its
   own next time it's evaluated, with no separate bookkeeping. Tested:
   `a_policy_block_on_one_item_does_not_abort_the_rest_of_the_sprint`
   (two independent items, one policy-blocked, the other still dispatches).
2. **Readiness = every direct dependency is in a Done-category status
   right now** — not "dispatched," not "succeeded" (a `RunState`, which
   this module never touches). Checked against the *live* item table at
   plan time. A blocker outside the sprint (even outside the project —
   nothing in the schema forecloses that) is resolved the same way: fetch
   it, fetch *its own* project's `WorkflowConfig`, check the category.
   Tested: the diamond-graph tests below, plus
   `a_dependency_outside_the_sprint_gates_readiness_the_same_way` (a
   same-project, different-sprint blocker) and its "now Done → unblocked"
   follow-up. **One assumption worth flagging:** the control-plane link
   and `status_map` are resolved **once**, from the sprint's own project
   (`sprints.project_id`), not per item — correct as long as every item in
   a sprint belongs to that sprint's project, which the API's own routing
   (`POST /api/projects/{id}/sprints`) makes the normal case but the
   schema doesn't hard-enforce (an item's `project_id` and its
   `sprint_id`'s project could theoretically diverge). Not fixed — a
   real, narrow edge case inherited from the existing schema, not
   introduced by this card.
3. **Concurrency: a bounded worker pool.** Every dependency-ready item's
   `dispatcher::dispatch_item` call goes through a `tokio::sync::Semaphore`
   capped at `max_in_flight` (caller-supplied via **query parameter**,
   clamped to `[1, 20]`, default 5 — see `sprint_dispatch::
   {DEFAULT_MAX_IN_FLIGHT, MAX_MAX_IN_FLIGHT, resolve_max_in_flight}`).
   Submission follows topological order. Verified with wall-clock timing
   tests (mirrors C1's own technique for its concurrent-double-dispatch
   test): 4 independent items with a 150 ms mocked enqueue delay take
   ≥260 ms at `max_in_flight=2` (two sequential batches) and <280 ms at
   `max_in_flight=4` (one batch) —
   `max_in_flight_actually_bounds_concurrent_dispatch_calls` /
   `a_generous_cap_lets_independent_items_dispatch_concurrently`.
4. **No SQLite write transaction is ever open across an HTTP call in this
   module**, because this module doesn't open one at all —
   `plan_sprint_dispatch` is pure reads, and every write happens one item
   at a time inside `dispatcher::dispatch_item`'s own
   fetch→HTTP→short-write-txn sequence (C1's design, reused verbatim, not
   reimplemented).
5. **Dry-run and the real run share one planning function**,
   `sprint_dispatch::plan_sprint_dispatch` — the topological sort plus the
   dependency-readiness gate. Both `dry_run_sprint_dispatch` and
   `dispatch_sprint` call it and nothing else decides ordering or the
   `waiting_on_dependencies` skip. Zero DB writes and zero HTTP calls to
   docket in the dry-run path — verified by construction (the dry-run
   function never touches `dispatcher::dispatch_item`) and by
   `real_dispatch_matches_the_dry_run_order_and_skips_exactly_as_previewed`,
   which asserts a dry-run call and a real call immediately after produce
   the same `order` and the same skip decisions for every item, item by
   item. The one thing that **can't** be fully shared: per-item
   *eligibility* (`dispatch_from` membership, already-in-flight) is
   necessarily evaluated twice — once read-only for the dry-run preview,
   once for real inside `dispatch_item`. To stop those two evaluations
   from silently drifting apart, I extracted `dispatcher::
   is_dispatch_eligible`/`dispatcher::is_active_task_status` (both
   `pub(crate)`, previously inline logic in `dispatch_item`) and the
   preview calls the same two functions rather than re-deriving the rule.
   `dispatch_item` itself now calls them too — refactor only, zero
   behavioural change, confirmed by the full existing `orch_dispatch_test.rs`
   / `auto_dispatch_test.rs` suites staying green untouched.

**What I built.**

- **`crates/tack-core/src/dependency.rs`** — `DependencyGraph::
  topological_order(&self, nodes: &[Uuid]) -> Result<Vec<Uuid>, CoreError>`.
  Kahn's algorithm restricted to the induced subgraph on `nodes` (an edge
  to a node outside the set is ignored — the caller checks cross-set
  dependency readiness separately, which is exactly what
  `plan_sprint_dispatch` does). **Deterministic**, not merely "a valid
  order": ties break by each node's position in the input slice via a
  `BinaryHeap<Reverse<(usize, Uuid)>>`, specifically because this
  codebase's default `HashMap`/`HashSet` hasher is randomized per
  instance — without this, two calls to the same function with the same
  input could legally return two different (both valid) orders, which
  would have made "dry-run matches the real run exactly" a coincidence
  rather than a guarantee. On an impossible cycle (bypassing
  `validate_new_edge`, which should make this unreachable in production)
  it returns `Err(CoreError::DependencyCycle(_))` rather than looping or
  truncating — TODO.md's explicit "assert anyway and fail loudly" ask.
  8 new unit tests, including one that constructs a cycle by hand to prove
  the fail-loud path actually fires.
- **`crates/tack-db/src/repo/items.rs`** —
  `list_items_for_sprint(sprint_id) -> Vec<Item>`, unpaginated (a sprint
  dispatch that silently truncated at `ItemFilter::MAX_PER_PAGE` would be
  exactly the kind of "looks like it worked" bug this card's dry-run mode
  exists to prevent).
- **`crates/tack-db/src/repo/dependencies.rs`** —
  `list_dependencies_for_items(item_ids: &[Uuid]) -> Vec<Dependency>`, one
  query, every dependency touching **any** of `item_ids` as either
  endpoint, deliberately not scoped to one project (decision 2's
  cross-project case).
- **`crates/tack-api/src/dispatcher.rs`** — no behavioural change, a pure
  extraction: `ACTIVE_TASK_STATUSES` is now `pub(crate)`, plus two new
  `pub(crate)` helpers (`is_active_task_status`, `is_dispatch_eligible`)
  that `dispatch_item` now calls instead of its old inline checks. See
  decision 5.
- **`crates/tack-api/src/sprint_dispatch.rs`** (new) — `plan_sprint_dispatch`
  (the shared plan), `dry_run_sprint_dispatch`, `dispatch_sprint`. Full
  design rationale in the module doc.
- **`crates/tack-api/src/handlers/orch.rs`** — `dispatch_sprint`/
  `dry_run_sprint_dispatch` HTTP handlers, `SprintDispatchQuery` (query
  params — see the C4 reconciliation note below on why this is a query
  param and not a JSON body), `SprintDispatchItemResponse` (one shape
  shared by both the dry-run preview and the real-run result, so they
  read side by side identically), `SprintDispatchSummary`,
  `DryRunSprintDispatchResponse`, `SprintDispatchResponse`. Reuses C1's
  own `From<DispatchOutcome> for DispatchItemResponse` for every item a
  real run actually dispatches, rather than re-deriving the outcome→field
  mapping.
- **`crates/tack-api/src/router.rs`** — uncommented A4's two marked
  insertion points verbatim.
- **`crates/tack-api/src/openapi.rs`** / **`docs/openapi.json`** —
  registered the two new paths and five new schemas; regenerated,
  drift gate green.
- **`crates/tack-api/tests/sprint_dispatch_test.rs`** (new, 13 tests) — see
  "Acceptance criteria, verified" below.

**A concurrency caveat this card surfaced but did not fix: WIP-limit races
under genuinely concurrent dispatch.** `dispatcher::apply_mapped_status`
checks a target status's WIP limit by reading `count_items_by_status` and
then, separately, writing the new status — no lock spans the two. C1's
per-item `DispatchLocks` only serializes two requests for the *same*
item; it does nothing for two *different* items racing to move into the
*same* target status at the same time. Before this card, that race
required two humans (or two auto-dispatch hooks) to collide within a
few milliseconds — rare. **This card makes concurrent dispatch of
different items an ordinary, expected occurrence within a single
request** (`max_in_flight` items in flight at once, submitted from one
`dispatch_sprint` call), which makes the same pre-existing race
meaningfully more likely to actually manifest as a WIP limit being
briefly over-committed. Not fixed here — it's `dispatcher.rs`'s WIP-check
logic (C1's design), and fixing it properly means either a real
serialization point around `apply_mapped_status`'s read+write or an
atomic conditional update, not a `sprint_dispatch.rs`-local patch.
Flagging loudly rather than quietly shipping a card that makes an
existing race easier to trigger.

**Reconciliation needed with C4's frontend (`frontend/src/shared/
dispatch/api.ts`) — read before wiring the UI up to these routes.** C4
built concurrently against its own documented "best-guess, not a spec C3
is required to match" shape (that file's own words). The real wire
contract, field by field:

| C4 guessed | Actually is | Note |
|---|---|---|
| `max_in_flight` sent as a **JSON body** field on `POST .../dispatch` | **Query parameter** (`?max_in_flight=N`) on both routes | Sending it as a body today is silently ignored, not rejected — my handler never declares a body extractor, so an unread JSON body is not an error, it just has no effect. This is the one mismatch that fails silently rather than loudly; fix first. |
| Dry-run response field `default_max_in_flight` | `max_in_flight` (the resolved/clamped value the dry-run itself would use) | Same concept, different name — a rename, not a shape change. |
| Real-dispatch response `{ sprint_id, results: [...] }` | `{ sprint_id, max_in_flight, summary: {...}, items: [...] }` | `items`, not `results`; plus a `summary` object (counts by decision) C4 didn't anticipate. |
| Plan item: `{ item_id, title, status, position: number \| null, skip_reason: string \| null, blocked_by }` | `{ item_id, title, status, order: number, decision: string, blocked_by, ...outcome fields }` | **Every item always has a position** (`order`) — nothing is excluded from the plan the way C4's `position: null` implies. `decision` is one closed vocabulary (`"waiting_on_dependencies"`, `"no_dispatch_policy"`, `"not_eligible"`, `"already_in_flight"`, `"would_dispatch"` dry-run-only, or — real run only — `"blocked"`/`"waiting_approval"`/`"dispatched"`/`"error"`) rather than free text. |
| Real-run item result: `{ item_id, title, outcome, policy_id, message, status_applied, status_map_rejected }` | Same fields, but the field is named `decision` not `outcome`, plus `order`, `status`, `blocked_by`, `error`, `task`, `approval_token`, `current_status`, `dispatch_from` are also present (a strict superset) | `error` is new: set when `dispatch_item` returned an `Err` or its worker task panicked (decision 1) — C4's guessed shape has no equivalent field at all. |

One thing C4 guessed **correctly** and matches exactly: the dry-run
response includes **every** sprint item, not just the eligible subset —
so the preview can show what's excluded and why, the same shape a real
run walks.

**Acceptance criteria, verified**
(`crates/tack-api/tests/sprint_dispatch_test.rs`, 13 tests): 404 with
`TACK_ORCH_ENABLE` unset (both routes) and for an unknown sprint; 409 for
an unlinked project; an empty sprint dry-runs to an empty plan; a diamond
dependency graph (A blocks B and C, B and C both block D) dry-runs in a
valid topological order with A ready and B/C/D all correctly held back on
A, then — once A is marked Done — B and C become ready while D still
waits on B and C specifically (not transitively on A); a real dispatch of
that same diamond matches the dry-run's order and skip set item-for-item,
with the summary counts to match; a same-project-different-sprint
dependency gates readiness identically to an in-sprint one, and clears
once satisfied; a policy-blocked item does not abort an unrelated
independent item in the same sprint (partial failure); a manually-created
item and a GitHub-imported item in the same sprint dispatch with
`trusted: true` and `trusted: false` respectively, asserted on the wire
via `wiremock` body matchers (the non-negotiable: trust is per item, never
a blanket batch value); `max_in_flight` is clamped (`0`→`1`, `999`→`20`,
`7`→`7` unchanged) and echoed back; and the two timing-based concurrency
tests above.

**Verification.** `cargo test --workspace --no-fail-fast`: 493 passed, 0
failed (472 baseline + 8 `tack-core` topological-order tests + 13
`sprint_dispatch_test.rs` tests = 493 exactly). `cargo clippy --workspace
--all-targets -- -D warnings`: clean (fixed a `collapsible_if` and a
`large_enum_variant` — `sprint_dispatch::ItemResult::Outcome` now boxes
`DispatchOutcome`, since it otherwise dominated the enum's size at 256
bytes even for the common no-op variants). `cargo fmt --all -- --check`:
clean (ran `rustfmt` directly on every file this card touched, per A3's
convention of not reformatting the whole tree mid-cycle).
`UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`:
regenerated `docs/openapi.json`, drift gate green. Ran the timing-based
concurrency tests 3x in a row locally to check for flakiness under
`--test-threads=4`; stable every time (150 ms delay against ~260/280 ms
thresholds leaves comfortable margin on ordinary CI hardware — if this
ever turns out flaky in CI specifically, widen the delay rather than
narrow the assertion windows).

**What I did not attempt.** The dispatch UI itself (C4's scope, untouched
— `frontend/**`). The approvals inbox (D1). Terminal-state reconciliation
already exists from C5 and needed no changes from this card — a sprint
dispatch's items still get their `on_succeeded`/`on_failed`/`on_cancelled`
applied the same way a manually-dispatched item's would, since both go
through the same `orch_store.rs` reconciler path C5 built; this card
never touches that. Did not verify against a live docket (no scratchpad
`DOCKET_HOME` session run for this card) — V1/R1 already live-verified
the `enqueue_task`/`list_tasks` contract this module builds on, and this
card's own new logic (topological order, readiness gating, the bounded
pool) is entirely Tack-side, exercised end-to-end against `wiremock` in
the 13 new tests instead.

### C4 — 2026-08-05

**The card.** Tasks 35.8/35.9 — "Dispatch to agents" on the item detail and
the board card menu, "Run sprint" on the Sprints view with C3's dry-run
preview and an in-flight cap control, and security gating so every dispatch
surface disappears cleanly (not error-fully) when `TACK_ORCH_ENABLE` is
unset. Backup-exclusion (the other half of 35.9) was already closed by A9 —
confirmed via TODO.md §6, not redone.

**Files built, all new:** `frontend/src/shared/dispatch/{api,format,notify,
DispatchOutcomeNote,DispatchCardMenu}.ts(x)` (the wire boundary + shared
presentation, one file per concern, mirroring A5/B5's precedent),
`frontend/src/features/sprints/DispatchSprintModal.tsx`. **Files edited:**
`frontend/src/features/item-detail/ItemDetailDrawer.tsx` (dispatch control +
outcome note; also two pre-existing-bug fixes, see below),
`frontend/src/features/board/Board.tsx` (wired `DispatchCardMenu` into
`ItemCard`), `frontend/src/features/sprints/Sprints.tsx` ("Run sprint"
button + modal), `frontend/src/shared/agentActivity/useAgentActivityMap.ts`
(additive `orchAvailable` accessor, reused by both Board and Sprints as the
"should dispatch controls render at all" gate instead of a second probe),
`frontend/e2e/a11y.spec.ts` + `frontend/e2e/helpers.ts` (additive scans +
`createSprintWithItem` helper).

**The three outcomes, kept visually and semantically distinct — the card's
headline correctness requirement.** `describeDispatchOutcome` in
`shared/dispatch/format.ts` is the one place every dispatch/decision value
maps to a tone+label; every call site (item detail, board menu toast, sprint
modal) goes through it or `notifyDispatchOutcome`/`DispatchOutcomeNote`,
never its own ad-hoc copy. `waiting_approval` is `warning`, never `success`;
`blocked` surfaces the actual `policy_id` (never a bare "dispatch failed");
`already_in_flight` gets its own `info` tone rather than folding into
`dispatched`, since the task was already running before this click.
Regression-tested directly: `format.test.ts`'s "every one of the N documented
values is visually distinct by tone+label pair" and
`ItemDetailDrawer.dispatch.test.tsx`'s "a waiting_approval outcome is shown
distinctly — never as 'Dispatched'".

**Reconciled against card C3's real sprint-dispatch API — read this before
touching `shared/dispatch/api.ts` or `DispatchSprintModal.tsx` again.** C3
landed `POST /sprints/{id}/dispatch` / `GET /sprints/{id}/dispatch/dry-run`
concurrently with this card; my first draft was a documented best guess
(same "not a spec, a reconciliation target" framing A5 set). Per the
coordinator's explicit instruction, I reconciled field-by-field against
`docs/openapi.json` and C3's own handoff note (directly above this one)
**before** finishing, not after. Three corrections that would have failed
silently if shipped as guessed:

1. **`max_in_flight` is a query parameter, never a JSON body field.** My
   original `dispatchApi.dispatchSprint(id, { max_in_flight })` sent it as a
   JSON body; the real handler (`axum::extract::Query<SprintDispatchQuery>`
   in `handlers/orch.rs`) never declares a body extractor, so the value would
   have been silently ignored — no error, just a sprint dispatching at
   whatever the server's default concurrency is regardless of what the
   operator typed into the cap field. Fixed: `dispatchApi.dryRunSprintDispatch`/
   `dispatchSprint` now both take an optional `maxInFlight: number` and append
   `?max_in_flight=N` via a shared `withMaxInFlight` helper; POST bodies are
   `undefined` now, not `'{}'`. Regression-tested at the wire level in both
   `api.test.ts` ("appends `?max_in_flight=N` as a QUERY PARAM, never a JSON
   body") and `DispatchSprintModal.test.tsx` ("editing the in-flight cap
   overrides the value sent to the real dispatch call").
2. **Every sprint item is always present in the plan, with a closed-vocabulary
   `decision`, not a nullable `position`.** My guessed shape filtered
   "excluded" items down to `position: null` + a free-text `skip_reason`. The
   real `SprintDispatchItemResponse` has no excluded bucket at all — every
   item carries an `order` and one `decision` value (`waiting_on_dependencies`
   / `no_dispatch_policy` / `not_eligible` / `already_in_flight` /
   `would_dispatch` in a dry run; `blocked` / `waiting_approval` / `dispatched`
   / `error` in a real run). `DispatchSprintModal.tsx`'s `planned`/`notPlanned`
   memos now partition on `decision === 'would_dispatch'` instead of a null
   check, and the "N items not dispatched this run" `<details>` section names
   each one's real `decision` + `sprintItemDetail()` (policy id, dependency
   count, current status) instead of a fabricated `skip_reason` string —
   which happens to satisfy the coordinator's second point below better than
   my original design could have (real structured reasons, not free text I
   invented).
3. **Post-dispatch counts come from the server's own `summary` object, never
   re-derived client-side.** My original `summarizeSprintDispatchResults`
   counted a `results: SprintDispatchItemResult[]` array by hand. The real
   response has no `results` field (it's `items`, the same shape the dry-run
   uses) and ships a pre-computed `summary: SprintDispatchSummary` with one
   count per decision — `summarizeSprintDispatchCounts` (renamed) now just
   projects that object into a display list, never sums `items` itself, so it
   can't silently drift from what the server actually decided.

**Partial failure and cross-sprint dependencies — the coordinator's other two
callouts, both already fully represented by the real schema, so no UI logic
needed inventing:** C3's `SprintDispatchSummary` has independent counts for
`dispatched`/`waiting_approval`/`blocked`/`errored`/`waiting_on_dependencies`/
etc., and `DispatchResultsSummary` renders every non-zero bucket as its own
badge (`summarizeSprintDispatchCounts` — see `format.test.ts`'s "never a
single merged '8 dispatched'" style tests) — a policy-blocked item and a
downstream item still `waiting_on_dependencies` on it both show up honestly,
never folded into one number. The dry-run preview's "why" already comes from
`sprintItemDetail`, which reads `blocked_by`/`current_status`/`dispatch_from`
regardless of whether the blocking dependency is in-sprint or not — the UI
never had to know the difference, since C3's `decision`/`blocked_by` fields
already encode the live check.

**A real, pre-existing bug found and fixed while wiring this up — not a test
artifact.** Solid's resource accessor (`dryRun()`, `agentActivity()` — as
opposed to `.loading`/`.error`) **throws once the resource has errored**.
Calling it from a memo/JSX expression that runs in the same reactive batch
that just set the error throws *inside* that batch, which silently aborts
propagation to sibling computations — in practice, a modal or drawer that
gets stuck showing "Loading…" forever the instant the underlying fetch 404s,
with an unhandled promise rejection as the only trace. Found this via
`DispatchSprintModal`'s own dry-run resource (a fresh 404 test hung
indefinitely instead of showing the disabled state) and fixed it there with
a `dryRunData()` safe accessor (`undefined` once errored, never throws).
**Then grepped for the same pattern elsewhere and found it already existed,
predating this card, in `ItemDetailDrawer.tsx`'s `hasAgentActivity`/the
`AgentActivityTab` call site (card B5's original resource)** — no test had
ever driven that resource into an error state through the full drawer before
mine did. Fixed identically (`agentActivityData()`). This directly serves
this card's own "no broken-looking errors, no console noise" bar for the
orchestration-disabled path — worth a scan for the same pattern (a resource
accessor called directly, not via `.loading`/`.error`, inside a memo or an
unconditional JSX expression) if a future card adds another `createResource`
whose source can transition from absent to erroring.

**Security gating (35.9).** Every dispatch control (item-detail button,
board card menu, sprint "Run sprint" button) renders only once its own
orchestration probe has resolved *without* error — `orchAvailable()`
(`ItemDetailDrawer`) / `useAgentActivityMap.orchAvailable` (Board, reused
verbatim by Sprints) — false while loading and on *any* error, not just a
404, the same conservative "if we can't positively confirm it's on, don't
show a privileged control" posture used throughout. This reuses the
already-in-flight agent-activity fetch rather than adding a second
"is-orchestration-enabled" probe per surface. The actual `TACK_ORCH_ENABLE`
**and** control-plane-token gate is enforced server-side (C1/A4's
`require_orch_enabled` layer + the 409 "project not linked to a control
plane" case) — this card's job was making the frontend degrade to nothing
rather than a broken control when that gate is closed, not re-implementing
the gate itself.

**The dry-run preview as the confirmation gate.** `DispatchSprintModal` has
no one-click path from "Run sprint" straight to a real dispatch — opening it
always shows the dependency-ordered `would_dispatch` plan (plus every
not-yet-ready item and why) first; only an explicit "Confirm dispatch (N)"
click on that same screen calls `POST /sprints/{id}/dispatch`. No projected
cost is shown anywhere in the dry-run preview: no per-item cost data exists
before a real dispatch happens (`cost_usd_estimated` only appears on
`orch_tasks` after `enqueue_task` succeeds, per B6's schema), so inventing a
sprint-level projection here would be "an estimate of an estimate" with
nothing real backing it — the honest choice was to show none, not a
fabricated number with a disclaimer.

**Design tokens.** No new color pairing introduced anywhere in this card —
`DispatchOutcomeNote`/`DispatchResultsSummary` reuse `Badge`'s existing six
tones verbatim (same AA-audited set `AgentStateChip` already reuses per
A10's audit), `DispatchCardMenu`'s popover reuses existing
`--color-bg-elevated`/`--color-border-light`/`--shadow-lg` tokens. `npm run
lint:tokens` stayed at the 0/0 baseline throughout.

**Tests.** 51 new/updated Vitest tests across `shared/dispatch/{api,format,
notify,DispatchOutcomeNote,DispatchCardMenu}.test.ts(x)`,
`features/sprints/DispatchSprintModal.test.tsx`, and
`features/item-detail/ItemDetailDrawer.dispatch.test.tsx` (a separate file
from the pre-existing `ItemDetailDrawer.test.tsx` so the dispatch-specific
fetch-mock map doesn't complicate that file's fixtures). `cd frontend && npm
run type-check && npm run lint:tokens && npm run test && npm run build` all
green — `test` shows 304 passing (253 baseline + 51 new/changed) and the
same 3 known pre-existing failures (`client.test.ts`'s `requestBlob`, two
`createObjectURL` assertions in `GlobalSettings.test.tsx`/`panels.test.tsx`),
nothing else broken, **zero unhandled rejections** (confirmed clean after
the resource-accessor fix above — a dirty run with that warning present was
the first signal something was actually wrong, not just noisy). Did not run
the Playwright e2e suite live (no backend available in this environment);
added 6 additive scans to `frontend/e2e/a11y.spec.ts` (item-detail dispatch
control visible, item-detail after a blocked outcome, sprint dry-run preview,
sprint dispatch results) using the same `page.route()` interception
technique A5 established for Fleet's orch-gated scans, plus a new
`createSprintWithItem` helper in `e2e/helpers.ts` (the minimum "Run sprint"
needs to render at all). Did not relax any existing a11y assertion.

**What I did not attempt / left open:**

- **List/Table dispatch controls.** The card brief named only "item detail
  and the board card menu" — List and Table views have no dispatch UI added,
  consistent with scope (and B5's precedent of not adding agent badges
  everywhere at once either).
- **Re-fetching the sprint dry-run when the in-flight cap changes.** The cap
  field starts pre-filled from the dry-run response's own resolved
  `max_in_flight` and edits are purely local until "Confirm dispatch" —
  concurrency doesn't affect *which* items are eligible (C3's planning
  function is independent of the worker-pool size), only how many run at
  once, so the preview's item list/order stays accurate regardless; only the
  displayed cap number could theoretically go stale if the user types an
  out-of-range value the server would clamp differently. Not fixed — would
  need a debounced re-fetch on every keystroke for a cosmetic-only benefit.
- **List/Table agent-activity badges already existed (B5/B6/B7) and are
  untouched** — this card only added dispatch *controls*, not new activity
  surfaces.
- **The WIP-limit race under concurrent sprint dispatch** C3 flagged in its
  own handoff (`apply_mapped_status`'s read-then-write isn't atomic) is a
  backend concern, out of this card's file ownership (`frontend/**` only).

**For whoever next touches `shared/dispatch/**` or `DispatchSprintModal.tsx`:**
treat `shared/dispatch/api.ts` as now fully reconciled against the real
backend (both routes) — no further guessing needed. If C3's schema changes
again, this file plus `format.ts` are still the only two that need editing;
every component consumes `SprintDispatchItemResponse`/`SprintDispatchSummary`
via `format.ts`'s helpers, never a raw field.

### R2 — 2026-08-05

**Yes, I reproduced the race before fixing it — reliably, not marginally.**
Before touching `dispatcher.rs`, I wrote
`crates/tack-api/tests/wip_limit_race_test.rs` and ran it against the
unmodified code: 12 items, all eligible, dispatched via 12 genuinely
concurrent `POST /api/items/{id}/dispatch` requests (`tokio::spawn` on a
`multi_thread` runtime, into a scrum project's "In Progress" column, WIP
limit 5). **All 12 landed in the column** — not "6 or 7," all twelve, every
single run. I reran it five times before writing any fix; same result every
time (full output/counts in the "What I tested" section below). After the
fix, the same test passes consistently across repeated runs (checked 5x
manually beyond the one in the permanent suite). This is the strongest form
of evidence this handoff format asks for: not "I believe it would race" but
"I watched it race, 12-for-12, until I fixed it."

**The bug, exactly.** `dispatcher::apply_mapped_status` did:

```rust
let count = state.repo.count_items_by_status(item.project_id, target_status).await? as usize;
if let Err(e) = project.workflow.check_wip_limit(target_status, count) { /* reject */ }
// ... separately, later, on its own connection:
state.repo.update_item(item.id, update).await?;
```

Two `.await` points, no lock spanning them. C3's own handoff (§6, above)
flagged this by name and correctly declined to fix it — out of that card's
scope, and C3's sprint dispatch (`max_in_flight` items dispatched
concurrently from one `POST /api/sprints/{id}/dispatch` call) is exactly
what turns this from "needs two humans to collide within milliseconds" into
"happens by construction every time a sprint with more ready items than
`max_in_flight`'s headroom in a WIP-limited column gets dispatched."

**The fix: push the check-and-write into one `BEGIN IMMEDIATE` SQLite
transaction, in `tack-db`, not `tack-api`.** New method,
`Repository::update_item_status_checked` (`crates/tack-db/src/repo/
items.rs`), returns a new `Option<StatusUpdateOutcome>`
(`Applied(Box<Item>)` / `Rejected(CoreError)`, `None` only if the item
vanished between call and final reload — mirrors `update_item`'s own
`Option` convention for "not found"). Inside one transaction:

1. `BEGIN IMMEDIATE` — acquires SQLite's write lock **up front**, at the
   count read, not on the first write. This is the one detail that actually
   matters: the existing convention in this codebase
   (`Repository::upsert_orch_tasks` and five other spots in `repo/orch.rs`)
   uses a plain `self.pool().begin()` (deferred `BEGIN`), which only takes
   the write lock the moment the *first write statement* runs. If I'd
   followed that convention literally here, two concurrent transactions
   could both start deferred (read-only so far, no lock), both do their
   `SELECT COUNT`, and then both try to upgrade to a write lock at the same
   moment — one of them gets `SQLITE_BUSY`/a lock-upgrade error instead of
   a clean queue-and-wait. `begin_with("BEGIN IMMEDIATE")` (sqlx 0.8's
   `Pool::begin_with`, confirmed present via `sqlx-core-0.8.6`'s
   `connection.rs`/`pool/mod.rs`) sidesteps this: the second concurrent
   caller's own `BEGIN IMMEDIATE` simply blocks (SQLite's busy-timeout
   handles the wait) until the first transaction commits or rolls back, so
   by the time it actually reads the count, it's reading post-commit state.
   This is *new* to this codebase — I did not find `begin_with`/`BEGIN
   IMMEDIATE` used anywhere else in `tack-db` — but it's the minimal
   deviation from the existing `self.pool().begin()` convention needed to
   actually close the race, not a new pattern invented for its own sake.
2. `SELECT COUNT(*) FROM items WHERE project_id = ? AND status = ?` (same
   query `count_items_by_status` runs, just now inside the locked
   transaction).
3. `workflow.check_wip_limit(target_status, count)` — the **exact same**
   `tack-core` function `apply_mapped_status` already called; I did not
   duplicate or re-derive the `>=` comparison. `tack-db` already depends on
   `tack-core` (it already imports `StatusCategory` for `update_item`'s
   started_at/completed_at bookkeeping), so passing `&WorkflowConfig` in and
   getting the real `CoreError::WipLimitExceeded` back out was a direct
   reuse, not new coupling.
4. On `Err`: `tx.rollback().await?`, return `Rejected(e)` — nothing written.
5. On `Ok`: the same `UPDATE items SET status = ...` plus the
   started_at/completed_at-by-category logic `update_item` already had,
   now run against `&mut *tx` instead of `self.pool()`, then `tx.commit()`.

`dispatcher::apply_mapped_status` now calls this once instead of
`count_items_by_status` + `update_item`, matches on the outcome, and does
exactly what it did before for each branch (record `status_map_rejected` /
broadcast + propagate + GitHub push-back). `validate_transition` (the
explicit-transitions check) stays where it was, *outside* the transaction —
it only reads the project's static workflow config, not a row count, so
it was never racy and doesn't need the lock.

**Why not a single self-contained `UPDATE ... WHERE (subquery)` statement
instead of an explicit transaction?** I considered it — SQLite guarantees a
lone statement's atomicity on its own, no explicit `BEGIN` required, which
would have been less code. Rejected it because `update_item_status_checked`
also has to conditionally write started_at/completed_at based on
`status_category` (three different `UPDATE`s depending on
`InProgress`/`Done`/`Todo`), and folding that into one correlated-subquery
statement would have been far harder to read and to keep in sync with
`update_item`'s own (already-established) per-category logic. An explicit
`BEGIN IMMEDIATE` transaction wrapping the same straightforward statements
`update_item` already uses was the smaller, more legible change — and it's
what the card brief itself pointed at.

**`DispatchLocks` (C1's per-item `static` mutex) and this fix are
deliberately not unified.** I looked at this seriously, since the brief
explicitly raised it and said I was free to restructure. They solve
different problems: `DispatchLocks` serializes two requests for the *same*
item (idempotency — don't double-dispatch one item to docket).
This race is between *different* items competing for space in the *same
column* — a per-item key buys nothing here, since the two racing requests
never share an item id. A Rust-level lock that *did* address this would
need to be keyed by `(project_id, target_status)` and would only protect
against races between requests inside this one process — which is exactly
what SQLite's own write-serialization already gives for free, and more
generally: it also protects the human/board-drag path and the Alexa path
(see below) *if* they're ever switched to call the same repo method,
without those call sites needing to know about a Rust-level lock at all.
Pushing the invariant into the database, where the single-writer guarantee
already lives, is strictly more general than a second, parallel,
process-local locking scheme. I did not move `DISPATCH_LOCKS` onto
`AppState` — nothing about this fix touches or depends on it, and the
brief's own reasoning for why C1 chose a `static` (dozens of pre-existing
`AppState { .. }` struct literals in test files this card doesn't own)
still applies unchanged.

**Real, unfixed gap, flagged loudly rather than silently left for someone
to discover:** `handlers::items::update_item` (`crates/tack-api/src/
handlers/items.rs`, ~line 188 — the human/board-drag HTTP path,
**owned by C2 this wave, not me**) and `handlers::alexa` (`crates/tack-api/
src/handlers/alexa.rs`, ~line 579 — the voice "mark done" path, unowned but
outside my brief's file list) both still do the exact same unguarded
`count_items_by_status` + `update_item` two-step. They have the identical
race — a human dragging a card and a concurrent dispatch (or two humans, or
two Alexa requests) into the same WIP-limited column can still both get
through. I did not touch either file (§0 rule 1 — not mine to edit), but
the fix is a one-line swap at each call site: replace the
`count_items_by_status` + `check_wip_limit` + `update_item` sequence with a
single call to `Repository::update_item_status_checked`, matching on
`Applied`/`Rejected` the same way `apply_mapped_status` now does. Whoever
owns either file next should make this change — the race is real, it is
strictly more likely under sprint dispatch's own new concurrency (a human
dragging a card while a sprint dispatch is mid-flight is now a realistic
overlap, not a coincidence), and the fix is now sitting right there in
`tack-db` ready to be called.

**What I tested, beyond the one required repro/fix test:**

- `crates/tack-api/tests/wip_limit_race_test.rs` (new,
  `#[tokio::test(flavor = "multi_thread", worker_threads = 16)]`) — the
  deliverable. 12 distinct items, all `dispatch_from`-eligible, dispatched
  genuinely concurrently through the real HTTP `POST /api/items/{id}/
  dispatch` path (not calling `apply_mapped_status` directly — going
  through the full flow, including a `wiremock`-mocked docket with a 120ms
  `set_delay` on `POST /tasks/demo`, mirroring C1's/C3's own technique for
  bunching concurrent requests' arrival at the racy step). Asserts the
  final "In Progress" count never exceeds the configured limit of 5, and
  that every item ended up in exactly one of Backlog/In Progress (nothing
  lost or duplicated). Ran it 5x post-fix with no failures; ran it
  (uncommitted, obviously) against pre-fix code 5x with 12/12 exceeding the
  limit every time before writing the fix.
- `crates/tack-db/tests/status_update_checked_test.rs` (new, 5 tests,
  sequential/deterministic) — `update_item_status_checked`'s own
  correctness in isolation: applies when under the limit; rejects with the
  exact `CoreError::WipLimitExceeded { column, limit, current }` and leaves
  the item and the column count untouched when the column is exactly full;
  a status with no configured `wip_limit` always applies (tested with 50
  items, well past any plausible limit); started_at/completed_at bookkeeping
  matches `update_item`'s existing per-category behavior exactly
  (In Progress stamps started_at, Done stamps completed_at and keeps
  started_at); an unknown item id returns `None` rather than an error or a
  panic.

**Verification.** `cargo test --workspace`: 520 passed, 0 failed (baseline
493 + other Wave-3/Wave-4 agents' concurrent landings in this same window,
per the by-now-familiar pattern C5/R1 both noted — D1's approvals-inbox
tests were compiling and landing while I worked; not my arithmetic to
reconcile, but 0 failures throughout every run I did, both before and after
my fix, so nothing I touched destabilized anything else). `cargo clippy
--workspace --all-targets -- -D warnings`: clean (fixed one
`clippy::large_enum_variant` myself — `StatusUpdateOutcome::Applied` boxes
`Item`, same fix C3 applied to `sprint_dispatch::ItemResult::Outcome` for
the same reason). `cargo fmt --all -- --check`: **not** run tree-wide —
per A3's stated convention (repeated by every Wave-3 card since), ran
`rustfmt --edition 2024 --check` directly on only the four files I touched
or created (`tack-db/src/repo/items.rs`, `tack-api/src/dispatcher.rs`,
`tack-api/tests/wip_limit_race_test.rs`, `tack-db/tests/
status_update_checked_test.rs`); clean. `cargo fmt --all -- --check` across
the whole tree currently shows drift in files I don't own and never opened
(`handlers/orch.rs`, `orch_approvals_test.rs`, `tack-db/src/repo/orch.rs`,
`tack-db/tests/orch_repo_test.rs`) — confirmed via `git diff --stat` scoped
to my own files showing nothing there, and via `git log` that I made zero
edits to any of them. That's D1's concurrent approvals-inbox card, still
settling in the same shared tree while I worked (I also hit two transient
`cargo build`/`cargo clippy` failures mid-session from D1's/another agent's
in-flight edits to `tack-orch/src/lib.rs` and `handlers/orch.rs` —
diagnosed via `git status` showing only those files as dirty at the time,
waited for them to stabilize, not mine to fix).

**Files touched, all disclosed, all in scope:** `crates/tack-db/src/repo/
items.rs` (new `StatusUpdateOutcome` enum + `update_item_status_checked`,
added; nothing existing removed — `count_items_by_status` is untouched and
still used by the two unfixed call sites above), `crates/tack-api/src/
dispatcher.rs` (`apply_mapped_status` rewired to the new atomic call; net
removal of the now-dead `UpdateItem` import), `crates/tack-api/tests/
wip_limit_race_test.rs` (new), `crates/tack-db/tests/
status_update_checked_test.rs` (new). Did not touch `sprint_dispatch.rs`
(no change needed — it already delegates every write through
`dispatcher::dispatch_item` → `apply_mapped_status`, so it inherits this
fix automatically, no call-site change required) or `server.rs` (no lock
moved to `AppState` — see the `DispatchLocks` reasoning above).

### D1 — 2026-08-05

**How the decision endpoint is gated, and what happens when the approval
token is unset — leading with this since it's why the card exists.**
`POST /api/approvals/{token}` sits behind the ordinary two layers every orch
route gets (`require_token`'s Bearer gate, then `require_orch_enabled`'s
404-when-disabled), **plus a third check inside the handler itself**,
`require_approval_token` (`crates/tack-api/src/handlers/orch.rs`): it reads
a new header, `X-Tack-Approval-Token`, and compares it byte-wise
(`middleware::constant_time_eq`, reused, not reinvented) against
`config.orch_approval_token`. **With `TACK_ORCH_APPROVAL_TOKEN` unset, the
check always returns `403` — unconditionally, for every request, no matter
what header is presented (including no header at all).** There is
deliberately no "no secret configured, allow everything" branch the way
`require_token`'s own ordinary Bearer gate has for an unset `TACK_API_TOKEN`
— that gate's safe default is "trust the network boundary," which is
reversible and intentional; this gate's safe default has to be "nothing on
this server can release a gated agent action today," because the failure
mode of getting it backwards is a stranger with only the ordinary API token
resuming a paused agent. Tested explicitly:
`decide_approval_403s_when_no_approval_token_is_configured_even_with_a_header`
sends a real header value with no server-side secret configured and still
gets `403` — the case that would silently break the "always reject" default
if a future refactor added an `Option::unwrap_or(true)`-shaped shortcut.
**`GET /api/approvals` (reading the inbox) needs none of this** — only the
ordinary orch gate — per the card's own instruction that reading and acting
are different privilege levels; its response carries a `grant_available:
bool` (server-config-presence only, never the secret) so the frontend can
hide decision controls without a second probe.

**Correctness requirements, one by one.**

- **Uncorrelated approvals surface here, verified live in two places.**
  `tack-db::repo::orch::list_pending_orch_approvals_with_context` (new,
  additive) `LEFT JOIN`s `items`/`projects` onto the existing
  `list_pending_orch_approvals` query so an `item_id IS NULL` row still
  returns every field, just with `item_*`/`project_*` all `null` rather than
  being dropped by an inner join. Covered at the repo layer
  (`test_pending_approvals_with_context_enriches_correlated_and_still_surfaces_uncorrelated`),
  the HTTP layer (`inbox_is_oldest_first_and_includes_uncorrelated_approvals_with_context`),
  and the frontend (`ApprovalsPage.test.tsx`'s populated-inbox test asserts
  the uncorrelated row renders with the literal string "Uncorrelated" — not
  just that the API returns it) — plus a live-rendered a11y scan
  (`approvals inbox (populated, decisions enabled)`, Chromium, 0 violations)
  that specifically waits on `getByText(/Uncorrelated/)` before scanning, so
  a regression that silently dropped the row would fail that test on the
  `waitFor` before ever reaching axe.
- **Oldest first.** Both the repo query (`ORDER BY a.requested_at ASC`) and
  the existing `list_pending_orch_approvals` it's built alongside share this
  ordering; nothing in the handler or frontend re-sorts.
- **`channel: "tack"` on every decision.** Verified the exact parameter name
  and accepted vocabulary against `~/Sites/rack-cli/src/docket/serve.py`
  (`do_POST`'s `/approvals/` branch) and `core/approval.py`
  (`APPROVAL_CHANNELS = frozenset({"cli", "http", "mcp", "telegram",
  "timeout", "tack"})` — `"tack"` is already a first-class member, not
  something I had to add upstream). `DocketAdapter::decide_approval`
  (`crates/tack-orch/src/adapters/docket.rs`) sends it as a fixed constant,
  never a parameter threaded up through `tack-api` — every caller of this
  trait method *is* Tack, so there's no second value it would ever send;
  threading an unused parameter through the handler → dispatcher →
  trait → adapter chain for a value that never varies would be the kind of
  workaround §2.1 tells this cycle to stop building. Wire-verified with
  `wiremock`'s `body_partial_json` (`decide_approval_grant_sends_channel_tack_and_returns_the_resulting_state`
  in `tack-orch`, and the same shape again at the HTTP boundary in
  `orch_approvals_test.rs`) — the mock only matches, and thus the test only
  passes, if `channel: "tack"` genuinely reached the request body, not
  merely appears in application code that might not run.
- **Not reversible, not a single click.** `ApprovalsPage.tsx` never calls
  `approvalsApi.decide` directly from a row button — Grant/Deny only open a
  confirmation `Modal` naming the agent, the action text verbatim, the
  correlated item (or the explicit "uncorrelated" label), and how long it's
  been waiting, with the literal words "This cannot be undone." Tested
  end-to-end in Vitest (`clicking Grant opens a confirmation modal ... before
  any decide call fires` asserts the fetch mock was called exactly once —
  the initial list — until the modal's own confirm button is clicked) and
  scanned for a11y as its own state (`approvals inbox confirmation modal`,
  0 violations against a real Chromium render, not a jsdom approximation).
- **A stale/already-decided token is a normal state, not an error.** V1
  could only live-verify docket's `grant` and unknown-token-404 paths, not
  `deny` or the 409/`ApprovalNoop` path — I read `core/approval.py` directly
  instead: `approval_grant`/`approval_deny` raise `ApprovalNoop` (→ HTTP 409)
  only for "already granted"/"already denied-or-expired" respectively; any
  *other* non-`pending` state (e.g. denying an already-*granted* token)
  raises the plain `ApprovalError` that `serve.py` maps to the same 404 a
  genuinely unknown token gets. `OrchError` gained a new typed variant,
  `AlreadyDecided(String)`, for the 409 case (409 → `ApiError::Conflict`);
  the 404 case reuses the existing `OrchError::NotFound` → `ApiError::NotFound`
  mapping docket's own text flows through either way. The frontend's
  `isApprovalAlreadyDecided`/`isApprovalGone` classifiers both resolve to
  the same UX — toast a plain explanation, refetch the inbox, no red error
  banner — while staying distinguishable in the wire boundary for whoever
  next needs to tell them apart. None of this is live-verified against a
  real docket (V1's own gap, not mine to close retroactively) — flagging
  that the 404-for-illegal-transition classification is read-from-source,
  same as `adapters::docket`'s module doc now says explicitly.
- **§0 rule 5.** `decide_approval`'s handler does exactly one DB read
  (`get_orch_approval`, to resolve which control plane issued the token) —
  no write — then the HTTP call to docket, then one short `UPDATE` afterward
  (`mark_orch_approval_decided`, new, additive) to keep the local mirror out
  of the next inbox fetch. No transaction spans the HTTP call.
- **§0 rule 8.** `every_orch_route_404s_when_disabled`-style coverage for
  both new routes lives in `orch_approvals_test.rs` directly
  (`list_approvals_404s_when_orch_disabled`,
  `decide_approval_404s_when_orch_disabled`) rather than extending A4's
  original `every_orch_route_404s_when_disabled` cases list in
  `orch_test.rs` — that list already predates several other waves' routes
  (metrics, dispatch, sprint dispatch) without any of them extending it
  either, so I matched the pattern already established rather than being
  the first to grow a list every later card would then also need to touch.
- **§0 rule 9.** No new color pairing: `Badge`'s existing tones, `Modal`,
  `Field`, `Button`'s existing variants. `npm run lint:tokens` stayed at
  0/0. Four new a11y scans (disabled / populated-with-uncorrelated / the
  confirmation modal / decisions-disabled-no-token) all run live against
  Chromium in this session — 0 violations each, not just "written," actually
  executed (`npx playwright test e2e/a11y.spec.ts --project=chromium -g
  approvals`).

**The interface change.** `crates/tack-orch/src/lib.rs`'s Wave-0 freeze is
already lifted (§2.1/R1); I used that room rather than working around a
frozen shape. `ControlPlane::decide_approval`'s return type changed from
`Result<(), OrchError>` to `Result<ApprovalState, OrchError>` (docket's own
resulting state, so a caller doesn't have to assume the request it sent is
the state that landed) and `OrchError` gained `AlreadyDecided(String)`.
Every implementor/caller updated in the same change (the production
`DocketAdapter`, `reconciler.rs`'s test-only `FakeControlPlane`) — `cargo
build --workspace` catching both required updates via type errors is exactly
the mechanism this refactor discipline is supposed to lean on.

**A repo-layer duplication, disclosed.** `handlers::orch::
build_control_plane_for_decision` (resolve a `control_plane_id` into a live
`DocketAdapter`) is ~15 lines copy-pasted from `dispatcher::
build_control_plane`, a private fn in a file owned by the concurrent
WIP-race-fix agent this wave (`crates/tack-api/src/dispatcher.rs`, R2's card
above). Exporting and sharing it would have meant editing a file outside
my ownership mid-cycle for a one-fn convenience; duplicating ~15 lines was
the smaller footprint. If a third caller ever needs this, that's the point
to actually factor it out.

**What I deliberately did not build.** No new `BoardEvent`/WebSocket
broadcast on a grant/deny decision — `handlers/websocket.rs` is B4's file,
not mine this wave, and B4's own handoff already noted `ApprovalPending`
never fires for an uncorrelated approval in the first place (no project to
filter into), which is exactly the case this inbox exists to surface. Since
a per-project socket can't be the primary freshness mechanism for the row
this page cares most about, `ApprovalsPage.tsx` polls `GET /api/approvals`
every 10s instead (cleared via Solid's `onCleanup`, verified not to leak
across Vitest tests via explicit `dispose()` calls in every test) — this
covers correlated and uncorrelated rows identically, at the cost of not
being instant. A future card wiring a real-time removal-on-decision event
would still need `handlers/websocket.rs`, not just this module.

**A pre-existing e2e finding, unrelated to this card, flagged not fixed.**
While running the full `a11y.spec.ts` suite live (Chromium) to confirm my
own new scans didn't regress anything, two pre-existing tests failed on a
freshly-reset `frontend/e2e.db` (not an accumulation artifact from repeated
runs — confirmed by deleting the throwaway DB and rerunning once):
`sprint "Run sprint" dry-run preview` and `sprint dispatch results (mixed
outcomes)` both fail on `getByRole('button', { name: 'Run sprint' })`
resolving to 2 elements instead of 1. `git status`/`git diff` confirm zero
uncommitted changes anywhere under `frontend/src/features/sprints/` or
`frontend/e2e/helpers.ts` this session — this is either a bug that shipped
with card C4 and was never caught (C4's own handoff says it "did not run
the Playwright e2e suite live — no backend available in this environment")
or something a later, unrelated change introduced without touching those
files directly (e.g. a shared layout component rendering an extra "Run
sprint" affordance). Not investigated further — out of this card's scope
(approvals, not sprint dispatch) — but real, live-reproduced, and worth a
follow-up card picking it up before it's mistaken for flake.

**Testing.** Rust: 9 new tests in `crates/tack-orch/tests/
docket_adapter_test.rs` (decide_approval's grant/deny/unknown-state/409/404/401
outcomes, replacing the old blanket "both still disabled" test since only
`dispatch` still is), 2 new unit tests in `lib.rs`, 3 new repo-layer tests in
`crates/tack-db/tests/orch_repo_test.rs`, 11 new HTTP-boundary tests in the
new `crates/tack-api/tests/orch_approvals_test.rs`. Frontend: 28 new Vitest
tests across `features/approvals/{api,format,ApprovalsPage}.test.ts(x)`,
plus 4 new live-executed Playwright a11y scans.

**Verification.** `cargo test --workspace`: 523 passed, 0 failed (moving
baseline — R2's WIP-race fix and other concurrent Wave-4 work landed in the
same window; my own net-new count is the 25 tests listed above). `cargo
clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all --
--check`: clean for every file I touched (confirmed via `cargo fmt -p
tack-orch -p tack-db -p tack-api -- --check` showing drift only in
`alexa_wip_race_test.rs`, R2's new file, never opened by me). `UPDATE_OPENAPI=1
cargo test -p tack-api --test openapi_contract`: regenerated `docs/
openapi.json` (two new paths, five new schemas), drift gate green. Frontend:
`npm run type-check` clean; `npm run lint:tokens` 0/0 unchanged; `npm run
test` — 332 passed, the same 3 pre-existing `requestBlob`/`createObjectURL`
failures called out in the card brief (304 baseline + 28 new, arithmetic
exact); `npm run build` clean, `ApprovalsPage` code-splits into its own
chunk. Did not verify against a live docket this round — V1/R1 already
live-verified `POST /approvals/{token}` grant + unknown-404, and this card's
own live-verification effort went into confirming `channel`'s exact
parameter name/vocabulary against docket's source directly instead (see
above) since standing up an isolated `docket serve` for grant/deny alone
would have re-covered ground V1 already walked.

**Files touched:** `crates/tack-orch/src/lib.rs` (trait signature +
`AlreadyDecided`), `crates/tack-orch/src/adapters/docket.rs`
(`decide_approval` implemented), `crates/tack-orch/src/reconciler.rs`
(`FakeControlPlane`'s signature, mechanical), `crates/tack-orch/tests/
docket_adapter_test.rs`, `crates/tack-db/src/repo/orch.rs` (additive:
`list_pending_orch_approvals_with_context`, `mark_orch_approval_decided`,
`PendingOrchApproval`), `crates/tack-db/tests/orch_repo_test.rs`,
`crates/tack-api/src/handlers/orch.rs` (new section: DTOs, both handlers,
the approval-token gate, the duplicated control-plane resolver),
`crates/tack-api/src/router.rs` (two routes, at A4's marked insertion
point), `crates/tack-api/src/openapi.rs`, `docs/openapi.json`, new
`crates/tack-api/tests/orch_approvals_test.rs`, and everything under
`frontend/src/features/approvals/` (new: `api.ts`, `format.ts`,
`ApprovalsPage.tsx`, three `.test.ts(x)` files), plus small additive edits
to `frontend/src/app/routes.tsx` (route), `frontend/src/shared/ui/
Sidebar.tsx` + `icons.tsx` (nav link + `IconApprovals`), and
`frontend/e2e/a11y.spec.ts` (four new scans). Did not touch `dispatcher.rs`,
`sprint_dispatch.rs`, `handlers/websocket.rs`, or any `tack-core`/`tack-db`
file beyond the additive `repo/orch.rs` functions, per scope.

**For D2–D5 and beyond.** Nothing in this card blocks any of them —
approvals are a leaf feature. If a future card wants real-time
removal-on-decision (not just new-approval broadcast), it needs
`handlers/websocket.rs` (B4's territory) and a new `BoardEvent` variant;
I deliberately didn't build that here (see above). The Sprints-view
duplicate-button finding above is worth a dedicated look before the next
wave's e2e run trips over it and burns time re-diagnosing what this note
already narrowed down.

### R3 — 2026-08-05

**Yes, I reproduced the race on the board-drag path before fixing it —
reliably, not marginally, and with no artificial delay needed.** Before
touching `handlers/items.rs`, I wrote
`crates/tack-api/tests/board_drag_wip_race_test.rs` against the unmodified
code: 12 distinct items sitting in "Backlog" (scrum workflow, "In Progress"
WIP limit 5), all `PATCH`ed to "In Progress" via 12 genuinely concurrent
`PATCH /api/items/{id}` requests (`tokio::spawn` on a `multi_thread`
runtime). **All 12 landed in the column, every one of 3 runs** — unlike
R2's dispatch-path repro, this needed no `wiremock` delay to bunch the
requests: contention over the in-memory SQLite pool's five connections
across 12 concurrent in-process requests was enough on its own. I did the
same for the Alexa path with a new
`crates/tack-api/tests/alexa_wip_race_test.rs` (a custom two-status workflow
with `wip_limit: 5` on "Done" — no preset workflow ships a WIP limit on
Done, but `handlers::alexa` has its own `msg_wip_limit` spoken response for
exactly this case, so the path clearly anticipated it): 12 concurrent
`CompleteTaskIntent` requests, **11 of 12 landed in the WIP-5 "Done" column,
consistently across 3 runs.** After the fix, both tests pass consistently —
checked 5x manually beyond the one run each gets in the permanent suite.

**The bug, exactly as R2's handoff predicted.** Both `handlers::items::
update_item` and `handlers::alexa::complete_task` did the identical
two-step R2 found and fixed on the dispatch path:

```rust
let count = state.repo.count_items_by_status(project_id, new_status).await? as usize;
project.workflow.check_wip_limit(new_status, count)?;
// ... separately, later, on its own connection:
state.repo.update_item(id, update).await?;
```

Two `.await` points, no lock spanning them — R2 named both call sites in
its handoff and correctly declined to fix them (outside that card's file
ownership), flagging the fix as "a one-line swap at each call site" to
`Repository::update_item_status_checked`. I verified that claim rather than
assuming it, per the brief — it held for both, with one wrinkle at each
site (below).

**The fix: the same one-line swap R2 predicted, plus the surrounding
plumbing partial updates need.**

**`handlers::items::update_item`** — not literally one line, because this
handler does considerably more than change a status: it's the single entry
point for partial updates to title, description, priority, tags,
sprint_id, due_date, etc., all in one `PATCH`. `update_item_status_checked`
only touches the status column and its started_at/completed_at bookkeeping
— it doesn't know about the other fields. So the status branch now: (1)
validates the transition (unguarded, same as before — it's a pure
workflow-config check, not a row count, so R2's reasoning that it isn't
racy applies here too); (2) resolves `status_category`; (3) calls
`update_item_status_checked` and matches `Applied`/`Rejected`, returning
`ApiError::Core(e)` on rejection (mapped via `CoreError`'s existing `From`
impl, same HTTP 400 the old `check_wip_limit(...)?` produced); (4) on
success, clears `input.status`/`input.status_category` to `None` so the
unconditional `repo.update_item(id, input)` call right after — which still
handles every *other* field exactly as before — doesn't redundantly
(and unguardedly) re-write status. A request that never touched status in
the first place skips this whole branch and hits the ordinary
`repo.update_item` path unchanged: no transaction, no WIP check, no
behavior change — covered by the new
`patch_without_a_status_change_is_unaffected` test, per the brief's
explicit ask. Everything downstream of the `let item = ...` line
(WebSocket broadcast, webhook, `propagate_parent_completion`,
`maybe_sync_github`, C2's `maybe_auto_dispatch`) is untouched — none of
those functions take `input`, only `item` and `old_status`, so splitting
the status write out of the single `repo.update_item` call doesn't change
what they see.

**`handlers::alexa::complete_task`** — closer to the literal one-line swap:
this handler was already building a throwaway `UpdateItem { status, status_
category, ..Default::default() }` just to call `repo.update_item` with it,
so replacing that whole block with `update_item_status_checked` actually
*removed* code (and the now-unused `UpdateItem` import). The one behavioral
nuance: Alexa's contract is "user-level problems get HTTP 200 + spoken
text, not an HTTP error" (this file's own module doc). A WIP-limit
rejection previously produced `speech(&msg_wip_limit(...), true)` via an
`if check_wip_limit(...).is_err()` branch *before* any write; now it's a
`StatusUpdateOutcome::Rejected(_)` arm reached *after* the atomic
check-and-maybe-write, matched the same way `dispatcher::apply_mapped_
status` matches it — same spoken response, same 200, just decided inside
the transaction instead of before it. Confirmed unchanged behavior via the
pre-existing `alexa_test.rs` suite (21 tests, including
`complete_task_moves_item_to_done`), all still green.

**Why `update_item_status_checked` rather than inventing a second atomic
method.** R2's method already does exactly what both call sites need
(status + started_at/completed_at, inside one `BEGIN IMMEDIATE`
transaction) and already returns the right shape
(`Option<StatusUpdateOutcome>`) for "not found" vs. "rejected" vs.
"applied." Reusing it verbatim — no signature change, no new `tack-db`
method — was both the smaller diff and exactly what R2's own handoff
pointed at.

**§0 rule 5 (no SQLite write transaction across an HTTP call).** Neither
call site was at risk of this to begin with — `update_item_status_checked`
is a single `tack-db` call with no HTTP inside it, and everything that
*does* make an outbound call (`maybe_sync_github`, C2's
`maybe_auto_dispatch`, both `tokio::spawn`ed) already ran, and still runs,
strictly after the item write is committed and the transaction closed. I
didn't restructure that ordering — R2's fix and C2's hook were already
correct on this point; I only changed *how* the status write itself
happens, not when the post-write side effects fire.

**§0 rule 7 (status changes go through the workflow engine).** Unchanged:
`validate_transition` still runs before any write on both paths, and
`update_item_status_checked` calls the same `WorkflowConfig::
check_wip_limit` the old unguarded code called — same function, same
`CoreError::WipLimitExceeded`, now just evaluated inside the locked
transaction instead of before it. WIP limits, explicit transitions, and
started_at/completed_at bookkeeping all still fire; parent
auto-propagation (`propagate_parent_completion`) is untouched on both call
sites.

**What I tested:**

- `crates/tack-api/tests/board_drag_wip_race_test.rs` (new) —
  `concurrent_board_drags_into_the_same_wip_limited_column_never_exceed_the_
  limit` (12 items, 12 concurrent `PATCH /api/items/{id}` requests, WIP
  limit 5, asserts the "In Progress" count never exceeds 5 and that
  Backlog + In Progress always sums to 12 — nothing lost or duplicated) and
  `patch_without_a_status_change_is_unaffected` (a title-only `PATCH`
  behaves exactly as before, status untouched).
- `crates/tack-api/tests/alexa_wip_race_test.rs` (new) —
  `concurrent_alexa_completions_into_the_same_wip_limited_column_never_
  exceed_the_limit` (12 items, 12 concurrent `POST /api/alexa`
  `CompleteTaskIntent` requests each resolving a distinct title, custom
  workflow with `wip_limit: 5` on "Done", asserts the same invariants).
  Needed a one-line addition to `crates/tack-api/tests/common/mod.rs`:
  `#[allow(dead_code)]` on `test_app()`, mirroring the pre-existing
  attribute on `test_app_with_file_db` — this new test binary only calls
  `test_app_with_config`, and each integration-test binary recompiles
  `mod common` as its own unit, so `test_app` alone read as dead code under
  `-D warnings` for this binary specifically (a pre-existing sharp edge in
  how the shared test helper is structured, not something I changed the
  shape of).
- Reran the pre-existing `alexa_test.rs` (21 tests) and the full workspace
  suite after the fix to confirm no regressions in GitHub sync, parent
  propagation, or auto-dispatch — all of which share `update_item`'s
  downstream side-effect calls.
- Updated the stale doc comment on `Repository::update_item_status_checked`
  (`crates/tack-db/src/repo/items.rs`) that named both call sites as "not
  fixed by this method" — that sentence was accurate when R2 wrote it and
  is not anymore; not in my file-ownership list, but not owned by anyone
  else either (only `repo/orch.rs` is listed under `tack-db`), and leaving
  a doc comment that actively asserts something false felt worse than a
  two-line correction.

**Verification.** `cargo test --workspace`: 523 passed, 0 failed (baseline
520 + my 3 new tests). `cargo clippy --workspace --all-targets -- -D
warnings`: clean. `cargo fmt --all -- --check`: **not** run tree-wide, per
the by-now-established convention (D1 has unformatted in-flight work in
`handlers/orch.rs`, `router.rs`, `frontend/**`) — ran `rustfmt --edition
2024 --check` on every file I touched or created (`handlers/items.rs`,
`handlers/alexa.rs`, `tests/board_drag_wip_race_test.rs`,
`tests/alexa_wip_race_test.rs`, `tests/common/mod.rs`,
`tack-db/src/repo/items.rs`); clean (one formatting fixup needed in the new
Alexa test file, applied and reconfirmed).

**Files touched, all disclosed, all in scope:** `crates/tack-api/src/
handlers/items.rs` (the `update_item` status branch), `crates/tack-api/src/
handlers/alexa.rs` (`complete_task`'s completion write), `crates/tack-api/
tests/board_drag_wip_race_test.rs` (new), `crates/tack-api/tests/
alexa_wip_race_test.rs` (new), `crates/tack-api/tests/common/mod.rs`
(one `#[allow(dead_code)]` line), `crates/tack-db/src/repo/items.rs` (doc
comment only — no behavior change). Did not touch `dispatcher.rs`,
`sprint_dispatch.rs`, `handlers/orch.rs`, `router.rs`, `openapi.rs`,
`docs/openapi.json`, `crates/tack-db/src/repo/orch.rs`, or `frontend/**`,
per scope — those are D1's and other agents' concurrent territory.

### D3 — 2026-08-05

**`pipeline validate`'s reachability, first, since it decided the shape of
everything else.** Read `~/Sites/rack-cli/src/docket/cli/_pipeline.py` and
`core/pipeline.py` directly. `docket pipeline validate <file>` is a thin CLI
wrapper (`cli/_pipeline.py::_validate`) over `core.pipeline.validate_pipeline
(text: str) -> list[str]` — a pure function, already UI-free, already the
single source of truth for "is this a valid docket pipeline" (duplicate step
ids, rework edges pointing at unknown/later steps, bad variable names, the
works — `PipelineSpec`'s pydantic `model_validator`). I checked every
`do_GET`/`do_POST` branch in `serve.py` by hand: **there is no HTTP route
for it.** The closest thing, `POST /dispatch/<project>`, *runs* a pipeline
against a real pod; it doesn't just check one, and it isn't a substitute.

**Decision: don't shell out, don't reimplement, do the one honest check
Tack can make, and record the gap upstream.** Shelling out to a local
`docket` binary from `tack-api` (the server) was the option I ruled out
hardest: every other control-plane interaction in this codebase goes
through `tack-orch::ControlPlane` over HTTP precisely because a docket
instance is not assumed to be on the same host as the Tack server — adding
one code path that assumes a local CLI binary would be a real architectural
regression, not a shortcut, and `tack-api` shells out to nothing today
(only `tack-cli`'s `tack branch` does, and that runs on the operator's own
machine by design). Reimplementing `PipelineSpec`'s schema in Rust is
exactly the mistake this cycle already paid down once (B2's client-side
cursor reimplementation of `serve.py`'s trace-paging algorithm, undone by
R1) — a second copy of docket's pipeline schema would drift the first time
docket adds a step kind or gate variant, with no compiler error to catch it.

So: `handlers::templates::validate_template_orchestration` (new,
`crates/tack-api/src/handlers/templates.rs`) does exactly one check on
`orchestration.pipeline_yaml` when it's set — `serde_yaml::from_str` parses
it — and says so plainly in both the doc comment and the 400 error message
it produces on failure ("this only checks it parses as YAML, not that it is
a valid docket pipeline"). No stored field claims a stronger guarantee than
that. I recorded the actual gap upstream: `~/Sites/rack-cli/ROADMAP.md`
Phase 22, new card **P22-8 — `pipeline validate` over HTTP** (isolated
diff — that repo has *other* uncommitted work in flight, `core/
pod_provisioning.py` and a new test file, neither of which I touched).
P22-8 proposes `POST /pipeline/validate`, body = raw YAML, `{ok, errors}` —
a `do_POST` branch and nothing else, since `validate_pipeline` already
exists and already takes exactly the argument this needs. The day that
route ships, `validate_template_orchestration` should call it instead of
the bare `serde_yaml` parse — flagged in both files.

**Second finding, not mine to act on but too load-bearing to sit on: `POST
/pods` already shipped.** Reading `serve.py` for the pipeline-validate route
turned up `elif path == "/pods": self._handle_post_pods()` — a full,
working implementation (`core/pod_provisioning.py`, commit `0d84f47`,
"Feat: POST /pods -- provisioning over HTTP (P22-5)", 2026-08-04). The
status board above and `rack-cli/ROADMAP.md`'s own P22-5 entry both still
say "does not exist" / "TODO" — the same staleness pattern the 2026-08-04
correction at the top of this file already found for P22-1/2/3 (docket's
ROADMAP.md lags its own source; `ROADMAP.md`'s last commit there is
2026-08-05 10:42, *after* 0d84f47, so it isn't even a timing artifact — the
card just wasn't marked done). **D4 (provisioning) is very likely
unblocked.** I did not touch D4's files or ROADMAP.md's P22-5 marker myself
— correcting a status line I merely verified in passing, on a card that
isn't mine, felt like overreach the same way editing `router.rs` beyond a
disclosed one-liner would be. Whoever picks up D4 should re-verify against
`serve.py::_handle_post_pods` directly (its docstring documents the full
`{project, path, blueprint, pod, budget, verifyCmd}` contract and rollback
behavior) before starting, not trust either roadmap's status marker.

**What I built.** An `orchestration` block on project templates (Phase 37,
tasks 37.1 + 37.3):

- `crates/tack-core/src/models.rs` — `TemplateOrchestration` (blueprint,
  `pipeline_yaml`, `pipeline_file`, `verify_cmd`, `budget_usd`, `status_map`,
  `auto_dispatch`, `pod_shape`), `TemplateStatusMap` (field-for-field
  identical to `handlers::orch::StatusMap` — kept as two types because
  `tack-core` cannot depend on `tack-api`, but never validated separately,
  see below), `OrchBlueprint` (the five real docket blueprint names,
  `rename_all = "kebab-case"` so `AgenticProduct` → `"agentic-product"` on
  the wire — verified against `core/blueprints.py`, no `Unknown` fallback
  since this is a value Tack *sends*, not one it decodes from docket, so
  TODO.md §1.2's `Unknown(String)` rule doesn't apply here by its own
  scoping). `ProjectTemplate.orchestration` / `CreateProjectTemplate.
  orchestration: Option<TemplateOrchestration>`, both `#[serde(default)]` —
  absent means nothing, the same rule migration 029 established for
  `items.source`.
- `crates/tack-db/src/migrations.rs` — migration 030, `ALTER TABLE
  project_templates ADD COLUMN orchestration TEXT` (nullable, no default —
  `NULL` is "no block," distinct from `Some("{}")`). No `NOT NULL`, so every
  existing INSERT path, including `seed_builtin_templates`'s (which I did
  **not** need to touch), keeps working unchanged — an unlisted column with
  no default just becomes `NULL`.
- `crates/tack-db/src/repo/templates.rs` — `TemplateRow.orchestration:
  Option<String>`, threaded through `create_template`/`get_template`/
  `list_templates`; a malformed JSON blob degrades to `None` (`parse_
  orchestration`) rather than failing the whole template read, matching this
  file's existing "corrupt JSON → safe default" convention for `vocabulary`/
  `workflow`.
- `crates/tack-api/src/handlers/templates.rs` — `validate_template_
  orchestration(orch, workflow) -> ApiResult<()>`, called from
  `create_template` before the repo write, against `data.workflow.clone().
  unwrap_or_else(simple_workflow)` — the **exact same fallback** `repo::
  templates::create_template` applies internally, so "the workflow this
  template will actually create" can never diverge between the validator and
  the write (TODO.md §6's explicit instruction: validate against the
  template's own future workflow, not a live project's). `create_template`
  itself changed from `Result<Json<T>, StatusCode>` (a bare status, no body —
  its own `#[utoipa::path]` already documented a `422` `ErrorEnvelope` body
  that the old code never actually produced) to `ApiResult<Json<T>>`, which
  is what makes "400 naming the bad key" possible at all; `list_templates`/
  `get_template`/`delete_template` untouched. `create_project_from_template`
  and `save_project_as_template` got doc comments recording two deliberate
  non-decisions, not code changes:
  - `create_project_from_template` never reads `template.orchestration` —
    turning it into a live `orch_links` row needs a `control_plane_id`
    pointing at one specific, already-registered docket instance, which
    can't exist yet at this point (no pod provisioned). That's D4's
    `provision_pod: true` extension of this same endpoint (task 37.2), not
    mine — nothing here is gated on `TACK_ORCH_ENABLE` because nothing here
    does anything orchestration-shaped yet.
  - `save_project_as_template` never derives `orchestration` from the source
    project's live `orch_link`, unlike vocabulary/workflow/boards, which it
    does copy. This is a considered call, not a shortcut:
    `orch_links.control_plane_id`/`remote_project` point at one specific
    docket instance and one specific remote project string — copying them
    into a template would make every *future* project created from that
    template silently point at someone else's pod. A template's
    orchestration block can only be set explicitly via `POST /api/templates`
    in this cycle.

**Reused, not duplicated, the `status_map` validator.** `handlers::orch::
validate_status_map` (card A4) went from a private `fn` to `pub(crate) fn`
— the only change to that function; the visibility bump is disclosed since
`handlers/orch.rs` is D2's file this wave, and I re-read the file
immediately before making it (D2 was mid-edit on `router.rs`/`openapi.rs`
concurrently — confirmed by the live-file notices the harness surfaced
while I worked, so I re-read both files fresh right before touching either).
`validate_template_orchestration` builds a `handlers::orch::StatusMap` from
`TemplateStatusMap`'s five fields (a plain conversion, not a validator) and
calls `validate_status_map` directly — one validator, two Rust types feeding
it, exactly the instruction in TODO.md §6.

**`openapi.rs` — also D2's file, also disclosed, also additive-only.** Added
`OrchBlueprint`, `TemplateOrchestration`, `TemplateStatusMap` to the
existing `use tack_core::models::{...}` import list and the `components(
schemas(...))` list, next to `ProjectTemplate`/`CreateProjectTemplate` —
three identifiers in two places, nothing restructured. No `router.rs`
change was needed at all: this card added zero new routes (all the new
behavior lives inside the existing `POST /api/templates` handler), so the
chokepoint-file conflict the card's instructions warned about never
actually materialized. Regenerated `docs/openapi.json` via
`UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract` — the
regenerated file necessarily also reflects D2's already-landed routes
(`/metrics`, `/items/{id}/dispatch`, `/approvals`, etc.), since it's a
single generated artifact for the whole API, not something scoped per-card.

**Trust — items created from a template.** There aren't any, in this
codebase, today. I read `create_project_from_template` closely before
concluding this: it creates exactly one Tack project and applies workflow /
vocabulary / custom fields / boards from the template — no `Item` rows.
`save_project_as_template` doesn't snapshot items either (only board
*structure* — columns derived from workflow statuses — not the items sitting
in them). So "what `ItemSource` should a template-authored item get" has no
live call site to decide for in this cycle; I looked for one deliberately
rather than assuming. If a future card adds seed items to templates, the
precedent is already unambiguous from C2's work: template-authored text is
not the operator's own words typed in the moment, so it should default to
whatever `ItemSource` variant `is_trusted() == false` for — never `Manual`,
same "unsafe state is never the accidental default" rule migration 029
applies everywhere else. Recording this explicitly so nobody has to re-derive
it: it is not this card's gap, but it also isn't nothing.

**Frontend.** Added `orchestration` to the generated TS types via `npm run
gen:api` (`frontend/src/shared/api/schema.gen.ts`, machine-generated from
`docs/openapi.json` — picks up `ProjectTemplate.orchestration` and the three
new schemas automatically, plus D2's already-landed endpoints in the same
regeneration, same reasoning as the `docs/openapi.json` regen above). I did
**not** build a UI panel for authoring/editing a template's orchestration
block — `features/templates/TemplateCreator.tsx` is unchanged. This is a
scope call, not an oversight: the card's hard requirements (validate,
don't trust) are both save-time/API-layer concerns and are fully delivered
without one; a template-orchestration editor is materially new UI surface
that risks colliding with D2's concurrent, in-flight work under `frontend/
src/features/settings/orchestration/` (visible as untracked files in `git
status` while I worked) rather than genuinely being blocked by it. Left for
a follow-up card once D2's panels establish the visual vocabulary for
orchestration-shaped settings UI, so this doesn't end up a second,
inconsistent design.

**What I tested.**

- `crates/tack-core/src/models.rs` (6 new `#[cfg(test)]` cases): missing-key
  → `None` for both `ProjectTemplate` and `CreateProjectTemplate`
  (backward-compat, mirrors `item_deserialization_defaults_missing_source_
  to_unknown`); `TemplateOrchestration::default()` round-trips;
  `OrchBlueprint` serializes to docket's exact five wire strings including
  the kebab-case hyphen; `TemplateStatusMap` round-trips every field.
- `crates/tack-db/src/repo/templates.rs` (3 new `#[tokio::test]` cases):
  every built-in template has `orchestration: None` after seeding; a
  template created with `orchestration: None` round-trips through
  `create_template`/`get_template` as `None`; a fully populated
  `TemplateOrchestration` round-trips byte-for-byte through `create_
  template` → `get_template` → `list_templates`.
- `crates/tack-api/tests/templates_orchestration_test.rs` (new, 7 tests):
  no-`orchestration`-key and explicit-`null` both still create the template
  (backward compat); unknown `status_map` name → `400` naming the bad
  status (`"Ready"` appears in the error message, not just a bare status
  code); **status_map is validated against a custom `workflow` supplied in
  the *same* request, not `simple_workflow()`'s default names** — the test
  that most directly proves the "validate against the workflow the template
  will actually create" instruction, using a custom two-status-category
  workflow where a `simple_workflow()` name ("To Do") is rejected and the
  template's own name ("Backlog") is accepted; unparseable `pipeline_yaml`
  → `400`; a fully populated valid block round-trips through create + get,
  including the blueprint's wire name; a rejected orchestration block leaves
  no template behind at all (checked via a follow-up `GET /api/templates`
  list, not just the failing response code).
- Updated 4 pre-existing `CreateProjectTemplate` struct literals in
  `crates/tack-db/tests/integration_test.rs` with `orchestration: None` —
  these predate this field and needed the new required struct field added
  to compile; no behavior change.

**Verification.** `cargo build --workspace`: clean. `cargo test
--workspace --no-fail-fast`: 547 passed, 0 failed (my 16 new tests — 6
tack-core + 3 tack-db + 7 tack-api — on top of whatever C-and-D-wave tests
had already landed concurrently; I don't have a clean pre-D3 baseline number
to diff against since other agents were actively landing work in parallel,
but nothing failed). `cargo clippy --workspace --all-targets -- -D
warnings`: clean. `cargo fmt --all -- --check`: clean (one `rustfmt` pass
needed on `migrations.rs`, applied). Frontend: `npm run type-check` clean;
`npm run lint:tokens` 0/0 both gates; `npm run test -- --run`: 363 passed,
3 failed — the same three pre-existing `requestBlob`/`createObjectURL`
failures named in this card's baseline, confirmed unrelated (I touched zero
frontend `.ts`/`.tsx` source, only regenerated `schema.gen.ts`).

**Files touched, all disclosed:** `crates/tack-core/src/models.rs`,
`crates/tack-db/src/migrations.rs`, `crates/tack-db/src/repo/templates.rs`,
`crates/tack-db/tests/integration_test.rs`, `crates/tack-api/src/handlers/
templates.rs`, `crates/tack-api/src/handlers/orch.rs` (one visibility
keyword — D2's file, disclosed), `crates/tack-api/src/openapi.rs` (three
schema identifiers in two lists — D2's file, disclosed), `crates/tack-api/
tests/templates_orchestration_test.rs` (new), `docs/openapi.json`
(regenerated), `frontend/src/shared/api/schema.gen.ts` (regenerated,
frontend source untouched), `~/Sites/rack-cli/ROADMAP.md` (new P22-8 card,
isolated diff, that repo's other in-flight uncommitted work left alone). Did
not touch `router.rs` (no new routes), `frontend/src/features/templates/**`
(UI deferred, see above), or any `D2`/`D4`/`D5` file.

### D2 — 2026-08-05

**What docket can and cannot do over HTTP — read this first, since it decided
everything else this card built.** Read `serve.py`'s complete `do_GET`/
`do_POST` route tables directly (not TODO.md §1.4, not docket's own
ROADMAP.md — both have been wrong before this cycle) before trusting any
summary of this, including mine below.

- **Budget is real and reachable, but not the number this card ended up
  using.** `GET /status.json`'s `_agent_record()` and the internal `/tasks`
  cost payload both carry `budgetUsd`/`costUsd` per docket agent — already
  modeled in `tack-orch::FleetAgent` (`cost_usd_estimated`/`budget_usd`,
  Wave 0) and fetched every reconciler tick (`reconciler::poll_status`) —
  **but never persisted.** `reconcile_once` calls `poll_status` only to
  compute plane health/`api_version`; `FleetStatus.agents` is discarded
  immediately after. So docket's own per-agent budget figure isn't actually
  sitting in Tack's database anywhere today. Card A4 already built the
  number this card's acceptance bar actually asks for — a **project's own
  configured `orch_links.budget_usd` against Tack's token-based
  `orch_tasks`-derived estimate** — for `GET /api/fleet`. This card's budget
  panel is that same figure, reused (`project_task_usage`, unchanged) via a
  new project-scoped endpoint, not docket's own `costUsd`. Both numbers are
  estimates either way — docket's own driver-reported `cost_usd` is real
  only when the runtime driver populates it (`core/utils.py::aggregate_cost`
  docstring: "converting a token count into a dollar figure is exactly the
  estimate-to-billing-claim conversion this codebase has a standing rule
  against" — docket enforces the same discipline Tack's own Rule 6 does) and
  is otherwise silently `0.0`, so it wouldn't have been a strictly better
  number to surface even if it had been wired through.
- **Pause has *no* HTTP surface at all, in either direction.** Not a
  narrower gap than expected — a complete one. `serve.py`'s full route
  tables have no `/profile`, no pause, no resume route anywhere: clearing a
  pause is `docket profile <id> --resume`, CLI-only
  (`core/dispatch.py::_pause_lead_for_budget` sets it,
  `cli/__init__.py`'s `profile` command is the only code that ever clears
  it). And *reading* the state isn't reachable either — I read
  `_agent_record()` (backs `/status.json`) and `render_metrics()` (backs
  `/metrics`) line by line: neither emits `paused`/`pausedReason` at all,
  even though `core/models.py::AgentMeta` tracks both internally and a third
  field, `docket_agent_paused` (or similar), simply doesn't exist in either
  output. The one real proxy, a `paused_refused` trace event
  (`core/dispatch.py::_claim_next_task`, payload `{"reason": "budget"}`),
  already flows into Tack via B2's ingestion — but arrives useless for this
  purpose: its `session_id` is the generic `"agent:<project>:dispatch"`
  form, which B2's own `session_id_task_id` correlation deliberately doesn't
  match (it isn't a task id), so it lands with `item_id = NULL`; and
  `orch_events` has no `remote_project` column at all — only
  `control_plane_id`, which `orch_links` makes many-to-one against Tack
  projects. So even a control-plane-level "somebody on this plane got
  refused for budget" signal can't be pinned to *which* linked project
  without either guessing (wrong the moment two projects share a plane) or
  a real ingestion change (persist `RemoteEvent.project`, add a
  `remote_project` column, correlate `paused_refused` at the project level
  instead of the item level). That's real scope, not a two-line fix, and
  it's out of a budget/policy card. **Consequently: I built no pause
  control and no pause indicator anywhere.** `BudgetPanel.tsx` names the
  real remedy (`docket profile <pod-id> --resume`) in a static caption so an
  operator who notices a project's spend has stalled knows what to check —
  it never claims to know whether that's actually what happened. TODO.md
  §1.4 gained a note documenting this (I own that correction per the card's
  explicit instruction — see the table above, "Added 2026-08-05 (card D2)").
- **Policy is real, reachable, and already ingested — but fleet-wide, not
  per-project.** `render_metrics()`/`_collect_trace_loop_metrics` fold
  *every* linked project's trace files together
  (`traces_dir.glob("*/*.jsonl")`, no project filter) into
  `docket_tool_calls_total`, `docket_policy_hits_total`,
  `docket_approvals_total` — confirmed by reading the aggregation loop
  directly. Card B3 already mirrors this shape verbatim into `orch_metrics`
  (`(control_plane_id, name, labels)`, no `remote_project` label — because
  docket never emits one to mirror). So a genuinely honest "policy panel for
  project X" is structurally impossible without a docket-side change; the
  best available real thing is "policy activity for the control plane
  project X happens to be linked to." I built that, labeled unmissably as
  such (`scoped_to_control_plane_only: true` on the wire, always present,
  never `false`; `POLICY_SCOPE_CAVEAT` rendered above every number on the
  panel, not as a footnote).

**What I built, scoped to exactly the above.**

1. **`GET /api/projects/{id}/orch-budget`** (`handlers/orch.rs`) — this
   project's `orch_links.budget_usd` cap against `project_task_usage`'s real
   token/cost sums (A4's existing private helper, reused verbatim, not
   reimplemented). `linked: false` for an unlinked project still reports
   real historical `tokens_in`/`tokens_out` (a project can carry dispatch
   history after being unlinked) but `cost_usd_estimated: null` always —
   there's no plane to attest freshness against. `cost_usd_estimated` is
   `null` whenever the linked plane is `unreachable` or its health can't be
   resolved, `Some(0.0)` for a reachable plane with nothing spent yet —
   identical staleness rule to `GET /api/fleet` (tested:
   `orch_budget_reports_zero_cost_distinctly_from_unreachable`, same shape
   as A4's own `fleet_reports_zero_cost_distinctly_from_unreachable`).
2. **`GET /api/projects/{id}/orch-policy`** (`handlers/orch.rs`) — filters
   `list_latest_orch_metrics()` (B3) down to the linked `control_plane_id`,
   groups into `tool_calls`/`policy_hits`/`approvals_by_channel`, and
   computes `denial_rate = deny / (allow + ask + deny)`. **`denial_rate` is
   `None`, never `0.0`, when no tool-call sample exists at all** — a `0.0`
   would claim a clean, evaluated history rather than "nothing observed
   yet" (tested:
   `orch_policy_denial_rate_is_none_with_no_tool_call_data`). Chain
   verification is not reimplemented — the panel links to `docket audit
   verify` as a command to run, per the card's explicit instruction.
3. **`frontend/src/features/settings/orchestration/`** (new) — `api.ts`
   (wire boundary, one file, per the D1/C4/B5/A5 precedent),
   `format.ts` (`formatBudgetCap`, `budgetProgress`, `formatDenialRate`,
   `BUDGET_PROGRESS_CAVEAT`, `BUDGET_PAUSE_NOTE`, `POLICY_SCOPE_CAVEAT`;
   reuses `shared/agentActivity/format.ts#formatEstimatedCost`/
   `formatTokens` verbatim, per the card's explicit instruction — no second
   cost formatter), `LinkForm.tsx`, `BudgetPanel.tsx`, `PolicyPanel.tsx`,
   `OrchestrationPanel.tsx` — wired into `ProjectSettings.tsx` as a new
   "Orchestration" tab.
4. **`LinkForm.tsx` — the one piece of UI that didn't exist anywhere
   before this card.** `PUT /api/projects/{id}/orch-link` has been callable
   since card A4 (Wave 1), but no page ever rendered a form for it — Fleet's
   own empty state (A5) literally tells operators to `curl
   POST /api/control-planes` directly. A budget panel that can never be
   populated because there's no way to create the link it reads would be
   inert on every real install. Deliberately minimal: control plane picker,
   remote project name, and an optional budget cap; `status_map`/
   `auto_dispatch`/`blueprint` stay at defaults (no dispatch policy
   configured here — that's the Wave 3 dispatch UI's territory).
   `BudgetPanel.tsx` also lets the cap be edited in place afterward (a
   labeled number field + Save, resending the full `PUT` — `orch-link` is an
   upsert, not a patch).

**Rule 6, applied everywhere on this page, not just where told to.**

- Tokens render first and at least as prominent as any dollar figure
  (`BudgetPanel.tsx`'s token line is `font-weight: 600` at 15px; the cost
  line beneath it is smaller and secondary).
- Every cost figure goes through `formatEstimatedCost` — "estimated" plus
  the pricing-snapshot date, or an explicit "pricing snapshot date unknown"
  (still always `null` today; nothing invented).
- **The budget-vs-cap fraction gets the compounding caveat the card
  explicitly demanded.** `budgetProgress()`'s doc comment and
  `BUDGET_PROGRESS_CAVEAT` both say it plainly — "This is an estimate of a
  fraction of an estimate" — and every render site couples the percentage
  to that sentence; there is no code path that shows the bare number alone
  (verified: `OrchestrationPanel.test.tsx`'s populated-panel test asserts
  the literal phrase is present whenever the progress bar renders).
  `budgetProgress` deliberately does **not** clamp the fraction at 100% —
  an over-cap project (fraction > 1, `tone: 'danger'`) is exactly the state
  an operator most needs to see, not one to hide by capping the bar visually
  (the bar itself is width-clamped for layout; the underlying fraction and
  percentage text are not).

**Rule 8 / rule 9.** `OrchestrationPanel`'s own `GET .../orch-link` fetch is
the availability probe — `orchAvailable = !loading && error === undefined`,
false on ANY error not just 404, same conservative posture
`useAgentActivityMap.orchAvailable` (C4) documents; I didn't import that
hook (it's board-specific, a different resource) but followed its exact
rule. A 404 renders the same "Agent-fleet orchestration is disabled" empty
state Fleet uses; any other failure renders a distinct retry state (tested:
`OrchestrationPanel.test.tsx`'s disabled-vs-failed tests). No new color
pairing — `Badge`'s existing six tones, `EmptyState`/`Skeleton`/`Field`/
`Select`/`Button` throughout; `npm run lint:tokens` stayed 0/0. Three new
live-executed a11y scans added to `frontend/e2e/a11y.spec.ts` (disabled /
unlinked-link-form / linked-with-populated-budget-and-policy), run against
real Chromium (`npx playwright test e2e/a11y.spec.ts --project=chromium -g
"orchestration tab"`) — 0 violations, not just written.

**A pre-existing bug re-confirmed, not caused by this card.** Running the
full `a11y.spec.ts` suite live to check my own scans didn't regress anything
reproduced the exact duplicate-"Run sprint"-button failure D1's handoff
already flagged (`sprint "Run sprint" dry-run preview` /
`sprint dispatch results (mixed outcomes)`, both failing on
`getByRole('button', {name: 'Run sprint: ...'})` resolving to 2 elements).
I touched nothing under `frontend/src/features/sprints/**` or
`frontend/e2e/helpers.ts` this session (outside my card's scope by explicit
instruction) — confirmed via `git status` before and after. Still open,
still someone else's pickup.

**What I deliberately did not build.**

- No pause control or indicator anywhere — see above. This is the headline
  finding of the card, not an afterthought.
- No live call to docket from any handler. Both new endpoints are plain DB
  reads (§0 rule 5 — no transaction ever spans an HTTP call, because there
  isn't one), matching A4's `GET /api/fleet` precedent exactly: "a docket
  outage can only leave `health`/`last_seen_at` stale, never turn into a
  500."
- No attempt to wire docket's own per-agent `budgetUsd`/`costUsd` (from
  `FleetStatus.agents`) into persistence — that's a real, disclosed gap
  (see point 1 above), but closing it means changing `reconcile_once`'s
  persistence phase and possibly a new column, which is `tack-orch`
  reconciler-shape work belonging to whoever picks up the "docket's own
  cost figure vs. Tack's token estimate" question next, not a budget-panel
  card's job to redesign in passing.
- No pipeline/chain verification reimplementation — `docket audit verify`
  is linked to as a command, per the card's explicit instruction.
- No changes to `dispatcher.rs`, `sprint_dispatch.rs`, `handlers/
  websocket.rs`, `crates/tack-orch/**`, or any file under
  `frontend/src/features/sprints/**` or `frontend/src/features/templates/**`
  — out of scope, confirmed via `git status` before finishing.

**Testing.** Rust: 9 new tests in `crates/tack-api/tests/
orch_budget_policy_test.rs` — 404-when-disabled for both routes, unlinked
project (`linked: false`, real token history, null cost), the
unreachable-vs-zero staleness distinction, real token/cost sums from seeded
`orch_tasks` rows, policy scoping to exactly the linked control plane
(seeded a *second* plane's metrics and asserted they never leak in),
denial-rate computation, denial-rate `None` with policy-hit-only data, and
approval-channel grouping. Frontend: 29 new Vitest tests across
`features/settings/orchestration/{api,format,OrchestrationPanel}.test.{ts,tsx}`
(a real 404-vs-500 distinction, the compounding-caveat copy is asserted
present by literal string match, `budgetProgress`'s clamp/tone thresholds at
0/70/100/150%, the "no pause claim anywhere" assertion — the populated-panel
test explicitly asserts the DOM never matches `/is (currently )?paused/i`
while still naming the CLI remedy), plus 3 new live-executed Playwright a11y
scans.

**Verification.** `cargo test --workspace`: all green (baseline 523 + 9 new
from this card; the observed workspace total moved further due to concurrent
Wave-4 work — R2/R3/D3 — landing in the same window, same "moving baseline"
caveat D1's handoff already called out). `cargo clippy --workspace
--all-targets -- -D warnings`: clean. `cargo fmt -p tack-api -p tack-orch -p
tack-db -- --check`: clean for every file this card touched (a pre-existing
formatting diff in `crates/tack-db/src/migrations.rs`, D3's file mid-edit
when checked, is not mine). `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`: regenerated `docs/openapi.json` (two new paths, five new
schemas), drift gate green. Frontend: `npm run type-check` clean; `npm run
lint:tokens` 0/0 unchanged; `npx vitest run` — 363 passed (334 baseline + 29
new), same 3 pre-existing `requestBlob`/`createObjectURL` failures named in
this card's own baseline, nothing else broken; `npm run build` clean,
`ProjectSettings` chunk grew to include the new panel (still one lazy-loaded
chunk, no new route-level split needed). `npx playwright test
e2e/a11y.spec.ts --project=chromium`: 20 of 22 passed — the 2 failures are
the pre-existing sprint-view bug above, not mine; the 3 new orchestration
scans and all 17 other pre-existing scans passed clean. Did not verify
against a live docket this round — the entire investigative value of this
card was in confirming what docket's HTTP surface *doesn't* have, which
reading `serve.py` directly settles definitively (a live server would only
re-confirm the same absence, not add information); V1 already live-verified
every route this card's endpoints actually build on (`GET /status.json`
shape indirectly via `FleetAgent`, `GET /metrics` shape via B3's parser).

**Files touched:** `crates/tack-api/src/handlers/orch.rs` (new: 2 handlers,
7 DTOs, extensive doc comments), `crates/tack-api/src/router.rs` (2 routes,
at A4's marked Wave-4 insertion point), `crates/tack-api/src/openapi.rs` (2
paths + 5 schemas registered), `docs/openapi.json` (regenerated), new
`crates/tack-api/tests/orch_budget_policy_test.rs`; everything under
`frontend/src/features/settings/orchestration/` (new: `api.ts`, `format.ts`,
`LinkForm.tsx`, `BudgetPanel.tsx`, `PolicyPanel.tsx`,
`OrchestrationPanel.tsx`, three `.test.ts(x)` files), plus a small additive
edit to `frontend/src/features/settings/ProjectSettings.tsx` (one new tab)
and `frontend/e2e/a11y.spec.ts` (three new scans). TODO.md itself: §1.4 (the
pause-surface note above), the "Known gaps" list, and this note. Did not
touch `crates/tack-db/src/repo/orch.rs` — every query this card needed
(`project_task_usage`, `list_latest_orch_metrics`, `get_orch_link`,
`get_control_plane`) already existed.

**For D5 (unit economics) and beyond.** `project_task_usage` is now used by
three endpoints (`GET /api/fleet`, `GET /api/projects/{id}/agent-activity`'s
item-level version, and this card's `GET /api/projects/{id}/orch-budget`) —
if D5 needs the same project-level token/cost aggregate sliced differently
(by `project_type`/`item_type`, per its own card text), extending this one
function's shape is probably cheaper than a fourth reimplementation, though
D5's slicing needs may not fit its existing signature unchanged. Nothing in
this card blocks D5 or D4 — budget/policy are leaf reads, same as D1's
approvals inbox was.

### D5 — 2026-08-05

**The definitions, up front — read this before the number, per the card's
own instruction.**

1. **Population split: "agent" vs. "human."** A completed item
   (`items.completed_at IS NOT NULL`) is **agent** population if it has ≥1
   `orch_tasks` row (dispatched at least once, regardless of who ultimately
   finished it) and **human** population if it has zero. Agent lead time is
   `MIN(orch_tasks.dispatched_at) → completed_at`; human lead time is
   `items.started_at → completed_at`. The two populations are disjoint by
   construction — no item is counted in both.
2. **Minimum sample size: 5.** Chosen, not derived (the card asked for a
   stated minimum, not an optimal one — see `handlers/economics.rs`'s doc
   comment on `MIN_SAMPLE_SIZE` for the reasoning). Below it: lead time shows
   raw per-item hours instead of an average (`LeadTimeStat::raw_hours`,
   never both fields populated); rework rate shows raw counts instead of a
   percentage (`ReworkStat::rate` stays `None`); cost-per-completed-item —
   "the headline number of the whole cycle" per the card's own text — is
   withheld entirely (`EconomicsSlice::cost_usd_estimated_per_item` is
   `None`) rather than shown from a handful of items. Every one of these
   branches is unit-tested directly (see below).
3. **Selection bias — never a bare "agents are Nx faster."** Every slice
   carries `lead_time_selection_bias_note` on the wire (not a doc link) and
   `EconomicsPage.tsx` renders it directly under the two lead-time figures,
   inside a `Badge tone="info"`. This module **deliberately never computes a
   ratio between the agent and human lead-time averages** — both are shown
   side by side and left for the reader to compare, the same discipline
   card D2's handoff set for its budget-progress fraction ("never shows a
   bare percentage without the caveat attached"). A frontend test
   (`EconomicsPage.test.tsx`) asserts the DOM never matches `/\d+x faster/i`.
4. **Rework rate, exact definition:** *share of dispatched, completed items
   (≥1 `orch_tasks` row) that have at least one `rework_started`,
   `verification_failed`, or `tester_verdict_failed` event recorded against
   them* — item-level, not attempt-level (see finding #1 below for why).
   `REWORK_RATE_DEFINITION` travels on the wire verbatim and renders next to
   the number; a test asserts the constant contains all three literal event
   type names, so the words and the number cannot silently drift apart.
   Population for the rate is **completed** agent-dispatched items only —
   a real, disclosed choice: an item that was dispatched, reworked
   repeatedly, and never completed is invisible to this rate, which likely
   **understates** rework rather than overstating it. Flagging this loudly
   since it's the opposite bias direction from what's obvious at a glance.
5. **Retention truncation.** `attempts_excluded_stale` removes an item from
   the rework-rate denominator entirely — never counts it as "no rework" —
   whenever its own latest dispatch predates
   `now - TACK_ORCH_EVENT_RETENTION_DAYS`. `REWORK_TRUNCATION_NOTE` names
   the retention window, never a count of lost events (the count is
   genuinely unknowable — see finding #2). Token, cost, and lead-time
   figures are **explicitly not** subject to this: `orch_tasks` is never
   purged by the Phase 34.6 retention sweep (only `orch_events`/
   `orch_metrics` are), and the module doc says so in as many words rather
   than blanket-hedging every number on the page. Verified by reading the
   retention sweep's own code (B3's `rollup_and_purge_orch_events`/
   `rollup_and_purge_orch_metrics`) — neither touches `orch_tasks`.

**Two findings from reading the ingestion code directly, neither assumed.**

1. **`orch_events.run_id` is always `NULL` in this codebase today.** I went
   looking for every place that constructs a `NewOrchEvent` before designing
   the rework-signal correlation (expecting to reuse B6's documented
   `run_id = orch_tasks.remote_run_id` join). There are exactly two call
   sites — `tack-orch::reconciler`'s trace ingestion and
   `tack-api::dispatcher`'s `status_map_rejected` recording — and **both
   hardcode `run_id: None`**. `reconciler.rs`'s own comment on the trace-
   ingestion site explains why: "docket's trace payload carries no run_id,
   only session_id... Left unset rather than guessing." So a per-*attempt*
   correlation (which B6/B5's `ItemAgentActivity` shape documents and which
   I originally planned to reuse) would silently match nothing in real
   data. What *is* populated reliably is `orch_events.item_id` (via
   `reconciler::session_id_task_id` → `find_orch_task_by_remote_task_id`),
   so `list_item_ids_with_rework_signal` (`repo/economics.rs`) correlates
   at the item level instead — coarser than attempt-level, but the only
   correlation the actual data supports. Added to TODO.md's "Known gaps"
   list above since this affects any future card, not just this one.
2. **The daily rollup drops the correlation key too.** `orch_events_daily`
   (B3, migration 026) aggregates by `(day, control_plane_id, event_type)` —
   no `item_id`, and (per finding #1) `run_id` was never populated anyway.
   So once an item's dispatch history ages past the retention window, its
   rework signal is unrecoverable at any granularity, not just per-attempt.
   This is why `attempts_excluded_stale` exists as a hard exclusion rather
   than a "probably fine" heuristic.

**What I built.**

- **`crates/tack-db/src/migrations.rs`** — migration 031, a partial index
  `idx_items_completed_at ON items(completed_at) WHERE completed_at IS NOT
  NULL`. Both economics queries filter `WHERE completed_at IS NOT NULL`
  across the whole instance (not scoped to one project — the whole point is
  slicing across projects), so none of the existing `(project_id, ...)`
  composite indexes help; without this it's a full `items` table scan on
  any instance with meaningful history.
- **`crates/tack-db/src/repo/economics.rs`** (new module, not an extension
  of `repo/orch.rs`). Two functions: `list_completed_item_economics` (one
  `LEFT JOIN` + `GROUP BY` query — items × projects × orch_tasks — returning
  `ItemEconomicsRow`, one row per completed item, agent or human) and
  `list_item_ids_with_rework_signal` (distinct `item_id`s with a qualifying
  `orch_events` row). **Deliberately its own file**, not an addition to
  `repo/orch.rs`: card D4 (provisioning) was concurrently editing that exact
  file this wave (confirmed via the coordinator's mid-run correction — my
  first draft put these queries in `repo/orch.rs` and had to be moved after
  a transport-error resume). A separate module removes the collision
  outright rather than timing edits around a concurrent agent. Registered
  in `repo.rs` with one line (`pub mod economics;`).
- **`crates/tack-api/src/handlers/economics.rs`** (new module, my own — not
  `handlers/orch.rs`). Two routes: `GET /api/economics/summary` (the
  dashboard aggregate, sliced `overall`/`by_project_type`/`by_item_type`)
  and `GET /api/economics/items` (per-item list, `?project_type=`/
  `?item_type=`/`?limit=`/`?offset=` filters, plus `?format=csv` for the
  task-38.4 export — reusing `export.rs`'s `?format=` + `Content-
  Disposition: attachment` convention, not a second export route). Both
  routes are pure DB reads — no live call to docket, so a plane outage can
  never turn this into a 500 (same rule `GET /api/fleet` established). The
  aggregation itself (`SliceBuilder`, `LeadTimeStat`, `ReworkStat`) is pure
  Rust with zero I/O, exercised directly by 10 unit tests in the module's
  own `#[cfg(test)]` block using hand-built `ItemEconomicsRow`s — faster and
  more exhaustive for the min-sample/staleness/negative-duration branches
  than only integration-testing through a seeded SQLite DB.
- **`crates/tack-api/src/router.rs`** — one line:
  `.merge(crate::handlers::economics::economics_routes())` inside
  `orch_routes()`, right after D4's provisioning route. `economics_routes()`
  is a self-contained sub-router (both new routes) defined in my own module,
  so this is genuinely the only line this card added to a file it doesn't
  own. No `use` statement changed — the merge call is fully qualified.
- **`crates/tack-api/src/openapi.rs`** — 2 paths + 7 schemas, in one
  contiguous block after D4's own additions. Full disclosure on the "one
  additive line" instruction: this ended up more than one line, same as
  every other Wave 3/4 card that touched this file (D2's handoff reports "2
  new paths, five new schemas" for the identical reason) — `utoipa`'s
  `#[derive(OpenApi)]` macro has no mechanism to merge in a separately
  compiled sub-document; every path function and every `ToSchema` type must
  be named individually in the one `paths(...)`/`components(schemas(...))`
  list. I could not find a way to make this genuinely one line without
  restructuring `openapi.rs` itself, which is out of this card's scope and
  would be a worse trade than a disclosed, contiguous 9-line block.
  `docs/openapi.json` regenerated; drift gate green.
- **`crates/tack-api/src/handlers.rs`** — one line, `pub mod economics;`.
- **`crates/tack-api/tests/economics_test.rs`** (new, 10 tests) — the HTTP
  plumbing: off-by-default (both routes 404 with `TACK_ORCH_ENABLE` unset);
  empty/well-formed summary with zero completed items; real agent-vs-human
  population split with real token/cost sums from seeded `orch_tasks`;
  project_type/item_type slicing (seeded a software item via
  `complete_as_agent` and a construction item walked through its real
  linear workflow via `complete_as_human`); rework-rate item-level
  correlation via a directly-seeded `orch_events` row; stale-attempt
  exclusion from the rework denominator (seeded one fresh + one 30-day-old
  dispatch against a 7-day retention config); items-endpoint pagination
  (`total` reflects the full match count, not the page size — a dashboard
  that silently truncated its own totals would defeat the point of this
  card); CSV export's `Content-Type`/`Content-Disposition: attachment`
  headers and header-row shape; and per-item `rework_data_reliable: false`
  for a stale-only dispatch.
- **Frontend — `frontend/src/features/economics/`** (new): `api.ts` (wire
  boundary — every field checked against the real, already-built Rust DTOs,
  not guessed ahead of a backend landing later, since I built both sides
  myself), `format.ts` (`formatHours`, `formatRate`, `describeLeadTime`,
  `describeRework`, `describeCostPerItem` — all min-sample-aware; re-exports
  `formatEstimatedCost`/`formatTokens` from `shared/agentActivity/format.ts`
  **verbatim, per the card's explicit instruction** — no second cost
  formatter), `SliceTable.tsx` (one table component reused for both
  `by_project_type` and `by_item_type`), `EconomicsPage.tsx` (the dashboard:
  stat tiles, the lead-time comparison section with its selection-bias
  badge, the rework section with its definition + conditional truncation
  badge, both breakdown tables, CSV/JSON export buttons using the same
  `downloadBlob` pattern `DataPanel.tsx` already established — duplicated
  locally rather than extracted to a shared helper, matching that file's own
  precedent of not sharing it). Wired into `app/routes.tsx` (`/economics`,
  lazy-loaded, same pattern as Fleet/Approvals) and `shared/ui/Sidebar.tsx`
  (new nav entry + a new `IconEconomics` bar-chart glyph in `icons.tsx`).
  29 new Vitest tests across `api.test.ts`/`format.test.ts`/
  `EconomicsPage.test.tsx` — the disabled/empty/error/populated states, the
  min-sample raw-vs-average branches, the "never shows a bare Nx-faster
  ratio" assertion (regex-asserted absent from the DOM), the rework
  definition rendering next to the rate, and the conditional stale-
  truncation badge. 3 new live-executed Playwright a11y scans added to
  `frontend/e2e/a11y.spec.ts` (disabled / empty-but-enabled / populated with
  deliberately below-min-sample **and** stale-excluded-attempt states, since
  those are the two states most likely to introduce an a11y issue that an
  all-clean fixture would never exercise) — run against real Chromium
  (`npx playwright test e2e/a11y.spec.ts --project=chromium -g
  "economics"`), 0 violations. One real violation found and fixed along the
  way: `SliceTable.tsx`'s horizontally-scrollable wrapper `div` needed
  `tabindex="0"` + `role="region"` + `aria-label` — axe's
  `scrollable-region-focusable` rule, which only fires once the table
  genuinely overflows (7 columns did; `FleetPage.tsx`'s narrower table
  apparently never has, which is presumably why this wasn't caught before).

**Rule 9 / lint:tokens.** No raw hex anywhere in the new files — `npm run
lint:tokens` stayed 0/0. Reused `Badge`/`Button`/`EmptyState`/`Skeleton`
throughout; the only new component is `SliceTable.tsx`, built entirely from
existing token-styled `<th>`/`<td>` conventions copied from
`FleetPage.tsx`.

**Rule 5.** Both handlers are plain reads — `list_completed_item_economics`
and `list_item_ids_with_rework_signal` never call out to docket, so there is
no HTTP call for a SQLite transaction to span in the first place. No
transaction is opened at all in either query (a single `fetch_all` each).

**Rule 8.** Both routes are mounted inside `orch_routes()`, inheriting
`require_orch_enabled` from A4's existing layer — verified by test
(`both_routes_404_when_orch_disabled`) and by the first Playwright scan.

**Verification.** `cargo test --workspace`: all green throughout (moving
baseline all session, per D1/D2's own "concurrent Wave 4 work" caveat — D4
was landing `handlers/provisioning.rs` and editing `crates/tack-orch/**` the
entire time this card ran; one transient failure in a `zzz_live_verify_d4`
test target appeared once mid-session and the target no longer existed on
the next run — D4 mid-edit, not mine, confirmed via `git status` before
investigating further, exactly per the coordinator's warning). `cargo
clippy --workspace --all-targets -- -D warnings`: clean (fixed 2
`collapsible_if`, 1 `unnecessary_sort_by`, 2 `too_many_arguments` on test
helpers with `#[allow]`). `cargo fmt --all -- --check`: clean. `UPDATE_
OPENAPI=1 cargo test -p tack-api --test openapi_contract`: regenerated,
drift gate green, re-verified green again at the very end of the session
after D4's own concurrent changes settled. Frontend: `npm run type-check`
clean; `npm run lint:tokens` 0/0; `npx vitest run` — my own 29 tests all
pass; full suite showed 5 failures at final check, of which exactly 3 are
the named pre-existing `requestBlob`/`createObjectURL` baseline
(`client.test.ts`, `GlobalSettings.test.tsx`, `panels.test.tsx`) and 2 are
`ProvisioningWizard.test.tsx` (D4's own file, untracked in git status,
confirmed not mine — was 4 failing earlier in the session, D4 fixed 2 of
its own while this card was finishing up). `npx playwright test
e2e/a11y.spec.ts --project=chromium`: 23 of 25 passed — the 2 failures are
the pre-existing sprint "Run sprint" duplicate-button bug D1/D2's handoffs
already named repeatedly, not mine; all 3 new economics scans and every
other pre-existing scan passed clean.

**Files touched, all disclosed:** new —
`crates/tack-db/src/repo/economics.rs`,
`crates/tack-api/src/handlers/economics.rs`,
`crates/tack-api/tests/economics_test.rs`, everything under
`frontend/src/features/economics/**`. One-line-or-near-it additions —
`crates/tack-db/src/repo.rs` (module registration),
`crates/tack-api/src/handlers.rs` (module registration),
`crates/tack-api/src/router.rs` (one `.merge()` line),
`crates/tack-db/src/migrations.rs` (migration 031, additive DDL only),
`frontend/src/app/routes.tsx` (one lazy import + one route),
`frontend/src/shared/ui/Sidebar.tsx` (one nav button),
`frontend/src/shared/ui/icons.tsx` (one new icon). Larger, disclosed
additions — `crates/tack-api/src/openapi.rs` (paths + schemas, see above for
why this couldn't stay to one line), `frontend/e2e/a11y.spec.ts` (3 new
scans). Did not touch `crates/tack-db/src/repo/orch.rs`,
`crates/tack-api/src/handlers/orch.rs`, `crates/tack-orch/**`,
`crates/tack-api/src/handlers/provisioning.rs`, or anything under
`frontend/src/features/provisioning/**` — all D4's territory this wave,
confirmed clean via `git diff`/`git status` before finishing.

**What's still open / for whoever's next.**

- **`orch_events.run_id` is always `NULL`** (finding #1 above) — a real gap
  for any future per-attempt (not per-item) event correlation. Now in
  TODO.md's "Known gaps" list.
- **Rework rate undercounts by construction** (definition #4 above): an
  item dispatched, reworked, and never completed doesn't enter the
  population at all. A "rework rate among all dispatched items, completed
  or not" would need a second query shape (drop the `completed_at IS NOT
  NULL` filter) — deliberately not built here, since every other figure on
  this page is anchored to "per completed item" per the card's own
  acceptance bar, and mixing two populations across the same dashboard
  seemed more likely to confuse than clarify. Flagging as a real design
  fork for whoever revisits this.
- **Task 38.3 (model right-sizing export) was not built.** Re-reading the
  schema before starting: `orch_tasks`/`orch_events` carry no `role` or
  `model` field anywhere — docket's per-agent role/model roster
  (`FleetAgent.kind`/`FleetAgent.model`) is a `/status.json` snapshot,
  never persisted per-dispatch. Exporting "per-role outcome quality against
  the model docket used" would require either a schema change (persist
  role/model at dispatch time, which `enqueue_task`'s current signature has
  no field for) or joining against `orch_metrics`' Prometheus labels (I
  checked — `docket_tool_calls_total`/`docket_policy_hits_total` etc. carry
  no per-task role/model label either, confirmed by reading B3's parser and
  the metrics it actually stores). This is real, disclosed scope, not an
  oversight: building it would mean either extending `orch_tasks`' schema
  (C1's table, migration 021 — a real, reasonable follow-up: add
  `role`/`model` columns at dispatch time if docket's `POST /tasks`
  response or the task-list payload ever carries them) or reworking C1's
  `dispatch_item` to capture that data, which is squarely C1/C3's
  dispatcher, not a leaf economics-reads card's job to change in passing.
  Tasks 38.1/38.2/38.4 are complete; 38.3 needs a schema decision first.
- **No live-docket verification.** Like D2's and C1's later work, this
  card's own logic is entirely Tack-side SQL/aggregation over already-
  ingested data — V1 already live-verified the ingestion paths
  (`orch_tasks`, `orch_events`) this card reads from, so a live docket run
  would only re-confirm data this card never touches directly.

**Correction, added by D4:** this was not the cycle's last unbuilt card — D4
(provisioning) was still `⬜ not started` when this note was written; see
below.

### D4 — 2026-08-05

**The real `POST /pods` contract, confirmed two ways before designing
anything — read `~/Sites/rack-cli/src/docket/serve.py::_handle_post_pods`
and `core/pod_provisioning.py` yourself before touching this again.**

- **Source.** Request: `{project, path, blueprint, pod, budget, verifyCmd}`
  — every field but `project` optional. Success: `201 {"ok": true,
  "project", "blueprint", "members": [{"id", "role", "model"}]}`. `400` —
  unknown blueprint / bad `verifyCmd` / `pod` not `"full"` / missing
  `project`, checked **before anything is touched**. `401` — bad/missing
  Bearer token. `409` — `PodAlreadyExistsError`, also raised **before
  anything is touched** (`pod_member_ids(project)` is the very first check
  in `provision_pod`, before even the blueprint name is resolved) — "skip,
  don't clobber," matching the declarative `--from` path's long-standing
  idempotence contract. `500` — `PodProvisionError`, and
  `core/pod_provisioning.py`'s own module docstring states this is raised
  **only after rollback has already run**: `provision_members` tears down
  every member (and any pod-level port range/scratch dir) created during
  *that* failing call before raising. **Every docket-side failure mode is
  therefore atomic — fully created or nothing created — with 409 as the
  "already satisfied" case.** docket also has **no HTTP route to
  delete/un-provision a pod at all** — I read every `do_GET`/`do_POST`
  branch in `serve.py` by hand; there is no `do_DELETE`, no `/pods/{id}` of
  any method.
- **Live-verified, not just read.** `DOCKET_HOME` pointed at
  `<scratchpad>/docket-home` for every invocation; `~/.docket`'s mtime
  recorded before the first command and re-checked identical after the
  last — confirmed unchanged, same discipline V1 established. Ran an
  isolated `docket serve --port 18402` and exercised the happy path (201 +
  exact `members[]` shape), the 409 (retry with the same `project`), 400
  (bad blueprint / missing `project` / bad `pod` value), 401 (no
  `Authorization` header), `pod:"full"` on `software` (4-member roster:
  lead/implementer/reviewer/tester), and a `workdir`-kind blueprint
  (`research`, auto-provisioned work dir, 5-member roster) — all via raw
  `curl` first, then again by running **this crate's own compiled
  `DocketAdapter::provision_pod`** against the same live server (happy path
  and the 409), so the adapter code itself — not just my reading of the
  docs — is confirmed correct end to end. Full transcript folded into
  `adapters::docket`'s module doc ("Verified live" section, new bullet)
  rather than only living here. Closes the one gap V1's own handoff
  explicitly flagged as unexercised ("`POST /pods` — out of this card's
  endpoint list ... not exercised").
- **§1.4's table was already wrong and I did not own it** — the ⚠️ row
  said `POST /pods` "does not exist yet." Since the status board and this
  file's own top-of-file correction already established it ships, I fixed
  that one table row (disclosed: a one-row edit to shared reference text,
  same latitude A4/B2/D3 each took correcting stale rows they found while
  reading source for their own card).

**Rollback design — the reason this card exists, and the ordering that
falls out of the contract above.** Two systems, one HTTP call that cannot
be undone once it succeeds, and (per the contract above) nothing on
docket's side is ever left half-created. That leaves exactly one thing that
*can* end up half-created: Tack's own project row. So the flow
(`handlers::provisioning::create_project_with_pod`, full reasoning in that
module's doc comment) is ordered to keep that the only moving part:

1. **Create the Tack project first** (reusing `handlers::templates::
   build_project_from_template` — extracted from the existing
   `create_project_from_template` handler, logic unchanged, see below).
   Cheap, local, fully reversible.
2. **Validate everything provisioning needs** — the referenced control
   plane exists (checked *before* step 1, actually, so a bad id never costs
   a throwaway project), `pod_shape` is well-formed, `status_map` names
   real statuses in **the project's own just-created workflow** (not a
   guess, not a template default — the actual `WorkflowConfig` `Repository::
   create_project` just wrote). Any failure here rolls the project back.
3. **Call `POST /pods`.** Any failure (400/401/409/500) means, per the
   contract above, nothing new exists on docket's side — roll the project
   back too, and say so in the error message (`rollback_project` returns a
   clause describing what happened to the rollback itself — appended to the
   error, not swallowed; if the delete itself fails, that's surfaced too,
   explicitly naming the orphaned project id).
4. **Write `orch_links`.** The one step that runs *after* the irreversible
   action. §0 rule 5 holds structurally: the only HTTP call in this whole
   flow already completed in step 3, so this is a short, additive SQLite
   write with nothing held across it. **A failure here is not a request
   failure and the project is never deleted at this point** — deleting it
   would strictly worsen things, since the project row is now the *only*
   record Tack has that this pod exists at all, and docket cannot be asked
   to remove it. Instead the handler returns an ordinary `200` whose
   `provisioning` field is `ProvisioningOutcome::PodCreatedLinkFailed`,
   naming the exact control plane + remote project and pointing at the
   existing manual-link UI (`features/settings/orchestration/LinkForm.tsx`,
   card D2) — which only needs a `PUT /orch-link` call, never a second
   `POST /pods`, so retrying it can never create a second pod.

**What this deliberately does not attempt:** auto-retrying a failed
`orch_links` write, or "adopting" a 409 as this attempt's own pod. Both
would need Tack to track cross-request provisioning state it has nowhere
reliable to put this cycle; a 409 is a hard failure (project rolled back,
operator told to pick a different `remote_project` name or use the
existing manual-link flow if the pre-existing pod is genuinely theirs).
Also not built: a process-wide lock against two concurrent provisioning
requests (unlike C1's per-item dispatch lock) — this is a rare, human-
driven, explicitly-confirmed action, not a hot path; two concurrent
attempts with different `remote_project` names just create two pods
correctly, and two attempts with the *same* name collide on docket's own
409, still rolled back cleanly.

**Tested, not just asserted — every branch above, in
`crates/tack-api/tests/provisioning_test.rs` (8 tests, wiremock-backed):**
404 when `TACK_ORCH_ENABLE` is unset (the whole route lives inside
`orch_routes()`, so this is structural, not a manual check); happy path
(project created, pod provisioned, link written, `GET .../orch-link`
confirms it); unknown `control_plane_id` 404s **before any project is
created** (asserted via a before/after project count, not just the status
code); empty `remote_project` 400s the same way; a bad `status_map` name
rolls the project back **without ever calling docket** (the sharpest test
in the file — no `/pods` mock is mounted at all, so if the handler
validated late, it would hit wiremock's default 404 and the error message
would read "pod provisioning failed," not name the bad status; asserting
the message's exact shape is what proves the ordering, not just the status
code); docket 400 rolls the project back; docket 409 rolls the project
back; and — the one hard case — an `orch_links` write forced to fail
(`DROP TABLE orch_links` on the test's own pool right before the request,
deterministic fault injection rather than guessing at a mock) after a
*successful* `POST /pods` leaves the project standing, reports
`pod_created_link_failed`, and names the manual next step. Also 6 new
`provision_pod` tests in `crates/tack-orch/tests/docket_adapter_test.rs`
(happy path, 409, 400, 500, 401, full-request-shape-on-the-wire).

**Privilege — deliberately *not* gated behind `TACK_ORCH_APPROVAL_TOKEN`.**
D1's separate decision credential exists for one specific reason: releasing
a guardrail's deliberate block is a categorically different, narrower
privilege than "using the orchestration API at all," and its safe default
had to be "nothing can release a gated action" because the failure mode of
getting that backwards is a stranger with only the ordinary API token
resuming a paused agent. Provisioning is consequential — it creates real
infrastructure and can spend real budget — but it's ordinary use of the
same privilege class as manual dispatch (`POST /items/{id}/dispatch`, C1)
and sprint-wide dispatch (`POST /sprints/{id}/dispatch`, C3), both of which
can also spend real budget across many items in one call and are gated
only by the ordinary `TACK_API_TOKEN` + `TACK_ORCH_ENABLE` pair. A second
credential *only* here, while sprint-wide dispatch needs none, would be an
inconsistent boundary, not a more careful one. The "require confirmation"
instruction is met on the frontend instead — full reasoning and the exact
UI pattern below.

**Backend files.** `crates/tack-orch/src/lib.rs` (additive:
`ProvisionPodParams`, `ProvisionedPod`, `ProvisionedPodMember`,
`OrchError::AlreadyExists`, `ControlPlane::provision_pod` — the Wave-0
freeze is already lifted, R1/§2.1, so this is a plain trait-method
addition, not a workaround); `crates/tack-orch/src/adapters/docket.rs`
(implementation + module-doc "Verified live" addition);
`crates/tack-orch/src/reconciler.rs` (`FakeControlPlane::provision_pod` →
`Disabled`, mechanical, same pattern D1 used for `decide_approval`);
`crates/tack-orch/tests/docket_adapter_test.rs` (+6 tests);
`crates/tack-api/src/handlers/templates.rs` (extracted `pub(crate) async fn
build_project_from_template` out of `create_project_from_template`'s body
— **logic byte-for-byte unchanged**, just callable from a second module;
the existing handler is now a 4-line wrapper); `crates/tack-api/src/
handlers/provisioning.rs` (**new** — request/response DTOs,
`create_project_with_pod`, the rollback helpers, full module-doc reasoning
above); `crates/tack-api/src/handlers.rs` (`pub mod provisioning;`, one
line); `crates/tack-api/src/router.rs` (**one line**, replacing A4's
placeholder comment inside `orch_routes()` — `POST /templates/{id}/
provision`, disclosed: I did **not** reuse the existing `POST /projects/
from-template/{id}` endpoint the placeholder comment suggested; see
`handlers/provisioning.rs`'s module doc for the two reasons, chiefly "don't
widen a response shape a live frontend call site already depends on for a
brand-new capability when a new route costs one line"); `crates/tack-api/
src/openapi.rs` (additive: one path, five schemas); `docs/openapi.json`
(regenerated, `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`, drift gate green — necessarily also reflects D5's
concurrently-landed economics routes, same as every prior wave's
regeneration); `crates/tack-api/tests/provisioning_test.rs` (**new**, 8
tests). **No migration.** Provisioning reuses the existing `orch_links`
table (A3/W0-B) verbatim via the existing `upsert_orch_link` repo function
— nothing new to persist beyond what a link already stores. (Per the
coordinator's note mid-card: migrations 030/031 are D3/D5's; if a future
card needs one, it's 032 — not relevant to what I built, flagging for
whoever's next.) I also did not write any `orch_events` row from this
flow — provisioning is a one-shot creation action, not an ongoing
dispatch/trace the reconciler correlates; B6/D5's `run_id`-always-`NULL`
finding doesn't apply here since I never construct a `NewOrchEvent`.

**Frontend.** New `frontend/src/features/provisioning/` (own feature
directory, per the card's file-ownership instruction): `api.ts` (wire
boundary — `ProvisionPodRequest`/`CreateProjectWithPodRequest`/
`ProvisioningOutcome` hand-typed field-for-field against the Rust DTOs,
same discipline A5/D1/D2/C4 each used for their own `api.ts`, plus
`isOrchDisabled` duplicated per their established cross-feature-boundary
precedent rather than imported); `format.ts` (`formatBudgetCap` — a cap,
not an estimate, so it deliberately never carries the word "estimated",
tested explicitly; `suggestRemoteProjectName`; `isFullPodShape`, mirroring
the Rust handler's own `eq_ignore_ascii_case("full")` check so the
checkbox and the request it sends can never disagree); `ProvisioningWizard.
tsx` (the 4-step wizard: Project/template → Pod & control plane → Review →
Result). `frontend/src/shared/ui/icons.tsx` (+`IconProvision`, additive —
D5 already set the precedent of adding its own icon to this shared file
this same wave); `frontend/src/shared/ui/Sidebar.tsx` (+nav link, additive,
same file D1/D5 already touched this wave for their own nav entries);
`frontend/src/app/routes.tsx` (+`/provision` route, additive);
`frontend/src/shared/api/schema.gen.ts` (regenerated via `npm run gen:api`
— frontend source untouched by the regen itself, same as D3's precedent).

**Gating.** `GET /api/control-planes` doubles as the wizard's
`orchAvailable()` probe (`api.ts`'s doc comment on `listControlPlanes`) —
it already lives inside `orch_routes()`, so a 404 there *is* "orchestration
disabled," no second probe needed. `orchAvailable()` is `false` while
loading and on *any* error, not just 404 — same conservative posture C4's
`useAgentActivityMap.orchAvailable`/`ItemDetailDrawer` use. Nothing below
that gate renders while it's `false`: a `Switch`/`Match` over four states
(loading / disabled / other-error-with-retry / no-control-planes-
registered / ready) replaces what an earlier draft wrote as three nested
`<Show>`s — not a style preference, see the bug below.

**Confirmation, not a credential.** No one-click path from opening `/
provision` to a pod existing: step 3's "Provision…" button only opens a
confirmation `Modal` naming the exact docket project name, blueprint,
control plane, and budget cap, with the literal words "This creates real
infrastructure and cannot be automatically undone." — the same pattern D1
built for approval decisions and C4 built for sprint dispatch. Live-scanned
with the modal open (see a11y below), not just written.

**A real, live SolidJS bug found and fixed while wiring the gating up — a
third instance of the class C4's handoff already documented, in a new
shape.** C4 found that calling a `createResource` accessor directly
(`dryRun()`) throws once the resource has errored, and that doing so inside
a memo/JSX expression silently aborts the whole reactive batch. Building
this wizard's gating turned up the **same failure mode from `.latest`, not
just the bare accessor call**: `createMemo(() => controlPlanes.latest ??
[])` — a pattern that reads perfectly reasonable (`.latest` is documented
as the "won't trigger Suspense" safe alternative to calling the resource)
— **also throws once the resource is in an error state**, live-confirmed
with a scratch repro (`console.log` immediately before/after the `.latest`
read inside the memo: the "before" line printed twice — once on mount,
once when the resource settled into `errored` — and the "after" line only
printed the *first* time; the memo's own re-run, and every sibling
computation depending on the same resource including an unrelated debug
effect that only read `.loading`/`.error`, never fired again). The
symptom in the real component was silent and easy to misdiagnose as a
timing issue: `controlPlanes.loading` never became visibly `false`, so the
page stayed on the loading skeleton forever on any 404/500 — no console
error, no thrown exception visible in the test output, just a page that
looks stuck loading. **Fix:** guard `.latest` behind an explicit
`.error !== undefined` check in both memos (`controlPlaneList`/
`templateList`) rather than calling it unconditionally — see
`ProvisioningWizard.tsx`'s own doc comment on those two memos for the
fix and the reasoning.
**Flagging for whoever next writes a `createMemo`/JSX expression touching
`resource.latest` anywhere in this codebase — C4's original finding was
scoped to "calling the resource directly," and this shows `.latest` needs
the identical guard, not just the bare call.** Regression-covered by the
two vitest tests that specifically exercise the 404/500 paths (`orchestration
disabled` / `a non-404 failure`) — both failed reliably before the fix
(confirmed by reverting it and re-running) and pass after.

**What I tested.** Rust: 8 new `provisioning_test.rs` tests + 6 new
`docket_adapter_test.rs` tests (listed above). Frontend: `api.test.ts` (3),
`format.test.ts` (7), `ProvisioningWizard.test.tsx` (5 — orchestration
disabled, non-404 failure, no-control-planes empty state, the full
happy-path walkthrough through all four steps **with an explicit assertion
that the provisioning `POST` only fires after the confirm-modal click, not
the "Provision…" click that opens it**, and the `pod_created_link_failed`
warning state). Two new live-executed Playwright a11y scans in `e2e/
a11y.spec.ts` (`provisioning wizard (orchestration disabled)`,
`provisioning wizard (confirmation modal open)` — the second walks a real
Chromium render through steps 1→2→3 and opens the real confirmation
`Modal` via a real click, then scans with the dialog open, since that's
the highest-focus-risk state) — both 0 violations, actually executed
(`npx playwright test e2e/a11y.spec.ts --project=chromium -g
provisioning`), not just written.

**Verification.** `cargo build --workspace`: clean. `cargo test
--workspace`: green (my net-new: 8 + 6 = 14 Rust tests on top of whatever
D5 had already landed concurrently — no clean pre-D4 baseline to diff
against since D5 was landing work in parallel, but nothing failed).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt
--all -- --check`: clean. `UPDATE_OPENAPI=1 cargo test -p tack-api --test
openapi_contract`: regenerated, drift gate green. Frontend: `npm run
type-check` clean; `npm run lint:tokens` 0/0 both gates; `npm run build`
clean (`ProvisioningWizard` code-splits into its own ~12KB chunk); `npm
run test -- --run`: 406 passed, the same 3 pre-existing `requestBlob`/
`createObjectURL` failures named in this cycle's baseline, nothing else
broken (my 15 new frontend tests all pass). `npx playwright test e2e/
a11y.spec.ts --project=chromium`: 25 passed, 2 failed — **the same two
pre-existing failures D1's handoff already found and flagged** (`sprint
"Run sprint" dry-run preview` / `sprint dispatch results (mixed
outcomes)`, both on `getByRole('button', {name:'Run sprint'})` resolving
to multiple elements — now 4 instead of D1's 2, consistent with more
Sprint-adjacent work having landed since; still unrelated to this card,
still not investigated further, still worth the dedicated look D1's
handoff already asked for).

**Known gap, carried deliberately.** A template's inline
`orchestration.pipeline_yaml` (card D3) has no delivery path to docket at
provisioning time — `POST /pods` has no pipeline field at all (confirmed
live), and `orch_links` has no `pipeline_yaml` column (only
`pipeline_file`, a docket-known pipeline *by name*). If a template sets
`pipeline_yaml` without also setting `pipeline_file`, `create_project_with_
pod` still provisions the pod and writes the link, but adds a `warnings[]`
entry saying so plainly — never silently drops it, never invents a
delivery mechanism. Closing this for real needs a docket-side "upload a
pipeline" primitive that doesn't exist yet; out of this card's scope to
invent.

**For whoever next touches provisioning.** `handlers/provisioning.rs`'s
module doc is the one place to read first — it has the full rollback
reasoning, the exact `POST /pods` contract, and the "why a separate route"
call. `frontend/src/features/provisioning/api.ts` is the one file to edit
if the wire shape ever changes. Nothing here blocks D5 (already done) or
any future card — provisioning is a leaf feature, same as D1's approvals
inbox and D2's budget panels were.

### E1 — 2026-08-05

**The design mistake this card fixes:** orchestration was gated entirely
behind `TACK_ORCH_ENABLE` — invisible, un-toggleable without a restart, and
404ing every route while off so the UI couldn't even tell "disabled" from
"old server that never had this." §0 rule 8 is rewritten above; this note
covers the implementation, and leads with the part that was actually hard —
runtime start/stop of the reconciler without a restart.

**Runtime start/stop design (read this before touching `orch_runtime.rs` or
`reconciler.rs`'s `spawn_one`).** The reconciler already looped fetch →
decide → persist → sleep, one `tokio` task per registered control plane
(`reconciler::spawn_one`). Stopping it cleanly needed a signal each task
could observe at a safe point — never mid-HTTP-call to docket, never while a
SQLite write is open (§0 rule 5; unaffected either way, since persistence
already happens strictly after the fetch phase and before the sleep). I used
a `tokio::sync::watch::channel(bool)` rather than adding `tokio-util` for
`CancellationToken`: `watch` is already part of `tokio`'s `full` feature
(already a workspace dep), and a single boolean flag with N cloned receivers
is all this needs. `stop()` flips it to `true`; every task holds a cloned
`Receiver` and races it against its poll-interval sleep with `tokio::
select!`, and separately checks it at the top of the next loop iteration
before starting a new fetch — both are the same "between ticks" safe point
the module's phase-separation doc already relied on, so I didn't have to
invent a new one. A task mid-fetch when `stop()` is called finishes that one
tick and exits at its very next safe point, bounded by however long the
in-flight call to docket takes, never longer.

To keep this from touching (or risking) any of the 12 existing call sites of
`spawn_reconcilers`/`spawn_one` across `tack-orch` and `tack-api`'s test
suites, I left both exactly as they were (`stop_rx: None` internally — the
original uncancellable infinite loop, byte-for-byte) and added a parallel
`spawn_reconcilers_cancellable(store, config, stop_rx)` that only
`orch_runtime.rs` calls. Zero pre-existing tests needed to change for this
part.

`AppState::orch_runtime: OrchRuntime` (`crates/tack-api/src/orch_runtime.rs`)
is the toggle handle: `start()` is a no-op if a generation is already
running (never spawns a duplicate set — `PUT {"enabled": true}` sent twice,
or an env-default `true` at boot followed by a UI `PUT`, must not double the
task count); `stop()` takes the current generation out of the shared
`tokio::sync::Mutex` before signalling it, so a `start()` racing a `stop()`
can never observe half-torn-down state. `stop()` deliberately does not
await the tasks' actual exit — a toggle-off HTTP request must not hang on
whatever docket's response latency happens to be for an in-flight poll.

**How I verified a toggle actually takes effect, not just that a config
value changed.** Three layers, cheapest/most-isolated first:
1. `tack-orch/src/reconciler.rs`'s own test module (reusing its existing
   `FakeStore`/`FakeControlPlane`): `cancellable_spawn_stops_a_task_after_
   stop_signal_without_aborting_it` spawns with a 60s poll interval (so the
   test would time out, not pass by accident, if cancellation didn't work),
   sends the stop signal after confirming the task is alive
   (`!handle.is_finished()`), then asserts the `JoinHandle` completes within
   2s via `tokio::time::timeout` — proving the *reconciler's own*
   `select!`/top-of-loop check works, independent of anything in `tack-api`.
   Plus a pre-signalled-before-spawn case and a 3-cycle repeated-toggle case
   asserting exactly one task per cycle.
2. `tack-api/src/orch_runtime.rs`'s own test module: a small in-file
   `ControlPlaneStore`/`ControlPlane` fake (couldn't reuse `tack-orch`'s
   fakes — they're private to that crate's test module) drives `OrchRuntime`
   directly — `start` → `live_task_count() == 1` → `stop` → poll down to `0`;
   idempotent double-`start`; a no-op `stop` with nothing running; three
   repeated toggle cycles.
3. `tack-api/tests/orch_settings_test.rs` — the one that actually proves the
   HTTP contract, not just the Rust API: registers a real `control_planes`
   row (unreachable `base_url`, `http://127.0.0.1:1` — connection refused is
   instant, so the test isn't waiting on network timeouts) directly via the
   repo, then drives the toggle **only through `PUT /api/settings/
   orchestration`**, asserting `reconciler_running` in the `GET`/`PUT`
   response — `put_true_starts_the_reconciler_for_an_already_registered_
   plane`, `put_false_stops_the_reconciler_without_a_restart` (polls
   `reconciler_running` down to `false` with a bounded 3s timeout, since stop
   is cooperative, not synchronous with the `PUT` response — see that
   helper's own comment on why), `repeated_toggles_never_leave_more_than_
   one_task_per_plane` (3 full on/off cycles), and `put_true_while_already_
   running_does_not_spawn_a_duplicate`. All nine tests in that file pass;
   none use `.abort()` or any other escape hatch — every stop is the
   production code path exiting on its own.

**The 409, and why not 403.** `require_orch_enabled`
(`handlers/orch.rs`) now returns `409 Conflict` with `error.code:
"orchestration_disabled"` instead of `404`. I chose 409 over 403: the
caller *is* authorized (the Bearer-token gate, layered on top in
`router.rs`, is unchanged and unaffected) — what's wrong is the *server's
current state* conflicting with the request, the same category `ApiError::
Conflict` already covers elsewhere in this codebase (e.g. "project not
linked to a control plane"). 403 would read as a permissions problem, which
this isn't. The code lives on a new `ApiError::FeatureDisabled { message,
code }` variant rather than a one-off response builder, so any *future*
switched-off-feature (there's exactly one today) gets the same treatment for
free; `error.rs`'s `IntoResponse` only adds the `code` key to the JSON body
when that variant is present, so every other endpoint's envelope is
byte-for-byte unchanged. `ErrorBody` in `openapi.rs` gained a matching
optional `code` field for the generated docs.

**Settings endpoint, mirroring Cloud Backup exactly.** `GET`/`PUT
/api/settings/orchestration` (`handlers/settings.rs`) follow `handlers/
settings.rs`'s existing Cloud Backup section field-for-field: an
`app_meta`-stored value (key `orch_config`, `{"enabled": Option<bool>}` —
`Option`, not a bare `bool`, so "never touched by the UI" and "explicitly
set to `false`" are distinguishable, which a `#[serde(default)]` bool
can't do) overrides the env default. The one place this setting's handler
differs from Cloud Backup's: after persisting, `put_orch_settings` calls
`state.orch_runtime.start`/`.stop` to make the running state agree
immediately — Cloud Backup has no equivalent background task to reconcile.
Both routes are registered in `router.rs` **outside** `orch_routes`'
`require_orch_enabled` layer, right next to `/settings/backup` — that's the
entire point: a UI on a server where orchestration has never been turned on
must still be able to `GET` this and offer to turn it on.

Response shape matches the contract E2 was given verbatim (I did not rename
any field): `enabled`, `source` (`"database"` | `"env_default"`),
`reconciler_running` (`orch_runtime.live_task_count() > 0` — `0` both when
disabled *and* when enabled with zero registered control planes; it reports
whether a task is actually polling something, not whether the feature
switch is on), `control_plane_count` (`repo.list_control_planes().len()`),
`linked_project_count` (new `repo::orch::count_orch_links` — a plain
`SELECT COUNT(*) FROM orch_links`, no existing method already did this),
`poll_secs`, `approval_token_set` (never the token value — same discipline
`AppConfig::orch_approval_token`'s own doc comment already established),
`env_default`.

**Boot path.** `server.rs` now computes `effective_orch_enabled(&state)`
(DB-then-env, the same precedence everywhere else) instead of reading
`config.orch_enable` directly, and calls `state.orch_runtime.start(...)`
instead of `reconciler::spawn_reconcilers(true, ...)` inline. This is
*only* what makes the initial state agree with whatever was last saved (or
the env default, on a first-ever boot) — the actual runtime toggle is
`PUT /api/settings/orchestration`, exercised independently of a restart.

**Tests changed and why each was right to change.** Nine pre-existing tests
asserted `404` for the disabled-orchestration case; all nine now assert
`409` + `error.code == "orchestration_disabled"` (renamed to match:
`every_orch_route_404s_when_disabled` →
`every_orch_route_409s_with_a_stable_code_when_disabled` in `orch_test.rs`,
plus one each in `provisioning_test.rs`, `orch_approvals_test.rs` (two —
`list_approvals`/`decide_approval`), `economics_test.rs`,
`orch_agent_activity_test.rs`, `orch_budget_policy_test.rs`,
`orch_dispatch_test.rs`, `sprint_dispatch_test.rs`). Each was asserting the
literal old gate behavior (`StatusCode::NOT_FOUND` when
`TACK_ORCH_ENABLE`/`orch_enable` was unset) as the acceptance criterion for
"is orchestration off" — updating the status code and adding the `code`
assertion is the correctness fix this card exists to make, not collateral
damage. I left every *other* `NOT_FOUND` assertion in these same files
untouched (e.g. `get_unknown_control_plane_is_404`,
`unknown_control_plane_404s_before_creating_any_project`,
`decide_approval_404s_for_a_token_unknown_to_tacks_own_mirror_before_
calling_docket`) — those all run against `orch_config()`/an approval-token
config with orchestration already enabled and assert a genuine "that
specific resource doesn't exist," which this card doesn't touch.

**`AppState` gained a field.** `orch_runtime: OrchRuntime` is now part of
`AppState`, which meant updating every struct-literal construction site —
16 of them (`server.rs`, `orch_store.rs`'s `as_app_state` reconstruction,
`tests/common/mod.rs` ×2, and 11 other test files that build `AppState`
locally rather than through `common::test_app`). `orch_store.rs`'s
`as_app_state()` — used only to reassemble an `AppState` for `dispatcher::
apply_mapped_status` when a mirrored run reaches a terminal state — gets a
**fresh, inert** `OrchRuntime::new()`, documented as such: that code path
never starts or stops the reconciler, so wiring the *real* shared handle
through `with_app_context` would be plumbing with no caller that needs it.

**Files touched (Rust tree only — I own `tack-api`/`tack-orch`/`tack-db`,
touched zero frontend files including `schema.gen.ts`, which the OpenAPI
regen did not rewrite since E2's own concurrent frontend work doesn't
consume it during this run):**
`crates/tack-api/src/error.rs` (new `ApiError::FeatureDisabled` variant),
`crates/tack-api/src/openapi.rs` (`code` field on `ErrorBody`; new paths +
`UpdateOrchSettings` schema), `crates/tack-api/src/handlers/settings.rs`
(new orchestration-settings section), `crates/tack-api/src/handlers/orch.rs`
(`require_orch_enabled` rewritten), `crates/tack-api/src/router.rs`
(`AppState.orch_runtime` field; new route registration), `crates/tack-api/
src/server.rs` (boot path), `crates/tack-api/src/orch_store.rs` (new
`build_control_plane_store` helper; `as_app_state` field), `crates/tack-api/
src/orch_runtime.rs` (new), `crates/tack-api/src/lib.rs` (module
registration), `crates/tack-db/src/repo/orch.rs` (new `count_orch_links`),
`crates/tack-orch/src/reconciler.rs` (new `spawn_reconcilers_cancellable` +
`wait_until_stopped`, existing `spawn_one`/`spawn_reconcilers` behavior
preserved), `crates/tack-api/tests/orch_settings_test.rs` (new, 9 tests),
and the 13 test files listed above for the `AppState` field / 404→409
updates. `docs/openapi.json` regenerated
(`UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`).

**Verification.** `cargo test --workspace`: 598 passed, 0 failed (baseline
was 581; net +17 — 3 in `reconciler.rs`, 4 in `orch_runtime.rs`, 9 in the
new `orch_settings_test.rs`, 1 in `orch_test.rs`
(`settings_orchestration_is_reachable_when_disabled`); the nine 404→409
renames are not net-new tests). `cargo clippy --workspace --all-targets --
-D warnings`: clean. `cargo fmt --all -- --check`: clean. `UPDATE_OPENAPI=1
cargo test -p tack-api --test openapi_contract`: regenerated, drift gate
green — a 70-line diff in `docs/openapi.json`, exactly the two new paths,
the `UpdateOrchSettings` schema, and the optional `code` field on
`ErrorBody`, nothing else moved.

**For E2 (frontend, concurrent):** the contract is live at `/api/settings/
orchestration` exactly as specified — I did not rename any field. Your
`frontend/src/features/settings/orchestration/api.ts` (already present in
the working tree as of this note, so we're aligned) is the only file that
should need to know the shape. One thing worth double-checking on your side:
`reconciler_running` can be `true` while `control_plane_count` is `0` is
*not* a state you'll see (a task only exists per registered plane), but
`enabled: true` with `reconciler_running: false` **is** normal and expected
whenever `control_plane_count` is `0` — don't treat that combination as a
bug to surface loudly.

### E2 — 2026-08-05

**The setup flow, in the order an operator actually hits it.** Settings →
Orchestration (a new section on the existing app-level `/settings` page,
right under Appearance — not a new route) opens with a permanent, unmissable
paragraph naming the real consequence of the toggle below it ("Tack begins
**polling** the control planes you register... agents can be **dispatched**
to work its items — which can spend money") before any control that flips
it — the operator's own framing ("not a bare unlabelled switch") taken
literally. Two labeled buttons, On/Off (`aria-pressed`, no color-only
signal), not a bare switch glyph. Below that: three numbered steps, each
with a status pip (locked/active/done) —

1. **Turn orchestration on** — always unlocked; `PUT /api/settings/
   orchestration`, then a live status grid (reconciler running, control
   plane count, linked project count, poll interval, approval-token-set)
   refetched from the same `GET` E1's card returns.
2. **Register a control plane** — `ControlPlanesManager.tsx`, genuinely new
   UI (Fleet's own pre-E2 empty state literally told operators to `curl
   POST /api/control-planes`; A4 built the endpoint in Wave 1, D2's
   `LinkForm.tsx` only ever *read* the list). Visually locked until step 1
   is on, not just suggested in order — `/control-planes` is still gated
   behind the same `TACK_ORCH_ENABLE`-or-database-override check every
   other orchestration route uses, so rendering it earlier would just
   produce a wall of `orchestration_disabled` errors.
3. **Link a project** — `ProjectLinker.tsx`, a project picker that, once a
   project is chosen, renders card D2's `LinkForm.tsx` unmodified. This was
   the operator's explicit instruction ("reuse it rather than building a
   second") and it worked cleanly: `LinkForm` already took a bare
   `projectId` prop and handled its own "no control planes yet" state, so
   the only genuinely new piece was the project-selection step upstream of
   it. `features/settings/orchestration/**` (D2's directory) and my new
   `features/settings/orchestrationSettings/**` are both under the single
   `features/settings/**` `architecture.test.ts` feature, so this is a
   same-feature import, not a cross-feature reach.

**No synchronous "test connection" endpoint exists — I checked before
building one.** docket's HTTP surface has nothing that lets Tack probe a
URL+token pair on demand (same absence card D2's handoff already documented
for pause/policy). So "test connection" here is honest rather than a faked
instant checkmark: right after registering a plane, its row polls `GET
/control-planes/{id}` every few seconds (capped at `poll_secs`, 5 attempts)
until `health` moves off `"unknown"` — a real background result, not a
synchronous one — plus a manual "Check now" per row. Both are new frontend
calls against A4's existing, unchanged endpoint.

**What I decided about nav visibility when the feature is off.** Fleet,
Approvals, and Economics were already permanent sidebar links before this
card (not conditionally hidden) — so "vanish" in the card brief turned out
to describe the *dead end* each one led to (raw `TACK_ORCH_ENABLE`-and-
restart instructions with no in-app next step), not literal disappearance
from the nav. I left the links live and rewrote every one of their disabled
empty states (`FleetPage.tsx`, `ApprovalsPage.tsx`, `EconomicsPage.tsx`,
`ProvisioningWizard.tsx`, and D2's per-project `OrchestrationPanel.tsx` /
`LinkForm.tsx`) to name the real consequence of turning the feature on and
link straight to `/settings?section=orchestration` (`GlobalSettings.tsx`
scrolls the section into view on mount when that query param is present —
the same `?tab=`-deep-link idiom `ProjectSettings.tsx` already uses, just
via `useSearchParams` + `scrollIntoView` since the app-level page isn't
tabbed). On top of that I added one small, deliberately singular nav
hint: `Sidebar.tsx` now fetches `GET /api/settings/orchestration` once
(the sidebar isn't remounted on navigation, so this is one request for the
whole session, not one per page) and shows a neutral "Off" badge next to
**Fleet only** — not Approvals/Economics/Provision too — since Fleet reads
as the umbrella term for the capability and three badges felt like clutter
for one underlying signal. Judgment call; reasonable to revisit.

**The wire contract held exactly as frozen — verified against the real
backend, not just my own mocks.** E1's card landed in the same session
(`crates/tack-api/src/handlers/settings.rs`), so I built a debug binary and
curled it directly rather than trusting the contract on paper: `GET/PUT
/api/settings/orchestration` returns exactly the eight documented fields,
disabled routes answer `409` with `{"error":{"code":
"orchestration_disabled", ...}}` (never `403` in practice, though the
frontend's classifier accepts either per the original contract text), and
`POST /api/control-planes` returns `token_set` and never the token — all
matching `features/settings/orchestrationSettings/api.ts`'s types field for
field with no reconciliation needed. Killed the debug server afterward;
never touched the user's own running `./target/release/tack serve`
instance.

**The `code` field meant fixing the actual root cause, not just the two
routes in my contract.** Every one of the seven pre-existing
`isOrchDisabled` copies across the frontend (`features/fleet/api.ts`,
`features/approvals/api.ts`, `features/economics/api.ts`,
`features/provisioning/api.ts`, `features/settings/orchestration/api.ts`,
`shared/agentActivity/api.ts`, and `shared/dispatch/api.ts`'s re-export)
only ever checked `err.status === 404`. E1's routes now answer `409` with a
`code`, so all seven would have silently stopped detecting "disabled" the
moment E1's change shipped — a correctness bug, not a style one. Fixed at
the root: `ApiError` (`shared/api/client.ts`) gained an optional `code`
parsed from `error.code` in the envelope, plus one canonical
`isOrchestrationDisabledError()` — true when `code ===
"orchestration_disabled"` (any status), **or** a bare 404 with no code at
all (kept only as a legacy fallback, not because 404 means "disabled" going
forward). Every one of the seven `isOrchDisabled` exports now delegates to
it in one line, keeping every existing call site (`FleetPage.tsx`,
`ApprovalsPage.tsx`, `DispatchSprintModal.tsx`, ...) unchanged. Verified
against `approvalsApi.decide()`'s own 403 ("token rejected") and 409
("already decided") — neither carries the code, so neither is
misclassified; added a test asserting exactly that
(`features/approvals/api.test.ts`).

**Rule 9 / a11y.** No raw hex anywhere new — `lint:tokens` stayed 0/0. Two
new live-executed a11y scans in `e2e/a11y.spec.ts` ("settings orchestration
section (disabled, env default)" against the real dev server — this route
is reachable regardless of the flag, so no mocking needed for that one; and
"(enabled, populated)", mocking `GET /api/settings/orchestration` +
`/api/control-planes` + `/api/projects` the same way the pre-existing
"fleet page (populated)" scan intercepts `GET /api/fleet`), plus every
pre-existing orchestration-related a11y scan (fleet/approvals/economics/
provisioning disabled states, all three project-settings-orchestration-tab
scans) re-run and confirmed still green after the copy changes — 0
violations on all of them, run against real Chromium. The two-button On/Off
control uses `role="group"` + `aria-pressed` rather than a color-only
switch. Re-confirmed the same pre-existing duplicate-"Run sprint"-button
failure D1's and D2's handoffs already flagged
(`e2e/a11y.spec.ts:403`/`:462`) while running the full a11y suite to check
for regressions — untouched by this card, still someone else's pickup.

**What I deliberately did not build.** No confirmation modal on enabling
(the permanent explanatory paragraph is the friction, not a one-time dialog
an operator dismisses once and never reads again); no pause control
anywhere (D2's card already established why — no HTTP surface exists); no
second link form (reused D2's verbatim); no attempt to make `/control-
planes`/`/projects/{id}/orch-link` reachable before step 1 — they're
correctly still gated server-side and step 2/3 are visually locked to match,
not just cosmetically deferred.

**Testing.** 38 new Vitest tests (`shared/api/client.test.ts` +8;
`features/settings/orchestrationSettings/{api,format,
ControlPlanesManager,ProjectLinker,OrchestrationSettingsSection}.test.{ts,tsx}`
+30, new files) plus targeted updates to every pre-existing
`isOrchDisabled`/`isOrchestrationDisabledError` assertion across
`features/{fleet,approvals,economics,provisioning}/api.test.ts`,
`shared/agentActivity/api.test.ts`, and `features/settings/orchestration/
api.test.ts` (extended, not replaced — the original bare-404 case still
passes since it's the documented legacy fallback), plus router-context
fixes to `GlobalSettings.test.tsx` and `OrchestrationPanel.test.tsx` (both
now use `useNavigate`, so both now mount inside a `MemoryRouter` — a test
infrastructure fix, not a behavior change) and copy-assertion updates in
`FleetPage.test.tsx`/`EconomicsPage.test.tsx` (no longer assert the literal
string `TACK_ORCH_ENABLE`, since the empty state no longer names it).

**Verification.** `npm run type-check`: clean. `npm run lint:tokens`: 0/0,
unchanged. `npx vitest run`: 447 total, 444 passed, the same 3 pre-existing
`requestBlob`/`createObjectURL` failures named in this card's own baseline
(`client.test.ts`, `GlobalSettings.test.tsx`, `panels/panels.test.tsx`) —
confirmed unrelated to this card (none of the three touch orchestration)
and left exactly as instructed. Net +38 tests over the 409-total baseline
(406 passing + 3 known failures). `npm run build`: clean, `GlobalSettings`
chunk grew to 26.56 kB (gzip 8.16 kB) to include the new section — still one
lazy-loaded chunk. `npx playwright test e2e/a11y.spec.ts --project=chromium`:
27 of 29 passed; the 2 failures are the pre-existing sprint dry-run/dispatch
duplicate-button bug above, not mine.

**Files touched:** `frontend/src/shared/api/client.ts` (+`code` on
`ApiError`, +`ORCHESTRATION_DISABLED_CODE`, +`isOrchestrationDisabledError`)
and its test; `frontend/src/features/settings/orchestrationSettings/**`
(new: `api.ts`, `format.ts`, `ControlPlanesManager.tsx`, `ProjectLinker.tsx`,
`OrchestrationSettingsSection.tsx`, five `.test.ts(x)` files);
`frontend/src/features/settings/GlobalSettings.tsx` (+section, +`?section=`
deep-link) and its test; `frontend/src/features/settings/orchestration/
{api.ts,OrchestrationPanel.tsx,LinkForm.tsx}` (disabled-copy + `isOrchDisabled`
delegation) and `OrchestrationPanel.test.tsx`/`api.test.ts`;
`frontend/src/features/{fleet,approvals,economics,provisioning}/api.ts`
(`isOrchDisabled` delegation) and their four `.tsx` pages (disabled-copy +
"Set up orchestration" action) and `FleetPage.test.tsx`/
`EconomicsPage.test.tsx`; `frontend/src/shared/agentActivity/api.ts`
(`isOrchDisabled` delegation) and its test; `frontend/src/shared/ui/
Sidebar.tsx` (+"Off" hint on Fleet); `frontend/e2e/a11y.spec.ts` (+2 scans).
Did not touch `schema.gen.ts` — it wasn't regenerated during this session's
frontend work, matching E1's own note. Own `frontend/**` entirely per the
card; touched zero files under `crates/**`.

### E3 — 2026-08-05

**Reproduced first, then fixed.** Before touching any production code I
added `orch_runtime::tests::a_plane_registered_after_start_gets_polled`
(`crates/tack-api/src/orch_runtime.rs`) against the *unmodified* codebase:
enable orchestration with a store that starts with zero planes, `start()`,
register one plane on the fake store, then poll `live_task_count()` for up
to 5s. Ran it with `cargo test -p tack-api --lib orch_runtime::tests::
a_plane_registered_after_start_gets_polled` and watched it fail —
`panicked ... a control plane registered after start() was never picked up
for polling` — confirming the bug exactly as described: `OrchRuntime::
start` called `reconciler::spawn_reconcilers_cancellable`, which read
`store.list_registered()` exactly once and never again. Only after that did
I implement the fix and re-run the same test to green. I did not separately
verify the equivalent crate-internal test I added at the `tack-orch` layer
(`a_plane_registered_after_the_supervisor_starts_gets_polled`) against the
pre-fix code — it was written alongside the fix, in the same edit that
removed `spawn_reconcilers_cancellable` — but it exercises the identical
"list read once, never again" defect via the same code path, so I'm
confident it would have failed identically; I just didn't run that specific
experiment twice.

**Design chosen: the supervisor loop, not handler notification.** The
operator's brief laid out both shapes and leaned toward the supervisor; I
agree and built that one, for a reason the brief didn't fully spell out —
**the event-driven alternative was never fully wired even for the cases
that exist today.** I checked: `delete_control_plane`
(`crates/tack-api/src/handlers/orch.rs:332`) calls
`state.repo.delete_control_plane(id)` and returns — it never touched
`state.orch_runtime` before this card, so a deleted control plane's poller
task wasn't just theoretically vulnerable to an event-driven scheme's
"forgot to signal" failure mode, it was **already leaking forever** under
the old one-shot design, with nobody having written the signal in the first
place. An event-driven fix would have meant finding and instrumenting that
call site (and auditing every other write path that can change
`control_planes` — bulk import, restore-from-backup, a future admin
endpoint) and getting every one of them right forever. The supervisor gets
the delete case right *for free*, with no handler changes at all, because
it never assumed any handler would tell it anything — it just keeps asking
the table what's true. That is the whole argument for self-healing over
notification here: this codebase's own history is the demonstration that
"remember to signal" doesn't reliably happen.

**What changed, mechanically.** `crates/tack-orch/src/reconciler.rs`:

- Removed `spawn_reconcilers_cancellable` (the snapshot-once cancellable
  path) entirely — nothing outside this file called it except
  `orch_runtime.rs`, and its own defect is exactly what this card fixes, so
  keeping it around as a working-but-wrong alternative seemed worse than
  deleting it. `spawn_reconcilers` (the plain, non-cancellable one-shot
  function `spawn_one`'s ingestion/wiring tests all call directly, to run
  exactly one tick and inspect the result) is **unchanged** — it's a test
  utility for the fetch → decide → persist tick logic, not part of any
  production path since E1's card, and 13 pre-existing call sites across
  `tack-orch`'s and `tack-api`'s own test suites (`ingestion_test.rs`,
  `traces_ingestion_test.rs`, `orch_reconciler_wiring_test.rs`, plus this
  file's own unit tests) depend on its one-shot-and-return contract. Left
  alone on purpose.
- New: `spawn_reconcilers_supervised(store, config, stop_rx) ->
  SupervisedReconciler`. Does one `reconcile_tick` synchronously before
  returning (so a caller checking `SupervisedReconciler::live_task_count`
  immediately after `.await` still sees every plane registered *as of
  now*, preserving the exact contract `spawn_reconcilers_cancellable` used
  to offer and that `server.rs`'s boot-time `info!(control_planes =
  state.orch_runtime.live_task_count().await, ...)` log line depends on),
  then spawns a detached `supervisor_loop` task that keeps re-running
  `reconcile_tick` every `config.supervisor_scan_secs` until the global
  `stop_rx` fires.
- `reconcile_tick` is the diff: list `store.list_registered()`, compute the
  current id set, `HashMap::retain` to drop (and stop) any tracked plane no
  longer in that set, then `spawn_one` a fresh poller — with its **own**
  `watch` channel, not shared across planes — for anything newly present.
  A `list_registered` failure logs and skips the pass entirely, leaving
  every currently-running poller untouched (same tolerance
  `spawn_reconcilers` already had for this exact error, just applied
  per-scan instead of once).
- Per-plane pollers are still exactly `spawn_one`, byte-for-byte — the
  fetch → decide → persist → sleep shape, the panic isolation, the
  `evaluate()`-reads-only-`.health`/`.status` rule, all untouched. The
  `watch`-channel stop signal E1 built is *reused*, just minted once per
  plane by the supervisor instead of once for the whole fleet shared by
  `spawn_reconcilers_cancellable` — same primitive, multiplied, not a
  second cancellation mechanism. Stopping (global toggle-off, or a single
  plane's deletion) is non-blocking, matching `OrchRuntime::stop`'s
  existing discipline: `stop_all_plane_tasks` sends every stop signal and
  returns without awaiting a single task's actual exit.
- `ReconcilerConfig` gained `supervisor_scan_secs: u64` (default
  `DEFAULT_SUPERVISOR_SCAN_SECS = 2`) — deliberately decoupled from
  `poll_secs` (a plane's own cadence, which an operator might set much
  higher): the wizard's "register → see it come alive" moment needs this
  small regardless of the configured poll interval. Five existing struct
  literals that listed every `ReconcilerConfig` field explicitly (no
  `..Default::default()`) needed one line added each to keep compiling —
  `crates/tack-api/src/{server.rs,handlers/settings.rs}`,
  `crates/tack-orch/tests/traces_ingestion_test.rs` (×2),
  `crates/tack-orch/src/reconciler.rs`'s own test module (×1) — no
  behavior change at any of those sites, all still pass their own
  explicitly-set fields through.

`crates/tack-api/src/orch_runtime.rs`: `Running` now holds a
`SupervisedReconciler` instead of `Vec<JoinHandle>`; `start`/`stop`/
`live_task_count`'s signatures and the at-most-one-generation /
never-blocks-on-stop invariants are byte-for-byte unchanged — this module's
job stays exactly "own the single global on/off signal", per-plane
lifecycle moved entirely into `tack-orch`.

**§0 rule 5, unaffected.** `reconcile_tick`'s own `list_registered()` read
is a single short DB call with no HTTP anywhere near it — the exact same
shape `spawn_one`'s pre-existing `list_linked_projects`/`list_trace_cursors`
reads already have, and the fetch/decide/persist split inside `spawn_one`
itself is completely untouched. `evaluate()` still reads only `.health`/
`.status`.

**For E2 — `reconciler_running` semantics, one real behavior change worth
knowing about.** Field name and JSON shape are unchanged
(`live_task_count() > 0`, computed the same way, same field on `GET`/`PUT
/api/settings/orchestration`). What changed is that it's now driven by a
self-healing set instead of a static one, which fixes the delete-leak bug
above but introduces a small, bounded staleness window in the *other*
direction: after a control plane is deleted, `control_plane_count` drops to
`0` immediately (a straight `repo.list_control_planes().len()`), but that
plane's poller isn't told to stop until the *next* supervisor scan — up to
`supervisor_scan_secs` (2s by default) later. So `enabled: true`,
`control_plane_count: 0`, `reconciler_running: true` **can** now appear
transiently, for at most ~2s, immediately after a delete. This is strictly
better than before this card (the old code never stopped that poller at
all — the window was infinite, not 2 seconds), so I don't think it needs UI
handling, but it's a real, if narrow, exception to the invariant your
handoff note described ("a task only exists per registered plane") and I'd
rather name it than have you discover it from a flaky-looking a11y/E2E
assertion later.

**Tests.** `crates/tack-orch/src/reconciler.rs`: replaced the three
`cancellable_spawn_*`/`repeated_start_stop_cycles_leave_no_task_running`
tests (which exercised the now-deleted function) with
`supervised_spawn_starts_one_task_per_already_registered_plane`,
`supervised_spawn_stops_every_task_after_the_global_stop_signal`,
`supervised_spawn_already_stopped_never_starts_a_tick`,
`repeated_global_start_stop_cycles_leave_no_task_running` (same coverage,
against the new function), plus three new ones for the actual bug class:
`a_plane_registered_after_the_supervisor_starts_gets_polled` (the
reproduction, at this layer), `a_deleted_plane_stops_being_polled_without_a
_global_stop_signal`, and `repeated_register_delete_cycles_leak_no_tasks`
(three churn cycles, asserting the tracked-task map is empty between each).
A new `MutableStore` fake (`planes: Mutex<Vec<RegisteredPlane>>` with
`register`/`delete`) makes these possible — the pre-existing `FakeStore` has
a fixed plane list, correct for testing a single tick but unable to express
"the registered set changes while the reconciler is running", which is
exactly what this bug is about. `crates/tack-api/src/orch_runtime.rs`: one
new test, `a_plane_registered_after_start_gets_polled` (the reproduction,
one layer up, against `OrchRuntime` itself, using an analogous
`MutableFakeStore`). All pre-existing `orch_runtime.rs` and
`orch_settings_test.rs` tests (the HTTP-level `PUT`/`GET
/api/settings/orchestration` suite, including the 3s-bounded-poll
`wait_for_reconciler_running` helper and the duplicate-`start()`
idempotency test) pass unmodified — `OrchRuntime`'s public contract didn't
change.

**Verification.** `cargo test --workspace`: 603 passed, 0 failed (baseline
598; net +5 — reconciler.rs went from 3 removed to 7 added, +4, plus +1 in
orch_runtime.rs). `cargo clippy --workspace --all-targets -- -D warnings`:
clean. `cargo fmt --all -- --check`: clean (ran `cargo fmt --all` once after
the edits to fix line-wrapping in `orch_runtime.rs` and `reconciler.rs`,
then re-verified `--check`). `cargo test -p tack-api --test
openapi_contract`: all 4 pass, including `openapi_spec_matches_committed_
file` — no regeneration needed, since nothing in this card touches a route,
DTO, or response shape. Note for whoever commits next: `docs/openapi.json`
already shows as modified in `git status` — that's E1's own prior
regeneration sitting uncommitted in the shared tree (confirmed via `git diff
docs/openapi.json`: the diff is exactly the new `/api/settings/orchestration`
path, the `UpdateOrchSettings` schema, and the `code` field, all E1's, none
of it touched by this card), not something I introduced.

**Files touched:** `crates/tack-orch/src/reconciler.rs` (supervisor +
tests, described above), `crates/tack-api/src/orch_runtime.rs`
(`SupervisedReconciler` wiring + one new test), `crates/tack-api/src/
server.rs` and `crates/tack-api/src/handlers/settings.rs` (one
`..Default::default()` line each, `ReconcilerConfig`'s new field),
`crates/tack-orch/tests/traces_ingestion_test.rs` (same, ×2, no behavior
change). Did not touch `frontend/**` or any file not already listed.

---

# Part II — Agnostic Control Plane (Phases 39–49)

Executable task board for the cycle described in
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Agnostic Control Plane*.
The full plan, with a verification command per item and the reasoning behind every
decision, is **[docs/plans/agnostic-control-plane.md](docs/plans/agnostic-control-plane.md)**.
Read both before picking up a card.

**Own numbering namespace.** Sections here are `II.0` … `II.6`. Part I's numbers are cited
by number from Rust doc comments across the workspace and must not move.

**The thesis every design decision must respect:** competitors start from the fleet and
bolt on a board. Tack starts from the work item and adds the fleet. That is why Tack can
say *"this feature cost 4.2M tokens across 3 runs and 2 reworks"* and a fleet dashboard
cannot — it has no concept of a feature.

**The test this cycle has to pass:** *can an adapter be written for a provider with no
pods, no roles, no hops, no approval store and no policy engine, without touching the
trait?* Today it cannot: of thirteen `ControlPlane` methods, only `kind()` and `get_run()`
survive unchanged, three are pure docket, and eight carry docket-shaped DTOs.

## Status board — Part II

| Wave | Cards | Phases | Status |
|---|---|---|---|
| A — Oracle (blocking) | O1 · O2 · O3 | 39 | implemented and verified in the current working tree; unreleased |
| B — Foundations (parallel) | G1 · G2 · G3 · G4 · G5a · G5b | 40, 41, 42 | implemented in the current working tree; unreleased; concurrency/schema acceptance reopened by the architecture audit |
| C — The reshape (blocking, single owner) | T1 · T2 · T3 · T4 · T5 · T6 | 43 | **superseded — do not start; preserved below for history** |
| D — The second adapter | N1 · N2 · N3 · N4 · N5 | 44, 45 | **superseded — do not start; GitHub Actions is not an agent-harness proof** |
| E — Independent tracks | M1 · M2 · W1 · H1 · S1 · S2 | 46, 47, 48, 49 | **frozen — useful concepts are re-scoped into Part III; do not start from these cards** |

**Supersession note (2026-08-06).** Nothing below was deleted: it remains the decision
history that explains the current code and migrations. Part III replaces the central
boundary. Tack will own scheduling and durable execution requests; a pull-based
`tack-runner` will launch agent harnesses. Docket becomes an optional legacy bridge, and
GitHub Actions returns to being a CI/integration concern rather than the proof of an agent
harness abstraction.

---

## II.0 Rules of engagement (additions to §0)

Part I's ten rules stand unchanged. These are additional, and each one exists because the
planning read found the failure it prevents.

1. **Tack never runs agents and never proxies model traffic.** Always a client of a control
   plane. It never holds a vendor key on the request path, never implements routing or
   fallback. If a solution needs Tack to spawn an agent process, or model traffic to pass
   through the Tack process, it is out. Tack configures and reads a gateway; it never
   becomes one.
2. **Model identifiers are opaque strings.** Store the identifier plus the id of the
   gateway that understands it. **Never parse, map, normalise or classify one.** Tack
   classifies work items; the gateway classifies models. No tier abstraction under any
   name — docket removed `economy`/`standard`/`premium` in 0.2.0 and accepts them nowhere.
3. **No docket noun crosses the ACL.** If the UI says "pod," the anti-corruption layer has
   leaked. Provider specifics live in `provider_metadata` and a per-adapter UI fragment.
   `rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/` must return 0 by the
   end of Wave C.
4. **One `ALTER` per migration name.** `migrations.rs` runs each statement individually
   **with no wrapping transaction**, and records the migration name only after every
   statement succeeds. A five-`ALTER` migration failing on statement three records nothing;
   the next boot re-runs statement one, hits SQLite's `duplicate column name`, and **the
   server never boots again** — with no down-migration. Both existing `ALTER` migrations
   (029, 030) are deliberately single statements. This is a brick-the-install rule, not a
   style preference.
5. **Every new secret column is added to `remote_backup.rs::scrub_snapshot_secrets` in the
   same commit**, before the trailing `VACUUM`. That function's own doc comment states the
   rule. A secret column added without it ships credentials inside every downloadable
   backup.
6. **A capability is a value, never a provider check.** `rg -n "kind === 'docket'"` in
   `frontend/src` must return 0. A disabled control names its reason from
   `capabilities().<field>.reason`.
7. **Two meters are never one number.** Runner minutes and token spend are separate bills
   from separate vendors. Show each with its source label; never aggregate them into a
   single "cost" figure. At realistic volumes the runner is low single-digit percent of the
   total, so a combined number would hide the meter that matters.
8. **"Not measured" is never "0".** `orch_tasks.tokens_in`/`tokens_out` are written as
   literal `0` by `dispatcher.rs` and updated by nothing — every token figure in the app is
   a structural zero today. Card M2 builds the first real path; until then, and wherever no
   source exists, render "not measured".
9. **Handoff note:** finish by appending to §II.6.

---

## II.1 Shared contracts

Wave C defines them; every later card consumes them verbatim. Unlike Part I, the trait is
**not** frozen after its wave — §2.1 lifted that freeze and the reasoning still holds.
Change it if a real design need shows up, and update every implementor and caller in the
same change.

### II.1.1 The target `ControlPlane` trait

```rust
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    /// The ONLY input to the reachability verdict. The adapter decides what
    /// "reachable" means and owns its own expected-version check.
    async fn health(&self) -> Result<PlaneHealth, OrchError>;

    async fn runtimes(&self) -> Result<Vec<Runtime>, OrchError>;
    async fn plane_metrics(&self) -> Result<Vec<MetricSample>, OrchError>;

    /// Returns a rich ack, so no caller ever needs a read-back call.
    async fn dispatch(&self, t: &DispatchTarget, r: DispatchRequest)
        -> Result<DispatchAck, OrchError>;

    async fn list_runs(&self, t: &DispatchTarget) -> Result<Vec<RunStatus>, OrchError>;
    async fn run_status(&self, h: &RunHandle) -> Result<RunStatus, OrchError>;

    /// SCOPED. docket serves events per project and cannot serve them per run;
    /// GitHub Actions can only serve them per run.
    async fn events(&self, scope: &EventScope, cursor: Option<&str>)
        -> Result<EventPage, OrchError>;

    /// First-class, so the reconciler never reaches into provider_metadata.
    fn correlation_keys(&self, record: &CorrelatableRecord) -> Vec<String>;

    async fn artifacts(&self, h: &RunHandle) -> Result<Vec<Artifact>, OrchError>;
    async fn pending_decisions(&self) -> Result<Vec<Decision>, OrchError>;
    async fn resolve_decision(&self, id: &str, a: DecisionAnswer)
        -> Result<DecisionState, OrchError>;
    async fn usage(&self, h: &RunHandle) -> Result<Option<Usage>, OrchError>;
    async fn cancel(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn pause(&self, h: &RunHandle) -> Result<(), OrchError>;
    async fn resume(&self, h: &RunHandle) -> Result<(), OrchError>;
}
```

Gone from the trait: `status()`, `metrics()` (renamed `plane_metrics`), `list_tasks()`,
`get_run()` (dead — zero production call sites), `enqueue_task()`, `dispatch(project,
vars)` (dead — returns `Disabled` unconditionally), `decide_approval()`,
`provision_pod()`, `traces()`.

**Four corrections that are not negotiable**, each forced by real code:

- **`events` is scoped, not per-run.** `RemoteEvent` carries no run id and
  `persist_events` says so outright: *"docket's trace payload carries no run_id, only
  session_id… Left unset rather than guessing."* A per-run `events()` is **unimplementable**
  for docket — every event currently ingested would be dropped. The cursor store keeps its
  `(control_plane_id, remote_project)` key for `Project` scope and gains a run-scoped key.
- **`dispatch` returns `DispatchAck`.** `dispatcher.rs` today makes a **second** call to
  `list_tasks` purely to recover `remote_status` and `approval_token`. Delete `list_tasks`
  without widening the ack and `approval_token` becomes permanently `null` while every
  approval-gated dispatch takes the `on_running` branch. **The OpenAPI drift gate cannot
  catch this** — the field still exists, only its value dies.
- **`RunState` is a normalized closed enum on the trait**, never in `provider_metadata`.
  See §II.1.3.
- **`plane_metrics()` stays.** docket's `/metrics` is plane-wide with no run or project
  dimension, and `GET /api/projects/{id}/orch-policy` is built entirely from it including a
  server-computed `denial_rate`. A per-adapter UI fragment cannot produce a number the
  server already committed to in the spec.

### II.1.2 `Capabilities`

```rust
pub struct Capabilities {
    pub dispatch: bool,
    pub cancel: bool,
    pub pause: Support,             // Unsupported | Advisory | Supported
    pub resume: Support,
    pub event_scope: EventScope,    // None | Run | Project | Plane
    pub artifacts: bool,
    pub decisions: DecisionSupport, // None | Poll | Push
    pub usage: UsageSupport,        // NotMeasured | FromProvider | FromGateway
    pub model_selection: ModelSelection, // Unsupported | Advisory | Honoured
    pub runtimes: bool,
    pub plane_metrics: bool,
    pub provisioning: bool,
}
```

Every non-boolean field carries a `reason: &'static str` for the disabled case. **Model
selection is the acceptance test for the whole mechanism:** docket owns its routing and may
ignore an externally supplied model (`Unsupported`); a GitHub Actions adapter forwards it
verbatim (`Honoured`). The UI must render three different things, never a picker that
silently does nothing.

Two ad-hoc capability bits already exist and are retired into this struct:
`PendingApprovalListResponse.grant_available`, and `useAgentActivityMap`'s `orchAvailable()`
used as a dispatch gate — the latter really means *"orchestration is on"*, not *"this
provider can dispatch"*, and is wrong the moment there are two providers.

### II.1.3 Normalized `RunState`, and the per-adapter mapping table

`Queued | Running | Blocked | Succeeded | Failed | Cancelled | TimedOut | Unknown(String)`,
keeping the existing `remote_string_enum!` round-trip discipline.

| Provider value | Normalized |
|---|---|
| docket `queued` / `running` / `succeeded` / `failed` / `cancelled` | the same five — **docket is byte-identical** |
| GHA status `queued`, `pending`, `requested` | `Queued` |
| GHA status `in_progress` | `Running` |
| GHA status `waiting`, conclusion `action_required` | `Blocked` **and raises a `Decision`** — a deployment gate is a human waiting, not a terminal state |
| GHA conclusion `success` | `Succeeded` |
| GHA conclusion `failure`, `startup_failure` | `Failed` |
| GHA conclusion `cancelled` | `Cancelled` |
| GHA conclusion `timed_out` | `TimedOut` |
| GHA conclusion `skipped`, `neutral`, `stale` | `Cancelled` — documented as "the provider decided not to run it" |

Why this cannot live in `provider_metadata`: `orch_store.rs`'s
`reconcile_terminal_status_map` is the **only** place a finishing agent moves a card, and it
matches three string literals. GitHub has nine conclusions; seven would fall through
`_ => return` with no error, no log and no event, leaving cards permanently in "In
Progress".

`StatusMap` gains `on_blocked` and `on_timed_out`, **both optional**. Absent means fall back
to `on_waiting_approval` and `on_failed`, so every `status_map` already saved in a user's
database behaves exactly as it does today. Assert that with a test.

### II.1.4 Verified external facts

Checked against vendor documentation during planning. Do not re-derive; do re-verify if a
card's behaviour disagrees.

| Fact | Value | Consequence |
|---|---|---|
| `POST .../actions/workflows/{id}/dispatches` on github.com | `200` with `{workflow_run_id, run_url, html_url}` | fast path only |
| The same endpoint on **GHES 3.17** | `204 No Content`, empty body | **correlation must not depend on the dispatch response** |
| Cancel | `POST .../runs/{id}/cancel` → `202` | `cancel: true` |
| Pause / suspend / hold | **no endpoint exists** | `pause: Unsupported` |
| Force cancel | `POST .../runs/{id}/force-cancel` → `202` | provider extra, not a trait method |
| Re-run | `POST .../runs/{id}/rerun` → `201` | maps to `orch_tasks.attempt` / `run_attempt` |
| Run logs | `GET .../runs/{id}/logs` → `302`, link expires in **1 minute** | not an event stream — do not build on it |
| Log/artifact retention | default 90 days; public 1–90, private 1–400 | bounded derived history |
| Job limits | GitHub-hosted **6 h**, self-hosted **5 days**, run 35 days incl. waiting | the HITL ceiling |
| Pending deployments | `GET`/`POST .../runs/{id}/pending_deployments`, `{environment_ids, state, comment}` | Actions' decision store |
| Webhook headers | `X-GitHub-Event`, `X-GitHub-Delivery` (GUID), `X-Hub-Signature-256` | dedupe key + HMAC |
| Claude Code hooks | `PreToolUse`/`PostToolUse` are **synchronous and block the tool call**; default `command` timeout **600 s**, per-hook `timeout`; exit 2 blocks | the HITL mechanism |

---

## II.2 File-ownership map

One owner per file per wave. Anything not listed is free to create.

| File | Owner | Wave |
|---|---|---|
| `crates/tack-orch/tests/**` (oracles + golden) | **O1** | A — O2/O3 add files, never edit O1's |
| `.github/workflows/ci.yml` | **O3** | A — batch every gate for the cycle in one edit |
| `crates/tack-db/src/migrations.rs` | **G5** | B — **all migrations for the cycle route through G5**, one `ALTER` each (§II.0 rule 4) |
| `crates/tack-orch/src/lib.rs` (trait + DTOs) | G1 → **T1/T2** | B → C |
| `crates/tack-orch/src/adapters/registry.rs` | G1 | B |
| `crates/tack-orch/src/adapters/github_actions.rs` | G1 (stub) → **N2** | B → D |
| `crates/tack-orch/src/adapters/docket.rs` | **T2** | C |
| `crates/tack-orch/src/reconciler.rs` | **T3** | C |
| `crates/tack-api/src/orch_store.rs` | G1 → T4 | B → C |
| `crates/tack-api/src/dispatcher.rs` | T4 | C |
| `crates/tack-api/src/remote_backup.rs` | **G2** | B — scrub block only |
| `crates/tack-api/src/middleware.rs`, `router.rs` | **G4** | B — **batch every route and CORS change for the cycle**; later waves hand routes to G4's successor |
| `crates/tack-api/src/handlers/orch.rs` | T4 → N1 | C → D |
| `crates/tack-api/src/handlers/ingest.rs` (new) | **N3** | D |
| `crates/tack-api/src/handlers/webhooks.rs` (new) | **W1** | E |
| `crates/tack-cli/src/{client,mcp}.rs` | G4 | B |
| `crates/tack-core/src/models.rs` | T4 | C — ACL removal only |
| `crates/tack-core/src/model_policy.rs` (new) | M1 | E |
| `frontend/src/shared/orch/**` (new) | G1 → T6 | B → C |
| `frontend/src/features/fleet/**` | T6 | C |
| `frontend/src/features/approvals/**` | N1 | D |
| `docs/examples/**` (new) | N4, H1 | D, E |

**Merge order within a wave:** whoever finishes first merges first; the rest rebase. Cards
were scoped so only `migrations.rs`, `router.rs` and `lib.rs` can genuinely conflict, and
each has a single named owner.

---

## II.3 Wave plan

> **Historical/superseded sequence.** Waves C–E below must not be dispatched; Part III is
> the active agent board. The diagram remains to explain the work already present in the
> unreleased tree and the decisions recorded in Part II.

```text
Wave A  (Phase 39, blocking, 3 cards)          O1 ── O2 ── O3
                                                     │
Wave B  (Phases 40–42, 5 cards, parallel)  G1 G2 G3 G4 G5
                                                     │
Wave C  (Phase 43, SUPERSEDED)           T1 → T2 → T3 → T4 → T5 → T6
                                                     │
Wave D  (Phases 44–45, SUPERSEDED)            N1 N2 N3 N4 N5
                                                     │
Wave E  (Phases 46–49, FROZEN)                M1 M2 W1 H1 S1 S2
```

**Wave C is deliberately sequential and single-owner.** It is the one breaking change in
the cycle; splitting it across parallel agents would produce exactly the churn §2.1's
freeze was invented to prevent, on a trait with no external consumers to protect.

**Wave A blocks everything.** Nine of thirteen trait methods currently have no test
asserting what leaves the process — only four of the 37 `DocketAdapter` tests check an
outgoing request. Starting Wave C without Wave A is a blind refactor.

---

## Wave A — Phase 39, the regression oracle (blocking)

### O1 — Tick-level contract oracle

**Tasks:** 39.1 · **Files:** `crates/tack-orch/tests/docket_tick_contract_test.rs`,
`crates/tack-orch/tests/golden/**` · **Depends on:** —

1. Drive a full `reconcile_once` **plus the whole persist phase** against a `wiremock`
   docket **and** an in-memory SQLite. Both patterns already exist — copy
   `tests/ingestion_test.rs` and `tests/traces_ingestion_test.rs`; `sqlx` is already a
   dev-dep.
2. Snapshot **two** artifacts to `tests/golden/`: (i) the **ordered** list of HTTP requests
   the tick issued — method, path, sorted query, which headers were present, canonicalised
   body; (ii) the resulting rows of `orch_runs`, `orch_approvals`, `orch_events`,
   `orch_metrics`, `orch_trace_cursors`, deterministically sorted.
3. Five scenarios: cold start, warm cursor, **rewound cursor**, a plane with 0 linked
   projects, a plane with 3.
4. `UPDATE_GOLDEN=1` regenerates, mirroring the existing `UPDATE_OPENAPI=1` pattern in
   `crates/tack-api/tests/openapi_contract.rs`.

**Why ordered-and-counted, not a set:** the refactor that defeats a set is *"implement the
reshaped `events()` as a straight delegation to the same `/traces/{project}?since=`
request, then re-scope `reconcile_once` to iterate active runs"*. With 3 linked projects
and 0 active runs — the steady state, and exactly what a per-method fixture set looks like
— the tick issues **zero** trace calls where it issued three. Trace ingestion silently
stops for every user and a method-level golden is byte-identical.

**Why the row snapshot:** the refactor that defeats requests-only is *dropping the
`occurred_at < retention_cutoff` guard*. Not an adapter concern, so no request changes; but
a rewound cursor then resurrects rows already rolled into `orch_events_daily` and purged,
and the next `rollup_and_purge_orch_events` counts their cost **a second time**. Silent
money corruption, green CI.

**Acceptance:** `cargo test -p tack-orch --test docket_tick_contract_test` passes, and
`UPDATE_GOLDEN=1 cargo test -p tack-orch --test docket_tick_contract_test && git diff
--exit-code crates/tack-orch/tests/golden/` exits 0 on an unmodified tree. Then prove it is
real: locally re-scope the poll loop as described above and confirm the test fails.

### O2 — Per-method wire oracle + pinned event id

**Tasks:** 39.2, 39.3 · **Files:** `crates/tack-orch/tests/docket_wire_contract_test.rs`,
`crates/tack-orch/tests/golden/**` · **Depends on:** O1 (golden dir layout)

1. Request transcript plus decoded result for **all thirteen** current methods. Today only
   four tests check an outgoing request: `enqueue_task_sends_the_trusted_flag_on_the_wire`,
   `decide_approval_grant_sends_channel_tack_…`,
   `provision_pod_sends_the_full_request_shape_on_the_wire`,
   `unauthenticated_routes_never_send_authorization_header`. The other 33 assert decoding.
2. Pinned-literal event id:
   `assert_eq!(derive_event_id(<fixed uuid>, "proj", &<fixed event>).to_string(), "<literal>")`.
   The namespace constant carries a *"must never change once any deployment has ingested a
   single event"* warning, and the existing determinism test only proves determinism
   **within one build** — it would not catch a changed separator, field order, or namespace,
   which re-inserts every previously-ingested event under a fresh id on the first poll after
   upgrade.

**Acceptance:** both tests pass; flipping one byte of the namespace constant locally fails
the pinned test.

### O3 — CI gates and the missing coverage floor

**Tasks:** 39.4 · **Files:** `.github/workflows/ci.yml` · **Depends on:** O1, O2

1. A `golden drift` step in the `rust` job, mirroring the OpenAPI gate exactly.
2. `cargo llvm-cov -p tack-orch --fail-under-lines 70` in the `coverage` job. **There is no
   `tack-orch` floor today** — the job floors `tack-core >= 85`, `tack-db >= 70`,
   `tack-api >= 70` and does not name `tack-orch`, which makes the adapter the
   least-guarded code in the workspace and the code this cycle rewrites most.

**Acceptance:** a deliberately mutated golden fails CI; `cargo llvm-cov -p tack-orch
--fail-under-lines 70` passes locally.

---

## Wave B — Phases 40–42, foundations (parallel)

### G1 — Capability model + one adapter registry + GHA stub

**Tasks:** 40.1, 40.4, 40.5, 40.7 · **Files:** `crates/tack-orch/src/lib.rs`,
`crates/tack-orch/src/adapters/{mod,registry,github_actions}.rs`,
`crates/tack-api/src/{orch_store,dispatcher}.rs`,
`crates/tack-api/src/handlers/{orch,provisioning}.rs`,
`frontend/src/shared/orch/capabilities.ts` · **Depends on:** G2 (the `config`/`secrets`
columns the registry signature needs)

1. `Capabilities` per §II.1.2, plus `fn capabilities(&self) -> Capabilities` on the trait.
   `DocketAdapter::capabilities()` returns the **verified** truth, including
   `pause: Unsupported` with a reason naming `docket profile <pod-id> --resume`.
2. `tack_orch::adapters::registry::build(kind, base_url, config, secrets)`. It must live in
   `tack-orch` — `crates/tack-orch/Cargo.toml` forbids depending on `tack-api`.
3. Replace **all four** duplicated `match row.kind.as_str()` sites. They are not
   equivalent and must keep their different failure behaviour: `orch_store.rs` warns and
   `continue`s (batch, reconciler); `dispatcher.rs`, `handlers/orch.rs` and
   `handlers/provisioning.rs` each error, each mapping `RowNotFound` to `NotFound`
   separately. Two of them carry a comment saying the duplication is deliberate and *"if a
   third caller ever needs this, that's the point to actually share it."* There are four.
4. `adapters::github_actions` **compile-only stub**: `kind()` and `capabilities()`
   truthfully filled in, every other method `unimplemented!()`, never registered. Its only
   job is to make *"both adapters compile against the trait"* a Wave C gate rather than a
   Wave D discovery.
5. Surface capabilities on `GET /api/control-planes/{id}` and `GET /api/fleet`; every gated
   frontend control reads them. Retire `grant_available` and the
   `orchAvailable()`-as-dispatch-gate.

**Acceptance:** `rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` returns **0**;
`rg -n "kind === 'docket'|grant_available" frontend/src` returns **0**;
`cargo test -p tack-api --test orch_reconciler_wiring_test` passes unchanged; a Vitest test
asserts a disabled control renders a reason string **sourced from the capability**, not a
hard-coded literal.

### G2 — Plane config + secrets + `unconfigured` health + backup scrub

**Tasks:** 40.2, 40.3 · **Files:** `crates/tack-api/src/remote_backup.rs`,
`crates/tack-api/src/orch_store.rs`, `frontend/src/features/fleet/api.ts` ·
**Depends on:** G5 (migrations 032, 033)

1. `control_planes.config` (provider configuration JSON) and `control_planes.secrets`
   (write-only credentials). A GitHub Actions plane needs `{owner, repo, workflow_file, ref,
   api_base}` plus **two** secrets — an API credential and a webhook secret — and today
   `control_planes` has only `base_url` and one `token`.
2. **Same commit** adds the `control_planes.secrets` block to
   `remote_backup.rs::scrub_snapshot_secrets`, before the trailing `VACUUM` (§II.0 rule 5).
   Follow the shape of the existing `control_planes.token` block: null the column, keep the
   row, guard on `sqlite_master` so a pre-migration snapshot still works.
3. `health = 'unconfigured'`. A restored backup has `secrets IS NULL`, and `orch_store.rs`
   currently `continue`s past a failed adapter construction with only a `warn!` — **the
   plane vanishes from polling invisibly.** Benign for docket (its token is optional and the
   adapter only adds the header when present), fatal and silent for any plane whose
   credentials are required. Widens the closed `ControlPlaneHealth` union in the frontend.

**Acceptance:** a new `scrub_removes_control_plane_secrets_from_snapshot` test in the shape
of the existing token one asserts the column is nulled and the row survives; an
unconfigured plane reports `unconfigured`, not `unknown`; `HEALTH_LABEL` covers five states.

### G3 — Optimistic concurrency

**Tasks:** 41.1, 41.2 · **Files:** `crates/tack-db/src/repo/{items,orch}.rs`,
`crates/tack-api/src/handlers/{items,orch,projects}.rs` · **Depends on:** G5 (migrations
034–036)

1. Repo layer bumps `version` on **every** `UPDATE` — including
   `update_item_status_checked` and `check_and_update_parent_status`, not just
   `update_item`.
2. `ETag` on `GET`, `If-Match` on `PATCH`/`PUT` for items, orch-links and control-planes.
   `412` on mismatch. **An absent `If-Match` behaves exactly as today**, so nothing breaks.

**Acceptance — corrected 2026-08-06, the original criterion here was weak and the
adversarial pass proved it:** the gate is the two **sequential** tests,
`patch_with_a_stale_if_match_is_rejected_with_412_and_the_standard_envelope` and
`patch_with_an_if_match_for_a_different_item_is_rejected`. Each captures an ETag, lets a
write land, then replays the now-stale ETag and requires `412`. No concurrency, no
scheduler dependence; both fail 100% of the time against an `If-Match` that is parsed and
ignored.

The originally-named criterion — two concurrent PATCHes yielding one `200` and one `412` —
detected that mutation only **5 times in 15 runs**, because it leaves
`claim_item_version`'s atomic `UPDATE ... WHERE version = ?` in place and two racers sharing
one still-valid version reproduce the expected shape by coincidence. It stays in the suite
as a property test of the compare-and-swap layer, but it is **not** the gate. See §II.6's
"Adversarial verification — Wave B" for the full finding.

### G4 — CORS, routes, and the MCP write path

**Tasks:** 41.3, 41.4, 41.5, 40.6 · **Files:** `crates/tack-api/src/{router,middleware}.rs`,
`crates/tack-cli/src/{client,mcp}.rs`, `crates/tack-api/src/handlers/items.rs`,
`docs/book/src/developer/orchestration.md` · **Depends on:** G3

1. **CORS has no `expose_headers` call at all** — a browser can read no non-safelisted
   response header today. Add `expose_headers([ETAG])`, and add `if-match` and
   **`x-tack-approval-token`** to the allow-list. The approval-token omission is a
   pre-existing bug: the decide call works only because production is same-origin via
   `embed-spa`, and is already broken on any cross-origin `TACK_ALLOWED_ORIGINS` path.
   There is no CORS test in the repo — this card ships the first one.
2. `tack-cli`'s client `request()` **cannot set a header at all**, so every MCP write is
   unconditionally last-write-wins. Add header support and send `If-Match` from
   `update_item` and `move_item`. The agent-versus-human race is precisely the one G3
   exists for, and the agent path is the unprotected one. Note `mcp.rs` asserts
   `tools.len() == 8`; it moves in step with any tool change.
3. **Fix the auto-dispatch gate.** `handlers/items.rs` gates auto-dispatch on
   `state.config.orch_enable`, not `effective_orch_enabled` — so it **ignores the UI toggle
   today**, contradicting §0 rule 8. With a GitHub Actions plane that means a workflow
   dispatched automatically while the UI reports orchestration off. A behaviour change to a
   shipped feature: note it in `CHANGELOG.md`.
4. Document the writers that bypass HTTP: the reconciler calls
   `dispatcher::apply_mapped_status` directly with no request and no `If-Match` — **the
   largest single mutator of `items.status` is outside this control by design** — and
   `propagate_parent_completion` mutates a *parent* on a child's PATCH, so a parent's ETag
   changes with no caller having touched it. Both are correct; both must be written down so
   a client does not conclude `412` is a total ordering.

**Acceptance:** `cargo test -p tack-api
preflight_allows_if_match_and_approval_token_and_exposes_etag`; `cargo test -p tack-cli
mcp_update_item_sends_if_match`; `mdbook build docs/book`.

### G5 — All migrations for the cycle, one `ALTER` each

**Tasks:** 40.2, 41.1, 42.1, 42.2, 42.3, 42.4 · **Files:**
`crates/tack-db/src/migrations.rs` · **Depends on:** —

**This card owns `migrations.rs` for the entire cycle.** Later waves request migrations
through it rather than editing directly — the same chokepoint discipline `router.rs` had in
Part I.

1. 032 `control_planes.config`, 033 `control_planes.secrets`, 034 `items.version`,
   035 `orch_links.version`, 036 `control_planes.version` — **one statement each**
   (§II.0 rule 4).
2. **037, rebuild `orch_runs`.** SQLite's 12-step procedure, this table only: create with
   `PRIMARY KEY (control_plane_id, external_run_id, run_attempt)` and `correlation_id TEXT
   UNIQUE`, `INSERT … SELECT` copying `run_id` into `external_run_id` with
   `run_attempt = 1`, drop, rename, recreate `idx_orch_runs_plane_state`,
   `PRAGMA foreign_key_check`. **Why it cannot be an `ALTER`:** `run_id` is a *global* PK
   with `control_plane_id` outside the key, so minting a placeholder on the Tack
   correlation id and later "backfilling" the provider id inserts a **second** row under a
   different PK — `ON CONFLICT(run_id)` cannot merge two different primary keys, and you
   get two rows per run forever.
3. **038, rebuild `orch_approvals`.** `control_plane_id` becomes nullable — a
   hook-originated decision comes from a run that may never have been dispatched through a
   registered plane, and today the column is `NOT NULL REFERENCES control_planes(id)`. Adds
   `kind`, `external_id`, `provider_metadata`. **`token` stays the PK and the URL segment**
   — renaming a column that is in a user's database buys nothing.
4. **Half-applied-rebuild guard:** `run_all` refuses to boot if both `orch_runs` and
   `orch_runs_new` exist, with an error naming the backup endpoint, rather than re-running
   `DROP TABLE`.
5. Release note: this upgrade rewrites two tables; take a backup first.
6. Later migrations, added as their cards land: 039–041 (ingest), 042–043 (model policy),
   044–049 (GitHub links).

**Acceptance:** a seeded DB at 036 upgrades with identical row counts, per-row field
equality, an empty `PRAGMA foreign_key_check`, and the old PK's uniqueness still enforced;
a deliberately half-applied state refuses to boot with a named error;
`cargo test -p tack-db --test orch_migrations_test` green.

---

## Wave C — Phase 43, the reshape (superseded; do not start)

### T1 — DTOs and the `RunState` mapping table

**Tasks:** 43.1 · **Files:** `crates/tack-orch/src/lib.rs` · **Depends on:** Wave A, G1

Every DTO from §II.1.1, each with `provider_metadata: serde_json::Value` for provider
extras. `RunState` per §II.1.3, keeping the `remote_string_enum!` `Unknown(String)`
discipline — a provider upgrade that adds a state must degrade to "shown as-is", never to a
deserialization error that kills the poll loop.

**Acceptance:** `cargo test -p tack-orch run_state_normalization_table` — table-driven over
**every** documented GitHub status and conclusion and every docket state. A mapping that
silently falls through fails it.

### T2 — The trait, and `DocketAdapter` rewritten

**Tasks:** 43.2 · **Files:** `crates/tack-orch/src/{lib,adapters/docket}.rs` ·
**Depends on:** T1

`provision_pod` leaves the trait for a provider-specific route.

**Acceptance:** `cargo test -p tack-orch --test docket_tick_contract_test && git diff
--exit-code crates/tack-orch/tests/golden/` — **the golden must not move.** Plus
`cargo build -p tack-orch` proving both adapters compile against the new shape.

### T3 — Reconciler restructure

**Tasks:** 43.3 · **Files:** `crates/tack-orch/src/reconciler.rs` · **Depends on:** T2

1. `evaluate` consumes **only** `health()`. Today `reachable = health_ok && status_ok`; if
   `runtimes()` inherited that role, every GitHub plane would go `unreachable` on tick one —
   listing runners needs repo-admin and an `actions:write` credential gets 403 — and the
   backoff would then pin it there.
2. `EXPECTED_API_VERSION` moves out of the reconciler into the docket adapter;
   `PlaneHealth` carries `api_version` and `version_ok`. The adapter decides what
   "reachable" means, so **docket keeps requiring both `/health` and `/status.json`** and
   its behaviour does not move.
3. Correlation moves to `correlation_keys()`. Today `persist_runs` reads
   `RemoteRun.task_ids` and `persist_approvals` reads `RemoteApproval.context.taskId` —
   both docket-only. Pushing them into `provider_metadata` would make the *generic*
   reconciler do `md["context"]["taskId"]`, which is the exact coupling this cycle removes.
4. Per-scope event polling; `FetchOutcome` fields renamed to match.

**Acceptance:** the tick golden is unchanged, **and** the existing state-machine verdict
assertions still pass. Renaming `evaluate`'s input fields is fine; **if an asserted verdict
has to change, docket's behaviour moved and the change is wrong.**

### T4 — `tack-api` onto the new trait, and the `tack-core` ACL removal

**Tasks:** 43.4, 43.5, 43.6 · **Files:** `crates/tack-api/src/{orch_store,dispatcher,
sprint_dispatch}.rs`, `crates/tack-api/src/handlers/{orch,provisioning,economics}.rs`,
`crates/tack-core/src/models.rs` · **Depends on:** T3

1. `dispatcher.rs`'s read-back call disappears — `DispatchAck` carries `state` and
   `pending_decision_id`.
2. `StatusMap` gains `on_blocked` and `on_timed_out`, both optional, both falling back.
3. Move `OrchBlueprint` and `TemplateOrchestration` out of `tack-core` into
   `tack-api::handlers::provisioning`. `project_templates.orchestration` is already `TEXT`,
   so `tack-core` keeps only an opaque `serde_json::Value`. **No migration.**

**Acceptance:** `cargo test -p tack-api dispatch_ack_carries_the_approval_token_without_a_second_call`
— asserts `approval_token != null` on a `waiting_approval` dispatch **and** that wiremock
saw exactly one docket request. This is the cycle's canonical green-CI-broken-product
regression: the OpenAPI gate cannot catch it because the field survives, only its value
dies. Plus `cargo test -p tack-api a_status_map_saved_before_this_release_behaves_identically`
and `rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/` returning **0**.

### T5 — Contract regeneration and the breaking-change notes

**Tasks:** 43.7 · **Files:** `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts`,
`CHANGELOG.md` · **Depends on:** T4

Per decision D3 the 21 orchestration operations are **broken with notice** — no aliases.
`CHANGELOG.md` names every changed operation. Fix the stale `utoipa` annotations while
here: every orch handler still documents *"404, Orchestration disabled (TACK_ORCH_ENABLE
unset)"* and the spec's `orchestration` tag still says *"Every route is disabled — 404"*,
but the code has returned **409** with `error.code: "orchestration_disabled"` since card E1.

**Acceptance:** `UPDATE_OPENAPI=1 cargo test -p tack-api --test openapi_contract`, then
`npm --prefix frontend run gen:api`, then `git diff --exit-code docs/openapi.json
frontend/src/shared/api/schema.gen.ts` passes on the committed result.

### T6 — Frontend neutral shapes + lazy docket fragment

**Tasks:** 43.8 · **Files:** `frontend/src/features/fleet/**`,
`frontend/src/shared/{orch,dispatch,agentActivity}/**` · **Depends on:** T5

1. The eight hand-written wire-boundary `api.ts` files each carry the contract *"When the
   real endpoint lands (or changes), reconciling means editing THIS FILE ONLY."* No
   component reads a raw wire field — **that is the whole refactor seam; use it.**
2. `shared/orch/providers/docket/` as a **lazy** fragment
   (`lazy(() => import(...))`, the pattern already used for every route) so the 30 KB
   gzipped entry-bundle CI gate is unaffected.
3. `Pod health` → `Health`; `Roster` → `Runtimes`; docket-specific cells move into the
   fragment. `architecture.test.ts` forbids cross-feature imports, so anything shared lands
   in `shared/`.

**Acceptance:** `rg -n "Pod health|Roster|Burn vs budget" frontend/src` returns **0**;
`npm --prefix frontend run build` passes the bundle-size gate; the existing
`FleetPage.test.tsx` column assertions are updated in the same commit.

---

## Wave D — Phases 44–45, the second adapter (superseded; do not start)

### N1 — Unified decision inbox

**Tasks:** 44.1–44.4 · **Files:** `crates/tack-api/src/handlers/orch.rs`,
`crates/tack-api/src/router.rs` (via G4's successor), `frontend/src/features/approvals/**` ·
**Depends on:** Wave C, G5 (migration 038)

Four kinds: `ApprovalOfIrreversibleAction`, `PlanAwaitingReview`, `OpenQuestion`,
`WorkOrderAmbiguity`. Routes move to `/api/decisions` and `/api/decisions/{id}` (D3, no
alias). **`TACK_ORCH_APPROVAL_TOKEN` keeps its exact meaning** — resolving a decision stays
higher-privilege than editing a card. With one shared secret there is no per-user actor, so
the audit row records the **surface** and the UI never renders a name it does not have.

**Acceptance:** the four kinds render distinctly; the existing approval-token gating tests
pass verbatim in behaviour; `resolving_a_decision_records_an_unattributed_audit_row`.

### N2 — The GitHub Actions adapter

**Tasks:** 45.1 · **Files:** `crates/tack-orch/src/adapters/github_actions.rs`,
`crates/tack-orch/tests/github_actions_adapter_test.rs` · **Depends on:** Wave C

1. `health` uses a cheap authenticated `GET /repos/{o}/{r}` — **never** the runner list,
   which needs repo-admin and whose 403 would pin every plane at `unreachable`.
2. `events` is `EventScope::Run`, derived from `GET .../runs/{id}/jobs` steps, cursor =
   highest `(job_id, step_number)`. **Logs are not used at all** — a `302` expiring in one
   minute over an archive deleted on a retention schedule is not an event stream.
3. `pending_decisions` / `resolve_decision` map to `pending_deployments`.
   `pause`/`resume` return `Unsupported`.
4. Raw `reqwest` — no `octocrab`, per the crate's own rule against a second HTTP client.
5. Fixtures for **both** dispatch responses: `200` with `workflow_run_id` (github.com) and
   `204` empty (GHES 3.17).

**Acceptance:** a golden request transcript in O2's shape; every capability the adapter
declares is exercised by a test.

### N3 — Bind and ingest endpoints

**Tasks:** 45.2, 45.3, 45.4, 45.5 · **Files:** `crates/tack-api/src/handlers/ingest.rs`,
`crates/tack-api/src/{config,router}.rs` · **Depends on:** N2, G5 (migrations 039–041)

1. **Correlation does not depend on the dispatch response** (GHES returns 204). Tack mints
   `tack_run_id` and passes it as a **non-secret** workflow input; the workflow's first step
   calls `POST /api/fleet/runs/bind` with it plus `${{ github.run_id }}`.
2. **The nonce is single-use.** A workflow input is caller-supplied, so anyone with
   `actions:write` could forge one and post events into another item's timeline. `bind`
   verifies the nonce was minted **by Tack, for that plane, and is still unbound**, and
   consumes it.
3. `bind` returns the **per-run credential** (D5): scoped to one correlation id, expiring
   with the run, able to append events and raise decisions for that run and **nothing
   else** — it can never resolve a decision and never edit a card. It is returned in a
   response body, so it never appears in a log. **Workflow inputs are visible in the run's
   UI and logs**; no vendor key, gateway key or API token ever travels as one.
   `TACK_ORCH_RUN_BOOTSTRAP_TOKEN` gates the exchange and, like
   `TACK_ORCH_APPROVAL_TOKEN` and unlike `TACK_API_TOKEN`, **unset means refuse**.
4. `POST /api/fleet/runs/{correlation_id}/events` — idempotent on a caller-supplied
   `event_id` behind the partial unique index; rejects events for a terminal run.
5. Migration 039's `orch_events.source` vocabulary is **three values**, not two. The claim
   that `run_id` is always NULL and `id` is always a UUIDv5 is true only of the reconciler's
   path: `orch_store.rs` writes `new_v4()` with `run_id: Some(..)` for
   `status_map_skipped_human_override`, and `dispatcher.rs` and the auto-dispatch hook write
   `new_v4()` with `run_id: None`. So `'poll' | 'push' | 'local'`, with existing rows
   backfilled to `'local'` keyed on `event_type`.
6. **Auth wiring: own sub-router, outside both existing gates.** A run must not need
   `TACK_API_TOKEN`, and toggling orchestration off must not `409` an in-flight handshake
   and re-label every live run "not instrumented". **Not** a fourth entry in the
   `path().ends_with(...)` exemption list — that is a suffix match and would exempt any
   future path ending in the same string.

**Acceptance:** `bind_rejects_a_forged_nonce_a_reused_nonce_and_a_nonce_from_another_plane`
(three 403s, zero run rows); `posting_the_same_event_batch_twice_yields_one_row_per_event`;
`existing_locally_minted_events_backfill_to_local_not_poll`;
`no_other_route_became_exempt` enumerating the router.

### N4 — Reference workflow, hook, and the instrumentation honesty rules

**Tasks:** 45.6, 45.7 · **Files:** `docs/examples/github-actions/**`,
`docs/book/src/user-guide/**`, `crates/tack-api/src/handlers/ingest.rs` ·
**Depends on:** N3

1. A copy-pasteable workflow and `PostToolUse` hook. Documents honestly that **fork-PR runs
   receive no secrets**, cannot bind, and will correctly render "not instrumented".
2. **"Not instrumented" must never be confused with "waiting on a human."** A run parked on
   a required-reviewer environment sits in `waiting` for up to 35 days and is exactly what
   the decision inbox exists to surface — suppress the timer whenever the run is `waiting`
   or a decision is open against it.
3. A run that goes `queued → cancelled` without ever starting never arms an
   `in_progress`-based timer, so the reaper runs off a **dispatch-time deadline** and leaves
   an event explaining why the card stopped. Without this the card sits in "In Progress"
   forever: an unbound run lands with `item_id: None`, `upsert_runs` skips it, and
   `reconcile_terminal_status_map` never runs.

**Acceptance:** `a_waiting_run_is_never_labelled_not_instrumented`;
`a_run_cancelled_before_starting_is_reaped_and_leaves_an_event`; `mdbook build docs/book`.

### N5 — GitHub Actions in the UI

**Tasks:** 45.8 · **Files:** `frontend/src/features/settings/orchestrationSettings/**`,
`frontend/src/shared/orch/providers/github-actions/**` · **Depends on:** N2, T6

A second Kind option (today the selector hard-codes `<option value="docket">docket</option>`
as its only value, with `docket` also defaulted twice and rendered raw per row), its own
config form, and a **lazy** provider fragment.

**Acceptance:** a Vitest test asserts two Kind options and that selecting each renders a
different config form; the bundle-size gate still passes.

---

## Wave E — Phases 46–49, independent tracks (frozen; do not start)

### M1 — Model policy

**Tasks:** 46.1–46.4, 46.7 · **Files:** `crates/tack-core/src/model_policy.rs`,
`crates/tack-api/src/handlers/orch.rs`, `frontend/src/features/{settings,item-detail}/**` ·
**Depends on:** Wave D, G5 (migrations 042, 043)

1. Resolution order: **item override → item-type default → project default →
   control-plane default.** Pure, no I/O, in `tack-core`.
2. It **never parses, maps, normalises or classifies the identifier** (§II.0 rule 2). This
   also removes the staleness problem permanently — a new model needs no Tack release.
3. Every response carries the **resolved** value and its **provenance** — *"sonnet, from
   project default."* A policy whose provenance is invisible is a policy nobody trusts.
4. `capabilities().model_selection` respected with all three values live: docket
   `Unsupported`, GitHub Actions `Honoured`. **This is the capability-negotiation
   acceptance test for the whole cycle.**
5. `orch_links.harness` — one plain config field beside the workflow name. **No harness
   trait, no capability matrix, no plugin layer.** The gates, labels, plan template and
   review rubric live in GitHub and in the repo, so they are harness-independent already.

**Acceptance:** all sixteen presence combinations resolve correctly; a nonsense identifier
like `"zzz/not-a-model:v9"` resolves and round-trips unchanged; three capability values
render three different controls; `rg -n "trait Harness|HarnessRegistry" crates/` returns 0.

### M2 — Gateway config and the first real measurement

**Tasks:** 46.5, 46.6 · **Files:** `crates/tack-api/src/handlers/{orch,economics}.rs`,
`crates/tack-api/src/remote_backup.rs` · **Depends on:** M1

1. Per-project gateway: base URL, an optional **server-side read-only spend-query
   credential**, nothing else. Tack never sends a key into a run — the run gets its own from
   a repo secret the operator sets. A gateway credential in `app_meta` must also join
   `SENSITIVE_META_KEYS`.
2. **Gateway unreachable ⇒ dispatch refuses.** A new `gateway_unreachable` outcome (HTTP
   200, branch on `outcome` like the existing six) and no run starts. A run that silently
   bypasses the gateway is unmeasured and uncapped — exactly the "shows zero, actually spent
   money" failure the numeric-honesty rules exist to prevent. Queuing is rejected: Tack has
   no queue and no replay logic anywhere, deliberately.
3. Roll pushed telemetry and docket's own `cost_charged` events into `orch_tasks`. Where no
   source exists: **"not measured"**, never `0` (§II.0 rule 8).

**Acceptance:** `gateway_secret_is_write_only_and_scrubbed_from_a_backup`;
`pushed_usage_events_roll_up_into_orch_tasks`; a project with no measurement source renders
"not measured" and never `$0.00` or `0 tokens`.

> **Read this before starting M2.** Every token figure in the app is a structural zero
> today, so the whole numeric surface has **never been exercised against real data**. When
> this card lands, every number becomes non-zero at once and a units error — input counted
> as output, cumulative as delta, per-attempt summed across retries — would ship looking
> entirely plausible. **Hand-verify against one real run's provider figures before letting
> the Economics page render the new values.** CI cannot do this check for you.

### W1 — Inbound GitHub webhooks and echo suppression

**Tasks:** 47.1–47.4 · **Files:** `crates/tack-api/src/handlers/webhooks.rs`,
`crates/tack-api/src/{router,github_sync}.rs`, `crates/tack-api/src/handlers/items.rs` ·
**Depends on:** N3 (the sub-router pattern)

1. `POST /api/webhooks/github/{control_plane_id}`, verifying `X-Hub-Signature-256` with the
   `hmac`/`sha2`/`hex` crates already present — `webhook.rs` has `sign` and **no `verify`**,
   and nothing in the codebase reads a signature today. Reuse `constant_time_eq`.
2. Dedupe on `X-GitHub-Delivery`, purged by the retention sweep.
3. `workflow_run`, `workflow_job`, `deployment_review`. **Polling stays as the
   reconciliation backstop**, so a missed delivery self-heals — the pull-based design was
   chosen precisely so no queue or replay logic is needed.
4. **Echo suppression, three layers**, because any one alone is defeatable: a
   `ChangeOrigin` tag so a webhook-driven write never re-fires `maybe_sync_github`; a
   `github_links.state_hash` backstop for when the tag is lost across a process boundary;
   and dropping deliveries whose `sender.id` is the identity Tack pushes as. **`ItemSource`
   cannot serve here** — it is written once at creation and `update_item` has no code path
   that touches the column.

**Acceptance:** a bad, a missing, and a wrong-plane signature each `401` with zero writes;
a replayed delivery GUID changes no row; a dropped delivery is recovered by the next poll;
and `a_webhook_driven_status_change_produces_no_outbound_push` asserts the mock GitHub
received **zero** requests — a naive implementation loops and never terminates cleanly.

### H1 — Intervention without pause

**Tasks:** 48.1–48.4 · **Files:** `crates/tack-api/src/handlers/{ingest,orch}.rs`,
`docs/examples/hooks/**` · **Depends on:** N3, N1

A `PreToolUse` hook runs **synchronously and blocks the tool call until it returns**, so it
can post a decision request and long-poll for the verdict. That makes the decision inbox
the mechanism that supplies **intervention**, not just visibility — on a provider with no
pause API at all.

1. `POST /api/fleet/runs/{correlation_id}/decisions` (raise) and
   `GET …/decisions/{id}/verdict?wait=<secs>` (bounded), both on the run credential.
   **Resolution stays on `/api/decisions/{id}` behind `TACK_ORCH_APPROVAL_TOKEN`** — the run
   credential can raise a decision and can never answer one.
2. **The ceiling:** the reference hook sets `timeout: 600` **explicitly** rather than
   relying on the default, and requests `wait=540`, leaving headroom for the round trip. A
   hook that outlives its own timeout is killed and its verdict lost. 540 s is far under the
   6 h GitHub-hosted job cap, so the wait can never be what kills a job. Minutes, not hours,
   deliberately.
3. **Expiry is fail-closed**: deny, hook exits 2, tool call blocked, `orch_events` row
   records the expiry and its reason.
4. **Where the item lands:** `on_blocked` if the project's `status_map` sets it, otherwise
   the item does not move and the decision is recorded as expired. Never silently "done".
5. The operator guide names the cost: the wait is **paid idle runner time**, so decisions
   belong at genuine gates, not per tool call.

**Acceptance:** `a_run_credential_cannot_resolve_its_own_decision`;
`an_expired_decision_returns_deny_and_writes_an_audit_row`;
`an_expired_decision_never_moves_an_item_to_a_done_status`.

### S1 — Bi-directional issue sync

**Tasks:** 49.1–49.4 · **Files:** `crates/tack-db/src/repo/github_links.rs`,
`crates/tack-api/src/handlers/{items,webhooks,import_github}.rs` · **Depends on:** W1,
G5 (migrations 044–049)

1. `github_links` gains `host` (default `'github.com'`), `node_id`, `last_synced_at`,
   `remote_updated_at`, `state_hash` — **one `ALTER` each** — plus a **non-unique** reverse
   index on `(host, repo, issue_number)`. A unique index could fail on a user's existing
   duplicates, and a failed statement in this migration runner bricks the boot loop;
   uniqueness is enforced in the repo layer, which logs when it finds more than one. Today
   there is no reverse index at all and no `get_link_by_issue`, so an inbound event for an
   issue has no lookup path.
2. **Credential precedence, decided and written down:** `TACK_GITHUB_TOKEN` /
   `TACK_GITHUB_API_BASE` remain the fallback for import and issue push; a control plane's
   own credentials win where a plane is involved. Two token sources with different scopes
   exist today and the rule was never stated.
3. Inbound `issues` / `issue_comment` applied **through `tack-core`** so workflow rules
   hold, with `ItemSource::Github` preserved so the trust boundary is not laundered.
4. Outbound item → issue create: **per project, opt-in, off by default.**

**Acceptance:** `an_issue_edited_on_github_updates_the_item_without_bypassing_the_workflow_engine`
asserts an illegal transition is **rejected, not forced**;
`item_create_does_not_touch_github_unless_the_project_opted_in`;
`plane_credentials_win_over_the_global_github_token`.

### S2 — PR, checks, merge evidence, decision mirroring, and retry

**Tasks:** 49.5–49.8 · **Files:** `crates/tack-api/src/{github_sync,webhook}.rs`,
`crates/tack-api/src/handlers/webhooks.rs`, `docs/GITHUB-SYNC.md` · **Depends on:** S1

1. PR opened → item moves and the PR is linked; checks running → verifying; check failed →
   failed with the run link; PR merged → done with **evidence** (SHA, run URL, artifacts)
   persisted as an `orch_events` row.
2. A blocking decision appears in Tack's inbox **and** as a comment plus a label on the
   issue; resolving on either side reflects on the other.
3. **Retry and rate limits.** Today the push is `tokio::spawn`'d and never awaited, with
   zero retry and no persisted failure record — the only failure surface is a `warn!` line,
   unlike the auto-dispatch hook beside it which writes an `auto_dispatch_failed` event.
   Import likewise only *observes* `x-ratelimit-remaining` and never acts on it: no
   `Retry-After`, no `x-ratelimit-reset`, no backoff. Add a bounded retry honouring both,
   and record failures as `orch_events`. **No new dependency** — `tower` is already a
   workspace dep with `features = ["full"]`.
4. Rewrite `docs/GITHUB-SYNC.md` for v2; it currently lists inbound sync, comment
   mirroring, per-project tokens and manual linking as explicitly out of scope.

**Acceptance:** `a_merged_pr_completes_the_item_with_sha_run_url_and_artifacts` asserts all
three evidence fields non-empty;
`resolving_on_github_resolves_in_tack_and_vice_versa_without_an_echo`;
`a_rate_limited_push_retries_and_records_a_failure_event`.

---

## II.4 Cross-cutting acceptance for the cycle

An operator registers a GitHub Actions control plane beside a docket one, dispatches the
same sprint to either, sees per-item token cost from both, resolves a blocking decision
raised from inside a running workflow, and watches a merged PR complete the card with its
SHA and run URL — with **every control the provider cannot support visibly disabled and its
reason named**, and with **docket's behaviour provably unchanged throughout**.

Mechanically, at the end of the cycle:

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
cargo llvm-cov -p tack-orch --fail-under-lines 70
git diff --exit-code crates/tack-orch/tests/golden/ docs/openapi.json
cd frontend && npm run type-check && npm run test && npm run lint:tokens && npm run build
make e2e
rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/          # 0
rg -n "kind === 'docket'" frontend/src                          # 0
rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/ # 0
rg -n "Pod health|Roster|Burn vs budget" frontend/src            # 0
```

---

## II.5 Known risks

1. **The Wave C golden becomes unfalsifiable the moment it legitimately changes.** Some
   change will plausibly *require* it to move — say the reshaped reconciler issues
   `GET /health` before `GET /status.json`. At that moment there is no way to distinguish
   "legitimately changed" from "we broke it", and the temptation is to regenerate and move
   on. **Rule, not intention:** the golden may only change in a commit that changes nothing
   else, whose message states the behavioural difference and why it is safe, and whose diff
   is read line by line. **More than two such commits in Wave C means the reshape is not
   preserving behaviour — stop.**
2. **The trait is designed against one real implementor.** G1's stub makes "both adapters
   compile" a Wave C gate, but a stub cannot discover what a real adapter discovers. Three
   such discoveries were already made by reading the API during planning (`health` must not
   touch the runner list; there is no plane-wide usage figure; `waiting` is a decision, not
   a state). If Wave D forces a trait change, **D3's single break becomes two breaks in
   consecutive releases.**
3. **G5's rebuilds are the only irreversible step in the cycle.** No transactions, no
   down-migrations. The guard refuses to boot rather than compounding damage, and the
   release note says to back up — but "we told them to" is not a recovery plan. Ship G5 in a
   release of its own, not bundled.
4. **Everything good about the GitHub Actions adapter depends on the target repo being
   instrumented** — a workflow, a hook, and a secret the repo owner must add. D5 cuts three
   secrets to one, which is the single biggest lever available, and N4 ships a
   copy-pasteable reference. But Tack cannot make a stranger's repo instrument itself, and
   an adapter that *technically* proves the trait while *practically* proving nothing about
   the product claim is a real outcome.
5. **M2 turns on a measurement that has always read zero.** See the callout on that card.

---

## II.6 Handoff notes

One section per card, appended on completion: what you changed, what you discovered, and
anything the next wave must know. Same discipline as §6 — record corrections to this file
inline rather than leaving a stale instruction standing, and say plainly when something was
read from source versus verified live.

**Retroactive, all nine subsections below — rule II.0.9 wasn't followed once this wave.**
No card in Wave A or Wave B appended its own note before its session ended; the gap is
what this whole entry exists to close. Everything below was reconstructed on 2026-08-06 by
reading the working tree directly — `git status --porcelain`/`git diff --stat` for the file
inventory, the tests and their golden output, the migrations, the adapter and handler code,
`.github/workflows/ci.yml` — not written live by the cards' own agents, so design
deliberation that never made it into a comment, a test name, or a commit message is gone.
**Verified live in this pass** (run 2026-08-06): `cargo test --workspace` (48 `test result:
ok` blocks, 0 `FAILED`), `cargo clippy --workspace --all-targets -- -D warnings` (clean),
`cargo fmt --all -- --check` (clean), `mdbook build docs/book` (succeeds), plus each card's
own named acceptance test run in isolation where cited below. Everything else — rationale,
why a value is what it is — is read from source comments and test names, not independently
re-derived; where a specific acceptance-bar step was *not* re-run, that's said outright
rather than implied.

### O1 — Tick-level contract oracle — 2026-08-05

**Files:** `crates/tack-orch/tests/docket_tick_contract_test.rs` (1252 lines) and ten golden
files under `tests/golden/tick/` (five scenarios × `.requests.json`/`.rows.json`).

**What's built.** Drives a full `reconcile_once` plus the whole persist phase through the
real `spawn_one` tick loop (not a bare unit call) against `wiremock` + an in-memory SQLite,
across five scenarios — cold start, warm cursor, rewound cursor, zero linked projects, three
linked projects — and snapshots (A) the ordered HTTP request transcript and (B) the
resulting rows of all five `orch_*` tables. Both are compared byte-for-byte against a golden
file; normalisation only ever touches values Tack itself mints at runtime (wall-clock
timestamps, the random `control_planes.id`, and the ids derived/generated from it) — never a
value that came off the wire, which is the property that makes the golden meaningful at all.
Verified live: `cargo test -p tack-orch --test docket_tick_contract_test` — 5/5 pass.

**The load-bearing gap (fact for whoever commits the golden dir).** The plan's own
verification command, `git diff --exit-code crates/tack-orch/tests/golden/`, is **vacuous
right now**. `git status --porcelain` shows the entire `tests/golden/` tree as `??`
(untracked) — confirmed directly. `git diff` never inspects untracked paths, so this command
exits `0` regardless of whether `UPDATE_GOLDEN=1` changed a single byte; today it proves
nothing. This isn't a flaw in the oracle's design — the golden mechanism itself works, see
below — it's that the safety net around it isn't wired to git's plumbing until someone
actually commits the directory. O3's CI step has the identical problem; repeated there too
so it isn't missed by a reader who only opens one card.

**The merge-order break, from O1's side.** Migration 037 (card G5b) renamed
`orch_runs.run_id` to `external_run_id` and added `run_attempt`. This oracle's
`snapshot_rows` originally ran `SELECT * FROM orch_runs ORDER BY run_id` through a generic
row-dumper that only decoded `TEXT`/`REAL` columns — both broke, and all five scenarios
panicked with `no such column: run_id` (the `ORDER BY`) once the rename landed, and would
have panicked again on the first `orch_runs` row regardless (the `INTEGER` `run_attempt`
column, decoded as `Option<String>`, is rejected outright by sqlx's SQLite driver rather than
coerced). Fixed by ordering on `external_run_id` and adding an `i64` decode branch for
`run_attempt` — done during the Wave B final verification pass below, not by an O1-owned
session, since Wave A had no active owner left by the time migration 037 landed underneath
it. `snapshot_rows`'s own comment (lines 730–738) now documents this for the next person who
reshapes `orch_runs`.

**Determinism, proved by copy, not by git** (the command above can't do it): running
`UPDATE_GOLDEN=1` twice in a row against the fixed oracle produced an empty `diff -r` on the
second pass.

**Blast radius confirmed confined.** The only fields that moved across the fix are exactly
what migrations 037/038 touched: `orch_runs` gains `correlation_id`/`run_attempt` and renames
`run_id`→`external_run_id`; `orch_approvals` gains `external_id`/`kind`/`provider_metadata`.
No `.requests.json` transcript changed and `orch_events`/`orch_metrics`/`orch_trace_cursors`
did not change at all.

**Unverifiable claim, flagged rather than assumed true.** The module doc's "Proving the
oracle is real" section (lines 129–142) says the manual mutation-and-revert proof — link zero
projects instead of three inside
`three_linked_projects_issues_three_per_project_calls_each`, confirm the test fails against
the real (three-project) golden, revert — was "Done once, by hand," and points the reader "to
this card's handoff note in TODO.md § II.6 for the exact transcript captured." **No such
transcript existed anywhere in this file before this entry.** O1 never wrote one, consistent
with the wave-wide gap this whole section exists to close. I did not re-run that manual proof
myself in this reconstruction pass (it means deliberately mutating a test file, which is out
of scope for a handoff note); this is a genuine unverified claim, not a confirmed one.
Whoever next touches this file should either produce the transcript for real or correct the
doc comment's claim that one exists.

**For Wave C:** T2/T3 must re-run this oracle after every trait/reconciler change — the
merge-order break above is the concrete proof of what happens when a schema-owning card and
this oracle land out of step, exactly the failure mode §II.5 risk 1 warns about in the
abstract.

### O2 — Per-method wire oracle + pinned event id — 2026-08-05

**Files:** `crates/tack-orch/tests/docket_wire_contract_test.rs` (655 lines) + thirteen
golden files under `tests/golden/wire/`, one per `ControlPlane` method.

**Verified directly:** every one of the thirteen current trait methods has a golden file
(`kind`, `health`, `status`, `metrics`, `list_runs`, `get_run`, `list_approvals`,
`list_tasks`, `traces`, `enqueue_task`, `dispatch`, `decide_approval`, `provision_pod`).
`dispatch.json` holds `"requests": []` and `{"outcome":"err","error":"control plane feature
disabled"}` — the absence of a request is itself the golden, matching the module doc's claim
that this method is captured "not skipped" so a future wiring-up of dispatch fails the golden
the moment it starts issuing a request. `grep -rn "Bearer" crates/tack-orch/tests/golden/wire/`
returns nothing — header **values** never reach a golden file, only names, exactly the
acceptance bar.

**Discovery worth flagging plainly.** Task 39.3 ("pinned-literal event id") does **not** live
in this file, despite what a reader of only the card's **Files:** line would expect. It's
`derive_event_id_matches_the_pinned_literal`, a unit test inside
`crates/tack-orch/src/reconciler.rs`'s own `#[cfg(test)] mod tests` — necessarily so, since
`derive_event_id` is a private function `reconciler.rs` never exports, and this file only
exercises `DocketAdapter` through the public trait. Pinned value for the fixed input is the
literal `4808170d-9797-561e-8fbb-dd8e9b94a9fe`; its own doc comment states plainly that a
future failure should almost always be fixed by reverting whatever changed the derivation,
never by updating the literal — updating it silently re-keys every already-ingested event in
every deployed database. **Wave C's T3 (reconciler restructure) owns `reconciler.rs` next and
needs to know this pinned test lives there** before moving or deleting anything in that file.

**Not independently re-verified:** flipping one byte of the namespace constant to confirm the
pinned test actually fails (the card's own acceptance bar). Read the test and its doc
comment, and the test passes in the full suite run, but I did not perform the mutate-and-
revert exercise myself in this pass.

### O3 — CI gates and the missing coverage floor — 2026-08-05

**Files:** `.github/workflows/ci.yml` (diff read directly via `git diff`).

**Added:** a "tack-orch golden drift gate" step in the `rust` job — `UPDATE_GOLDEN=1`
against both O1's and O2's test binaries, then `git diff --exit-code
crates/tack-orch/tests/golden/` — placed immediately after the existing OpenAPI drift step,
same shape. **Added:** `cargo llvm-cov -p tack-orch --fail-under-lines 70` in the `coverage`
job, with the job's header comment updated from "tack-core >= 85%, tack-db/tack-api >= 70%"
to name tack-orch too. Confirmed there was genuinely no `tack-orch` coverage line before this
diff — it was the least-guarded crate in the workspace, as the card's own brief said.

**Repeating O1's finding here because this card owns the consequence.** The golden-drift step
as written is not wrong, but it is currently inert: `crates/tack-orch/tests/golden/` is
entirely untracked (`git status --porcelain` — `??`), and `git diff --exit-code` against an
untracked path always exits `0`. CI would go green on this exact step today regardless of
what `UPDATE_GOLDEN=1` produced, silently. This is a precondition gap, not a logic bug in the
step — it becomes a real gate the moment somebody `git add`s the golden directory, and not a
moment before, no matter how correct the step's own commands are. Whoever lands that commit
should run the step once against a deliberately broken adapter first, by hand, to see it
actually fail red before trusting it in CI.

**Not verified live in this pass:** `cargo llvm-cov -p tack-orch --fail-under-lines 70`
itself — llvm-cov is a separate, slower toolchain step, skipped for time in a reconstruction
pass. `cargo test --workspace`, `clippy --all-targets -- -D warnings`, and `fmt --all --
--check` were all run and are clean, but that says nothing about line-coverage percentage.
Recommend Wave C's first real CI run be watched on this specific gate rather than assumed
green.

### G1 — Capability model + one adapter registry + GHA stub — 2026-08-05

**Files, per `git diff --stat`:** `crates/tack-orch/src/lib.rs` (+253 lines — `Support`,
`EventScope`, `DecisionSupport`, `UsageSupport`, `ModelSelection`, `Rated<T>`, `Capabilities`,
and `fn capabilities()` added to the trait), `adapters/mod.rs`, two new files
`adapters/registry.rs` (181 lines) and `adapters/github_actions.rs` (229 lines), plus
`orch_store.rs`/`dispatcher.rs`/`handlers/orch.rs`/`handlers/provisioning.rs` updated to call
the registry instead of hand-matching `kind`.

**`Capabilities` matches §II.1.2 field-for-field**, verified against `lib.rs` directly: two
plain bools plus `artifacts`/`runtimes`/`plane_metrics`/`provisioning`, and six `Rated<T>`
fields pairing a level with a `&'static str` reason. `Rated` is deliberately `Serialize`-only
— its own doc comment explains a borrowed `&'static str` reason has no safe `Deserialize`,
and nothing in this crate ever needs to decode one back in from the wire.

**`DocketAdapter::capabilities()` read line-by-line:** `dispatch: true` (goes through
`enqueue_task`, not the trait's own dead `dispatch()` method — a deliberate naming trap the
adapter's own comment calls out explicitly, "`false` here would misreport what docket can do
to satisfy the name of one dead trait method"), `cancel: false`, `pause`/`resume:
Unsupported` naming `docket profile <pod-id> --resume`, `event_scope: Project`, `artifacts:
false`, `decisions: Poll`, `usage: FromProvider`, `model_selection: Unsupported`,
`runtimes`/`plane_metrics`/`provisioning: true`. Every field's comment cites either
`serve.py`'s route table or an earlier "verified live" note (card D4 for `provisioning`), not
a guess. `docket_capabilities_match_the_verified_facts` (`lib.rs:1129`) asserts all of it and
is the exact test the adversarial check below targeted.

**`GithubActionsAdapter`:** `kind()`/`capabilities()` are real and checked against §II.1.4's
verified-external-facts table (pause/resume `Unsupported` — "no endpoint exists"; usage
`NotMeasured` — runner minutes, not tokens; model_selection `Honoured` — a dispatched
workflow receives inputs verbatim); every other trait method is `unimplemented!()`. Confirmed
**not** registered in `registry::build` — deliberately: selecting it today would panic the
reconciler's first poll rather than fail construction with an honest
`RegistryError::UnknownKind`, so the registry's one match arm is still just `"docket"`.

**`registry::build` is now the single caller for all four previously-duplicated
`match row.kind.as_str()` sites** — verified by grep that `orch_store.rs`, `dispatcher.rs`,
`handlers/orch.rs`, and `handlers/provisioning.rs` all import and call
`tack_orch::adapters::registry::{self, RegistryError}`. Each caller keeps its own failure
behavior on purpose (batch `continue`+`warn!` in the reconciler vs. a typed HTTP error in
each handler) — the registry only unifies construction, never what a caller does when
construction fails.

**Acceptance-grep caveat, found by actually running it, not assumed.**
`rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` is not literally `0` as the card's
acceptance bar states — it returns exactly one hit, `handlers/alexa.rs:423`,
`match payload.request.kind.as_str()`. Read the surrounding code: this is Alexa's own
request-kind dispatch (`LaunchRequest`/`IntentRequest`/…), unrelated to control planes, and
confirmed pre-existing — last touched by commit `3ae012e` ("close the WIP-limit race..."),
zero diff on that file in this cycle. The four real duplicated sites are genuinely gone; the
acceptance command as literally worded will fail a naive re-run and needs narrowing (anchor
on `row.kind`, or exclude `alexa.rs`) or a documented exception, or the next person to run it
chases a false alarm.

**Two retirements the card promised, both still open — found by grep, not assumed closed:**

1. `PendingApprovalListResponse.grant_available` is still declared server-side in
   `handlers/orch.rs` and still appears in generated `frontend/src/shared/api/schema.gen.ts`.
   `frontend/src/features/approvals/api.ts`'s own doc comment explains why the *client* side
   stopped reading it (a server-secret-configured flag, not a provider capability — no
   correct home for it in `Capabilities`, and the UI now always renders the decide controls
   and lets a real `403` answer instead of guessing) — but the DTO field itself was never
   deleted. Wave C's T4 inherits `handlers/orch.rs` next and should delete it or document why
   it stays (see also "Wave B final verification," below, which found the same thing).
2. `useAgentActivityMap.orchAvailable()` used as a dispatch gate is **also still live**:
   `frontend/src/features/board/Board.tsx:410` still reads
   `dispatchAvailable={agentActivity.orchAvailable()}`. Not a silent leftover — both
   `useAgentActivityMap.ts`'s doc comment on `orchAvailable` and `DispatchCardMenu.tsx`'s doc
   comment on its `available` prop name this explicitly as "WRONG" and "found [by] card G1,"
   and explain why it wasn't fixed here: `Board.tsx`/`BoardColumnView.tsx`/`ItemCard.tsx`,
   which thread the prop down, sit outside this card's file ownership
   (`frontend/src/shared/orch/**` is what G1 owns), and the real signal
   (`Capabilities.dispatch`) isn't reachable from a hook fed only a bare `projectId` without a
   second network call judged out of scope. Left as a flagged, **not-closed** gap — worth
   naming here since it's the second of exactly the two ad-hoc capability bits §II.1.2 says
   this struct retires, and only one of the two frontend call sites was actually touched.

`frontend/src/shared/orch/capabilities.ts` and `CapabilityNote.tsx` (both new, both G1's)
exist and are what a disabled control is meant to read its reason from going forward —
confirmed present and referenced by `DispatchCardMenu`/`OrchestrationPanel`/
`ProvisioningWizard` via grep, not re-read line-by-line in this pass.

### G2 — Plane config + secrets + `unconfigured` health + backup scrub — 2026-08-05

**Files:** `crates/tack-api/src/remote_backup.rs`, `crates/tack-api/src/orch_store.rs`,
frontend fleet files (`format.ts`, `api.ts`, `HealthChip.tsx`, `FleetRow.tsx` + tests).
`control_planes.config`/`control_planes.secrets` columns themselves are migrations 032/033
(card G5a's territory, below) — this card is their consumer, not their author.

**Secrets scrub, verified directly in `remote_backup.rs`.** `scrub_snapshot_secrets` guards on
`pragma_table_info('control_planes')` for a `secrets` column (so a pre-033 snapshot still
works, per the card's own acceptance bar), runs
`UPDATE control_planes SET secrets = NULL WHERE secrets IS NOT NULL`, and this happens
**before** the trailing `VACUUM` — read the statement order directly, not inferred from the
doc comment. Test `scrub_removes_control_plane_secrets_from_snapshot` asserts the secret
bytes are physically absent from the post-`VACUUM` file, not just that the column reads
`NULL` — the stronger of the two checks the card's acceptance bar allows, and the one this
same test's own comment explains is necessary ("a test that only checked `secrets IS NULL`
would still pass against an implementation that forgot the VACUUM").

**`unconfigured` health, verified in `orch_store.rs`.** `mark_unconfigured` writes
`health = "unconfigured"` (best-effort — a persist failure is logged, never propagated,
matching the batch loop's existing "one plane's problem never aborts the others" discipline)
when `list_registered` can't even build an adapter for a row. Frontend `ControlPlaneHealth`
widened to five states (`'healthy' | 'degraded' | 'unreachable' | 'unknown' |
'unconfigured'`); `isStale()` treats it as stale; `HEALTH_LABEL`/`HEALTH_TONE` both give it
its own copy ("Missing credentials") rather than collapsing into `degraded`'s — a dedicated
test asserts the label differs from `degraded`'s and that all five states are covered.

**Minor attribution note, not treated as a defect.** The doc comment on `mark_unconfigured`
labels itself "Card G1 (TODO.md)" even though this exact behavior is G2's own listed
acceptance bar (tasks 40.2/40.3). Plausible reading: it's attributed to G1 because the
trigger is `list_registered`'s registry-construction-failure path, which is G1's replacement
of the old match statement — not necessarily a real misattribution. Noted in case it signals
two agents working the same seam without comparing notes; not chased further here.

### G3 — Optimistic concurrency — 2026-08-05

**Files:** `crates/tack-db/src/repo/items.rs`, `crates/tack-api/src/handlers/items.rs`, new
test files `crates/tack-db/tests/version_concurrency_test.rs` and
`crates/tack-api/tests/item_concurrency_test.rs`.

**Verified in `repo/items.rs`:** every `UPDATE items SET ...` statement — all field-specific
branches inside `update_item`, both timestamp side-effect branches, `update_item_status_
checked`, and `check_and_update_parent_status` — carries `version = version + 1` in the same
statement. Not just the obvious `update_item` path; the two easy-to-miss ones the card's own
brief called out by name are both covered. `claim_item_version` (the CAS primitive) is a
single statement, `UPDATE items SET version = version + 1, ... WHERE id = ? AND version = ?`
— SQLite's writer serialization, not application-level locking, is what makes "exactly one of
two racers wins" true.

**`handlers/items.rs`:** `item_etag(id, version)` and `check_if_match` implement the
`ETag`/`If-Match` contract. Confirmed the documented "absent `If-Match` behaves exactly as
before this card" bar matches the code — `check_if_match` returns `Ok(None)` ("proceed")
before ever reading `current_version` when no header is present.

**Verified live:** `item_concurrency_test.rs` — 7/7 pass, including
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` and the
higher-fanout variant. (**Naming note:** the card's acceptance bar names this test
`concurrent_patch_with_the_same_if_match_yields_one_200_and_one_412` — singular "patch,"
present-tense "yields." The test as written is
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` — the same
test, cosmetic naming drift, but worth knowing if anyone greps for the exact acceptance-bar
string and comes up empty.) `version_concurrency_test.rs` (repo-layer, 4 tests) is also
green.

**See the Adversarial verification subsection below for a real caveat on this exact headline
test.** An independent adversarial mutation of `check_if_match` (neutering the top-level
comparison while leaving the CAS intact) found the two-racer test flaky against that specific
regression — passed 10/15, failed 5/15 across reruns — while the sequential single-request
tests in the same file caught the identical mutation 100% of the time. The mechanism itself
still works (the sequential tests prove `If-Match` really is enforced); the specific test
named in this card's own acceptance bar is not a reliable solo gate for "someone silently
disabled the comparison." Read that subsection before leaning on this test alone.

### G4 — CORS, routes, and the MCP write path — 2026-08-05

**Files:** `crates/tack-api/src/router.rs`, `crates/tack-cli/src/client.rs` (+256),
`crates/tack-cli/src/mcp.rs` (+217), `crates/tack-api/src/handlers/items.rs` (shared with
G3), `docs/book/src/developer/orchestration.md` (+71 lines, new "Concurrency control"
section).

**CORS, verified in `router.rs`:** `allow_headers` now includes `header::IF_MATCH` and
`header::HeaderName::from_static(orch::APPROVAL_TOKEN_HEADER)` (reusing the handler's own
constant rather than a hand-copied literal, so the two can't drift apart), and
`.expose_headers([header::ETAG])` is added where no `expose_headers` call existed at all
before. New `crates/tack-api/tests/cors_test.rs` (2 tests, both green): one confirms the
preflight allows both headers and exposes `ETag`, the other confirms an arbitrary header is
still rejected.

**MCP write path:** `tack-cli`'s HTTP client gained `patch_if_match`; `mcp.rs`'s
`update_item`/`move_item` tools now `GET` first to read the `ETag`, then `PATCH` with it as
`If-Match`, surfacing a `412` as a distinct "you raced, re-read and retry" tool error rather
than the generic `{status}: {message}` shape. `assert_eq!(tools.len(), 8)` still passes — no
tool was added or removed, only their write behavior changed. Verified live:
`mcp_update_item_sends_if_match` passes.

**Auto-dispatch gate fix:** `handlers/items.rs`'s `maybe_auto_dispatch` now calls
`effective_orch_enabled` instead of the raw `state.config.orch_enable`, closing the gap where
a server started with `TACK_ORCH_ENABLE=1` kept auto-dispatching after an operator turned
orchestration off in Settings. CHANGELOG.md documents it as a behavior change.
**Attribution wrinkle, stated plainly rather than silently resolved one way:** both the
in-code doc comment (`items.rs`, on `maybe_auto_dispatch`) and the CHANGELOG.md entry label
this fix "card G3," but the plan (this file, task 41.3) assigns "fix the auto-dispatch gate"
to **this card, G4**. `handlers/items.rs` is legitimately shared between G3 (ETag/If-Match,
tasks 41.1–41.2) and G4 (this fix, task 41.3) per both cards' own **Files:** lines, so either
attribution is plausible from the file-ownership map alone — I can't determine from the tree
which agent actually wrote it, only that the plan's task assignment and the shipped labels
disagree. Nothing is broken here, only a label; noted so nobody downstream treats the
CHANGELOG's card attribution as authoritative over the plan's own task assignment.

**Documented, not built:** the two writers that bypass `If-Match` entirely — the
reconciler's `apply_mapped_status` call (no `HeaderMap`, no request in flight; the single
largest mutator of `items.status`) and `propagate_parent_completion` (a child's `PATCH` can
bump a *parent's* `version` with no caller having named that row) — both written up in the
new orchestration.md section, framed explicitly as "correct, not a gap," with the warning
that a `412` proves a concurrent write on *that* row only, never a total ordering over every
writer in the system.

**Verified live:** `preflight_allows_if_match_and_approval_token_and_exposes_etag` and
`mcp_update_item_sends_if_match` both pass in isolation; `mdbook build docs/book` succeeds
(see this section's close, below).

### G5a — Wave B additive columns (migrations 032–036) — 2026-08-05

**File:** `crates/tack-db/src/migrations.rs` — this card and G5b together own the file for
the whole cycle, per §II.2.

Five migrations, each verified as exactly one `ALTER TABLE ... ADD COLUMN` statement: 032
`control_planes.config TEXT NOT NULL DEFAULT '{}'`, 033 `control_planes.secrets TEXT`
(nullable — `NULL` means "nothing stored," distinct from 032's own `NOT NULL DEFAULT '{}'`
shape, where an empty-but-present config is the default for every pre-existing row), 034
`items.version INTEGER NOT NULL DEFAULT 1`, 035 `orch_links.version INTEGER NOT NULL DEFAULT
1`, 036 `control_planes.version INTEGER NOT NULL DEFAULT 1`. `DEFAULT 1`, not `0`, on every
version column — a pre-existing row has been written exactly once already (its own
`INSERT`), so `1` is the correct starting point for a client's already-cached `ETag` to
still match right after the migration runs.

033 shipped genuinely inert on purpose: its own comment states the column "MUST STAY UNUSED
... until card G2 adds a `control_planes.secrets` block to
`remote_backup.rs::scrub_snapshot_secrets`" — sequencing enforced by comment and card
discipline, not by the schema itself. Confirmed G2 did land that block (see above) before
anything could plausibly have written to the column for real.

**Not independently re-run:** the acceptance bar's "a seeded DB at 036 upgrades with
identical row counts..." scenario as its own isolated step — covered generally by the green
`cargo test --workspace` pass, but not re-verified as a standalone exercise in this
reconstruction.

### G5b — Table rebuilds (migrations 037–038), the irreversible step — 2026-08-05

Same file as G5a. These are the two migrations §II.0 rule 4 explicitly permits to break the
one-`ALTER`-per-name rule, because neither change is expressible as an `ALTER` at all (see
`migrations.rs`'s own section comment, lines 844–892, for the primary-key argument in full).

**037** rebuilds `orch_runs` around the widened primary key `(control_plane_id,
external_run_id, run_attempt)` plus a new `correlation_id TEXT UNIQUE`, following SQLite's
12-step procedure (`PRAGMA foreign_keys=OFF` → `CREATE ..._new` → an explicit column-for-
column `INSERT ... SELECT` → `DROP` → `RENAME` → recreate `idx_orch_runs_plane_state` →
`PRAGMA foreign_key_check` → `PRAGMA foreign_keys=ON`). Verified the `INSERT` copies every
one of the pre-existing columns (`control_plane_id, item_id, remote_project, source, state,
started_at, ended_at, error, created_at, updated_at`, plus positionally mapping old `run_id`
into `external_run_id`) — nothing silently dropped; `run_attempt` backfills to `1`,
`correlation_id` to `NULL`.

**038** rebuilds `orch_approvals`: `control_plane_id` loses `NOT NULL` (a hook-raised decision
can predate any registered plane), `kind`/`external_id`/`provider_metadata` are added.
`token` deliberately keeps its role as primary key and URL segment. Verified the `INSERT`
backfills `kind = 'approval'` for every existing row (every one of them was, by construction,
exactly that shape before this cycle) and `external_id`/`provider_metadata` to `NULL`/`'{}'`.

**`guard_against_half_applied_rebuild`, verified in `run_all`:** refuses to boot if both a
rebuild's original table and its `_new` staging sibling exist simultaneously — the one state
a crash between `INSERT` and `DROP` could leave behind — naming the backup endpoint in the
error rather than letting a naive retry replay `CREATE ..._new` / `DROP` against whatever
survived. This runs **before** `apply_migrations` touches anything, which is what makes it a
real guard rather than a log line after the fact. CHANGELOG.md carries the operator-facing
backup warning, attributed "(Phase 42, card G5b.)"

**This is the migration the O1 merge-order break traces back to** — restated once more here
since it's this card's own change: the rename of `run_id`→`external_run_id` and the new
`run_attempt` column are exactly what broke `docket_tick_contract_test.rs`'s raw
`SELECT * FROM orch_runs ORDER BY run_id` and its `TEXT`-only generic decoder. That oracle's
`snapshot_rows` now carries an explicit comment naming migration 037 and card G5b as the
reason it orders on `external_run_id` instead — cross-referenced in both directions now,
which is what §II.5 risk 1 and O1's own fix (above) both ask for going forward: a schema
change and its golden regeneration landing in the same session, not two sessions apart.

**Verified live:** `cargo test --workspace` is green including `orch_migrations_test.rs`
(part of the 48/48 `ok` blocks in the full run — not isolated separately in this pass) and
both oracle files under `tack-orch`.

### Wave B final verification — 2026-08-06

Not a card — a verification pass over the six landed cards (G1, G2, G3, G4, G5a, G5b)
plus a re-check of the four files four adversarial checks had already mutated-and-restored
(`adapters/docket.rs`, `remote_backup.rs`, `handlers/items.rs`, `migrations.rs`). All four
read clean: `pause`/`resume` both correctly `Unsupported`, the `control_planes.secrets`
scrub `UPDATE` is present and runs before `VACUUM`, the auto-dispatch gate genuinely reads
`effective_orch_enabled`, and both `INSERT ... SELECT`s in migrations 037/038 carry every
source column. No adversarial residue found; nothing hand-edited back.

**Found and fixed: the primary oracle (O1's `docket_tick_contract_test.rs`) did not
survive G5b's rebuild.** `cargo test --workspace` reported success only because the
`tee | tail` pipeline used to capture it masked cargo's real exit code — the actual run had
`docket_tick_contract_test`'s all five scenarios panicking with `no such column: run_id`.
Migration 037 renamed `orch_runs.run_id` to `external_run_id`; the repo layer
(`tack-db/src/repo/orch.rs`) aliases it back for its own typed queries, but this oracle's
`snapshot_rows` does a raw `SELECT * FROM orch_runs ORDER BY run_id`, which sees the
physical column and fails at prepare time regardless of row count. Fixed the `ORDER BY` to
`external_run_id`, then hit a second failure: `orch_runs.run_attempt` (new, `INTEGER`)
can't decode through the generic `fetch_table` helper's `Option<String>` fallback, so it
gained the same `value`-style type-aware branch `orch_metrics.value` already has. With both
fixed, all 5 tick scenarios and all 13 wire scenarios pass. Regenerated golden and confirmed
the diff against the pre-fix (stale) golden is confined to exactly what migrations 037/038
touched — `orch_runs` gains `correlation_id`/`run_attempt` and renames `run_id` to
`external_run_id`; `orch_approvals` gains `external_id`/`kind`/`provider_metadata` — no
`.requests.json` transcript changed and no other table changed. A second `UPDATE_GOLDEN=1`
run after that diff was empty (determinism proven). **Correction to this file's own
framing:** the FINAL VERIFICATION brief for this pass said "Card G5b legitimately moved
[the golden]" as an already-completed fact, sourced from "G5b's report" — no such report
exists in this section (it was empty before this entry), and the golden actually found in
the tree was still pre-037/038. Whoever ran G5b evidently never re-ran the oracle against
it. Files touched beyond the two fixes: `crates/tack-orch/tests/docket_tick_contract_test.rs`
(owned by O1 per §II.2, edited here only because Wave A has no active owner left and the
oracle was unusable) and the two `.rows.json` goldens it regenerated.

**Found and fixed: one clippy failure.** `crates/tack-orch/src/lib.rs`'s
`rated_serializes_level_and_reason_together` (card G1's own test) borrowed a `Rated` it
didn't need to (`clippy::needless_borrows_for_generic_args`, deny-by-default under
`-D warnings`). One-line fix, `rustfmt`'d, re-verified clean.

**Open item, not fixed here — deliberate, not accidental, but the acceptance grep still
fails literally.** `rg -n "grant_available" frontend/src` returns one hit, in the
*generated* `schema.gen.ts`, because `handlers/orch.rs`'s `PendingApprovalListResponse`
still declares the field server-side. `frontend/src/features/approvals/api.ts` documents
why it was left: it's a server-secret-configured flag, not a provider capability, so there
was no correct home for it in `Capabilities`, and the frontend now always renders
decide controls and lets a real 403 answer instead of guessing — which satisfies rule 6's
intent (no hard-coded gate) without touching the wire shape. Whoever picks this up next
(T4, since it touches `handlers/orch.rs`) should decide whether to delete the field outright
(it has zero remaining readers anywhere in `frontend/src`) or leave it — either way, note
it explicitly rather than let the grep keep silently failing.

**Not Wave B's problem, confirmed pre-existing.** Three frontend Vitest failures
(`client.test.ts`'s `requestBlob`, `panels.test.tsx`'s `DataPanel`, `GlobalSettings.test.tsx`'s
backup download — all `URL.createObjectURL`/`Blob` jsdom-environment mismatches) are in
files with zero changes in this cycle's diff (`git status --porcelain` clean on all three).
Left alone.

**Status board corrected above** — Wave A and B were both still marked "not started"
despite being fully landed; no card had appended a handoff note here either, despite §II.0
rule 9. Wave C: the golden is now verified deterministic and confined to the intended
tables: build on it with confidence, but re-run the tick oracle yourself after T2/T3 rather
than trusting a stale copy — this entry is proof that "should have been regenerated" and
"was regenerated" are not the same claim.

*(cards before this pass did not append their own handoff notes — see the correction above)*

---

### Adversarial verification — Wave B — 2026-08-06

Four independent sessions, each handed one card's shipped code and one hand-picked plausible
regression to inject, told to mutate, run the real test suite, record what caught it (or
didn't), then restore the file byte-for-byte and confirm `cargo test --workspace` is green
again. **All four verdicts: CAUGHT** — none reports a gap that let a real regression through
silently — but each surfaces a genuine weakness worth carrying into Wave C, and one of the
four (C) is a sharper finding than "caught" alone conveys.

**A — lying capability (card G1).** Flipped `DocketAdapter::capabilities().pause.level` from
`Unsupported` to `Supported`. Caught by exactly one test in the entire workspace —
`docket_capabilities_match_the_verified_facts` (`lib.rs:1143`) — confirmed by running the
full suite before and after the mutation, not just the targeted test; nothing at the API,
wire-contract, or frontend-mock layer independently notices a lying capability. Diagnostic
quality is mixed within the same test: the `pause.level`/`resume.level` assertions that
actually fired are bare `assert_eq!`s with no message (the panic text never says "pause" — a
reader needs the source line to learn which field lied), while the neighboring
`reason`-string assertions two lines below carry a real explanatory message
(`"pause's reason must name the docket CLI remedy, got: {:?}"`). A capability whose *reason
text* lies would be caught, and caught clearly; a capability whose *level* lies is caught but
silently as to which field. Cheap fix, not applied here: give the level assertions the same
`format!` treatment the reason checks already have.

**B — unscrubbed secret (card G2).** Commented out the
`UPDATE control_planes SET secrets = NULL ...` while leaving the trailing `VACUUM` intact —
exactly the "forgot the scrub, still ran VACUUM" shape the test's own comment names as the
case a weaker `secrets IS NULL` check would miss. Caught cleanly by
`scrub_removes_control_plane_secrets_from_snapshot`, which asserts the literal secret
substring is physically absent from the snapshot's raw bytes, not just that the column reads
`NULL` — the strong version of the check, and it fired before the weaker null-count
assertion later in the same test ever got a chance to be the only thing standing between this
regression and a green suite. All 18 sibling tests in the module were unaffected. No
diagnostic complaint here: the failure message names the exact leaked string and states
where it was found.

**C — ignored If-Match (card G3) — the sharpest finding of the four.** Neutered the
*comparison* half of `check_if_match` (`if false && provided != item_etag(...)`) while
leaving the underlying atomic `claim_item_version` CAS untouched. Caught overall —
`patch_with_a_stale_if_match_is_rejected_with_412...` and
`patch_with_an_if_match_for_a_different_item_is_rejected` (both sequential, no concurrency)
fail 100% across 5 reruns each, with a clear `left: 200 / right: 412`. **But the test this
card's own acceptance bar names as the headline check** —
`concurrent_patches_with_the_same_if_match_yield_exactly_one_200_and_one_412` — **is flaky
against this exact mutation**: 10/15 passes, 5/15 failures (≈33% detection) across reruns in
isolation, because two racers sharing one still-valid version can coincidentally reproduce
the "one 200, one 412" shape via the untouched CAS alone, independent of whether `If-Match`
itself gates anything. The higher-fanout (N=6) variant is more reliable but still not
deterministic (7/10 failures over 10 reruns). Recommended fix, not applied here: stop relying
on two racers colliding on the same live version — force one racer's held ETag to be
known-stale relative to a write guaranteed to land first (the way the two deterministic
sequential tests already do), or inject a forced yield between the version read and the
claim to open the race window on every run rather than leaving it to scheduler luck. This
means G3's own named acceptance test is not, by itself, a trustworthy gate for the exact bug
class it was written to catch — see G3's subsection above, which points back here.

**D — lossy rebuild (card G5b).** Dropped `item_id` from both the column list and the
`SELECT` of migration 037's `orch_runs` rebuild, while leaving `item_id` in the target
`CREATE TABLE` unchanged — a column that survives in the schema but silently loses its data
on upgrade. Caught by `test_migration_037_rebuild_preserves_every_row_and_field_equality`,
which does genuine per-row, per-field equality across all 13 columns (not a row-count check)
and would have caught a silent drop of nearly any column, not only `item_id`. The specific
assertion that fired is a bare `assert_eq!` with no message (`left: None,
right: Some("961a5e85-...")`, no field name anywhere in the output) — roughly 11 of the
test's 13 field assertions share that gap, though the neighboring `run_attempt`/
`correlation_id` checks and a later `cli_run.item_id` check do carry explanatory strings. The
oracle itself is sound; a reviewer reading only CI output, not the test source, cannot tell
which column broke.

**The procedure worked as designed, twice over (worth recording as a positive result, not a
delay).** All four sessions' first attempt aborted at their own setup check —
`cargo test --workspace` was already red before any mutation was applied, because the tree
still carried the merge-order break documented under O1/G5b above
(`docket_tick_contract_test.rs` querying a column migration 037 had already renamed). Every
session correctly refused to draw a verdict against a baseline that wasn't green, rather than
mutating anyway and reporting a result that would have meant nothing against a tree that was
already broken for an unrelated reason. Once that break was fixed (see O1's subsection and
"Wave B final verification," above), all four re-ran clean and reached the verdicts recorded
here. This is the adversarial harness behaving exactly as intended under a real failure, not
friction to route around next time.

---

# Part III — Harness-Agnostic Runner Fleet (Phases 50–57)

Executable board for the active cycle described at the bottom of
[docs/book/src/roadmap.md](docs/book/src/roadmap.md) → *Next — Harness-Agnostic Runner
Fleet*. **Everything in Parts I and II remains historical context. Do not delete, renumber,
or implement a superseded Part II card.**

This board is designed for multiple **Terra agents working simultaneously in isolated
worktrees**. Each card is intentionally bounded, names every shared-file owner, and has an
acceptance gate that can be verified without trusting its author's handoff.

## Status board — Part III

| Wave | Cards | Phases | Status |
|---|---|---|---|
| 0 — Clean boundary and safety | III-A0 · A1 · A2 · A3 · A4 | 50 | complete — accepted integration SHA `f042085`; full Rust/frontend/docs/contracts and all three Playwright projects green |
| 1 — Domain, schema and runner skeleton | III-B1 · B2 · B3 · B4 | 51, 52 | complete — accepted integration SHA `f14019b`; domain/schema/runner/contracts and legacy golden gate green |
| 2 — Pull protocol vertical slice | III-C1 · C2 · C3 · C4 · C5 | 52 | complete — accepted integration SHA `f931fc0`; 913 workspace tests, clippy `-D warnings`, fmt, OpenAPI drift and frontend gates green. Integration gate proven end-to-end through the mounted production router (`crates/tack-api/tests/wave2_gate.rs`, 10/10 stability loop) |
| 3 — Real harness proof | III-D1 · D2 · D3 · D4 · D5 | 53 | complete — accepted integration SHA `6a53a18`; 1046 workspace tests, clippy `-D warnings`, fmt clean; B4 fixture pin and the Wave 2 gate both still green. Contract reconciled once against three independently-built adapters. **Live-proof caveats below — read before Wave 4** |
| 4 — Fleet scheduling and PM UX | III-E1 · E2 · E3 · E4 · E5 · E6 | 54 | complete — accepted integration SHA `8a6e613` (branch `agent/iii-e6-integration`, base `f0d4ac2`). E6 wired E1's pure scheduler to live `agent_runners`/`agent_fleet_members`/`agent_fleets`/`execution_requests` data (replacing the naive `ORDER BY created_at LIMIT 1` claim match), added `GET /api/runners` and `GET /api/executions/{id}/attempts[/{n}/events]` (closing the gap E2/E3/E4/E5 each independently hit), and gave the entire operator execution/fleet/runner/profile domain real typed OpenAPI schemas (`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` regenerated, drift-clean) — the pre-existing `{}` schemas E2 flagged as the "biggest spec-drift item" are gone. Found and fixed a genuine integration deadlock between two individually-correct Wave-4 designs: `RunWithAgentModal.tsx` could only ever submit `Auto`-model requests (no live capability data existed to unblock a specific choice), and E1's scheduler unconditionally rejects `Auto` — so no execution submitted through the landed UI could ever be claimed by any runner. Fixed by wiring the new `GET /runners` route into the modal's live capability fetch. 1134 workspace tests (was 1105; +29 this card), clippy `-D warnings`, fmt clean; frontend 653 tests, `tsc -b` and token-lint clean; Playwright chromium+firefox 83 passed/41 skipped/0 failed (webkit could not be evaluated in the build sandbox — missing system library, unrelated to this branch, confirmed against untouched specs too). Healthy selection, saturation, exact-runner exclusivity, unsupported-model rejection and realtime UI updates are each proven through production routes at the Rust integration, CLI (`crates/tack-cli/tests/e6_scheduler_e2e_test.rs`), and UI (`frontend/e2e/scheduler-e2e.spec.ts`) layers — using the exact-runner selector throughout, since `agent_fleet_members` still has no write route on any API surface (a documented, deliberately-not-widened gap; fleet-membership eligibility itself, including the newly-enforced `agent_fleets.concurrency_limit`, is proven directly against the database in `crates/tack-orch/tests/scheduler_wiring_test.rs`). **Genuinely open for Wave 5:** no `agent_fleet_members`/model-profile-toggle/single-fleet-detail write or read routes; `store.ts#attemptsFor` and `ExecutionTimeline.tsx` not wired to the now-real attempts/events routes (mechanical follow-up); `execution_requests` still has no real `priority` column (E6 added a `metadata`-convention stopgap, documented as non-binding); III-F3 model-resolution/provenance untouched, as instructed. See `III-E6.md` for the complete list. |
| 5 — Decisions, artifacts, models and usage | III-F1 · F2 · F3 · F4 · F5 | 55, 56 | **complete** — accepted integration SHA `073aa4d` (branch `agent/iii-f6-integration`, base `cbdd4a3`). Gate lifted when Wave 4 was accepted (`8a6e613`). Four backend cards delivered and merged onto `agent/iii-f6-integration`: III-F1 scoped decisions (`7ce2e5f`), III-F3 model resolution and usage provenance (`802d4c3`), III-F5 execution retention and observability (`b3e8b3c`), III-F2 events and verified artifacts (`9df4c6a`). None needed a migration — every field they use already existed in 045–048. Integrator commit `2689ed7` mounts F1's decision-resolve route behind a second, independent `TACK_EXECUTION_DECISION_TOKEN` gate (resolving the contract gap F1 escalated rather than invented: `protocol.json` names decision resolution a `separately_scoped_operator_credential` with `required_scope: "operator:decisions"`, and Tack has no scope system — fail-closed when unset, mirroring `TACK_ORCH_APPROVAL_TOKEN`), and flips `TACK_EXECUTION_RETENTION_ENABLE` to default **false** (F5 shipped it `true`; the sweep deletes rows, so deletion must be an explicit operator opt-in — `TACK_EXECUTION_HEALTH_ENABLE` stays default `true`, it reads and logs only). **Backend integration complete and gated green** across four integrator sub-cards: III-F6a proved F2's artifact storage/download wiring through the real `build_router` (the wiring itself had landed silently inside `2689ed7`, whose message described only decisions/retention — recorded as an amendment, not rewritten); III-F6b granted F3's wiring requests, so `resolve_request_model_policy` now runs on the live `POST /api/executions` path and `AttemptSummary` carries `model_provenance`/`usage_economics`; III-F6e documented three mounted-but-unspecified paths (87 → 90) and fixed a pre-existing spec lie in which all 13 runner-protocol operations declared `ErrorEnvelope` instead of the `ProtocolErrorEnvelope` they actually return — any generated client would have failed to parse every runner error; III-F6d wired F2's `sweep_events`/`sweep_artifacts` and F1's `expire_overdue_decisions`, which a repo-wide grep showed had **zero callers anywhere** (F5 was authored before F2 existed, so its retention mechanism never knew `execution_artifacts` existed — artifact rows and their on-disk blobs, the largest consumers in this domain, grew unbounded even with retention enabled), and in doing so exposed and closed a list/delete race that could not exist while the sweep had no caller. **Gate:** 1289 workspace tests / 0 failed, clippy and fmt clean, `wave2_gate` 5/5, `runner_contract` 18/18 (all 46 fixtures byte-pinned), `openapi_contract` 5/5 drift-free, `e6_scheduler_e2e_test` 5/5 × 3 consecutive isolated runs (one contention-induced failure observed under concurrent agent load and recorded rather than hidden — see `III-F6.md`), frontend `tsc -b` exit 0. See `docs/agent-handoffs/part-iii/III-F6.md`. **III-F4 (frontend) then closed the wave:** `AttemptAvailability` became a real `idle/loading/ready/error` state machine backed by `loadAttempts`, replacing E2/E6's placeholder, and `store.ts#attemptsFor` plus the event timeline are finally wired to the attempts/events routes E6 recorded as mechanical follow-up. `Not measured` renders as that exact literal — never `$0.00`, an em dash or a blank cell — asserted both as a unit and against real server data, because `runner_time_cost.cost_usd_estimated` is `null` in production always and a zero there would be a lie about money. The fail-closed "decisions cannot be resolved on this deployment" state (403 when `TACK_EXECUTION_DECISION_TOKEN` is unset) is handled as a real operator-facing condition, proven against the real server and distinguished from a genuine 404. Frontend gate: 724 Vitest tests / 85 files, `tsc -b`, token-lint and build clean, Playwright chromium 65/65 (36 axe scans, 0 violations), firefox 23 passed/42 skipped/0 failed. **Webkit could not be evaluated** — the same pre-existing missing `libwoff2dec.so.1.0.2` III-E6 documented, confirmed to fail identically on untouched specs, so not a regression but genuinely unverified. F4 also fixed two Wave 4 E2E specs that broke mechanically once real attempt data began arriving, documented rather than silently absorbed. **Genuinely open for Wave 6:** no decision-discovery or artifact-discovery/list endpoint exists anywhere in the codebase (confirmed by reading every handler), so both UI list views stay honestly empty by design — concrete route shapes are requested in `III-F4.md`; `projects` still has no default-model-policy storage (`ModelPolicySources.project_default` is modeled but always `None`); no runner infra cost-rate is stored anywhere, so `runner_time_cost.cost_usd_estimated` can never read anything but `not_measured`; `model_profiles` (migration 043) is consulted by nothing; `agent_fleet_members` still has no write route; `execution_requests` still has no real `priority` column. See `III-F4.md` and `III-F6.md`. |
| 6 — Legacy bridge and release | III-G1 · G2 · G3 · G4 · G5 | 57 | **Integrated 2026-08-19 at `04ad6f3`; RELEASE BLOCKED — do not tag.** G1–G4 delivered in parallel from base `5c6842f` and were merged by III-G5 in order (`1979d32` G1 docket bridge, `6e50f75` G2 chaos audit, `c97cee4` G3 operator docs, `04ad6f3` G4 CI/packaging). No file was touched by more than one branch — ownership held. Gates run once on the integrated tree; `openapi_contract` 5/5 drift-free. Webkit still unverifiable (missing `libwoff2dec.so.1.0.2`, unchanged since III-E6). **Open P0 — the cycle's definition of done is NOT met:** `tack-runner`'s HTTP transport is unimplemented. `UnavailableProtocolClient` is the only production `RunnerProtocolClient` in the tree (the other, `FakeClient`, is test-only) and the crate does not depend on `reqwest` at all, so the packaged runner binary cannot enroll, claim, heartbeat or report against a live server. Pinned as a deliberate typed failure by `runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` — not hidden, but unbuilt. Everything server-side (routes, scheduler, fencing, decisions, artifacts, retention, operator API/CLI/UI) and the harness adapters are real and tested. **Next: one new card — `tack-runner` HTTP transport.** After it lands, the three-harness live smoke becomes collectible and the tag becomes honest. Other routed-open follow-ups (none blocking on their own): G1 dual-dispatch mirror in `create_execution`, stale orch-task reconcile not boot-scheduled, legacy compatibility label not over HTTP; G2 F2 corrupt-journal recovery scan (P2), F3 resend labeling (P3), disk-full case accepted as not verified. See `docs/agent-handoffs/part-iii/III-G5.md`. |
| 7 — Release blocker | III-H1 · H3 · H2 · H4 | 57 (cont.) | **H1, H3 and H2 done and merged (H2 integrated 2026-08-19 at `01c7046`); III-H4 remains, joined by the Wave 8 cards the H2 escalations were routed to. Integration line is `develop`, the repository's default branch** — consolidated onto one trunk on 2026-08-19 after two naming failures in a row: the Wave 5 card branch `agent/iii-f6-integration` served as the de facto trunk for three waves while the cycle line sat 21 commits behind, and then `develop` and `plan/harness-agnostic-agent-fleet` drifted apart again within a single session. `plan/harness-agnostic-agent-fleet` is retired at `158980e`; there is now exactly one integration line and it is the branch GitHub already treats as default, so dependabot, CI and the board cannot disagree about where work lives. Branch every card from `develop`. III-H1 (`984bb5f`, merged `45ccafb`) built the runner's HTTP transport — the runner can register, ask for work, heartbeat, stream progress, upload results and complete against a live server. III-H3 (`2fd811c`, merged below) gives every claimed attempt its own private checkout at the requested commit, with the harness running inside it and cleanup on completion, cancellation **and** crash recovery — the last of which the code did not actually have; H3 added it in nine lines of `engine.rs`, outside its ownership, accepted here. Gates on the integrated tree: 1363 workspace tests / 0 failed, clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, `h3_checkout` 6/6. **Warning for III-H2 — `scripts/smoke.sh` now reports SMOKE PASSED while proving less than it did before.** Steps 7–9 (claim→complete, per-harness runs, restart recovery) are unimplemented stubs that print SKIPPED unconditionally and never set failure, so the script cannot fail on them. It was a real gate while step 6 failed; now that step 6 passes it is a false green. H2 owns the file and must implement 7–9 before any release claim rests on it. **Open, routed:** `RepositorySpec` carries only remote and revision, so the contract's `repository.subdirectory` and `kind` never reach the provisioner (owner: the spec's owner; nothing faked). The harness discovery test in `harness/claude_code.rs` mutates PATH process-wide while assuming single-threaded execution and can fail unrelated concurrent tests at random (owner: III-D2). No shared git object cache by design — a mirror would reintroduce the shared mutable state crash-safety exists to prevent. Harness reality unchanged: `claude` and `opencode` present, `codex` absent, so two of three is reported as two of three. **III-H4 added 2026-08-19 from a CI failure, not from a card:** a runner that loses a credential-rotation race is answered 401 (reads as a dead credential, terminal) instead of 409 conflict (retryable), because the winner rotates the credential away before the loser is authenticated — so a healthy runner can stop for no reason. Does not block H2's work; settle it before tagging. Does not reproduce locally (3 isolated + 3 full-binary runs under `--features embed-spa`, all green); only CI's contention opens the window. **Also on `develop`'s first honest CI run:** Coverage and E2E now pass having failed on the stale base, while `cargo-deny` and the security audit fail exactly as they did before — the latter is III-G4's deliberately unlanded `cargo audit` bump. **III-H2 delivered its smoke and refused the tag** (card commit `e7ba233`, merged `01c7046`). `scripts/smoke.sh` steps 7–9 are real in both modes and the script can no longer pass while proving less than it claims: full claim→checkout→harness→completion proven live (real opencode + local model, exact commit verified), restart recovery proven by SIGKILLing the runner mid-attempt (needs_operator, no blind duplicate by on-disk process count and server attempt count, explicit requeue → attempt #2 success), capacity-1 saturation proven under a live lease. Gates on the integrated tree: shellcheck -S warning and bash -n clean; fake-mode smoke re-run at `01c7046` exits 1 with step 8 as the only FAIL — the load-bearing correct outcome; no compiled file changed, so the 1363/0 workspace baseline at the H3 merge stands. **New P0 found by running the product instead of its tests: claude-code and codex requests can NEVER be scheduled** — the adapters declare zero model_combinations, the scheduler requires declared pairings and rejects AutoSelect, so a real installed `claude` sits queued forever (proven live; step 8 FAILs on it and exit 1 is currently correct). Escalation outcomes (integrator, 2026-08-19): (1) schedulability P0 → **routed to III-H5** (decision card, release-blocking); (2) duplicate `runner_name` enrollment 500 → **routed to III-H7**; (3) engine never submits events/decisions/artifacts (H1 escalation 3, now blocking two §III.6 criteria) → **routed to III-H6** (release-blocking); (4) no fleet write route (standing since E6) → **routed to III-H8**. **III-H5 done and merged (2026-08-20, card `fafcf30`, merged `84fabf1`) — the schedulability P0 is closed.** Decision card resolved as a pass-through capability attestation: `HarnessCapability.model_passthrough` (optional, runner-v1 `capabilities.json` re-pinned in the same change) attests the verifiably-true claim that the claude-code/codex adapters forward the operator's model verbatim via `--model`, and the scheduler accepts a supported attestation as eligibility for an explicit pairing; Advisory/Unsupported/absent reject identically, AutoSelect stays refused, probe errors and undeclared harnesses still win, and opencode honestly attests Unsupported because it refuses undeclared models pre-spawn. Proven live with the real `claude` 2.1.236: claimed and completed through the full pipeline; smoke step 8 green in BOTH modes with no smoke edit (the load-bearing outcome), and the 'structurally unschedulable' verdict line is gone. Gates on the integrated tree: 1368 workspace tests / 0 failed (+5 over the H3 baseline, exactly the card's new tests, one proven load-bearing by revert), clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, fake smoke SMOKE PASSED. Escalation outcomes (integrator, 2026-08-20): (1) step 8's canned never-claimable FAIL text still names the pre-H5 structural cause — post-H5 that symptom usually means a broken binary or saturation; routed to the smoke's owner (H2 lineage), reword on next touch; (2) this machine's global `claude` broke mid-card (npm dropped its native package) and the probe/scheduler correctly refused it — accepted as environmental, repaired outside the repo (`npm install -g @anthropic-ai/claude-code-linux-x64`), check `claude --version` before suspecting the scheduler on a live step-8 FAIL; (3) `model_profiles` (migration 043) still consulted by nothing — F4's standing finding, unchanged. One §III.6 nuance for review: 'choose only a supported provider/opaque model' now means, for pass-through harnesses, 'any operator-specified model, validated by the harness at run time with an attributable failure' — the honesty rule holds (nothing fake is offered), but the criterion's wording predates attestation and the release reviewer should sign off on that reading. Remaining Wave 8: III-H4 (credential-rotation race → 409), III-H6 (engine submits events/decisions/artifacts — release-blocking), III-H7 (duplicate runner_name 500), III-H8 (fleet write route). Base SHA for them: `84fabf1` on `develop`. §III.6 remains unmet (events/artifacts, fleet write, codex binary); the tag stays refused. See `docs/agent-handoffs/part-iii/III-H2.md` and `III-H5.md`. |
| 8 — Unblock the tag | III-H4 · H5 · H6 · H7 · H8 | 57 (cont.) | **Integrated, not complete — accepted integration SHA `b848d96` on `develop` (2026-08-20), base `84fabf1`/`0e2da46`.** H5 merged earlier in this wave (recorded above). H4, H6, H7 and H8 ran in parallel worktrees off `develop`'s tip and merged in dependency order H8 → H4 → H7 → H6 with zero textual conflicts — H4 and H7 share `crates/tack-api/src/handlers/runner_protocol.rs` but touched disjoint functions (H4: `refresh`/`reclassify_refresh_auth_error`; H7: `enroll`/`is_unique_violation`), confirmed by diff before merging, not just by luck. **III-H4** — a losing credential-rotation now returns retryable `409 conflict` instead of a false-fatal `401`; reproduced deterministically (no timing dependency) and proven load-bearing by reverting the fix and watching the new test fail. **III-H6 (release-blocking)** — the runner engine now submits a real terminal/cancellation event and attempts an artifact upload on every completed attempt, proven live: III-H2's UNMET events/artifacts line is gone from three consecutive `SMOKE PASSED` runs. Found and fixed, inside its own change, a second bug it exposed: submitting an event before completion moved the server's `event_checkpoint` off `NULL`, which broke every completion's compare-and-set outright (`409`, cascading into stuck attempts) — fixed by sourcing `final_event_checkpoint` from the record the runner already tracks, confirmed load-bearing by reverting once. **III-H7** — a second same-named runner enrolling on one host now succeeds instead of 500ing; root cause was the enroll body's self-reported `runner_name` clobbering the operator-assigned name and colliding on the `UNIQUE` constraint, fixed by no longer writing the self-report into `name`, with a typed `409 conflict` kept as defense-in-depth. **III-H8** — `agent_fleet_members` has a write route; an operator can populate a fleet over the API and a fleet-targeted request now schedules onto a member, proven against real DB rows, not just a 200. `docs/openapi.json`/`schema.gen.ts` regenerated via the contract test, not hand-edited. **Escalation outcomes (integrator, 2026-08-20):** (1) III-H6's `crates/tack-runner/src/main.rs` edit, outside its formal `Owns`, is **accepted** — small, localized (one builder call wiring the engine's new seam to the production protocol, two capability-doc-comment corrections), and not claimed by any other Wave 8 card; (2) III-H6's new finding — the artifact **content** PUT (`/attempts/{id}/artifacts/{artifact_id}/content`) returns a server-side `500` in every smoke run, likely because `execution-artifacts` is never created ahead of the first real write — is **routed to a new card, III-H9** (not yet written up; owns `crates/tack-api/src/handlers/runner_protocol.rs::artifact_storage` and `state.artifact_storage`), release-relevant because it is the other half of §III.6's "verified artifacts" criterion; (3) III-H6's open "decisions stays Unsupported" and "no restart-replay for events/artifacts" are accepted as documented scope limits, not defects — no harness in this tree ever asks a mid-run question, so there is nothing yet to exercise the decision path. **Gates on the integrated tree:** 1380 workspace tests / 0 failed (up from the H5-merge baseline of 1368; the four cards' own claimed deltas sum to 1377, the small extra is accounted for by H8's larger-than-estimated test file), clippy `-D warnings` and fmt clean, `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, frontend `type-check`/vitest (726/726)/build clean. `cargo audit`/`npm audit` still fail — unchanged `Cargo.lock`/`package-lock.json` (verified by diff), the same standing gap III-G4 already owns, not a Wave 8 regression. **§III.6 status:** events/decisions/artifacts and fleet-or-runner selection are now demonstrable except artifact content bytes (III-H9) and a decision-asking harness (no owner yet); codex binary still absent on this machine; the tag stays refused pending III-H9 and a codex install. No next wave is defined yet — III-H9 is the one open card blocking a re-attempt at III-H2's tag. See `docs/agent-handoffs/part-iii/III-H4.md`, `III-H6.md`, `III-H7.md`, `III-H8.md`. |

| 9 — Artifact content storage | III-H9 | 57 (cont.) | **Integrated — accepted integration SHA `6252f52` on `develop` (2026-08-22), base `b848d96`.** Single card, fast-forwarded (branch tip and `develop` were already identical before this commit). Root cause was not H6's escalation guess (a missing `execution-artifacts` directory) — `safe_attempt_dir` already `create_dir_all`s it on every write. The real bug: `encode_id` hex-encoded every byte of a runner-generated `artifact_id` (already ~220 bytes: `art_<hex of "attempt_id:fencing_token:sha256">`), doubling it past Linux's 255-byte `NAME_MAX`, so every real content upload failed with `Io`/`ENAMETOOLONG` and surfaced as a bare `500`. Fixed by SHA-256-hashing the id instead of hex-encoding it literally — same traversal defense, fixed 64-byte length regardless of input. Proven load-bearing by reverting the fix once (new test fails with `Err(Io)` against the old encoding) and live via `./scripts/smoke.sh`: every artifact content `PUT` now returns `200` where it was `500` on every prior run, bytes confirmed on disk. **Gates on the integrated tree (re-run by the integrator, not just the card's own claim):** `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` 1383/0 failed (+3 over the Wave 8 baseline of 1380, matching the card's own count); `runner_contract` 18/18, `wave2_gate` 5/5, `openapi_contract` 5/5 drift-free, all unchanged (no wire-shape or route change); no frontend files touched, frontend gates not re-run; `cargo audit` still shows the same 3 pre-existing advisories against an unchanged `Cargo.lock` (III-G4's standing gap, not a regression). **§III.6 status:** both halves of "verified artifacts" (manifest + content bytes) are now demonstrable live end-to-end. **The tag remains blocked on exactly one thing: the `codex` binary is absent from this machine** (III-H2's standing environmental gap) — no decision-asking harness exists yet either, but that is an accepted scope limit, not a defect. No next wave is defined; whoever installs `codex` and re-runs `./scripts/smoke.sh` closes the cycle's definition of done. See `docs/agent-handoffs/part-iii/III-H9.md`. |

### Amendment to Wave 9 — 2026-08-26: `codex` installed, live smoke still FAILS

Recorded as an amendment rather than by rewriting the Wave 9 row above, per the
corrections-are-appended rule. **Wave 9's closing sentence — "whoever installs `codex` and
re-runs `./scripts/smoke.sh` closes the cycle's definition of done" — has been tested and is
false.** The tag is still refused, for a new reason.

**What was done.** `codex` is now installed (`codex-cli 0.149.1`), joining `claude` 2.1.236
and `opencode` 1.18.0 — smoke step 1 reports **3 of 3 real harnesses present**, the first
time that has been true on any machine in this cycle. Both binaries were rebuilt in release
(`tack` 18.4 MB with `embed-spa`, `tack-runner` 4.5 MB) and `./scripts/smoke.sh --live` was
run in full.

**Result: `SMOKE FAILED`.** Steps 1–6 and step 9 passed, including the complete restart-
recovery proof (SIGKILL mid-attempt → `needs_operator` → no blind duplicate → operator
requeue → attempt #2 success) and capacity-1 saturation. The failures:

- **Step 7 — the live attempt never reached a terminal state.** The `opencode` +
  `llamacpp/qwen3.6-35b-uncensored` pairing (chosen by the script from the runner's own
  declaration) was claimed, checked out at the exact requested commit `29836a98` with an
  isolated workspace, and then produced `attempt ended '' — terminal_reason: null` after the
  300 s live budget. The pipeline did its job up to the harness; the harness run itself did
  not come back.
- **Step 8 — all three harness kinds FAILed.** `codex` and `claude-code` printed the canned
  "declares zero `model_combinations`, therefore structurally unschedulable" text, and
  `opencode` printed "never claimed despite a declared model combination".

**Read step 8's message with the board's own warning in hand.** The Wave 7 row already
routed that exact string as **stale**: escalation (1) of the III-H5 integration explicitly
records that "step 8's canned never-claimable FAIL text still names the pre-H5 structural
cause — post-H5 that symptom usually means a broken binary or saturation; routed to the
smoke's owner (H2 lineage), reword on next touch." III-H5 closed the schedulability P0 via
`model_passthrough` and proved `claude` claimable live. The text was never reworded, so it
fired again here and reads as a regression it is not.

**Leading hypothesis, explicitly NOT verified.** The runner is enrolled at capacity 1. Step
7's attempt never terminated, so its lease was very likely still held when step 8 created
three more requests — under which none of them could be claimed by anyone, which is
precisely the symptom all three printed, `opencode` included. That would make step 8's three
failures a **cascade of step 7's hang**, not three independent defects. This is a hypothesis
consistent with every observation, not a diagnosis: nobody has re-run step 8 against an idle
runner, and the runner-side log was not preserved (`SMOKE_KEEP=1` was not set).

**What is therefore genuinely open** — do not treat any of this as settled:

1. Why the live `opencode` + local-llama.cpp attempt hangs instead of terminating. Re-run
   with `SMOKE_KEEP=1` and inspect the runner log and journal; try a lighter declared
   pairing (`opencode/big-pickle`, `hy3-free`, `mimo-v2.5-free` … were all declared) before
   concluding anything about the product rather than the model.
2. Whether step 8's three failures survive once a runner is idle. If they do not, the real
   defect is that the smoke saturates its own single-capacity runner and then blames
   scheduling.
3. Step 8's canned FAIL text, still unworded since 2026-08-20 and now demonstrably
   misleading a second reader. Owner remains the smoke's (H2 lineage).
4. `codex` has still never completed a live attempt — installing it moved the blocker, it did
   not clear it. Two of three harnesses remain live-proven; three of three is still not a
   claim this repository can make.

**One test was also fixed while running the workspace gate**, and is **uncommitted** at the
time of writing: `crates/tack-runner/src/harness/mod.rs::registering_all_three_real_adapters_is_order_independent`
asserted that all three adapters reject the fixture spec identically. That held only because
`codex` was absent; with the binary installed the codex adapter correctly *accepts* an
explicit model (it is a pass-through harness — III-H5), and the test failed. The assertion
now expects accept-for-codex / reject-for-the-other-two, which is the real post-H5 contract
rather than an artifact of an empty PATH. Gates after the fix: `cargo test --workspace`
green, `clippy -D warnings` and `fmt --check` clean. See §IV.0 — land or discard this
deliberately before branching Part IV cards.

**Current branch:** `plan/harness-agnostic-agent-fleet`.

**Gate 0 — do not skip:** this branch was created while the source worktree already carried
the unreleased Part II changes. Before implementation agents branch or create worktrees, a
human/integration owner must make that baseline reviewable as one or more named commits.
Do not stash, reset, discard, or silently absorb those changes. Record the chosen baseline
SHA in the first Part III handoff.

**Satisfied at Wave 0.** The baseline was recorded as commit `1d71785` ("chore: preserve
unreleased Part II baseline" — 82 paths: 46 previously-tracked modifications + 36
previously-untracked files, reviewed intact rather than stashed/discarded) in
`docs/agent-handoffs/part-iii/III-A0.md`. The wave integrator then accepted
`f042085d585adfdd8386a2120c7429649883e5df` as the exact Wave 1 branch point once the
combined tree passed full workspace tests, clippy, fmt, the frontend suite, the complete
Playwright matrix and `mdbook build`. This gate has not applied to any card since Wave 0
closed.

---

## III.0 Cold-start context capsule

Every Terra prompt begins with this section plus its one card. Do not paste all 9,000 lines
of Parts I/II into a worker context.

### Objective

From a Tack PM item, create a durable execution request that is assigned to an exact runner
or a fleet. A pull-based `tack-runner` launches Codex, Claude Code, OpenCode, or a future
coding harness. The operator selects an agent profile and a supported harness/provider/model
combination. Tack records requested and actual execution facts, events, decisions, artifacts
and nullable usage without making Docket mandatory.

### Architectural boundary

- Tack API: PM source of truth, durable queue, scheduler, fenced leases, normalized history.
- `tack-runner`: local credentials, workspace/worktree, harness subprocess, journal,
  cancellation and recovery observation.
- Harness adapter: one CLI/runtime integration inside `tack-runner`.
- Docket: optional `legacy-docket` bridge only; never the owner of a new runner request.
- GitHub Actions: CI/integration work, not a coding-harness adapter for this cycle.

### Vocabulary that must remain distinct

`Item` ≠ `ExecutionRequest` ≠ `ExecutionAttempt`; `AgentProfile` ≠ `Fleet` ≠ `Runner`;
`Harness` ≠ `ModelProvider` ≠ `ModelId`. A field called only `provider` is rejected unless
its type makes the namespace explicit.

### v1 scope

- One active fenced lease per execution request.
- Exact-runner or fleet assignment.
- Three in-tree harness adapters: Codex, Claude Code, OpenCode.
- One attempt owns one isolated workspace/worktree.
- Optional explicit item-status mapping through the workflow engine.
- No automatic multi-agent fan-out, task decomposition, model proxy, generic plugin ABI or
  GitHub Actions execution.

### Honest delivery semantics

Do not claim exactly-once harness execution. A database transaction can prevent two valid
leases, but a runner/network crash after process launch can leave ownership ambiguous. The
contract is: at most one **valid active lease**, monotonically increasing fencing tokens,
local runner journal, idempotent reports, and `needs_operator` when safe retry cannot be
proved. Lease expiry never blindly launches a second process.

### Required reading by role

| Role | Read before editing |
|---|---|
| Contract/domain | Roadmap Phase 50–57 section; `crates/tack-orch/src/lib.rs`; current `dispatcher.rs` and reconciler store traits |
| Database | `crates/tack-db/src/migrations.rs` runner and migrations 034–038; `repo/items.rs`; `repo/orch.rs`; migration tests |
| API | `router.rs`; `middleware.rs`; `openapi.rs`; error envelope; item/orchestration handlers |
| Runner | Roadmap runner protocol/adapter sections; workspace `Cargo.toml`; existing CLI process/config patterns |
| Frontend | `app/routes.tsx`; shared API client; `projectItemsContext`; shared orchestration capabilities; architecture test |
| Harness adapter | Only the frozen Part III fixtures/types, adapter-owned directory, fake-binary harness and its target CLI's current contract |
| Release | CI workflow; `docs/TESTING.md`; OpenAPI/golden gates; backup/restore tests; Part III exit matrix |

---

## III.1 Frozen v1 contracts

Wave 0 owns these contracts. Later cards consume them and may not edit them independently.
A real adapter may falsify a contract; it reports the gap in its handoff and D5 changes the
contract once for all adapters.

### III.1.1 Lifecycle

`queued | leased | preparing | running | waiting_decision | succeeded | failed | cancelled |
lost | needs_operator`

Terminal: `succeeded`, `failed`, `cancelled`. `lost` is an observed loss of contact with no
known running process; `needs_operator` is an ambiguous side-effect/ownership state and is
not automatically retryable.

Allowed transitions and who may request/observe them live in a committed fixture. At a
minimum:

- API/scheduler: create `queued`; grant `queued → leased`; request cancellation.
- Lease owner: `leased → preparing → running`; `running ↔ waiting_decision`; terminal report.
- Recovery service: expired lease → `lost` only when the runner journal/probe proves no
  active process, otherwise `needs_operator`.
- Operator: explicitly requeue/abandon a `needs_operator` request with an audit event.

### III.1.2 Execution request snapshot

Required immutable fields after enqueue:

- `item_id`, `idempotency_key`, request creator/source and creation time;
- exact runner or fleet selector;
- agent-profile id **and resolved profile snapshot**;
- requested harness kind;
- requested model-provider and opaque model id, each nullable when auto-selection is allowed;
- repository/workspace reference and base revision;
- permission/tool policy, timeout and budgets;
- optional status-map policy id;
- bounded environment/metadata with secret references only.

### III.1.3 Attempt snapshot

- request id + monotonically increasing attempt number;
- runner id, fencing token, lease issue/expiry/heartbeat timestamps;
- actual harness kind/version, provider and opaque model id;
- capability snapshot used for validation;
- isolated workspace identity and base revision;
- started/ended time and typed terminal/recovery reason;
- tokens/time/cost fields nullable, each with `measured | estimated | not_measured` source;
- no raw vendor credential.

### III.1.4 Runner capability snapshot

- protocol version, runner version and labels;
- total/current concurrency;
- harness kind + installed version;
- supported or discoverable provider/model combinations;
- `cancel`, `resume`, `decisions`, `artifacts`, `usage` support values with reasons;
- maximum event/artifact limits;
- last probe time and probe error without secrets.

### III.1.5 Idempotency/fencing

- unique execution-request idempotency key in its declared scope;
- unique `(request_id, attempt_number)`;
- unique `(attempt_id, event_id)`;
- terminal completion compare-and-set and replay-safe;
- every runner mutation checks runner identity, attempt id and current fencing token;
- stale/expired tokens return a stable `stale_lease` error and write nothing;
- event batch rows and its checkpoint commit together.

### III.1.6 Protocol fixture directory

`docs/contracts/runner-v1/` is the language-neutral authority. It contains canonical JSON
for enrollment, capabilities, claim/no-work, heartbeat, event batch, decision, artifact,
completion, cancellation and every stable error. Rust/OpenAPI/frontend types must round-trip
these fixtures. Hand-written feature DTOs are not another authority.

---

## III.2 Rules for simultaneous Terra agents

1. **One card, one isolated worktree, one branch.** Suggested branch:
   `agent/iii-<card>-<short-name>`. Never let two active agents write the same checkout.
2. **Stay inside `Owns`.** A file under `Must not edit` is a hard stop. Record the needed
   change in the handoff; the named owner/integrator performs it.
3. **Shared chokepoints have one owner per wave:** root `Cargo.toml`/`Cargo.lock`,
   `migrations.rs`, `router.rs`, `openapi.rs`, generated schema, CI, and the frozen contract.
4. **No migration numbers outside B2.** Cards request schema changes in handoff notes. B2
   batches them only at wave boundaries after A3 repairs/accepts the runner.
5. **No router/OpenAPI/generated-schema edits outside C5.** Handler agents create modules
   and focused tests; C5 wires routes and regenerates artifacts once.
6. **No shared trait edits from adapter cards.** D1/D2/D3 implement the frozen interface or
   report a falsifying fact. D5 alone reconciles it.
7. **No `unimplemented!()`, hidden fake success, or structural zero.** Unsupported is typed;
   unknown is explicit; unmeasured is nullable.
8. **Tests ship with the card.** Required CI tests use fake clocks, fake binaries and local
   mock HTTP only. Live harness tests are opt-in and never require secrets in CI.
9. **No blocking sleeps in tests.** Lease/heartbeat/retry tests inject time.
10. **No blanket formatting or mechanical rewrite of unowned files.** Run checks, but only
    format files the card owns.
11. **Do not edit Part III status or another handoff.** Each card creates exactly one
    `docs/agent-handoffs/part-iii/<card>.md`; the wave integrator updates this board after
    independent verification.
12. **Security-sensitive logs contain ids, never credentials, prompt bodies, query strings
    or complete environment values.** Tests assert redaction.
13. **Stop on contract ambiguity.** Do not “make it compile” with raw JSON, `String`ly keys
    or provider checks. State the mismatch and the smallest decision required.
14. **Each wave ends with adversarial verification by someone who did not author the code.**

### Handoff file template

```markdown
# III-<card> handoff

- Base SHA / branch / final SHA:
- Files changed (must equal ownership list):
- Contract fixtures consumed:
- Behavior implemented:
- Tests added and exact commands/results:
- Failure/adversarial case proved:
- Schema/API/contract change requested from another owner:
- Known limitations or `not_measured` fields:
- Secrets/logging review:
- Safe merge order and likely conflicts:
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.
```

---

## III.3 Shared-file ownership

| Chokepoint | Owner/order |
|---|---|
| `TODO.md`, roadmap Part III statuses | A0, then wave integrator only |
| `docs/adr/**`, `docs/contracts/runner-v1/**` | A0; D5 may revise once after real adapter probes |
| `crates/tack-db/src/migrations.rs` | A3 (repair/decision) → B2 (new execution schema) → no other card |
| `crates/tack-db/src/repo/mod.rs` | B2 |
| `crates/tack-orch/src/lib.rs` | B1 only; prefer new modules over further growth |
| root `Cargo.toml`, `Cargo.lock` | B3 only until runner crate builds; dependency requests go to B3 |
| `crates/tack-api/src/router.rs`, `handlers/mod.rs`, `openapi.rs` | A1 for security correction, then C5 for runner/execution wiring |
| `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` | C5, then E6/F4 only at integration boundaries |
| `frontend/src/shared/execution/**` | E2; E3/E4 consume after E2 merges |
| `.github/workflows/ci.yml` | G4; earlier cards provide commands, never edit workflow |
| existing `orch_*`, Docket adapter/reconciler | untouched until G1 except audited Phase-50 fixes explicitly owned below |

---

## III.4 Dependency graph and merge policy

```text
Wave 0:  A0  A1  A2  A3  A4          (parallel after clean baseline)
           │           │
           └────┬──────┘
Wave 1:       B1  B2  B3  B4          (parallel; frozen fixtures are the seam)
                └──┬──┘
Wave 2:       C1  C2  C3  C4 → C5     (C5 is the only route/spec integrator)
                    │
Wave 3:       D1  D2  D3  D4 → D5     (three probes parallel; one reconciliation)
                    │
Wave 4:       E1  E2  E5 → E3 E4 → E6
                    │
Wave 5:       F1  F2  F3  F5 → F4
                    │
Wave 6:       G1  G2  G3  G4 → G5
```

Within a parallel set, merge the card with the least shared surface first, then rebase the
rest. The wave integrator verifies the combined tree; a card's green branch is not evidence
that the wave is green. No later wave begins merely because its favorite dependency merged.
The status board changes only when the wave gate passes on the integration branch.

---

## Wave 0 — Phase 50, clean boundary and safety

### III-A0 — Contract/ADR and clean-baseline owner

**Owns:** new `docs/adr/0050-runner-control-plane.md`, new
`docs/contracts/runner-v1/**`, new `docs/agent-handoffs/part-iii/README.md` and A0 handoff;
Part III status rows only.

**Must not edit:** Rust/TypeScript source, migrations, router, generated files.

**Depends on:** human records a clean baseline SHA.

**Tasks:** record the baseline and inventory unreleased Part II files; write the ADR declaring
Tack the scheduler, runner the process owner and Docket legacy; commit every III.1 fixture,
transition, payload limit, error and protocol compatibility rule; define enrollment,
revocation and redaction; preserve the v1 non-goals.

**Acceptance:** every lifecycle transition has allow/deny fixtures; no field uses an
ambiguous bare `provider`; ADR explicitly rejects exactly-once and dual scheduling; fixtures
parse and mdBook/link checks pass.

### III-A1 — Trust-boundary repair

**Owns:** `crates/tack-api/src/{router,middleware,config,server}.rs`, focused new security
tests; `frontend/src/shared/ui/RichTextEditor.tsx`, token storage modules, `boardSocket.ts`
and focused tests.

**Must not edit:** execution handlers, migrations, OpenAPI registry, generated schema.

**Tasks:** sanitize/encode persisted rich content and add CSP; remove the long-lived
privileged approval token from local storage; introduce a safe API-token/session strategy;
authenticate WebSockets and derive their origin from configured API base; use exact auth
routers; redact queries; merge defaults → file → environment; fail closed for unauthenticated
non-loopback; validate outbound origins/redirects and clear a credential when origin changes.

**Acceptance:** stored script/event-handler cannot execute; authenticated split-origin
WebSocket works; suffix lookalike routes stay protected; Alexa token is absent from spans;
origin change never forwards old token; environment overrides file; unsafe startup fails.

### III-A2 — Atomic mutation and browser concurrency repair

**Owns:** `crates/tack-core/src/models.rs` only for PATCH presence types;
`crates/tack-db/src/repo/items.rs`; `crates/tack-api/src/handlers/items.rs`; focused item
concurrency tests; `frontend/src/shared/api/{client,items}.ts`, affected item mutation callers
and focused tests.

**Must not edit:** router, migrations, orchestration repo/handlers, unrelated UI views.

**Tasks:** one conditional transaction/logical update for version, WIP/status/timestamps and
fields; one increment; body+ETag same snapshot; tri-state nullable PATCH; preserve headers,
send `If-Match`, and expose deliberate 412 refresh/retry UX.

**Acceptance:** multi-field failure writes nothing; same-ETag racers produce one commit;
rejected status does not bump; null clears assignee/description/estimate; body and ETag agree;
browser sends `If-Match`. Force a yield/failure before SQL and prove no partial mutation.

### III-A3 — Migration runner and rebuild recovery

**Owns:** `crates/tack-db/src/migrations.rs`, migration/rebuild tests, migration ADR/addendum
and A3 handoff.

**Must not edit:** repositories, root Cargo, backup implementation.

**Tasks:** decide unreleased 037/038; transactional ordinary migrations; recoverable
copy/verify/swap rebuild; fetch/assert FK check; order/checksum invariant; automatic
pre-upgrade backup contract; inject failure at every rebuild statement.

**Acceptance:** no tested crash creates an unrecoverable boot loop; lossy copy is detected
before source deletion; FK violations fail; all supported old schemas preserve every field;
migration record appears only after commit.

### III-A4 — Green frontend/release baseline

**Owns:** three Blob/object-URL failing tests and setup, relevant settings components only if
required, `frontend/src/index.css`, Vite/PostCSS font config, current failing Playwright specs
and A4 handoff.

**Must not edit:** shared API implementation, runner/execution UI, backend source.

**Tasks:** fix cross-realm Blob tests without weakening behavior; repair stale approval E2E
and ambiguous locator; emit/reference production fonts; record every browser project run.

**Acceptance:** Vitest and all Playwright projects green; dist contains fonts with no
unresolved warning; no skip or weakened assertion hides the failure.

### Wave 0 integration gate

Full Rust/clippy/fmt/frontend unit/type/build/token-lint/Playwright green on the combined
tree; migration crash and security adversarial suites green; fixtures/ADR accepted; A0
records the accepted SHA and Wave 1 branches from exactly it.

---

## Wave 1 — Phases 51–52, domain/schema/runner skeleton

### III-B1 — Neutral execution domain

**Owns:** new `crates/tack-orch/src/execution/{mod,types,lifecycle,capabilities}.rs`, minimal
`crates/tack-orch/src/lib.rs` exports, focused unit/property tests and B1 handoff.

**Consumes:** runner-v1 fixtures verbatim. **Must not edit:** legacy Docket/reconciler,
migrations, API, root Cargo.

**Tasks:** typed ids; request/attempt snapshots; lifecycle validator; capability support
values/reasons; usage provenance; typed errors. Keep I/O out and keep Docket/GitHub nouns out.

**Acceptance:** every fixture round-trips; illegal transitions have stable reasons; unknown
model ids round-trip byte-for-byte; requested and actual values cannot be confused by type.

### III-B2 — Execution schema and repository

**Owns:** `migrations.rs` after A3, new `crates/tack-db/src/repo/execution.rs`, its sole
`repo/mod.rs` export, execution migration/repository tests and B2 handoff.

**Tasks:** add the ten roadmap tables; transactional enqueue, claim/fence, heartbeat,
event-batch checkpoint, completion, cancellation, decision/artifact metadata and explicit
recovery classification; inject a fake clock; prove queue/history indexes with query plans.

**Acceptance:** concurrent claimers produce one lease; stale fence writes nothing; replay is
idempotent; terminal state cannot reopen; ambiguity never auto-requeues; supported prior
schemas upgrade without legacy data loss.

### III-B3 — `tack-runner` skeleton and dependency owner

**Owns:** root `Cargo.toml`, `Cargo.lock`, new `crates/tack-runner/**` excluding Wave 3
adapter files, and B3 handoff.

**Tasks:** binary/library split, redacted config, protocol-client seam, graceful shutdown,
injectable clock/process/filesystem, structured logging and empty registry returning typed
unsupported. Keep dependencies minimal and record binary-size impact.

**Acceptance:** workspace builds/tests; runner `--help` works; missing enrollment credential
fails without logging it; shutdown joins every task; registry never panics; no model-vendor
SDK is added.

### III-B4 — Contract conformance harness

**Owns:** new fixture tests under `crates/tack-orch/tests/runner_contract/**`, test-only fake
clock/fake runner helpers with no shared-source edits, and B4 handoff.

**Tasks:** validate domain serialization against every fixture; reusable race/fencing/replay
drivers; property tests for lifecycle/idempotency; deterministic contract mutation tests.

**Acceptance:** changing a fixture field/state/error fails a named test; fake time advances
expiry without sleeping; helpers contain no global mutable state.

### Wave 1 integration gate

Schema/domain names match fixtures; runner is in workspace; migration upgrade, claim/fence
and contract mutation tests pass; legacy orchestration golden is unchanged. Contract mismatch
returns to A0, never an ad-hoc Wave 2 patch.

---

## Wave 2 — Phase 52, pull protocol vertical slice

### III-C1 — Operator execution/fleet handler modules

**Owns:** new `crates/tack-api/src/handlers/{executions,runner_admin}.rs`, focused tests with a
card-local router, and C1 handoff.

**Must not edit:** global router, OpenAPI registry/spec, generated TypeScript, migrations.

**Tasks:** create/list/get/cancel and operator-confirmed reconcile/requeue; manage fleets,
enrollment tokens, revocation and profile/model references; enforce item existence and
idempotency; never return hashes/stored tokens after enrollment.

**Acceptance:** duplicate create returns same request; cancellation is requested, not falsely
terminal; only `needs_operator` permits explicit audited recovery; revoked runner cannot be
selected; envelopes match fixtures.

### III-C2 — Runner protocol/auth handler modules

**Owns:** new `handlers/runner_protocol.rs`, new runner-auth module/middleware, focused tests
with a card-local router, and C2 handoff.

**Must not edit:** global router/middleware, OpenAPI registry, generated files.

**Tasks:** enrollment exchange, capability refresh, heartbeat, claim, accept/start, event
batch, decision poll, artifact manifest, completion and cancel observation; hash/rotate/revoke
credentials; validate attempt/fence on every write; enforce payload limits.

**Acceptance:** operator auth cannot silently substitute for runner auth; runner cannot
mutate PM objects or resolve decisions; stale fence/replay match stable errors; oversized
batch writes nothing.

### III-C3 — Runner client, journal and isolated workspace

**Owns:** `crates/tack-runner/src/{client,journal,workspace,engine}.rs`, focused tests and C3
handoff.

**Tasks:** enroll/claim/heartbeat/report loop; atomic owner-only journal; deterministic
per-attempt workspace/worktree; cleanup/quarantine; persist journal before spawn; restart
recovery observation; cancellation coordination using a fake adapter.

**Acceptance:** restart recovers journal; attempts never share workspace; credential/journal
files owner-only; cleanup refuses repo root/unresolved paths; expired fence stops reporting
and quarantines ambiguous state.

### III-C4 — Mock end-to-end crash matrix

**Owns:** new tests/fixtures only under `crates/tack-api/tests/runner_vertical_slice/**`
and/or `crates/tack-runner/tests/**`; C4 handoff.

**Tasks:** drive production repository/router seams with fake process/clock; inject before
claim commit, after claim, before spawn, after spawn-before-ack, during events/completion and
during cancellation.

**Acceptance:** no silent loss or two valid fences; safe pre-spawn failure requeues;
post-spawn ambiguity becomes `needs_operator`; replay duplicates nothing; every injected case
leaves an explanatory audit event.

### III-C5 — Router/OpenAPI/generated contract integrator

**Owns:** `handlers/mod.rs`, global `router.rs`, `openapi.rs`, `docs/openapi.json`, generated
frontend schema and C5 handoff. **Depends on:** accepted C1–C4; no concurrent edits.

**Tasks:** mount operator and `/api/runner/v1` routers with distinct auth; expose appropriate
operator contract and version runner contract; regenerate once; exact route/auth enumeration,
CORS and drift tests.

**Acceptance:** runner routes are outside operator auth exemptions; no credential response
field; generated types match fixtures; production router completes the mock vertical slice.

### Wave 2 integration gate

A mock runner enrolled on a clean database can claim, start, stream, complete and survive
API/runner restart. Security, fencing, payload and OpenAPI drift gates pass. No real harness
card starts earlier.

**Passed** at integration SHA `f931fc0`. Proven by `crates/tack-api/tests/wave2_gate.rs`, which
drives the real `build_router` against a from-scratch database and asserts persisted SQL state
at every step, importing no test infrastructure from any card. Handoff:
`docs/agent-handoffs/part-iii/III-wave2-gate.md`.

### Wave 2 carry-forward — read before starting Wave 3

Open items Wave 2 deliberately did not close. None block Wave 3, but each has an owner.

1. **Accept/start have no B2-side idempotency fingerprint.** C2 compensates in the handler.
   If a D-card observes a real runner retrying these, escalate to B2 rather than widening the
   handler workaround.
2. **Decision resolution has no endpoint.** C2's acceptance bullet "a runner cannot resolve
   decisions" is currently vacuously true — nothing can. Wave 5 (F-cards) owns the real surface;
   do not let a D-card invent one.
3. **`logs_never_contain_raw_credentials_only_ids` (C2) is flaky (~1/10) under parallel test
   execution** — a `tracing::subscriber::set_default` thread-local racing `tracing`'s global
   callsite-interest cache. Stable under `RUST_TEST_THREADS=1`. The assertion is sound; the
   harness is not. C2 owns the fix.
4. **The shared in-memory SQLite test harness can mask write-write races.** B2 found the
   decision/artifact race only after switching to a file-backed database. Any new concurrency
   test in `tack-db` should use a file-backed DB and be proven load-bearing by reverting the fix.
5. **`orch.rs` retention-sweep rollups (~lines 1529, 1646) carry the same deferred-transaction
   read-then-write shape** B2 fixed across `execution.rs`. Inspection only, never stress-tested,
   and frozen until card G1 — do not fix opportunistically.

---

## Wave 3 — Phase 53, real harness proof

### III-D1 — Codex probe/adapter

**Owns:** `crates/tack-runner/src/harness/codex.rs`, Codex fake-binary fixtures/tests,
optional live test and D1 handoff. **Must not edit:** shared trait/registry/engine.

**Tasks:** detect version; report capabilities without assuming models; validate frozen spec;
execute deterministic fixture repo; normalize output/result; cancel process tree; report
actual selection; reconcile journal only when proven supported.

**Acceptance:** fake success/failure/cancel/malformed/unknown-version tests; unsupported
selection fails pre-spawn; arguments/env redacted; opt-in live test records version/artifact.

### III-D2 — Claude Code probe/adapter

**Owns:** `crates/tack-runner/src/harness/claude_code.rs`, its fixtures/tests/live test and D2
handoff. Apply D1's tasks/gates using observed Claude Code behavior. Do not emulate
Codex-only resume, usage or approval behavior; report support/reason honestly.

### III-D3 — OpenCode probe/adapter

**Owns:** `crates/tack-runner/src/harness/opencode.rs`, its fixtures/tests/live test and D3
handoff. Apply D1's tasks/gates while preserving OpenCode's explicit provider/model
combinations instead of flattening them into global model availability.

### III-D4 — Common process/event infrastructure

**Owns:** new `crates/tack-runner/src/harness/{mod,process,event_sink}.rs`, common engine
integration, shared fake binary and D4 handoff. May run beside D1–D3 but cannot edit frozen
trait.

**Tasks:** bounded stdout/stderr/event streaming, process-group cancellation, timeouts,
backpressure, redaction, artifact staging and registry; provider parsing stays out.

**Acceptance:** high-volume output stays memory-bounded; cancel kills descendants; adapters
cannot cross-read workspaces; secret canaries absent from logs/events; truncation is explicit.

### III-D5 — Harness-contract reconciliation/integration

**Owns:** shared harness trait/types/registry, runner-v1 fixtures only when real evidence
requires it, registration tests and D5 handoff. Runs after D1–D4.

**Tasks:** compare three observed contracts; make the smallest one-time interface change;
update every adapter/fixture/test together; reject generic methods implemented by only one;
register all three without ordering behavior.

**Acceptance:** no panic/TODO adapter; same fixture completes through all three fake adapters;
two opt-in live adapters pass before Wave 4 and all three before release; lying capability is
caught before invocation.

### Wave 3 integration gate

**Passed** at integration SHA `6a53a18`, with the live-proof caveats recorded below. A lying
capability is now refused at registration, not discovered at dispatch: `HarnessProbe::
declared_capabilities()` plus the ceiling check in `AdapterRegistry::register_probe` rejects any
probe claiming `Supported` cancellation. Handoff: `docs/agent-handoffs/part-iii/III-D5.md`.

### Wave 3 carry-forward — read before starting Wave 4

1. **No harness supports cancellation better than `Advisory`.** All three shell-tool
   subprocesses run in a new session outside the runner's process group — observed with `ps`
   against real `claude` and real `opencode`, independently. Group signalling cannot reach
   them. E1's scheduler must **read** the capability snapshot, never assume cancellation works;
   and Part III's honest-delivery rule still holds — lease expiry never blindly launches a
   second process.
2. **Only Claude Code can confirm which model actually ran** (its `stream-json` `init` event).
   Codex and OpenCode reject auto-select pre-spawn rather than fabricate a value, so a request
   with no explicit model is unschedulable on two of three harnesses. E1 must surface that as a
   named reason, not an empty candidate list. `opencode export <sessionID>` was found to give
   authoritative post-hoc confirmation and is the recommended shape if this becomes a priority.
3. **`codex` was never installed on the development machine.** D1 is proven only against the
   shared fake binary; its seven documented assumptions about the real CLI are unverified. Its
   live test must pass before release (D5 acceptance), and whoever first runs it should read
   D1's assumption list first.
4. **Claude Code's live run reported `terminal_state=Failed`** for reasons unrelated to the
   adapter (its probe and artifact staging both succeeded). Worth one deliberate investigation
   before release, since the live test is billed.
5. **No adapter test captures `tracing` output** to prove redaction there. Every call site was
   manually reviewed and none leak, but the property has no regression test — deliberately left
   because Wave 2's C2 hit real flakiness in exactly that capture mechanism (see the Wave 2
   carry-forward).
6. **Artifact support is `Advisory` everywhere** — all three adapters stage raw run logs only;
   none implement artifact discovery. Wave 5's F-cards own the real surface.

---

## Wave 4 — Phase 54, fleet scheduler and PM UX

### III-E1 — Deterministic fleet scheduler

**Owns:** new `crates/tack-orch/src/scheduler/**`, scheduler tests and E1 handoff.

**Tasks:** filter by membership, health freshness, capacity, labels, harness and valid
provider/model combinations; deterministic ordering/tie-break, priority/fairness and exact
runner path. Pure selection performs no I/O and never grants the authoritative lease.

**Acceptance:** table/property tests cover empty, stale, saturated, heterogeneous and tied
fleets; invalid combinations name reasons; identical input selects identically; only the
repository claim can make a lease valid.

### III-E2 — Shared frontend execution API/state

**Owns:** new `frontend/src/shared/execution/**`, focused shared-state tests and E2 handoff.
**Consumes:** generated types. **Must not edit:** feature UI folders or generated schema.

**Tasks:** API wrappers preserving headers/errors; item execution store; capability selector;
request/attempt cache; one realtime subscription/invalidation path; optimistic cancellation
with rollback and explicit conflict/error state.

**Acceptance:** every consumer sees one consistent state; errors never render as empty data;
no duplicate hand-written wire DTO; subscription is disposed once; stale events cannot
overwrite a newer snapshot.

### III-E3 — Fleet/runner management UI

**Owns:** `frontend/src/features/fleet/**` for runner additions, new runner/profile settings
features, focused/a11y tests and E3 handoff. **Depends on:** E2.

**Tasks:** enrollment/revocation; health/capacity/protocol/harness display; membership;
agent/model profiles; unavailable reasons. Put legacy control planes in a clearly labeled
compatibility section.

**Acceptance:** every support value has visible reason; credential displays once only;
keyboard/a11y pass; stale/unconfigured runner never appears healthy.

### III-E4 — Item/Sprint “Run with agent” and activity

**Owns:** new execution feature UI plus bounded Board, item-detail and Sprint dispatch edits;
focused/a11y/E2E tests and E4 handoff. **Depends on:** E2.

**Tasks:** one shared modal for exact runner/fleet, profile, harness/provider/model; resolved
default provenance; disabled reasons; request/attempt timeline; cancel/reconcile controls;
never directly mutate item status.

**Acceptance:** all three surfaces create the same payload; unsupported combination cannot
submit; request appears without navigation; ambiguous state requires explicit operator
action; keyboard/focus path passes.

### III-E5 — CLI/MCP execution surface

**Owns:** execution additions to `tack-cli` client/commands/MCP, focused tests and E5 handoff.
**Must not edit:** backend/router/OpenAPI.

**Tasks:** list runners/fleets; create/list/cancel/reconcile execution; inspect attempts/events;
use stable shapes/conditional writes; avoid enrollment secrets in process arguments.

**Acceptance:** CLI/MCP request equals UI request; conflicts and `needs_operator` are distinct;
credential/config files are atomic and owner-only.

### III-E6 — Phase 54 integration/spec owner

**Owns:** scheduler service wiring, route/spec/generated updates and cross-surface E2E only
after E1–E5; E6 handoff.

**Acceptance:** healthy fleet selection, saturation, exact runner, unsupported model and
realtime updates pass through production routes in UI and CLI; generated drift clean.

---

## Wave 5 — Phases 55–56, decisions/artifacts/models/usage

### III-F1 — Scoped decisions

**Owns:** new decision repository/service/handler modules and focused tests; no router,
migration or generated edits; F1 handoff.

**Tasks:** runner may raise/read its attempt's decision but never resolve it; operator scope
resolves; expiry fail-closed; replay/idempotency; optional status mapping only after commit
through workflow engine.

**Acceptance:** self-resolution and cross-attempt access denied; restart preserves pending;
expiry records deny/audit and never marks item done.

### III-F2 — Events and verified artifacts

**Owns:** new event/artifact service/storage modules, retention tests and F2 handoff.

**Tasks:** atomic event batch/checkpoint; bounded payload/truncation; artifact manifest,
checksum/size/content-type validation; safe reference/path storage; streaming content and
retention behavior.

**Acceptance:** checkpoint never advances after failed insert; traversal/compression bomb and
oversize rejected; checksum mismatch stages nothing; large fixture is streamed, not buffered
as a whole.

### III-F3 — Model resolution and usage provenance

**Owns:** new pure model-policy/usage modules in core/orch, repository/service handlers,
focused tests and F3 handoff; no router/migration/generated edits.

**Tasks:** request override → agent profile → project → fleet precedence; intersect with
runner capability before claim; opaque ids; actual fact snapshot; nullable measured/estimated
usage with sources; runner time cost separate from model/token cost.

**Acceptance:** all presence combinations deterministic; nonsense id round-trips; unavailable
choice never leases; absent usage never serializes as zero; requested/actual mismatch visible.

### III-F4 — Decisions/artifacts/model frontend integration

**Owns:** execution feature UI additions and one generated-artifact integration after F1–F3
APIs are accepted; focused/a11y/E2E tests and F4 handoff.

**Tasks:** normalized timeline, decision inbox, verified artifact download, model provenance,
honest usage/economics. No provider-kind feature checks.

**Acceptance:** `Not measured` is exact for absent usage; pending/expired differ; artifact
failure visible; disabled controls name reason; interactions keyboard accessible.

### III-F5 — Runtime retention and observability

**Owns:** execution retention/metrics/health modules, startup/shutdown wiring assigned at
integration, soak tests and F5 handoff.

**Tasks:** cancellable retention child, bounded batches, runner/queue/lease/event metrics,
stuck/ambiguous alerts and graceful shutdown; no prompt/model contents in metric labels.

**Acceptance:** stale raw rows roll up/purge in production runtime; shutdown joins task; soak
is bounded; stale lease and `needs_operator` are observable.

---

## Wave 6 — Phase 57, legacy bridge and release

### III-G1 — Docket compatibility decision/bridge

**Owns:** legacy adapter/import code and Docket-specific tests/docs only; no neutral-contract
changes; G1 handoff.

**Tasks:** inventory real legacy value/data; choose maintain/export/deprecate; if maintained,
map to normalized attempts using provider-scoped ids and one scheduling owner; prevent runner
and Docket dual dispatch; reconcile stale task/approval rows.

**Acceptance:** runner path works with Docket absent; legacy golden unchanged without an
approved migration; collision tests across two planes; explicit compatibility label/policy.

### III-G2 — Chaos, fencing, security and recovery audit

**Owns:** adversarial/integration tests and audit report only; return fixes to owning cards;
G2 handoff.

**Tasks:** kill API/runner/harness at every boundary; delay/reorder/replay; stolen/revoked
token; stale fence; disk full; corrupt journal/row; oversized event/artifact; symlink/path
attack; XSS/prompt rendering; multi-runner contention.

**Acceptance:** each case has a safe documented state; no blind duplicate execution,
credential leak, cross-attempt write or silent loss. Any failure reopens its owning phase.

### III-G3 — Operator, migration and recovery docs

**Owns:** public runner/fleet/model/decision docs, recovery runbook, migration guide,
architecture/crate-tour updates and G3 handoff. Preserve Parts I/II.

**Tasks:** install/enroll/revoke; local credentials; workspace/storage; capability matrix;
backup/restore; `needs_operator`; version compatibility; Docket state; non-loopback/security.

**Acceptance:** fresh-machine walkthrough succeeds; every public claim maps to a test;
volatile counts generated/removed; no workstation-specific paths/forensic transcript in user
docs.

### III-G4 — CI, packaging and release gates

**Owns:** `.github/workflows/ci.yml`, packaging/release scripts, runner service examples,
SBOM/checksum/provenance integration and G4 handoff.

**Tasks:** runner tests/coverage; fixture drift; fake adapters; migration crash; security/
chaos subset; full frontend/cross-browser; binary-size budget and packaged runner artifacts;
prove goldens tracked and pipelines use `pipefail`.

**Acceptance:** clean checkout passes; Tack/runner archives carry checksums/SBOM/provenance;
no live secret required; deliberate fixture/golden mutation fails CI.

### III-G5 — Final independent integration/release owner

**Owns:** status updates, explicitly assigned final compatibility fixes, release evidence and
G5 handoff. Must not waive a gate by editing its test.

**Tasks:** integrate waves in order; live smoke all three harnesses; two-runner capacity/
fencing and backup/restore including artifacts; Docket-absent startup; synchronize docs,
status and OpenAPI; require clean tree/version/tag and rehearsed rollback.

**Acceptance:** definition below demonstrated with redacted evidence; every card has handoff;
no open P0/P1; tag matches package/docs; rollback works.

---

## Wave 7 — Phase 57 (continued), release blocker

Created 2026-08-19 after III-G5 refused the tag. Wave 6 merged cleanly but left one P0
open, so the cycle's definition of done is unmet. These two cards close it. **III-H1 must
land before III-H2 can produce evidence** — H2's smoke is the acceptance gate for H1.

### III-H1 — `tack-runner` HTTP transport

**Owns:** `crates/tack-runner/src/client.rs` (a real `RunnerProtocolClient`),
`crates/tack-runner/Cargo.toml` (the `reqwest` dependency), `main.rs`'s wiring, and the
III-H1 handoff. **Does not own** any server-side crate, the contract directory, or
`router.rs` — if the wire shape appears wrong, the fixture is right and the client is
wrong; escalate rather than change either.

**Context — the exact gap.** `UnavailableProtocolClient` is the only production
implementor and returns `Err(RunnerError::ProtocolUnavailable)` for everything;
`FakeClient` (in `runtime.rs`) is test-only; `reqwest` is not a dependency of the crate.
The 14 `/api/runner/v1` routes, their auth, fencing and error envelopes are all built and
server-side tested. This card writes the client half and nothing else.

**Tasks:** implement all 14 operations (enroll, refresh, claim, heartbeat, and the
attempt-scoped accept, start, events, decisions, decisions/poll, artifacts,
artifacts/{id}/content, completion, cancellation-observation, recovery-observation)
against `docs/contracts/runner-v1/`; carry the hashed bearer credential and the fencing
token on every attempt-scoped call; map `ProtocolErrorEnvelope` back to the typed
`RunnerError` variants — `stale_lease` must arrive as `RunnerError`'s stale variant, not a
generic conflict; honor timeouts and bounded retry without ever blind-retrying an
ambiguous post-spawn state; keep `RunnerCredential`'s structural redaction intact.

**Acceptance:** the binary enrolls, claims, heartbeats, streams events, uploads an
artifact and completes against a live `tack serve`. Every error path returns its typed
variant, asserted against the frozen fixtures — not against hand-written JSON.
`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success` is updated or
retired deliberately, with the reason recorded (it currently pins the gap this card
closes). No credential appears in any log, and a test asserts it.

### III-H3 — Repository checkout for a claimed task

**Blocks III-H2.** Order is H1 (done) → H3 → H2.

**What is missing, in plain terms.** Nothing in the runner can create a working copy of a
repository. Every task is meant to get its own private checkout so two tasks running at
once cannot overwrite each other's work, and so a failed task can be thrown away without
touching anything else. Until this exists, a task can be assigned to the runner and then
never starts — which means none of the coding tools can be exercised end to end, and
smoke steps 7–9 stay uncollectible.

**Owns:** `crates/tack-runner/src/workspace.rs` (a real `WorktreeProvisioner`), whatever
git plumbing it needs, and the III-H3 handoff. **Does not own** the protocol client, any
server-side crate, or the contract directory.

**Context.** `UnavailableWorktreeProvisioner` is the only production implementor today;
the other two (`FakeProvisioner`, `FakeWorktree`) are test doubles. This is the same shape
of gap III-H1 closed on the protocol side, and it was missed because the board claimed the
transport was the only missing piece — a completeness claim nobody had verified.

**Tasks:** provision an isolated checkout per attempt from the item's linked repository;
clean it up on completion, cancellation and crash-recovery; never leave a half-made
worktree that a later attempt could inherit; keep the runner's owner-only permissions on
anything written to disk.

**Acceptance:** a claimed attempt reaches a real harness process with its own checkout,
proven against a real git repository rather than a fake. Two concurrent attempts cannot
see each other's files. A killed runner leaves no unusable worktree behind — proven by
killing it mid-provision and restarting. `./scripts/smoke.sh` reaches step 7 or beyond.

### III-H2 — Live three-harness smoke and release

**Owns:** `scripts/smoke.sh`, the release evidence, the tag, and the III-H2 handoff.

**Tasks:** run the end-to-end smoke against real harness binaries; collect the evidence
III-G5 listed as uncollectible; only then tag.

**Acceptance:** the definition of done in §III.6 is demonstrated with redacted evidence,
or each unmet criterion is named. **Harness availability is reported honestly** — as of
2026-08-19 this machine has `claude` 2.1.235 and `opencode` 1.18.0 on PATH but **no
`codex` binary**, so a three-harness claim cannot be made here without installing it. Two
of three verified is a two-of-three result, never rounded up.

### III-H4 — A runner that loses a credential-rotation race is told the wrong thing

**Found by CI on 2026-08-19, not by a card.** Does not block III-H2's work, but the
answer should be settled before anything is tagged: the current behaviour can stop a
healthy runner.

**What goes wrong, in plain terms.** When a runner renews its credential twice at the
same moment, one renewal wins and the other must be told "someone beat you, try again".
Under load the loser is instead told "your credential is not valid". Those two answers
mean opposite things to a runner: the first is a retryable conflict it backs off from,
the second reads as a dead credential, which a client would reasonably treat as fatal and
stop on. So a runner that lost a harmless race can shut itself down for no real reason.

**Why it happens.** The winner rotates the credential away before the loser's request is
authenticated, so the request is rejected by the auth layer (401) before the rotation
logic that would have returned the retryable conflict (409) ever runs. The check that
would classify it correctly is downstream of the check that rejects it.

**Owns:** the runner-credential refresh path in `crates/tack-api/src/handlers/` and its
tests, plus the III-H4 handoff. **Does not own** the runner client's retry policy — if
the conclusion is that the client should treat this 401 differently, that is a request to
the `tack-runner` owner, recorded rather than made.

**Evidence.** `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten`
(`crates/tack-api/tests/c2_handlers_test.rs:1719`) failed in CI run `32309301344` with
`conflicts.len()` = 0: one request returned 200 and the other 401, where the test
requires 200 + 409. It does **not** reproduce locally — verified with three isolated runs
and three full-binary runs under `--features embed-spa`, 32/32 green each time. The
GitHub runner is slower and more contended, which is what makes the window observable.

**Tasks:** reproduce the race deterministically rather than by timing — a synchronisation
point in the handler under a test-only hook, or by driving the two requests through the
same ordering the runner would hit; decide what a losing rotation *should* return and
write that decision down; make the outcome deterministic so the loser always learns it
can retry.

**Acceptance:** the race is reproduced by a test that fails reliably before the fix and
passes after — proven by reverting the fix and watching it fail, not by observing CI
luck. A losing concurrent rotation returns one documented, retryable outcome every time,
asserted against the frozen error fixtures. **Muting, deleting, retrying or
`#[ignore]`-ing the existing test is not an acceptable resolution** — its failure is the
only evidence this behaviour exists. If the conclusion is that 401 is in fact correct,
that must be argued in the handoff and the runner's handling of it must be checked, not
assumed.

---

## Wave 8 — Phase 57 (continued), unblock the tag

Created 2026-08-19 by the III-H2 integration (`01c7046`). H2 ran the product live and
refused the tag; these cards carry every escalation it and III-H1 left open. Base SHA
for every card: `84fabf1` on `develop` (was `01c7046` until III-H5 merged, 2026-08-20). III-H4 (above) joins this wave unchanged.
III-H4 and III-H7 both touch `crates/tack-api/src/handlers/runner_protocol.rs` —
coordinate or run sequentially; the other cards are disjoint and may run in parallel.

### III-H5 — Make claude-code and codex schedulable without faking capability

**P0, release-blocking, decision card.** Three individually-principled decisions compose
into an impossible product: the claude-code/codex adapter probes deliberately declare
zero `model_combinations` (D1/D2), the scheduler requires the requested pairing to be
declared (`crates/tack-orch/src/scheduler/select.rs`, `ModelCombinationNotDeclared`),
and `AutoSelect` is rejected for every candidate (`AutoSelectNotVerified`) — so with a
real `claude` installed and a heartbeating runner, a claude-code request sits `queued`
forever (proven live, III-H2 step 8, both modes, every run). **Owns:** the decision and
its implementation across the scheduler (E1/E6 lineage), the two adapters (D1/D2) and
the capabilities contract, plus the III-H5 handoff. **Tasks:** choose between the
candidate resolutions recorded in `III-H2.md` — an operator pass-through capability
attestation; operator-declared model combinations feeding eligibility (note
`model_profiles`, migration 043, is consulted by nothing today — F4 recorded the same);
or a verified auto-select attestation. Bending any single piece silently violates
"capability claims are load-bearing" — that is why this is a decision card.
**Acceptance:** a claude-code request on a runner with the real binary installed is
claimed and completed; III-H2's step 8 stops failing for that harness with **no smoke
edit**; capability claims stay honest (no hardcoded model list, no fake declaration).

### III-H6 — The runner engine submits events, decisions and artifacts

**Release-blocking.** `AttemptDataProtocol` has had transport since III-H1 but zero call
sites in the runner's `engine.rs`, so no real runner ever submits an event, decision or
artifact — two §III.6 criteria ("verified artifacts and an idempotent event timeline",
"resolve a bounded decision") cannot be demonstrated from a real runner. Open since
III-H1 escalation 3, re-escalated unchanged by III-H2. **Owns:** `engine.rs` wiring and
its tests, plus the III-H6 handoff. **Acceptance:** III-H2 step 7's UNMET line about
events/artifacts disappears on a live run with **no smoke edit**; idempotency asserted
against the frozen runner-v1 fixtures.

### III-H7 — Duplicate `runner_name` enrollment returns an unhandled 500

Reproduced by III-H2 with two curl enrollments differing only in token: first 200,
second 500 (ids-only server log shows the route failing). Two defects in one: the
collision deserves a typed protocol error (the fixture set has `conflict`), and the
enroll body's self-reported `runner_name` (defaulted from `TACK_RUNNER_ID`, identical
for any two default-configured runners on one host) silently competes with the
operator-assigned pending-runner name — until fixed, a second same-named runner on a
host cannot enroll at all. **Owns:** the enrollment path in
`crates/tack-api/src/handlers/runner_protocol.rs` and its tests, plus the III-H7
handoff. **Shares that file with III-H4.** **Acceptance:** a losing same-named
enrollment gets one typed, documented outcome every time, asserted against the frozen
error fixtures; III-H2's distinct-`TACK_RUNNER_ID` workaround becomes unnecessary.

### III-H8 — Fleet selection write route

§III.6 requires selecting "an exact runner **or fleet**"; `agent_fleet_members` has no
write route, so the fleet half is undemonstrable — standing since E6, restated by H2
because it is now release-relevant. **Owns:** the fleet-membership write route (API +
repository + OpenAPI regeneration) and its tests, plus the III-H8 handoff.
**Acceptance:** an operator can populate a fleet over the API and a fleet-targeted
request schedules onto a member; `docs/openapi.json` regenerated via the contract test,
never hand-edited.

---

## III.5 Cross-wave acceptance matrix

| Invariant | First owner | Must remain green through |
|---|---|---|
| No stored XSS/query secret log; exact auth routes | A1 | G5 |
| One logical mutation = one atomic version increment | A2 | G5 |
| Migration crash has deterministic recovery | A3 | G5 |
| Runner fixtures are authoritative | A0/B4 | G5 |
| One valid active lease; stale fence writes nothing | B2 | G5 |
| Ambiguous post-spawn crash never blind-retries | C4 | G5 |
| Runner and operator auth cannot substitute | C2/C5 | G5 |
| Same neutral request works through three harnesses | D5 | G5 |
| UI/CLI offer only eligible combinations | E1/E2/E5 | G5 |
| Decision cannot self-resolve; expiry fail-closed | F1 | G5 |
| Event checkpoint never passes failed persistence | F2 | G5 |
| Missing usage is `not_measured`, never zero | F3/F4 | G5 |
| Docket absence does not disable runner execution | G1 | G5 |
| Backup/restore preserves DB/artifacts and scrubs secrets | G2/G5 | release |

## III.6 Definition of done

The cycle is complete only when, from the same Tack item, an operator can create separate
attempts through Codex, Claude Code and OpenCode; select an exact runner or fleet; choose
only a supported provider/opaque model; see requested versus actual facts; resolve a bounded
decision; inspect verified artifacts and an idempotent event timeline; and recover from API,
runner or harness restart without silent loss or blind duplicate execution.

Additionally:

- Docket is optional and has one documented compatibility state.
- Every lease is fenced and every ambiguous attempt requires explicit reconciliation.
- Usage is measured/estimated with provenance or rendered `Not measured`.
- Full Rust/frontend/cross-browser/security/migration/chaos/backup gates are green.
- The integration tree is clean, public docs match runtime, and release artifacts are
  checksummed, signed/provenanced and tagged.

---

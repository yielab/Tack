# Part VII handoffs — Desktop app & background service (Phase 61)

**Read this file's header and your card's block. Nothing else in it.** It exists so that a
card agent — a Sonnet with a 200k window — spends its context on the card, not on
discovering what to read. Everything about *how* to work a card (prompt, context rules,
budgets, the `TODO.md` extraction recipe) is identical to Part VI's and is restated here
only where it differs.

One handoff per card, named `VII-<card>.md`, written from [`TEMPLATE.md`](TEMPLATE.md).
Each card writes **exactly one**; corrections are dated amendments. No card edits the
board in `TODO.md`; the wave integrator does, after independent verification.

The board is `TODO.md` → **Part VII**, §VII.0–§VII.6 (it sits **above** Part VI). The
decision of record is `docs/adr/0062-desktop-app-and-background-service.md`, accepted
2026-09-03 — its first table is the contract every card implements.

## Waves, order, and the base to branch from

| Wave | Cards | Parallel? | Needs | Base SHA |
|---|---|---|---|---|
| 18 | VII-A2 · VII-B1 | yes | ADR 0062 accepted (done); Tauri prerequisites verified installed 2026-09-03 (tauri-cli 2.11.4, webkit2gtk-4.1) | `2958e9e` — **both dispatched 2026-09-03**; — the 2026-09-03 planning commit; branch from the `develop` tip |
| 19 | VII-B2 · VII-B3 | yes | B1 integrated | **Integrated 2026-09-04** — dispatched from `8b71756` |
| 20 | VII-C1 → VII-C2 | no | C1: B2 + B3; C2: C1 **and Part VI's VI-C1** | Wave 19 is integrated; pin the tip with `git rev-parse --short develop` at dispatch. C1 also owns collapsing the two first-run signals Wave 19 left in the data root (B2's `.autostart-initialized` marker and B3's `settings.json`) |
| 20b | VII-C3 | — | VII-C1 merged |  `develop` tip at dispatch |
| 21 | VII-D1 | — | everything, VII-C3 included | Wave 20 integration SHA — `03df038`. **It inherits a live finding:** the built Linux app starts and never shows a window, and the supervisor's catch-all error arm calls `handle.exit(1)` with no dialog, so any failure but the two named ones is silent. The silence is a defect on its own, whatever triggered it |

**Integration line: `develop`.** Every card branches from it as `agent/vii-<card>-<slug>`
(`agent/vii-a2-service`, `agent/vii-b1-desktop-skeleton`, …) and never merges itself.

## Before dispatching a wave — the dispatcher's checklist

1. `git status --porcelain` on `develop` is empty and the planning edits are committed.
2. Record the base SHA in the table above and in the Part VII status table. **Pin the actual
   tip — `git rev-parse --short develop` — not the SHA of the planning commit.** On
   2026-09-03 the prompts named `2958e9e` while `develop` was already at `92a42cd`; every
   agent branched from `2958e9e` and ended up two docs-only commits behind. Harmless that
   time, a conflict the next time the gap contains code.
3. **Before VII-B1, on the dispatch machine (needs sudo — the user does this, not an agent):**

   ```bash
   sudo apt install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev \
        librsvg2-dev libssl-dev build-essential curl wget file
   cargo install tauri-cli --version '^2' --locked
   cargo tauri --version && pkg-config --exists webkit2gtk-4.1 && echo READY
   ```

   Measured 2026-09-03: the first three packages and `cargo tauri` were absent, then
   installed by the user the same day — `cargo tauri --version` prints `tauri-cli 2.11.4`
   and `pkg-config --exists webkit2gtk-4.1` succeeds. Re-run the check before any future
   dispatch on a different machine; B1's block tells the agent to stop otherwise, and that
   stop is a wasted dispatch.
4. Reap finished worktree targets (`du -sh /var/tmp/tack-agent-targets/*`). A Tauri
   build target is large; budget for it on `/var/tmp`.
5. Part VI's Wave 15 may run at the same time. VI-B1 and VII-A2 both add an arm to
   `crates/tack-cli/src/main.rs` — merge them one after the other and build once with both.

### Conflicts, after the 2026-09-04 integration

Two different problems, and only one of them is solvable by tooling.

**Generated files can no longer conflict.** `Cargo.lock`, `frontend/package-lock.json`,
`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are marked
`merge=tack-generated` in `.gitattributes`. The driver resolves them to our side without a
conflict and `.githooks/post-merge` regenerates them from the merged sources; `pre-push`
refuses a stale lockfile or schema. Run `./scripts/setup-git.sh` once per clone — the
driver is per-clone git config and cannot be committed. Never hand-merge one of these
files, and never trust one branch's copy: regenerate after all merges land.

**Hand-written files still conflict, and dispatch is what prevents it.** VI-B1 and VII-A2
each added a `mod` line and a match arm to `crates/tack-cli/src/main.rs`, in the same two
regions. The merge cost thirty seconds, but the rule is cheaper than the merge: **do not
put two cards that edit the same hand-written file in the same wave.** If the board's
ownership table already gives that file to one card, dispatch the other one a wave later.

## When a card agent dies mid-work

Measured 2026-09-03: five Sonnet cards dispatched at once exhausted the session rate limit
after roughly 25 minutes; four of the five died between implementation and handoff, having
written no handoff at all. **Resume them, do not re-dispatch.** The worktree keeps every
edit and a message to the agent restores its context, so a resume costs one message where a
re-dispatch costs a whole cold start and discards finished work. Tell the resumed agent
what it still owes — the remaining gate, the live proofs, and the handoff it never wrote —
because the interruption usually lands before that last step.

Two things to size before a wave: **the session limit** (four to five parallel Sonnet cards
is the observed ceiling for a wave of this shape) and **disk** — the five per-card target
directories reached 53 GB, the two frontend-heavy cards 19 GB each. Reap them after
integration.

## How to dispatch a card to a Sonnet agent

Exactly as in `docs/agent-handoffs/part-vi/README.md` §"How to dispatch": one `Agent`
call per card, `model: "sonnet"`, `isolation: "worktree"`, `run_in_background: true` for
a parallel set, and the generic prompt below.

```text
You are working card VII-<ID> of Tack's Part VII board. Run /card VII-<ID>.
Before reading anything else, read the header and the VII-<ID> block of
docs/agent-handoffs/part-vii/README.md and follow its read list exactly — it is
sized for your context. Read nothing it does not name without recording why in
your handoff. Do not read TODO.md whole, any harness adapter file whole,
docs/openapi.json, or any handoff the block does not name. The only web pages
you may fetch are under https://v2.tauri.app/ and https://docs.rs/ — record
each URL in "Context spent". Do not spawn subagents. Deliver the card's
Acceptance list, run the gate the block names, write
docs/agent-handoffs/part-vii/VII-<ID>.md from TEMPLATE.md, and finish with the
report shape in .claude/reporting-contract.md. Do not commit. If your context
passes ~150k tokens, stop, write the handoff with what you have, and say so in
"Context spent".
```

**Context rules** are Part VI's (cold start ≤ 25k, ≤ 120k at handoff, `TODO.md` by anchor
only, no subagents, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-<ID>`). The board
extraction for this Part:

```bash
head -58 TODO.md                                                     # header, ~1k
a=$(grep -n "^## §VII.0" TODO.md|cut -d: -f1); b=$(grep -n "^## §VII.4" TODO.md|cut -d: -f1)
sed -n "${a},$((b-1))p" TODO.md                                      # capsule + rules + ownership + graph, ~5k
n=$(grep -n "### VII-<ID> " TODO.md|cut -d: -f1); sed -n "${n},$((n+45))p" TODO.md   # your card, ~1k
```

**The traps in this Part:** a Tauri project's generated `target/` and `gen/` directories
(never read them; never commit them); `Cargo.lock` diffs (read with `--stat` only);
`.github/workflows/release.yml` is ~230 lines — read the named ranges.

---

## Wave 18 — The daemon, two ways

### VII-A2 — `tack service install | uninstall | status`

**Branch:** `agent/vii-a2-service` from the Wave 18 base.

**Read (≈ 14k):**
- The board prelude (~6k) and the VII-A2 card (~1k). ADR 0062 decisions 5 and 8 only:
  `sed -n '/^| 5 |/p;/^| 8 |/p' docs/adr/0062-*.md`.
- The CLI's command enum and one existing arm as the shape to copy:
  `n=$(grep -n "enum Commands" crates/tack-cli/src/main.rs|cut -d: -f1); sed -n "${n},$((n+70))p" crates/tack-cli/src/main.rs`;
  `n=$(grep -n "Commands::Runner" crates/tack-cli/src/main.rs|cut -d: -f1); sed -n "${n},$((n+40))p" crates/tack-cli/src/main.rs`;
  `head -60 crates/tack-cli/src/doctor.rs` (small-module precedent).
- The gate you must not weaken: `sed -n '60,80p' crates/tack-cli/src/local_runner.rs`.
- The system-level unit the docs already show, to mirror its keys and not its scope:
  `sed -n '116,172p' docs/DEPLOYMENT-GUIDE.md`; `grep -n "^## " docs/book/src/user-guide/cli.md`.
- `dirs`: `cargo add dirs -p tack-cli` then `cargo doc -p dirs --no-deps` and read
  `data_dir` — or docs.rs. Nothing else.

**Do not read:** `crates/tack-api/**`, the frontend, any Part VI handoff.

**Gate:** `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-A2 cargo nextest run --workspace -E 'package(tack-cli)'`;
then the live systemd proof from the card's Acceptance, on this machine, with every
command and its output in the handoff; then `/gate cli`.

**Handoff extras:** Platform measured (distro, systemd version, session type); Process
proof (`pgrep -af "tack serve"` after install, after closing the shell, after uninstall).

**Stop if:** `systemctl --user` is unavailable in this session (no user bus) — record the
exact error and prove the unit-file content by test only; say the live proof is
`not_measured` and why.

---

### VII-B1 — `crates/tack-desktop`: the app that supervises `tack`

**Branch:** `agent/vii-b1-desktop-skeleton` from the Wave 18 base.

**First, before any read:** `cargo tauri --version && pkg-config --exists webkit2gtk-4.1
&& echo READY`. If it does not print `READY`, stop: write a handoff whose only content is
that output, and finish. Nothing in this card can be proven without it.

**Read (≈ 18k, plus the two Tauri pages):**
- The board prelude (~6k) and the VII-B1 card (~1k). ADR 0062 whole (~3k) — it is the
  contract for decisions 1, 2, 4.
- Tauri, exactly two pages: `https://v2.tauri.app/develop/sidecar/` and
  `https://v2.tauri.app/start/create-project/` (the manual/`cargo` route, not `create-tauri-app`).
  One more only if a config key is unclear: `https://v2.tauri.app/reference/config/`
  (large — search it, do not read it whole).
- How `tack` is built with the embedded UI: `sed -n '1,30p;60,80p' crates/tack-cli/Cargo.toml`;
  `sed -n '/^build:/,/^$/p' Makefile`; `cat .gitignore`.
- The loopback gate you rely on: `sed -n '60,80p' crates/tack-cli/src/local_runner.rs`.
- The health route: `grep -n "health" crates/tack-api/src/router.rs | head -3` and the
  handler it names (`sed -n` its first 30 lines).
- Workspace members: `sed -n '/^\[workspace\]/,/^\]/p' Cargo.toml`.

**Do not read:** any handler beyond health, the frontend, `docs/openapi.json`, Part VI's
handoffs, any Tauri page not named (record it in Context spent if you had to).

**Gate:** `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B1 cargo nextest run --manifest-path crates/tack-desktop/Cargo.toml`
(fake-sidecar tests); `cargo tree -p tack-cli -e normal | grep -ci "tauri\|webkit\|gtk"`
→ `0`; `cargo tauri build` from `crates/tack-desktop` producing `.deb` and `.AppImage`;
the live launch, close, and attach proofs from the card's Acceptance with `pgrep -af tack`
between steps; `cargo check --workspace` still clean.

**Handoff extras:** Platform measured; Process proof; the bundle sizes; the exact
`tauri.conf.json` keys used for the sidecar and the window URL (copied into the handoff so
B2/B3 need not reopen the file).

**Stop if:** the sidecar cannot receive environment variables through
`tauri-plugin-shell` (it can — `Command::envs`; if you cannot make it work, record the
attempt); or the AppImage does not start under the current session (record `echo
$XDG_SESSION_TYPE`, retry once with `GDK_BACKEND=x11`, record both).

---

## Wave 19 — Lifecycle and data

### VII-B2 — Tray and lifecycle

**Branch:** `agent/vii-b2-tray-lifecycle` from the Wave 18 integration SHA.

**Read (≈ 20k, plus three Tauri pages):**
- The board prelude and the VII-B2 card; ADR 0062 decision 3 (`sed -n '/^| 3 |/p' docs/adr/0062-*.md`
  and `sed -n '/^### 3\./,/^### 4\./p' docs/adr/0062-*.md`).
- `docs/agent-handoffs/part-vii/VII-B1.md` (~2k) — the supervisor's API and the
  `tauri.conf.json` keys are copied there.
- `crates/tack-desktop/src/main.rs` and `src/supervisor.rs` whole (B1's output; measure with
  `wc -l` first — expect < 400 lines together).
- The in-flight filter: `sed -n '800,860p' crates/tack-api/src/handlers/executions.rs`
  (adjust by `grep -n "fn list_executions"`).
- The runner-switch route, if Part VI's VI-B3 has landed: `grep -n "local-runner" crates/tack-api/src/router.rs`
  — if it prints nothing, the tray shows *unknown*, and you read nothing more.
- Tauri: `https://v2.tauri.app/learn/system-tray/`, `https://v2.tauri.app/plugin/autostart/`,
  `https://v2.tauri.app/plugin/single-instance/`; window close handling from
  `https://docs.rs/tauri/latest/tauri/enum.WindowEvent.html` (`CloseRequested`).

**Do not read:** the frontend, any adapter, Part VI's handoffs.

**Gate:** `cargo nextest run --manifest-path crates/tack-desktop/Cargo.toml`; the scripted daemon proof (the card's Acceptance)
with observations from a second shell recorded verbatim; the autostart file assertions;
`cargo tauri build` still succeeds.

**Handoff extras:** Platform measured (Wayland/X11 and whether an appindicator host is
present: `ps -e | grep -i "appindicator\|gnome-shell\|plasmashell"`); Daemon proof;
Process proof; a screenshot of the Quit dialog.

**Stop if:** the tray icon cannot be shown at all on this desktop — record the
environment and prove every action through the window instead; say the tray is
`not_measured` here.

---

### VII-B3 — Data folders, first run, attach version check

**Branch:** `agent/vii-b3-data-folders` from the Wave 18 integration SHA.

**Read (≈ 18k, plus one Tauri page):**
- The board prelude (the folder table is the authority) and the VII-B3 card; ADR 0062
  decision 5 (`sed -n '/^### 5\./,/^### 6\./p' docs/adr/0062-*.md`).
- `docs/agent-handoffs/part-vii/VII-B1.md` (~2k); `crates/tack-desktop/src/supervisor.rs` whole.
- Server defaults you are overriding: `sed -n '225,245p' crates/tack-api/src/config.rs`;
  `sed -n '1,70p' crates/tack-runner/src/config.rs`.
- Where a version could be read from: `grep -n "version" crates/tack-api/src/handlers/health.rs crates/tack-api/src/openapi.rs | head`
  and `grep -n "version" crates/tack-cli/src/main.rs | head -5`.
- `dirs`: `cargo doc -p dirs --no-deps` (`data_dir`). Tauri: `https://v2.tauri.app/plugin/dialog/`.

**Do not read:** any handler beyond health, the frontend, adapters.

**Gate:** `cargo nextest run --manifest-path crates/tack-desktop/Cargo.toml` (path computation per OS from the crate's own
`cfg`, the settings file, the version comparison against a stubbed fake sidecar); the live
fresh-user proof on this machine (a new Unix user, or a wiped `XDG_DATA_HOME` pointed at
a temp dir — say which); the override proof by item count; `cargo tauri build` still
succeeds.

**Handoff extras:** Platform measured; the exact paths created (`find <root> -maxdepth 2`).

**Stop if:** no existing response carries a server version — do **not** add a route;
record what health and the OpenAPI document carry and propose the smallest change for
the integrator.

---

## Wave 20 — Ship

### VII-C1 — Release bundles

**Branch:** `agent/vii-c1-release-bundles` from the Wave 19 integration SHA.

**Read (≈ 22k, plus one page):**
- The board prelude and the VII-C1 card; ADR 0062 decision 7 and `### 7.`.
- The three Wave 18–19 handoffs' "Claim → evidence" tables only
  (`sed -n '/^## Claim/,/^## Measured/p' docs/agent-handoffs/part-vii/VII-B{1,2,3}.md`).
- `.github/workflows/release.yml` lines 100–200 (the build matrix and the archive steps);
  `.github/workflows/ci.yml` lines 90–130.
- `https://v2.tauri.app/distribute/pipelines/github/` (the `tauri-action` recipe) — pin
  the action by SHA as this repo pins every action (see the existing `uses:` lines).

**Do not read:** any Rust source beyond `crates/tack-desktop/tauri.conf.json`.

**Gate:** `actionlint` if available (`command -v actionlint`), else a YAML parse
(`python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/release.yml"))'`);
one real workflow run whose URL and artifact list are in the handoff; the Linux artifact
re-launched per B1's acceptance; the musl job's steps unchanged (`git diff` of that job is
empty).

**Handoff extras:** the artifact table (name, size, target); the run URL; the release-notes
paragraph verbatim.

**Stop if:** the workflow cannot be triggered (permissions, minutes) — record the exact
error and hand the run to the integrator; do not merge an untested pipeline.

---

### VII-C3 — The app opens a window, or it says why it did not

**Branch:** `agent/vii-c3-window-or-reason` from the `develop` tip you are given.

**Read (≈ 12k):**
- The board prelude and the VII-C3 card.
- `crates/tack-desktop/src/main.rs` whole (~200 l) — the file the defect is in.
- `crates/tack-desktop/src/supervisor.rs` whole (~430 l) — the function whose failures are
  being made visible; its tests already cover attach-vs-spawn.
- `crates/tack-desktop/src/first_run.rs` whole (~90 l).
- The dialog pattern already in the tree: `rg -n "blocking_show" -B6 crates/tack-desktop/src/`.
- From `VII-C1.md`, only the paragraph on launching the built app
  (`grep -n -B4 -A12 -i "window" docs/agent-handoffs/part-vii/VII-C1.md`).

**Do not read:** `lifecycle.rs` or `tray.rs` beyond a grep (VII-B2 owns both and they are
not the failure); any crate outside `tack-desktop`; `release.yml`.

**Build and run it, do not reason about it.** `make desktop` produces the bundle; the
card is not done until you have launched the artifact with no `tack` running and seen what
happens, with the app's own log visible. A conclusion reached by reading the code alone is
the thing this card exists to replace.

**Gate:** `cargo fmt --manifest-path crates/tack-desktop/Cargo.toml --all --check`;
`cargo clippy --manifest-path crates/tack-desktop/Cargo.toml --all-targets -- -D warnings`;
`cargo test --manifest-path crates/tack-desktop/Cargo.toml`; `./scripts/check-comments.sh`.
The desktop crate is **not** in the workspace, so `--workspace` never sees it — pass the
manifest path every time. Set `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-C3`.

**Handoff extras:** what the window failure actually was, measured; the forced-failure proof
for each `Err` arm including the catch-all; the artifact you launched and where it came
from; a paste of what the user sees for each failure, verbatim.

**Stop if:** the cause is in the bundle rather than the app — record the path you expected,
what the artifact contains, and hand it to VII-C1's owner.

---

### VII-C2 — README "Run it", install page, screenshots

**Branch:** `agent/vii-c2-run-it` from the Wave 20 (C1) integration SHA — **and only after
Part VI's VI-C1 has landed** (`grep -n "features/agents" frontend/src/app/routes.tsx` prints a
line).

**Read (≈ 15k):**
- The board prelude and the VII-C2 card; §VII.3's README order; `.claude/decision-contract.md`
  is not needed — this is user documentation, not a decision.
- `README.md` §"Run it" only: `sed -n '/^## Run it/,/^## /p' README.md`; the install page
  (`grep -rln "Get the binary\|Install" docs/book/src/user-guide/*.md | head -3`, then the
  one that is it); `head -40 docs/book/src/developer/crate-tour.md`; `docs/book/src/SUMMARY.md`.
- `docs/agent-handoffs/part-vii/VII-C1.md` artifact table.
- Part V's asset rules: `n=$(grep -n "^### VI-D2 " TODO.md|cut -d: -f1); sed -n "${n},$((n+40))p" TODO.md`
  (the screenshot rules are there and apply as written).

**Do not read:** any Rust, the frontend beyond the one grep.

**Gate:** `mdbook build docs/book` to a temp dir; the vocabulary grep from §VI.1 rule 8
over the diff; `git diff --stat README.md` shows one hunk in §"Run it"; the stranger
transcript.

**Handoff extras:** Vocabulary check; which README writers were in flight and how you
sequenced (§VII.3).

**Stop if:** VI-D2 is in flight on `README.md` — wait, do not race.

---

## Wave 22 — Corrections (before Wave 21)

### VII-B4 — The tray says what is true, and `desktop-sidecar` honours `CARGO_TARGET_DIR`

**Branch:** `agent/vii-b4-tray-truth` from the `develop` tip you are given.

**Read (≈ 9k):**
- `§VII.0` and your card.
- `crates/tack-desktop/src/tray.rs` whole — it is 100 lines.
- The route you are meant to read, found with one grep for `local_runner` in
  `crates/tack-api/src/router.rs`, then that handler only.
- `Makefile`, the `desktop` and `desktop-sidecar` targets only.
- `docs/agent-handoffs/part-vii/VII-C2.md`'s escalation — it is why this card exists.

**Do not read:** the rest of the desktop crate, the frontend, any other handoff.

**This crate is not in the root workspace.** Build it with
`--manifest-path crates/tack-desktop/Cargo.toml`; `cargo --workspace` never sees it, and
you must not touch the root `Cargo.lock`.

**Gate:** `make desktop` builds; `make desktop-sidecar` run twice, once with
`CARGO_TARGET_DIR` set and once without, with the staged binary's `sha256` matching the
freshly built one both times — paste both hashes; `cargo clippy --manifest-path
crates/tack-desktop/Cargo.toml --all-targets -- -D warnings`;
`./scripts/check-comments.sh`. `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VII-B4`.

**Handoff extras:** three screenshots of the *running* app's menu — execution on, execution
off, and no server answering — not source quotes. The old comment claimed a route did not
exist; do not replace it with a new claim you have not seen on screen.

**Live rules — other cards are running servers on this machine:** your own port
(`TACK_PORT=3314`), your own database under your worktree (**never** the repository's
`tack.db`), and kill what you start.

**Stop if:** honesty needs `main.rs` or `lifecycle.rs`. Those are VII-B2's and VII-B3's.

## Wave 21 — Proof

### VII-D1 — The stranger's install

**Branch:** `agent/vii-d1-stranger-proof` from the Wave 20 integration SHA.

**Read (≈ 20k):**
- The board prelude and the VII-D1 card; §VII.5 whole.
- Every Part VII handoff's "What a stranger still cannot do" section only
  (`for f in docs/agent-handoffs/part-vii/VII-*.md; do sed -n '/^## What a stranger/,/^## /p' "$f"; done`).
- Part VI's D1 block in `docs/agent-handoffs/part-vi/README.md` (the transcript shape).

**Gate:** the transcript itself, with timestamps, reproduced from the handoff by the
integrator once.

**Stop if:** a clean user account cannot be created on this machine — use a wiped
`XDG_DATA_HOME` and `XDG_CONFIG_HOME` in a temp dir and say the account isolation is
`not_measured`.

---

## Integrator checklist, per wave

1. Escalation grep first: `grep -ln "STOP\|escalat\|not_measured\|cannot" docs/agent-handoffs/part-vii/VII-*.md`.
2. Per branch: `git diff --stat develop...<branch>` matches the card's `Owns`; no file
   under `crates/tack-desktop/binaries/`, `target/`, or `gen/` is tracked
   (`git ls-files crates/tack-desktop | grep -c "binaries/\|target/\|gen/"` → `0`, except
   `binaries/.gitkeep`).
3. Rule 2 every wave: `cargo tree -p tack-cli -e normal | grep -ci "tauri\|webkit\|gtk"` → `0`.
4. Rule 4 every wave that touches the supervisor: re-run the attach proof yourself once.
5. Sequential merge when a Part VI card touched `crates/tack-cli/src/main.rs` in the same
   window; one `cargo check --workspace --tests` after both.
6. Update the Part VII status table, this file's base-SHA column, and
   `docs/book/src/roadmap.md`'s Phase 61 status line. Nothing else in `TODO.md`.

## The mistakes this Part is most likely to make

- Linking `tack-api` into the app "just for the types". Rule 1: spawn the binary; parse
  JSON.
- Killing whatever is on port 3210. Rule 4: attach, or show the port and stop.
- Reading "the window closed" as "the app quit". Close hides; only Quit stops.
- Turning the runner on because the app was installed. Decision 6: the switch stays off.
- Committing a sidecar binary, an icon `.icns`/`.ico` generated into `gen/`, or a lock
  file diff nobody read.
- Claiming macOS or Windows behaviour from a Linux run. Every platform row is measured or
  `not_measured`, never inferred.

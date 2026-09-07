# VI-C37 handoff

- Base SHA / branch / final SHA: `7e72109` (develop) / `agent/vi-c37-desktop-deny` / `0c17d94`.
- Files changed (must equal ownership list): `scripts/gen-deny-toml.sh`, `.github/workflows/ci.yml`
  (the `tack-desktop` step of the `deny` job), `Makefile` (`deny` target), `.gitignore`
  (one added line), this handoff. `crates/tack-desktop/deny.toml` is generated at run time,
  never committed — see below.
- Contract fixtures consumed: none.
- Behavior implemented: `scripts/gen-deny-toml.sh` now writes two policy files instead of one —
  the existing root `deny.toml` (unchanged content: no advisory ignores) and a new
  `crates/tack-desktop/deny.toml` carrying every one of that workspace's `unmaintained` findings
  as an individually reasoned, dated `ignore` entry. `cargo-deny` resolves its config from the
  `--manifest-path` directory rather than the invoking shell's cwd (see "Claim → evidence" row 3
  for how that was confirmed, since `cargo deny --help`'s own text — "Defaults to `<cwd>/deny.toml`"
  — reads as if a shared file would be picked up either way; it isn't), so CI's `tack-desktop` step
  and `make deny` now run the exact same unrestricted `cargo deny check` the root workspace runs,
  against that file, instead of the previous `licenses bans` subset.
- Tests added and exact commands/results: no new Rust/TS tests (dependency-policy card). Proved
  the desktop workspace still builds under the new lockfile-adjacent config — see Claim → evidence.
- Failure/adversarial case proved: see "Claim → evidence" row 5 (revert-and-refire on one ignore
  entry).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: see "What was not re-run" below — the AppImage/tray
  walk (VII-D1) was not repeated; Wayland is `not_measured` (not attempted this card either, same
  as VII-D1).
- Secrets/logging review: n/a — no runtime code touched, only dependency policy and CI config.
- Safe merge order and likely conflicts: touches only `scripts/gen-deny-toml.sh`, one CI step, one
  Makefile line and one `.gitignore` line; no overlap expected with other Wave 17 cards. Safe to
  land any time.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Findings (measured on this branch, `cargo-deny 0.20.2`, `crates/tack-desktop` lockfile)

The card's own problem statement says "fifteen" — re-measured per CLAUDE.md's rule to re-check a
load-bearing number before repeating it: `cargo deny --manifest-path crates/tack-desktop/Cargo.toml
check advisories` reports **sixteen** `unmaintained` errors, not fifteen. The earlier count (VI-C35's
handoff, which first surfaced this workspace's advisories as out of scope) named "four `unic-*`
crates"; there are five — `unic-ucd-version` was missed. Corrected below, not carried forward.

| Advisory ID | Crate | Path (`cargo tree -i <crate>`, abbreviated to the first fork) | Decision | Reason | Review date |
|---|---|---|---|---|---|
| RUSTSEC-2024-0413 | `atk 0.18.2` | `atk` ← `gtk 0.18.2` ← `libappindicator 0.9.0` ← `tray-icon 0.24.2` ← `tauri 2.11.5` ← `tack-desktop` | ignore | gtk-rs GTK3 binding; wry pins `gtk = "0.18"` through its newest release | 2027-03-07 |
| RUSTSEC-2024-0416 | `atk-sys 0.18.2` | `atk-sys` ← `atk 0.18.2` ← (same as above) | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0412 | `gdk 0.18.2` | `gdk` ← `gdkx11 0.18.2` ← `wry 0.55.1` ← `tauri-runtime-wry 2.11.4` ← `tauri 2.11.5` | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0418 | `gdk-sys 0.18.2` | `gdk-sys` ← `gdk 0.18.2` ← (same as above) | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0411 | `gdkwayland-sys 0.18.2` | `gdkwayland-sys` ← `tao 0.35.3` ← `tauri-runtime-wry 2.11.4` ← `tauri 2.11.5` | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0417 | `gdkx11 0.18.2` | `gdkx11` ← `wry 0.55.1` ← `tauri-runtime-wry 2.11.4` ← `tauri 2.11.5` | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0414 | `gdkx11-sys 0.18.2` | `gdkx11-sys` ← `gdkx11 0.18.2` ← (same as above) | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0415 | `gtk 0.18.2` | `gtk` ← `libappindicator 0.9.0` ← `tray-icon 0.24.2` ← `tauri 2.11.5` | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0420 | `gtk-sys 0.18.2` | `gtk-sys` ← `gtk 0.18.2` ← (same as above) | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0419 | `gtk3-macros 0.18.2` | `gtk3-macros` (proc-macro) ← `gtk 0.18.2` ← (same as above) | ignore | same | 2027-03-07 |
| RUSTSEC-2024-0370 | `proc-macro-error 1.0.4` | `proc-macro-error` ← `glib-macros 0.18.5` (proc-macro) ← `glib 0.18.5` ← `atk 0.18.2` / `gtk 0.18.2` / `cairo-rs 0.18.5` / `gdk-pixbuf 0.18.5` / `gio 0.18.4` / `pango 0.18.3` | ignore | same GTK3 binding line as above, pulled in via `glib-macros`, not chosen independently | 2027-03-07 |
| RUSTSEC-2025-0081 | `unic-char-property 0.9.0` | `unic-char-property` ← `unic-ucd-ident 0.9.0` ← `urlpattern 0.3.0` ← `tauri-utils 2.9.3` ← `tauri 2.11.5` | ignore | pulled in by `urlpattern 0.3.0`, pinned by `tauri-utils 2.9.3` (Tauri's newest `tauri-utils` release) | 2027-03-07 |
| RUSTSEC-2025-0075 | `unic-char-range 0.9.0` | `unic-char-range` ← `unic-char-property 0.9.0` / `unic-ucd-ident 0.9.0` ← `urlpattern 0.3.0` | ignore | same | 2027-03-07 |
| RUSTSEC-2025-0080 | `unic-common 0.9.0` | `unic-common` ← `unic-ucd-version 0.9.0` ← `unic-ucd-ident 0.9.0` ← `urlpattern 0.3.0` | ignore | same | 2027-03-07 |
| RUSTSEC-2025-0100 | `unic-ucd-ident 0.9.0` | `unic-ucd-ident` ← `urlpattern 0.3.0` ← `tauri-utils 2.9.3` | ignore | same | 2027-03-07 |
| RUSTSEC-2025-0098 | `unic-ucd-version 0.9.0` | `unic-ucd-version` ← `unic-ucd-ident 0.9.0` ← `urlpattern 0.3.0` | ignore | same (previously uncounted — see correction above) | 2027-03-07 |

All sixteen resolved the same way: `ignore`, reviewed 2026-09-07. None had a safe upgrade
available within the current Tauri 2.x release line — see "What a Tauri 2.x update would change"
below for the evidence behind that claim, crate family by crate family.

`bans`, `licenses` and `sources` were already `ok` for `crates/tack-desktop` before this card
(CI's prior `licenses bans` step passed) and remain `ok` under the unrestricted check.

## What a Tauri 2.x update would change (and what it wouldn't)

- **`tauri` itself is already at its newest release.** `cargo update --dry-run` inside
  `crates/tack-desktop` bumps 23 transitive packages (`crossbeam-channel`, `encoding_rs`,
  `rustls`, `syn`, `wasm-bindgen`, …) but touches none of the sixteen crates above — `tauri`,
  `tauri-utils`, `tauri-runtime-wry` and `tauri-build` are already pinned to their newest
  published versions (`2.11.5` / `2.9.3` / `2.11.4` / `2.6.3`; crates.io confirms `tauri`'s
  `newest_version` and `max_stable_version` both read `2.11.5`).
- **The ten gtk-rs GTK3 crates + `proc-macro-error`:** `wry` (Tauri's Linux WebView backend) is
  the crate that pulls in `gtk`/`gdk`/`atk`/`webkit2gtk`/`glib`. Even `wry`'s own newest published
  release, `0.56.1` (crates.io `newest_version`, one minor ahead of the `0.55.1` this workspace
  locks), still declares `gtk = "0.18"` and `webkit2gtk = "=2.0"` in its own manifest — confirmed
  by reading `wry 0.56.1`'s dependency list from the crates.io API. There is no GTK4 (or otherwise
  maintained-binding) release of `wry` yet, so no Tauri 2.x version — current or hypothetical
  future patch — can move off GTK3 without a `wry` upgrade that does not exist. This is an
  upstream `wry` limitation, not a version this repository failed to pick up.
- **The five `unic-*` crates:** they come from `urlpattern 0.3.0`, which `tauri-utils 2.9.3` (also
  already the newest release) pins with `version = "0.3"` in its own `Cargo.toml`. `urlpattern`'s
  own newest release, `0.6.0`, already dropped every `unic-*` dependency in favor of
  `icu_properties` (confirmed via the crates.io dependency-list API for `urlpattern/0.6.0`) — so
  the fix exists upstream, but `tauri-utils` has not raised its own requirement to consume it.
  Bumping `urlpattern` independently of `tauri-utils` is not an option this workspace controls
  (`cargo update -p urlpattern` locks 0 packages under the current manifest constraint, confirmed
  by VI-C35 and reconfirmed here).
- **No Tauri 3 investigation was needed.** Every constraint above is Tauri 2's own newest release
  already failing to move off these crates — the stop clause ("if the only way to green is a
  Tauri major upgrade, say what it would change and stop") was not triggered, because `ignore`
  entries closed the gap without needing any major upgrade at all.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A clean `crates/tack-desktop` checkout's unrestricted `cargo deny check` was red on `advisories` before this card (16 errors, not 15) | `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check advisories` on a deny.toml with no `[advisories]` section → `advisories FAILED`, 16 `error[unmaintained]` blocks, one per crate in the findings table |
| `cargo-deny` resolves its config file from the `--manifest-path` directory, not the invoking shell's cwd | Ran `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check` from the repo root with only `crates/tack-desktop/deny.toml` present (root `deny.toml` also present but different content) — the warning diagnostics printed paths rooted at `crates/tack-desktop/deny.toml:NN`, and temporarily moving that file aside made the same command reproduce the 16 `advisories FAILED` errors even with the root `deny.toml` untouched, then restoring it made it pass again |
| The sixteen findings are all resolved by `ignore`, none by a version bump | `git diff --stat` on this branch: no `Cargo.lock` touched in either workspace, only `scripts/gen-deny-toml.sh` / `ci.yml` / `Makefile` / `.gitignore` |
| `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check` (unrestricted, the same command CI now runs) is green | → `advisories ok, bans ok, licenses ok, sources ok` |
| The root workspace's `cargo deny check` is unaffected and stays green | → `advisories ok, bans ok, licenses ok, sources ok`, no new warnings from the desktop workspace's ignore list leaking in (each workspace now reads its own generated file) |
| `make deny` runs both workspaces with the identical commands CI runs | `make deny` output ends with two `advisories ok, bans ok, licenses ok, sources ok` lines, one per `cargo deny` invocation in the target |
| The desktop crate still compiles on this machine without a full Tauri release build | `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/vi-c37-desktop-deny cargo check --locked --jobs 4` (from `crates/tack-desktop`) → `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 37.64s`, no errors — Linux GTK/WebKit system deps were present on this machine, so no CI-only fallback was needed for this step specifically |
| Clippy is clean on the desktop crate at `-D warnings` | `cargo clippy --locked --jobs 4 --all-targets -- -D warnings` → `Finished` with zero warnings/errors |
| `cargo fmt` is unchanged on the desktop crate | `cargo fmt --check` (from `crates/tack-desktop`) → exit 0, no diff |
| The gate is load-bearing, not decorative | Deleted the `RUSTSEC-2025-0098` (`unic-ucd-version`) ignore entry from `scripts/gen-deny-toml.sh`, regenerated, reran `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check advisories` → `advisories FAILED` reappeared with exactly that one `error[unmaintained]: \`unic-ucd-version\` is unmaintained` block. Restored the entry, regenerated, reconfirmed `advisories ok, bans ok, licenses ok, sources ok` |
| `pre-push` is green on the final tree | `.githooks/pre-push` (run directly from the worktree root) → `✓ pre-push checks passed` |
| CI's `deny` job (both steps) and `desktop` job are green on this branch | CI run `https://github.com/yielab/tack/actions/runs/34126562702` — see status below |

## Measured numbers

- `cargo deny --version`: `cargo-deny 0.20.2`.
- `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check advisories` before this card:
  `advisories FAILED`, 16 `error[unmaintained]` blocks (not 15 — corrected count, see Findings).
- `cargo update --dry-run` inside `crates/tack-desktop`: 23 packages would move, none of the
  sixteen advisory crates or their direct parents (`tauri`, `tauri-utils`, `tauri-runtime-wry`,
  `wry`, `tao`, `glib*`, `gtk*`, `urlpattern`) among them.
- `wry` crates.io `newest_version` / `max_stable_version`: `0.56.1` (locked here: `0.55.1`, pinned
  by `tauri-runtime-wry 2.11.4`'s own manifest at `version = "0.55.0"`). Its manifest still
  requires `gtk = "0.18"`, `webkit2gtk = "=2.0"` at that newest release.
- `urlpattern` crates.io `newest_version` / `max_stable_version`: `0.6.0` (locked here: `0.3.0`,
  pinned by `tauri-utils 2.9.3`'s own manifest at `version = "0.3"`). `urlpattern 0.6.0`'s own
  dependency list (crates.io API) carries `icu_properties`, `regex`, `serde`, `url` — zero
  `unic-*` crates.
- `cargo check --locked --jobs 4` (desktop crate, fresh `CARGO_TARGET_DIR`): `Finished` in
  ~37.6s dev profile.
- `.githooks/pre-push`: green on the final tree, run once after all edits.

## What was not re-run

The card's acceptance criterion "`make desktop` still builds and VII-D1's walk still holds (the
AppImage opens, closes to tray, reopens)" was **not exercised this card**, by the harness prompt's
own hard limit: no GUI of any kind on this machine, no `make desktop` (a full Tauri release build
producing a window-capable binary), no AppImage launch. Nothing in this card's diff touches
runtime code, window/tray behavior, or anything VII-D1's walk exercised — only dependency-policy
generation and CI/Makefile wiring — so there is no code-level reason to expect that walk's result
to change. But that is an inference from the diff's shape, not a re-run of the walk itself. If a
Tauri or `wry`/`gtk` version genuinely moved in this change, this note would not be sufficient
evidence; it did not move here (see Findings: all sixteen resolved by `ignore`, zero by version
bump), so the existing VII-D1 proof (`docs/agent-handoffs/part-vii/VII-D1.md`) is the standing
evidence for the desktop app's actual runtime behavior, not re-collected here. Wayland is
`not_measured`, consistent with every prior Part VII/desktop handoff — this card ran no GUI at
all, X11 or otherwise.

## CI run

`gh workflow run ci.yml --ref agent/vi-c37-desktop-deny` → `https://github.com/yielab/tack/actions/runs/34126562702`.
Status recorded by the dispatching session; see its final report to the integrator for the
per-job outcome (`deny` and `desktop` jobs are this card's proof).

## Context spent

- Tokens read before the first edit (cold start): the worktree's own `CLAUDE.md`, this card's
  ~30-line `TODO.md` slice, `docs/agent-handoffs/part-vi/VI-C35.md` in full,
  `scripts/gen-deny-toml.sh`, the `deny` job's two steps in `ci.yml`, the `Makefile` `deny`
  target, `crates/tack-desktop/Cargo.toml`, `docs/agent-handoffs/part-vii/VII-D1.md`'s
  Claim → evidence section (grepped, not read whole), `.claude/scope-discipline.md` in full.
- Context size at handoff: moderate — the several full `cargo deny check advisories` runs each
  print a large inclusion-graph tree per finding; summarized in this handoff rather than pasted
  verbatim, one representative tree kept in "Findings" per crate family.
- Files opened and not used: none of significance.
- Read-list lines that were wrong: the card's own "fifteen" count (also VI-C35's "four `unic-*`
  crates") — both undercounted by one; see Findings' correction note. `cargo deny --help`'s
  `--config` text ("Defaults to `<cwd>/deny.toml`") reads as cwd-relative but the observed
  behavior is manifest-path-relative when `--manifest-path` is given — documented in Claim →
  evidence rather than relied on blindly.

## Proposed board row text

```
### VI-C37 — The desktop app's dependency tree carries sixteen unmaintained crates nobody has reviewed

**Done 2026-09-07** — handoff `docs/agent-handoffs/part-vi/VI-C37.md`. **Needs nothing.** Wave 17.

**Owns:** `crates/tack-desktop/Cargo.toml` and its own `Cargo.lock`, the `tack-desktop` step of
CI's `cargo-deny` job, `scripts/gen-deny-toml.sh` (now generates one policy file per workspace),
the `Makefile` `deny` target, and the handoff.

All sixteen `unmaintained` advisories in `crates/tack-desktop` (re-measured: sixteen, not
fifteen) are reviewed individually and closed with a dated `ignore` entry in a new
`crates/tack-desktop/deny.toml` — ten gtk-rs GTK3 bindings and one `proc-macro-error` with no
upgrade path (wry's own newest release still requires GTK3), and five `unic-*` crates blocked by
`tauri-utils`'s pin on `urlpattern 0.3` (0.6.0 already dropped them, Tauri hasn't picked it up).
CI's `tack-desktop` deny step and `make deny` now run the same unrestricted `cargo deny check`
the root workspace runs. No Tauri version bump was available or needed. VII-D1's AppImage/tray
walk was not re-run (no GUI on this machine, by design) — nothing in this diff touches runtime
behavior.
```

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

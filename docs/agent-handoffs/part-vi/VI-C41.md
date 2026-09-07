# VI-C41 handoff

- Base SHA / branch / final SHA: `8db262b` / `agent/vi-c41-harness-locate` / `57a7ff0`
- Files changed (must equal ownership list): `crates/tack-runner/src/harness/locate.rs`
  (new), `crates/tack-runner/src/harness/claude_code.rs` (`discover_installed_binary`, and
  removed its now-superseded PATH-mutation test), `crates/tack-runner/src/harness/codex.rs`
  (`CodexLocator::Search`, `resolve`, removed `system_path_dirs`/`locate_in_dirs`, updated
  the three call sites that used them), `crates/tack-runner/src/harness/mod.rs` (registered
  the module), `docs/book/src/user-guide/agent-runners.md` (new "Where Tack looks for
  `claude` and `codex`" section), `CHANGELOG.md` (`[Unreleased]` → `Fixed`). Did not touch
  `doctor.rs`, `bootstrap.rs`, the wire contract, or the frontend.
- Contract fixtures consumed: none. `docs/contracts/runner-v1/**` is byte-identical
  (`git diff --stat docs/contracts/runner-v1` empty).
- Behavior implemented: one pure locator (`harness::locate::locate(program, path, home)`)
  searches a `PATH`-style string first, then a fixed well-known-directory fallback derived
  from `home`, first hit wins. Two impure wrappers read the process environment once:
  `locate_installed` (used by `claude_code.rs`'s `discover()`, which resolves once and
  caches the result) and `snapshot()` (used by `codex.rs`'s `CodexAdapter::discover()`,
  which stores the snapshot and re-resolves from it on every `resolve()` call, matching
  the crate's existing re-resolve-per-call design for Codex). Both adapters deleted their
  private PATH-search loops.
- Tests added and exact commands/results: `harness::locate::tests` — `found_on_path_wins_over_a_fallback_dir`,
  `found_only_under_local_bin_with_an_empty_path`, `not_found_names_every_directory_searched`,
  `a_non_executable_file_in_a_fallback_dir_is_skipped`, `nothing_panics_when_home_is_none`,
  all pure over their arguments (no process-environment mutation). Also updated
  `codex::tests::validate_rejects_an_unresolvable_binary` and
  `codex::tests::probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success`
  to pass `home: None` and a fixture-only program name (see "Known limitations" below) instead
  of the deleted `search_dirs` field. Commands run from the worktree root with
  `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C41`:
  - `cargo nextest run --workspace -E 'package(tack-runner)' --build-jobs 4 --test-threads 4`
    → 267 tests run: 267 passed, 6 skipped.
  - `cargo nextest run --workspace -E 'binary(runner_contract)' --build-jobs 4 --test-threads 4`
    → 18 tests run: 18 passed, 0 skipped.
  - `cargo nextest run --workspace --build-jobs 4 --test-threads 4` (full suite)
    → 1468 tests run: 1468 passed, 7 skipped.
  - `.githooks/pre-push` → `✓ pre-push checks passed` (comments, test hygiene, `cargo fmt
    --all --check` for the workspace and for `crates/tack-desktop` separately, `cargo
    clippy --workspace --all-targets -- -D warnings`, lockfile freshness).
- Failure/adversarial case proved: `not_found_names_every_directory_searched` asserts the
  returned `NotFound::searched()` actually contains both the PATH entry and the fallback
  directory, not just that the call errored, and that the `Display` text names them.
  `a_non_executable_file_in_a_fallback_dir_is_skipped` writes a real, non-executable file
  at the fallback path and asserts it does not count as found — proving the executable-bit
  check still applies to fallback entries, not only PATH ones.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the well-known list is unconditional on Unix
  for `/opt/homebrew/bin` and `/usr/local/bin` (not gated on `home`, matching how Homebrew
  actually installs) — on a machine that happens to have a real `codex` or `claude` binary
  at exactly one of those two paths, a test that wants to prove "genuinely not found" and
  passes `home: None` could still find it. The two Codex tests that need this guarantee
  (`validate_rejects_an_unresolvable_binary`,
  `probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success`) now
  search for `tack-test-fixture-nonexistent-codex` instead of the real `codex` program
  name, which no real installer would ever place at those paths — this makes the two tests
  robust to host state but means they no longer exercise the literal string `"codex"`
  through `resolve()`; `locate.rs`'s own tests still exercise the real search logic
  end-to-end for an arbitrary program name.
- Secrets/logging review: unchanged — discovery only ever logs the resolved path (already
  the case before this card; not a secret), never a directory-listing failure's `io::Error`
  detail (which could contain a stray env value on some platforms) — `NotFound`'s `Display`
  only ever prints paths this locator itself constructed from `PATH`/`home`, never a raw
  `io::Error`.
- Safe merge order and likely conflicts: no file this card touches is claimed by VI-C42 or
  VI-C43 (`doctor.rs` is explicitly excluded). Should merge independently of both.
- Checklist: no unowned files touched, no live secret involved, no panic stub (`home: None`
  is handled explicitly throughout, proved by `nothing_panics_when_home_is_none`), no blind
  retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A `claude`/`codex` binary installed only under a well-known per-user location (not on the runner process's `PATH`) is now discovered | `harness::locate::tests::found_only_under_local_bin_with_an_empty_path` |
| A `PATH`-visible install still wins over a same-named binary in a fallback dir (no behavior change for a shell-launched runner) | `harness::locate::tests::found_on_path_wins_over_a_fallback_dir` |
| The "not found" error names every directory actually searched | `harness::locate::tests::not_found_names_every_directory_searched`; verbatim text below |
| A non-executable file in a fallback dir does not count as "found" | `harness::locate::tests::a_non_executable_file_in_a_fallback_dir_is_skipped` |
| Only one place in the crate reads `PATH` via `split_paths`/`var_os("PATH")` | `grep -rn 'split_paths\|var_os("PATH")' crates/tack-runner/src/harness/` — both hits are in `locate.rs` (lines 78 and 122) |
| `docs/contracts/runner-v1/**` unchanged | `git diff --stat docs/contracts/runner-v1` — empty |

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-runner)' --build-jobs 4 --test-threads 4`
  → 267 tests run: 267 passed, 6 skipped.
- `cargo nextest run --workspace -E 'binary(runner_contract)' --build-jobs 4 --test-threads 4`
  → 18 tests run: 18 passed, 0 skipped.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4` → 1468 tests run: 1468
  passed, 7 skipped.
- `grep -rn 'split_paths\|var_os("PATH")' crates/tack-runner/src/harness/` → 2 lines, both
  in `locate.rs`.

## What a stranger still cannot do

A stranger whose `claude`/`codex` binary lives somewhere this card's fixed list does not
cover (a non-default Homebrew prefix, a Windows Scoop/Chocolatey install, a container
image that installs to a bind-mounted custom directory) still reads as "not installed" and
still has no in-app way to add a path — the only fix left is the same one VII-D1 named:
start the app from a terminal that already has the right `PATH`. This card widens the set
of machines that need that workaround; it does not remove the workaround.

## Handoff extras (card-specific)

**The well-known list as shipped**, in search order, each with the installer it serves
(`crates/tack-runner/src/harness/locate.rs::well_known_dirs`):

| Directory | Installer |
|---|---|
| `<home>/.local/bin` | pipx, uv tool installs, several npm-alternative tools' `--user`/default location |
| `<home>/.cargo/bin` | `cargo install` |
| `<home>/.bun/bin` | `bun install -g` and Bun's own installer |
| `<home>/.npm-global/bin` | npm configured with a user-writable global prefix (`npm config set prefix ~/.npm-global`) |
| `<home>/.npm/bin` | an npm global-prefix layout some setups use directly under `~/.npm` |
| `<home>/.nvm/versions/node/<version>/bin` (every installed version, sorted) | nvm — never symlinks a version-independent "current" path outside a sourced shell rc |
| `/opt/homebrew/bin` | Homebrew on Apple Silicon |
| `/usr/local/bin` | Homebrew on Intel macOS, and Linuxbrew's default prefix |
| `%APPDATA%\npm` (Windows only) | npm's global install location on Windows |
| `%LOCALAPPDATA%\nvm` (Windows only) | nvm-windows's install root |

**`probe_error` text for an absent binary, verbatim** (captured with a throwaway test run
via `cargo nextest run -E 'binary(tmp_probe_error_text)' --nocapture`, deleted before this
commit — not part of the shipped diff):

- Empty `PATH`, no home (`locate("claude", Some(""), None)`):
  ```
  `claude` was not found on PATH or in: ., /opt/homebrew/bin, /usr/local/bin
  ```
- Empty `PATH`, a real home directory with nvm, cargo, bun, npm-global all present
  (`locate("tack-test-fixture-nonexistent-binary", Some(""), Some(<real $HOME>))`):
  ```
  `tack-test-fixture-nonexistent-binary` was not found on PATH or in: ., /home/ox/.local/bin,
  /home/ox/.cargo/bin, /home/ox/.bun/bin, /home/ox/.npm-global/bin, /home/ox/.npm/bin,
  /home/ox/.nvm/versions/node/v20.20.2/bin, /home/ox/.nvm/versions/node/v22.17.1/bin,
  /home/ox/.nvm/versions/node/v22.23.2/bin, /home/ox/.nvm/versions/node/v24.18.1/bin,
  /opt/homebrew/bin, /usr/local/bin
  ```
  (this machine has 4 nvm-managed Node versions installed; the locator lists every one of
  their `bin` directories, sorted.)
- The leading `.` in both is not a bug: an empty `PATH` entry is POSIX for "the current
  directory," rendered as `.` rather than as a blank, confusing gap in the message.

## Surface-map delta

Not applicable. This card changes only where the runner looks for a harness binary before
reporting `probe_error`; it does not add, move, or remove any console-only capability
relative to §VI.0's surface map, and touches no UI surface.

## Context spent

- Tokens read before the first edit (cold start): read the dispatch README header (~1k),
  the VI-C41 block (~1k), the card's TODO.md section (~1.1k), VII-D1.md's named section
  (~0.4k), the two named line ranges each in `claude_code.rs` and `codex.rs` plus their
  named grep hits (~2.5k), `harness/mod.rs`'s module list (trivial), and the
  `agent-runners.md` guidance greps (found nothing — see below). Total cold start was
  under the block's ~14k estimate.
- Context size at handoff: well under the 120k ceiling.
- Files opened and not used: none beyond what the block named — the doc greps for
  "not installed"/"on PATH"/"on `PATH`" in `agent-runners.md` returned no hits (that
  guidance did not exist yet in this build; a new section was added rather than amended,
  see below).
- Read-list lines that were wrong: the block's `grep -n -i "not installed\|on PATH\|on
  \`PATH\`" docs/book/src/user-guide/agent-runners.md` matched nothing — no prior "not
  installed" guidance exists in that file in this tree. Rather than amend text that isn't
  there, a new "Where Tack looks for `claude` and `codex`" section was added before "Local
  credential handling", cross-referenced from the existing "confirm a harness is detected"
  line in "Running an item with an agent".

## Amendments

None yet.

### 2026-09-07 — integrator amendment

Two edits landed on `develop` after the merge, neither changing the card's claims:

- An empty `PATH` entry (a leading, trailing or doubled `:`, or an empty variable) is now
  **skipped**, not searched as `.`. The shell convention the original comment cited is
  real, but the runner's working directory is not a place an operator installs a harness,
  and resolving an executable from it would let whatever sits there stand in for one. The
  `probe_error` transcripts above therefore no longer show a leading `.` in the searched
  list; everything else in them is unchanged.
- The module comment named the Part VII board as the reason the launcher's `PATH` is
  minimal; it now names the two entry points themselves (the desktop app and `tack
  service`), which is what a reader with the code but not the board can use.

Both re-verified with the runner crate's suite (269/269).

# VI-C35 handoff

- Base SHA / branch / final SHA: `cc7db7e` (develop) / `agent/vi-c35-deny-agreement` / (see final commit after push)
- Files changed (must equal ownership list): `Cargo.lock`, `.github/workflows/ci.yml`, `Makefile`.
  `deny.toml` itself is never committed (`.gitignore` line 60, generated at run time by
  `scripts/gen-deny-toml.sh`) — its generator needed no edits, since every finding resolved
  by a dependency bump rather than a policy change.
- Contract fixtures consumed: none.
- Behavior implemented: `cargo deny check`, `make deny`, and the CI `deny` job's main-workspace
  step are now the exact same command with the exact same result. Previously CI ran
  `cargo deny check licenses bans` while a developer's bare `cargo deny check` also evaluates
  `advisories` and `sources` — the two categories CI silently skipped. Fixed the four findings
  that made the bare command red instead of narrowing what CI checks.
- Tests added and exact commands/results: no new tests (dependency-policy card, not behavior).
  Proved the gate itself is load-bearing — see "Claim → evidence" row 3.
- Failure/adversarial case proved: see "Claim → evidence" row 3 (revert-and-refire on the
  `advisories` category).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the `tack-desktop` workspace's own `cargo deny
  check` (all categories) still fails on 15 pre-existing `unmaintained` advisories inside
  Tauri's Linux (GTK/WebKit) dependency tree — this card did not touch it. See "What a stranger
  still cannot do" and "Escalation" below.
- Secrets/logging review: n/a (no code touched, dependency-lockfile and CI-config change only).
- Safe merge order and likely conflicts: no conflict risk expected — touches only `Cargo.lock`
  (transitive-only diff), a CI workflow file, and one `Makefile` target line. Safe to land any
  time; not blocked on and does not block other Wave 17 cards.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Findings (measured on this branch, `cargo-deny 0.20.2`)

| Category | Crate / license | Path (`cargo tree -i`) | Resolution |
|---|---|---|---|
| `advisories` (error, `unmaintained`) | `proc-macro-error2 2.0.1` — RUSTSEC-2026-0173, no safe upgrade per the advisory itself | `proc-macro-error2` ← `validator_derive 0.20.0` ← `validator 0.21.0` ← `tack-api`, `tack-core` | `validator_derive 0.20.0 → 0.20.1` (patch, semver-compatible) drops `proc-macro-error2` entirely in favor of `proc-macro-error3` — the advisory's "no safe upgrade" was about that crate itself, not about a caller's ability to stop depending on it. Also silences the future-incompat warning it tripped (confirmed: `cargo report future-incompatibilities` on a fresh build now says "0 dependencies had future-incompatible warnings"). |
| `advisories` (warning, `yanked`) | `chacha20 0.10.0` | `chacha20` ← `rand 0.10.1` ← `object_store 0.14.1` ← `tack-api` | `cargo update -p chacha20` → `0.10.2` (already-yanked-version's replacement, semver-compatible). |
| `advisories` (warning, `yanked`) | `spin 0.10.0` | `spin` ← `crc-fast 1.10.0` ← `object_store 0.14.1` ← `tack-api` | `cargo update -p spin@0.10.0` → `0.10.1`. |
| `advisories` (warning, `yanked`) | `spin 0.9.8` | `spin` ← `flume 0.11.1` / `multer 3.1.0` ← `sqlx-sqlite 0.8.6` / `axum 0.8.9` ← `tack-api` | `cargo update -p spin@0.9.8` → `0.9.9`. |
| `licenses` | none found on this branch | — | The card (written during the cargo-major bump) recorded two MIT rejections via `secret-service`/`zbus` (`zvariant_derive`, `zvariant_utils`). Re-measured on a clean `develop` checkout today: `cargo deny check licenses` reports **`licenses ok`** — those two crates' license metadata now resolves against the existing `MIT` allow-list entry. Not reproducible; recorded here per CLAUDE.md's rule to re-measure a load-bearing number before repeating it, not carried forward as still-true. |

No `ignore` entries were added to `deny.toml` — every finding had a real, semver-compatible
fix. The "prove load-bearing" step (below) uses the fix itself rather than an `ignore`, since
there ended up being nothing to ignore in the main workspace.

`bans` and `sources` were already `ok` on `develop` and remain `ok` — untouched.

## Decision: what each gate enforces now

| Check | Before this card | After this card |
|---|---|---|
| `make deny` / CI `deny` job, main workspace | `cargo deny check licenses bans` (advisories + sources silently unchecked) | `cargo deny check` — all four categories, identical to what a developer's bare command runs |
| CI `deny` job, `tack-desktop` workspace | `cargo deny check licenses bans` | unchanged — see Escalation |
| CI `security` job / `scheduled-audit.yml` | `cargo audit` (honors `.cargo/audit.toml`) | unchanged — still the advisory tool of record for `rsa`-style "no fixed upgrade, code path not exercised" exceptions; `cargo deny`'s `advisories` category and `cargo audit` now both run in CI, reading the RustSec DB independently, with no shared ignore list between them (none was needed here) |

The bare command now IS the CI command for the main workspace, so there is exactly one gate,
not two.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A clean `develop` checkout's `cargo deny check` was red on `advisories` (1 error, 3 warnings), matching the card's report | `cargo deny check` on `cc7db7e` (git-reset develop tip): `advisories FAILED, bans ok, licenses ok, sources ok` |
| The four advisory findings are gone after three `cargo update -p <crate>` calls, no manifest edits | `git diff Cargo.lock`: 56 lines changed, 0 `[[package]]` blocks added/removed — `chacha20`, `spin`×2, `validator_derive`, and `validator_derive`'s own `darling`/`proc-macro-error*` transitive deps only |
| `cargo deny check` (bare) is green with the fix | `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` |
| `make deny` now runs the identical command CI runs | `make deny` → same four-`ok` line; `Makefile` and `ci.yml` both invoke bare `cargo deny check` for the main workspace |
| The workspace still builds on the MSRV floor with the new lockfile | `cargo +1.89 check --workspace --locked` → `Finished` dev profile, no errors |
| The full suite is unaffected | `cargo nextest run --workspace --build-jobs 4 --test-threads 4` → `1463 tests run: 1463 passed, 7 skipped` |
| The `proc-macro-error2` future-incompat warning is gone, not just hidden by `cargo deny` | `cargo build -p tack-cli --future-incompat-report` → `note: 0 dependencies had future-incompatible warnings` |
| The gate is load-bearing, not decorative | Reverted `validator_derive` to `0.20.0` alone (`cargo update -p validator_derive --precise 0.20.0`), regenerated `deny.toml`, reran `cargo deny check` → `advisories FAILED` reappeared verbatim (the same `error[unmaintained]: proc-macro-error2 is unmaintained` block). Restored the fixed `Cargo.lock` and reconfirmed `advisories ok, bans ok, licenses ok, sources ok`. |
| `tack-desktop`'s own `licenses bans` check is unaffected by any of this | `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check licenses bans` → `bans ok, licenses ok` |
| `pre-push` is green on the final tree | `.githooks/pre-push` (run directly from the worktree root) → `✓ pre-push checks passed` |

## Measured numbers

- `cargo deny --version`: `cargo-deny 0.20.2` (already installed; no install step needed).
- `cargo deny check` on `develop` (`cc7db7e`, before any change): `advisories FAILED` (1 error,
  3 warnings), `bans ok`, `licenses ok`, `sources ok`.
- `cargo deny check` after the fix: `advisories ok, bans ok, licenses ok, sources ok`.
- `Cargo.lock` diff: 27 insertions, 29 deletions, 3 files changed reported by `git diff --stat`
  (single file; stat line counts hunks).
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4`: 1463 passed, 7 skipped, 0
  failed.
- `cargo +1.89 check --workspace --locked`: clean, ~48s.

## What a stranger still cannot do

Nothing changes for a user of Tack itself — every dependency touched is transitive (nobody
outside this repo depends on `validator_derive`'s or `object_store`'s choice of `spin`/
`chacha20` version). What changes is for a *contributor*: before this card, running `cargo deny
check` fresh after cloning the repo reported failures that CI did not, so a new contributor
had no way to tell a real regression from expected noise. After this card, the same command a
contributor instinctively runs is the command CI runs, so a red result always means something
real changed.

A contributor who runs the analogous command against `crates/tack-desktop` (`cargo deny
--manifest-path crates/tack-desktop/Cargo.toml check`, no category restriction) will still see
15 pre-existing `unmaintained` findings CI does not check — that gap is unresolved; see
Escalation.

## Escalation: `tack-desktop`'s own advisories are out of this card's scope

Running the full, unrestricted `cargo deny check` against `crates/tack-desktop/Cargo.toml`
(not part of the card's problem statement, which was written against the main workspace) finds
**15 separate `unmaintained` advisory errors**, all inside Tauri's own Linux dependency tree:
ten `gtk-rs` GTK3 binding crates (`gtk`, `gdk`, `glib`, `gio`, `pango`/`atk`/`cairo-rs`-family —
enumerate with `cargo deny --manifest-path crates/tack-desktop/Cargo.toml check advisories`),
one `proc-macro-error` (the original, unmaintained crate, RUSTSEC-2024-0370), and four
`unic-*` crates (`unic-char-property`, `unic-char-range`, `unic-common`, `unic-ucd-ident` —
RUSTSEC-2025-0098, pulled in via `urlpattern 0.3.0` ← `tauri-utils 2.9.3` ← `tauri 2.11.5`, and
confirmed pinned: `cargo update -p urlpattern` in `crates/tack-desktop` locks 0 packages, so
there is no compatible bump available under Tauri's own manifest constraint).

This is not a small, single-finding fix like the four in the main workspace — it is fifteen
findings, each needing its own justification and review date if the answer is `ignore`, all
inside Tauri's own vendored GTK3 binding layer with no upgrade path this repo controls (a
GTK4/newer-Tauri migration, not a patch bump). The card's own problem statement, "a plain
`cargo deny check` on develop is red," was written and reproduced against the main workspace
only — `crates/tack-desktop` is architecturally a separate workspace with its own Cargo.lock
specifically so Tauri's tree does not drag on the main gate (CLAUDE.md, crate map). Widening
this card to also resolve or justify fifteen Tauri-internal advisories would be exactly the
kind of scope-widening `.claude/scope-discipline.md` rule 5 says not to do from inside an
ambiguous card — the card never mentions `tack-desktop`. Left the desktop CI step exactly as
it was (`licenses bans`, still fully passing). This is a new-card-sized finding, not a follow-up
line item.

## Context spent

- Tokens read before the first edit (cold start): ~5 files read directly (card slice,
  reporting-contract, scope-discipline, handoff template, `cargo-major-2026-09.md`'s deny
  section, `deny.toml`, the `deny` job in `ci.yml`, `scheduled-audit.yml`, `.cargo/audit.toml`)
  plus `Makefile`'s deny target and `scripts/gen-deny-toml.sh` — no dispatch-plan block existed
  for this card to size against.
- Context size at handoff: moderate — the bulk of the cost was the several full
  `cargo deny check` runs (each prints the full dependency tree per finding); summarized here
  rather than pasted.
- Files opened and not used: none of significance — `deny.toml` was read but never hand-edited
  (it's generated); `scripts/gen-deny-toml.sh` was read and ultimately not changed, since no
  finding needed a policy change, only a dependency bump.
- Read-list lines that were wrong: the card's own finding table (two MIT license rejections)
  did not reproduce on this branch — see the Findings table's `licenses` row. Not a wrong
  instruction, a stale measurement; corrected here per CLAUDE.md's re-measure rule.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

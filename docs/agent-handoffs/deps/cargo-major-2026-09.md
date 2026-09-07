# cargo-major dependency bump (2026-09) handoff

**Verdict: `toml` 0.8.2 → 1.1.5 and `validator` 0.20.0 → 0.21.0 land clean — neither bump
required a source change anywhere in the workspace. `sqlx` stays at 0.8.6: its 0.9.0 line
raises the crate's own `rust-version` to 1.94.0, five minors above this repo's pinned MSRV
floor of 1.89, so it was split out of Dependabot's cargo-major group rather than landed.**

- Base SHA / branch / final SHA: base `develop` at `cf6cbd9`, branch
  `agent/deps-cargo-major`, worked in `/var/tmp/tack-worktrees/DEPS-CARGO`.
- Files changed: `Cargo.toml` (two version specs), `Cargo.lock` (resolver output only —
  see "What actually moved" below), `.github/dependabot.yml` (new `ignore` on `sqlx`
  major updates), `CHANGELOG.md`, this handoff. `crates/tack-desktop/Cargo.lock` was
  checked and needed no change (see below).
- Contract fixtures consumed: none — `runner_contract` and `openapi_contract` both stay
  green with zero fixture edits (numbers below).
- Behavior implemented: none a user can observe. This is a dependency bump; both crates
  changed only their internal implementation, not the API surface this repo calls.

## The MSRV finding (the one thing not done)

`sqlx` 0.9.0's own `Cargo.toml` (fetched via `cargo info sqlx@0.9.0` and confirmed by
reading `~/.cargo/registry/src/*/sqlx-0.9.0/Cargo.toml:14`) declares
`rust-version = "1.94.0"`. This repo's floor is `1.89` (`Cargo.toml:21`, CI job "MSRV
(Rust 1.89 — dependency floor)", `.github/workflows/ci.yml:112,124,135`). Landing sqlx
0.9 would mean raising that floor — a policy decision (toolchain pin, README/CONTRIBUTING
badges, `rust-version` in `Cargo.toml`) that is out of scope for a dependency-bump card
and is the user's call, per the coordinator's explicit instruction. `sqlx` was left at
`0.8` (unchanged in `Cargo.toml:41`).

For comparison, the other two crates in this group were well inside the floor:
`toml` 1.1.5's `rust-version` is `1.85` (`~/.cargo/registry/src/*/toml-1.1.5+spec-1.1.0/Cargo.toml:14`);
`validator` 0.21.0's is `1.88` (`~/.cargo/registry/src/*/validator-0.21.0/Cargo.toml:14`).

`.github/dependabot.yml`'s root `cargo` entry now carries an `ignore` block scoped to
`sqlx`, `update-types: ["version-update:semver-major"]`, with a comment stating the
Rust-version conflict and when to remove it (once the floor rises to 1.94+). Dependabot
will keep proposing `sqlx`'s minor/patch releases as normal — only its next major is
suppressed.

## What actually moved, and why nothing broke

`grep -rn 'toml::' crates` found 8 call sites, all `toml::from_str` / `toml::to_string`
(`crates/tack-runner/src/journal.rs:140,157,223,428,434`, `crates/tack-runner/src/config.rs:188`,
`crates/tack-cli/src/config.rs:57`, `crates/tack-api/src/config.rs:362`). Both functions
keep the same signature and `Result`/error-type shape in `toml` 1.x — none of the eight
call sites needed a change, and the full nextest suite (including
`crates/tack-runner/src/journal.rs`'s own round-trip tests at lines 428/434) confirms the
wire format is unchanged.

`validator`'s derive attributes (`#[validate(length(...))]`, `#[validate(range(...))]`)
also needed no change. The reason surfaced while reading the lockfile diff: `validator`
0.21.0 still depends on `validator_derive = "0.20"` (`~/.cargo/registry/src/*/validator-0.21.0/Cargo.toml:80-82`)
— the derive-macro crate itself did not get a major bump this release, only the
`validator` crate (the runtime trait + built-in validation functions) did. `cargo update -p
toml -p validator` left `validator_derive` at `0.20.0` in `Cargo.lock`, confirming this.

`cargo update -p toml -p validator` touched exactly: `toml` (0.8.2 → 1.1.5, dropping
`toml_edit`/`toml_datetime` 0.x in favor of `toml`'s own new `toml_parser`/`toml_writer`
split), `serde_spanned` (0.6.9 → 1.1.1, a `toml`-family dependency), `validator` (0.20.0 →
0.21.0, dropping its `once_cell` dependency — 0.21 no longer uses it), and de-duplicated
two already-present `winnow` version lines into one (some other dependency already pulled
`winnow` 1.0.4; the old `toml_edit` 0.20.2's separate `winnow` 0.5.40 pin is what the
bump removed). No other package moved. `crates/tack-desktop/Cargo.lock` does not depend on
`toml` or `validator` from its own manifest at all — `toml` appears there only as a
transitive dependency of unrelated build tooling (`cargo_metadata`, `tauri-build`), at
versions that manifest's own resolver already chose independently — so no edit was needed
there; `./scripts/regen-generated.sh` (which runs `cargo metadata` against both manifests)
confirmed both lockfiles are current with zero diff.

## Gate results

All commands run from `/var/tmp/tack-worktrees/DEPS-CARGO` with
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/DEPS-CARGO`, `nice -n 19`.

- `cargo build --workspace -j 4`: clean, no errors.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean, zero warnings.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(runner_contract)'`:
  **18/18 passed**, no fixture edits.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(openapi_contract)'`:
  **5/5 passed**, `docs/openapi.json` unchanged (`git status` clean on that file both
  before and after).
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4` (full suite):
  **1459/1459 passed**, 7 skipped (pre-existing `#[ignore]`s, none touched).
- `./scripts/regen-generated.sh`: clean, zero diff to either `Cargo.lock`,
  `docs/openapi.json`, or `frontend/src/shared/api/schema.gen.ts`.
- `.githooks/pre-push` (run directly, from the worktree root): **exit 0** — comment gate,
  test-hygiene gate, `cargo fmt --all --check`, `cargo fmt --manifest-path
  crates/tack-desktop/Cargo.toml --all --check`, clippy, and the generated-files
  freshness check (including `npm run gen:api` against a real `frontend/node_modules`,
  installed via `npm ci` for this run) all passed with no diff produced.
- `cd frontend && npm run type-check`: clean (`tsc -b`, no errors).
- `cd frontend && npx vitest run`: **851/851 passed**, 91 test files.
- `cd frontend && npx playwright test --project=chromium --workers=2 --reporter=line`:
  **78/78 passed** (47.1s), against a real `tack serve` on port 3399 / Vite dev server on
  5199, both torn down cleanly at the end of the run (`ss -ltnp` confirmed both ports free
  afterward).
- `cargo +1.89 check --workspace --locked`: clean — the 1.89.0 toolchain was already
  installed (`rustup toolchain list`), so this ran directly rather than being skipped.
- `cargo deny check`: **`advisories FAILED`, `licenses FAILED`, `bans ok`, `sources ok`** —
  but every single finding is pre-existing on `develop` and untouched by this change, not
  introduced by it. Confirmed directly: `git diff Cargo.lock` touches only
  `toml`/`toml_edit`/`toml_datetime`/`toml_parser`/`toml_writer`/`serde_spanned`/`validator`/
  `winnow`-deduplication lines; `git show HEAD:Cargo.lock` already contained
  `proc-macro-error2 2.0.1` (RUSTSEC-2026-0173, unmaintained — pulled in via
  `validator_derive` 0.20.0, unchanged on both sides of this bump), `chacha20 0.10.0`
  (yanked, via `object_store`), and `spin 0.9.8`/`0.10.0` (both yanked, via `sqlx-sqlite`
  and `object_store` respectively) before any edit in this branch. The `licenses FAILED`
  lines are `zvariant_derive`/`zvariant_utils` (MIT, not on `deny.toml`'s allow list),
  pulled in via `secret-service`/`zbus` — `tack-runner`'s credential-store dependency,
  also untouched by this branch. None of these are addressed here; they are a pre-existing
  condition on `develop` worth a separate card, not something to wave through as part of a
  dependency bump that did not cause them.

## Look twice

- The `cargo deny check` failure is real and reproducible on a clean `develop` checkout
  too (not verified with a second checkout in this session, but the lockfile-diff argument
  above is direct: the failing lines never appear in the diff). A reviewer should not read
  "advisories FAILED / licenses FAILED" as something this branch introduced.
- `validator_derive` staying at 0.20.0 while `validator` moves to 0.21.0 is intentional
  upstream behavior (see "What actually moved" above), not a partial/failed update — `cargo
  update -p validator` alone cannot bump `validator_derive` past what `validator` 0.21.0's
  own manifest permits.
- The MSRV split (`sqlx` held back) is a deliberate scope decision from the coordinator,
  not an oversight — see "The MSRV finding" above for the exact numbers if this needs
  re-litigating later.

## Not checked

- `cargo audit` (a separate tool from `cargo deny`) was not run — the task specified
  `cargo deny check` only.
- macOS/Windows builds — this environment is Linux only, matching every other card here.
- firefox/webkit Playwright projects — the task specified chromium only.

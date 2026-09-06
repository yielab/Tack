---
name: gate
description: Run this repo's verification, cheapest-first and scoped to what changed (e.g. /gate api, /gate frontend, /gate full). Use before declaring any work done and after resolving merges.
---

# Verification — cheapest first, scoped to the diff

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.
>
> **Budget your reading with `.claude/context-budget.md`** — `TODO.md` whole is ~199k tokens,
> the three active boards ~33k (one Part ~15k; one card's cold start ~7k), all handoffs together ~240k. Grep before you read; read a range
> before a file. CLAUDE.md is already in your context — don't re-derive or re-read it.

Argument = scope: `core|db|orch|api|runner|frontend|contract|docs|full`. If no scope given,
derive it from `git diff --name-only <base>` — test the crates/dirs actually touched.
Never start with `--release` when a scoped check answers the question.

## The runner, and the two rules that keep it cheap

Rust tests run under **nextest**: `cargo nextest run --workspace …`. `.config/nextest.toml`
makes it print failures and a one-line summary and nothing else, so **a green run is ~8
lines (~150 tokens)**. `cargo test --workspace` prints ~2,700 lines (~84k tokens) to say
the same thing — never run it. If a green run prints more than a screen you are not in the
repo root, or nextest is missing (`cargo nextest --version`; install with
`cargo install cargo-nextest --locked` or the prebuilt from <https://get.nexte.st>).

- **Always `--workspace`; select with `-E`, never with `-p`.** `-p <crate>` resolves
  dependency features differently from `--workspace`, so the two forms keep separate copies
  of every tack crate in `target/` and each source change is compiled once per form you
  use — measured at 13 s of pure recompilation for switching forms with no code change.
  `-E` selects what *runs*; the build is the workspace's either way, and that is the point:
  `-E 'package(tack-api)'`, `-E 'binary(wave2_gate)'`, `-E 'test(/fencing/)'`.
- **A green summary line is the answer.** Do not read a green run's output, do not re-run
  it to "see it again", and do not "confirm" a cheap green check with an expensive one.
  When something fails, nextest prints that test's output and only that.

## Decide what to run before you run anything

Start from the diff, not from habit. `git diff --name-only <base>` first, then the cheapest
row that covers it. **Running more than the diff justifies is a defect**: it burns minutes
and tokens and teaches you nothing the cheap check did not already say.

| What the diff touches | Run | Do **not** run |
|---|---|---|
| Only `.md` outside `docs/book/` — README, boards, handoffs, ADRs | nothing | any cargo command |
| `docs/book/**` | `mdbook build docs/book` | any cargo command |
| Only comments, doc comments or log/error strings in `.rs` | `scripts/check-comments.sh` and `cargo check --workspace` | the test suite — a comment cannot move a test, and if one moves you changed behaviour by accident |
| Any test file, or a `#[cfg(test)]` module | `scripts/check-test-hygiene.sh` before the suite (~0.7 s) | discovering in CI that a new test leaves its scratch files behind |
| Rust code, any crate | `cargo nextest run --workspace` (~15 s to execute; the compile is only what you changed) and `cargo clippy --workspace --all-targets -- -D warnings` | `cargo test`; `-p` |
| Iterating on one failing test | `cargo nextest run --workspace -E 'test(<name>)'` until it passes, then the row above once | the full suite on every edit |
| Wire shapes, DTOs, handlers, the API surface | the row above **plus** the contract gates below | — |
| `frontend/**` | `cd frontend && npm run type-check && npx vitest run` | cargo anything |
| Several branches merged, or a release | `full` | — |

A change that cannot affect a test does not get a test run. Escalate scope only when the
cheap check fails, or when the diff genuinely reaches further than you first thought.

Disk note: parallel-agent builds fill `/home` — export `CARGO_TARGET_DIR` to the `/`
partition (see workspace memory).

## Rust

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```
Crate map is in CLAUDE.md. Two standing caveats:
- Concurrency claims must be proven against a **file-backed** DB — the shared in-memory
  harness can mask locking races.
- Live-harness runner tests are `#[ignore]` (some are billed): run them on purpose with
  `cargo nextest run --workspace --run-ignored ignored-only -E 'package(tack-runner)'`,
  never by default. The fake-harness path is the always-runnable one.

## Contract / spec (run when touching wire shapes, DTOs, handlers, or the API surface)

Three tests guard committed artifacts. All three run inside the full suite; the first two
also *regenerate* the artifact when asked, and the gate is that the regeneration changes
nothing:

```bash
UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)' && git diff --exit-code docs/openapi.json
UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test) | binary(docket_wire_contract_test)' && git diff --exit-code crates/tack-orch/tests/golden/
cargo nextest run --workspace -E 'binary(runner_contract)'   # never regenerated — the fixtures under docs/contracts/runner-v1/ are the authority
```

If the OpenAPI gate shows drift and the API change is intentional, regenerate — never
hand-edit — with `./scripts/regen-generated.sh` (spec, TypeScript schema and lockfiles) and
commit the generated files with the source change. Other named gates live under
`crates/*/tests/`; list them (`ls crates/*/tests/`) rather than trusting a remembered set,
and a cycle's dispatch README names the ones each card must run.

## Frontend

```bash
cd frontend && npm run type-check && npx vitest run && npm run build
```
E2E (`make e2e`) only when routes/journeys/response shapes changed. A response-shape
change must also update the matching mocks in unit/E2E tests. Record engines that cannot
run on this machine as *unverified* rather than chasing or hiding them (webkit has a
known missing-system-lib issue — check the latest handoff before burning time on it).

## `docs` (documentation-only cards — Part VI A1, A3, D1)

```bash
mdbook build docs/book                       # mdbook is at ~/.cargo/bin; CI runs this with a link check
grep -rn "claude_code" docs/book/ && echo "WRONG wire id — the harness is claude-code"
```
Plus whatever the card's dispatch block names (an executed example's transcript, a render
proof, a statement `diff`). A doc example that was not run is not verified — say so.

## `full`

All of the above: fmt, clippy, `cargo nextest run --workspace`, the three contract gates,
the frontend block. Add `make audit` for release-facing work. Every test runs once; there
is no separate list of "named gates" to run again afterwards — CI has none either.

Finish by running **`.githooks/pre-push`** itself. This list is a human transcription of it
and can drift; the hook cannot. It also checks two things easy to omit by hand: `cargo fmt`
on `crates/tack-desktop`, which is outside the workspace and so invisible to `--all`, and
whether the lockfiles and `schema.gen.ts` still match the tree after a merge.

## Reporting rules

- **Baselines come from the repo, not memory**: compare the summary line's test count
  against the most recent status-board entry or handoff in `docs/agent-handoffs/`. A drop
  in test COUNT is a finding even when everything passes.
- A status-code assertion alone doesn't prove a "writes nothing / rejects before X"
  claim — assert the absence directly, and prove new tests load-bearing by reverting the
  fix once and watching them fail.
- Flaky-under-contention failures get recorded, never silently rerun into green —
  `.config/nextest.toml` sets `retries = 0` for exactly this reason.

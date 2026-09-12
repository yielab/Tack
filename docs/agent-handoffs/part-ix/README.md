# Part IX handoffs — Human maintainability (Phase 63)

**Read this file's header and your card's block. Nothing else in it.** The whole file is
~3k tokens; a card's block is ~300. It exists so a card agent spends its context on the
card, not on discovering what to read.

One handoff per card, named `IX-<card>.md` (`IX-M1.md`, `IX-M4-tack-db-repository.md`, …),
written from [`TEMPLATE.md`](TEMPLATE.md). Each card writes **exactly one**; corrections are
appended as dated amendments and never rewritten. No card edits another card's handoff, and
no card edits the Part IX board in `TODO.md` — the wave integrator does, after independent
verification.

The board is `TODO.md` → **Part IX**, §IX.0–§IX.6 (the first Part in the file). The
specification is `docs/plans/human-maintainability.md`. The rule every card is held to is
§IX.1 rule 1: **behaviour is frozen** — a failing test is a defect in the card, never a test
to edit.

## Waves, order, and the base to branch from

| Wave | Cards | Parallel? | Needs | Base SHA |
|---|---|---|---|---|
| 27 | IX-M0 → IX-M1 → IX-M2 | no | nothing | `develop` tip at dispatch |
| 28 | IX-M3-db, then IX-M3-orch · IX-M3-api · IX-M3-runner · IX-M3-cli | db first, then four in parallel | Wave 27 integrated | Wave 27 integration SHA |
| 29 | IX-M4-<crate>-<binary>, 28 sub-cards | two at a time | Wave 28 integrated | Wave 28 integration SHA, then the latest integration SHA for each pair |
| 30 | IX-M5 | no | Wave 29's `tack-runner` sub-cards | that integration SHA |
| 31 | IX-M6 batches · IX-M7 | yes, on files no open Wave 29/30 card owns | Wave 27 integrated | latest integration SHA |
| 32 | IX-M8 | no | everything | latest integration SHA |

## Read list per card, with sizes

Every card: `TODO.md` §IX.0–§IX.1 (~2.5k tokens, `sed -n` the range from
`grep -n "^# Part IX" TODO.md`), its own card block (~400), and **one** section of the plan:

| Card | Plan section | Also | Do not read |
|---|---|---|---|
| IX-M0 | §2.3, §5 row M0 | `.githooks/pre-push`, the CI `rust` job's steps (~60 lines) | the plan whole, any handoff |
| IX-M1 | §5 row M1 | the `extract-tests` docstring in the script (`--help`) | any `src` file — the script reads them |
| IX-M2 | §3, §5 row M2 | harness audit §2.4 (~25 lines) | the preambles themselves — the script moves them |
| IX-M3-* | §2.1, §5 row M3 | `crates/<crate>/tests/common/mod.rs`; the Appendix A `grep` output for the crate | other crates' tests |
| IX-M4-* | §2.2, §5 row M4 | its binary's `measure` rows, `duplicate-tests` pairs, the layer map, its lines of `0064-fixed-waits.txt` | any other binary; production code beyond what a test calls |
| IX-M5 | harness audit §4–§6 | Wave 29's `tack-runner` handoffs | this plan beyond §5 row M5 |
| IX-M6 batch | §3, §5 row M6 | the batch's `comment-worklist --json` lines; `.claude/scope-discipline.md` "Comments" (~80 lines) | files outside the batch |
| IX-M7 | §4, §5 row M7 | `docs/book/book.toml`, `docs/book/src/SUMMARY.md`, `.gitattributes`, `scripts/regen-generated.sh` | the handoffs it moves to `docs/closed-cycles/` |
| IX-M8 | §1 table, §5 row M8 | the script's `BUDGETS` | anything else |

## The gate, per card

| Card | Run | Not |
|---|---|---|
| M0 | `.githooks/pre-push` | — |
| M1 | `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace && ./scripts/check-comments.sh && ./scripts/check-test-hygiene.sh && python3 scripts/maintainability.py check` | `cargo test` |
| M2, M6 | `cargo check --workspace && ./scripts/check-comments.sh && python3 scripts/maintainability.py check --changed` | the suite |
| M3-<crate> | `cargo nextest run --workspace -E 'package(<crate>)' && python3 scripts/maintainability.py check --changed` | the whole suite |
| M4-<crate>-<binary> | `cargo nextest run --workspace -E 'binary(<binary>)' && python3 scripts/maintainability.py check --changed`; the coverage job at integration | other binaries |
| M5 | harness audit §6 exit criteria + `-E 'package(tack-runner)'` | — |
| M7 | `mdbook build docs/book && ./scripts/regen-generated.sh && git diff --exit-code` | cargo tests |
| M8 | `python3 scripts/maintainability.py check` (hard mode) | — |

The full suite runs once per wave, by the integrator, on the integrated tree.

## Integrator checklist, per wave

1. `python3 scripts/maintainability.py check` on the integrated tree — the workspace ratio
   must not have grown; each handoff's *Budget check* matches what you measure.
2. Test count: a drop is expected in Wave 29 and Wave 30 only, and each drop is itemised in
   the handoff's *What was removed*. Anywhere else a drop is a finding.
3. CI coverage floors green (Wave 29, 30).
4. `git diff --stat develop..` touches only the files the card owns (§IX.2).
5. Regenerate generated files once; re-run `check`; update the board row.

## The mistakes this Part is built to avoid

- Editing a test to make a pruned binary green (§IX.1 rule 1).
- Re-baselining to make `check` green instead of bringing the file down.
- A "mechanical" card that hand-edits what the script refused.
- A pruning card that reads the whole crate "for context" — the input list is the context.

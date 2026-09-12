# Human maintainability: tests, comments and documentation

**Status: accepted into the roadmap 2026-09-11 as Part IX (Phase 63), the priority board;
no code changed yet.** Measured 2026-09-11 on `develop` at `f95fbc2`. Every
number comes from `scripts/maintainability.py` (this plan's tool, dry-run only so far) or
from the one-line commands in [Appendix A](#appendix-a--commands-behind-the-numbers); re-run
them before quoting. This plan is workspace-wide; the harness adapters have their own,
narrower audit in [`harness-maintainability-audit.md`](harness-maintainability-audit.md),
and its card T0 is referenced here rather than repeated.

**Decide:** approve (1) one place for each kind of test and a size budget per file and per
test, enforced by a ratchet so nothing gets bigger while the tree is brought under budget;
(2) the same budgets for comments, with prose that is about the vendor or the design moved
out of source files; (3) one source per documentation topic, with the book including the
authoritative files instead of copying them, the API reference generated from the spec,
and closed cycles archived out of the working tree; (4) an agent lane — scratch tests that
never enter the tree, a per-card budget checked by a script — so agents keep proving their
work without growing the corpus; and (5) an order of execution where every mechanical step
is one command and every judgment step is scoped to one file list, so no model reads a
file it will not change.

**Why now:** the code is fine; what surrounds it is not. Production Rust is 57k lines with
a 23 % comment share. Tests are 74k lines (1.29 : 1), 21k of them inside production files
so that the three largest "source" files are 70–90 % test. Non-generated docs are 91k
lines, 60k of which are agent handoffs. The whole suite runs in 18.8 s, so nothing here is
about speed; it is about how much a person must read before touching anything, at a tree
that grows 10k lines per active day.

**If you do nothing:** every card keeps adding 30-line tests with 90-character names in
four layers, every module keeps a 100-line preamble, and the corpus doubles again by the
next Part.

## 1. Where things are, and where they should be

| Measure | Now | After the mechanical steps (M1–M2) | Target (M8) | Command |
|---|---|---|---|---|
| Production lines (Rust, no inline tests) | 57 453 | 57 453 | — | `measure --totals` |
| Test lines / production lines | 74 014 / **1.29** | 74 014 / 1.29 (moved, not cut) | **≤ 0.8** (≈ 46k) | `measure --totals` |
| Test lines inside `src/*.rs` | 20 882 in 40 modules over budget | **0 over budget** | 0 over 150 | `extract-tests` (dry run) |
| Tests | 1 498 (18.8 s) | 1 498 | ≈ 1 000 | `nextest run --workspace` |
| Comment lines in production | 13 374 (23 %) | ≈ 12 300 (25 preambles moved) | ≤ 20 %, no file > 35 % | `measure` |
| Comment blocks over budget | 149 in 104 files | 124 | 0 | `comment-worklist` |
| Near-identical test names across files | 72 pairs | 72 | 0 | `duplicate-tests` |
| Fixed waits in test code | 74 `sleep(` | 74 | 0 | `measure --totals` |
| Live tests hiding in unit modules | 1 | 0 | 0 | `measure --json` |
| Non-generated docs lines | 91 089 | 91 089 | ≈ 30 000 in tree | Appendix A |

## 2. The test system

### 2.1 One place per kind of test

```
crates/<crate>/
  src/<module>.rs              production only; a trailing `mod tests` of ≤ 150 lines is fine
  src/<module>/tests.rs        that module's unit tests when they outgrow 150 lines
                               (`#[cfg(test)] mod tests;` — private access via `use super::*` unchanged)
  tests/
    common/mod.rs              the crate's shared fixtures — the ONLY place a helper lives
    <subject>.rs + <subject>/  one binary per subject (ADR 0064's grouping, unchanged)
    contract/                  byte-pinned: runner-v1 fixtures, OpenAPI drift, golden files
    live/                      anything that runs a real binary or bills — every test #[ignore]
    scratch_*.rs               gitignored; an agent's local proof; never tracked
crates/tack-test-support/      workspace crate for the layers below the API: migrated pool,
                               seeded project/item, controllable clock, fixture loading
```

Why `tests/common` stays per crate for API-level helpers rather than moving to the support
crate: a crate whose dev-dependency depends on it is built twice by Cargo. So
`tack-test-support` depends on `tack-core` and `tack-db` only; `tack-api/tests/common`
keeps `test_app*` and gains the request helpers that 12 files currently define for
themselves. Today `tests/common` exists in two crates with 180 lines between them and 25
importers; 18 files still define their own `project()`.

### 2.2 Rules for every test, human or agent

1. **One claim per test; variants are rows.** A family of `rejects_*` functions is one
   table-driven test whose cases fit on one screen.
2. **The name states the claim in ≤ 60 characters.** No articles, no narrative, no board
   vocabulary. `env_canary_is_redacted`, not
   `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome`.
3. **A body is ≤ 40 lines** (hard cap 60 until M8). Setup that takes longer is a fixture in
   `tests/common`.
4. **Assert state, not just status** — this rule stays from `CLAUDE.md`; it is what makes
   one test worth keeping.
5. **An invariant is pinned in at most two layers:** the repository (what the database
   ends up holding) and one router-level test (what the wire says). The contract
   fixtures pin the shape, not the behaviour. Today `replay` appears in test names across
   six crate/layer combinations and `stale` across eight.
6. **A test file's preamble is ≤ 10 lines:** what it proves and how to run it. Why it
   exists is an ADR's job.
7. **Vendor output is a file with a provenance line, never a string literal.**
8. **No fixed waits.** Poll with a bound or pause time; `docs/adr/0064-fixed-waits.txt` is
   the inventory and M4 empties it.
9. **A live test lives under `tests/live/` and is `#[ignore]`d.** Never an early `return`
   on an environment variable inside a unit module.

### 2.3 The agent lane

Agents need to prove their work; the tree does not need to keep the proof. Three
mechanisms, all checked by a script rather than a prompt:

- **Scratch tests.** `crates/*/tests/scratch_*.rs` is gitignored. Cargo still discovers
  and runs it, so an agent writes as many throwaway tests as it wants, runs them with
  `-E 'binary(scratch_x)'`, and none of it can be committed: `check` fails on a tracked
  scratch file.
- **A per-card budget.** At most 15 new tests and 600 new test lines per card, measured
  by `scripts/maintainability.py check --changed` and written into the handoff's
  "Measured numbers". Over budget is a finding for the integrator, not a merge.
- **The ratchet.** `check` compares each changed file with
  `scripts/maintainability-baseline.json`: a file may break a budget only if it was
  already over it and is not worse. New files must meet every budget. The workspace
  test : production ratio may not grow. The baseline is re-taken only by a card that
  deliberately brought files down, never to make a red check green.

## 3. Comments

The rule in `CLAUDE.md` is right and stays; `scripts/check-comments.sh` keeps enforcing
what a comment may not be about. This plan adds how much. Budgets, checked by the same
`check`:

| Block | Budget | What belongs there |
|---|---|---|
| `//!` preamble, production file | ≤ 30 lines | what the module owns, its invariants, what breaks if you change them |
| `//!` preamble, test file | ≤ 10 lines | what it proves, how to run it |
| One `///` block | ≤ 15 lines | what the item does when the name does not say, the non-obvious choice, the hazard |
| Comment share of a production file | ≤ 35 % (target 30 %) | — |

Where the rest goes: vendor behaviour at a version → the fixture directory's README, next
to the captures that prove it; design rationale → an ADR, or the book's developer guide;
history → nowhere, `git log` has it. During the transition, `extract-module-docs` parks an
over-budget preamble verbatim in `docs/dev-notes/<crate>/<module>.md` with a one-line
pointer left behind. `docs/dev-notes/` has an expiry: by the end of the cycle each note has
become an ADR section, a fixture README or nothing. Today 25 preambles are over budget
(the largest, `adapters/docket.rs`, is 213 lines) and eight production files are more than
35 % comment.

## 4. Documentation

One source per topic, generated where it can be, archived when closed.

- **The book includes, it does not copy.** `docs/TESTING.md` (384 lines) and
  `docs/book/src/developer/testing.md` (296) are two versions of one page;
  `docs/ARCHITECTURE.md` says itself that the book's crate tour is behind it. mdBook's
  `{{#include ../../TESTING.md}}` makes the `docs/*.md` files the only copy and the book
  their rendering. `CONFIG.md`, `ARCHITECTURE.md`, `TESTING.md`, `MCP.md`,
  `DEPLOYMENT-GUIDE.md` all follow this pattern.
- **The API reference is generated.** `docs/openapi.json` is already the spec of record and
  `API-REFERENCE.md` (1 556 hand-written lines) already says so in its first paragraph.
  A ~150-line `scripts/gen-api-reference.py` renders the spec to the book's
  `developer/api-reference.md`; the file joins `.gitattributes`' generated list and
  `regen-generated.sh`. `API-REFERENCE.md` shrinks to the parts a spec cannot say:
  auth surfaces, WebSocket, examples.
- **`cargo doc` is a CI step**, `--workspace --no-deps` with
  `-D rustdoc::broken_intra_doc_links`. Not `missing_docs`: that lint manufactures
  comments, the opposite of §3.
- **`CHANGELOG.md` from commits — adopted and wired 2026-09-11.** `cliff.toml` maps
  conventional commit types to Keep a Changelog sections; `make changelog` previews the
  unreleased section and `make changelog-release TAG=…` prepends it before tagging; the
  release workflow renders the same section as the GitHub release notes. Hand-written
  sections up to 0.1.0-beta.8 stay as history. Nothing left for a card.
- **Closed cycles move to `docs/closed-cycles/`** (decided 2026-09-11). `TODO.md` Parts
  I–III (10 200 of its 14 784 lines) go to `docs/closed-cycles/boards/part-<n>.md`, one
  file per Part; the handoffs of closed Parts (most of 230 files / 60k lines) go to
  `docs/closed-cycles/handoffs/part-<n>/`. A `docs/closed-cycles/README.md` says in its
  first line that nothing under it is current and that the live board is `TODO.md`. The
  directory is excluded from the book, from `context-budget.md`'s read lists and from
  `check-comments.sh`'s dead-pointer scan. Handoffs of the live Part stay in
  `docs/agent-handoffs/` and move when their Part closes.
  `TODO.md`'s "Which board is live" table gets one row per Part of ≤ 300 characters; today
  one cell is 18 906.
- **Screenshots and the roadmap** stay: they are product documentation. `roadmap.md`
  (3 601 lines) keeps only its `# Next` sections in the tree; the rest archives with the
  Parts it records.

## 5. Execution

Ordered so that every mechanical step lands before any judgment step, and every judgment
step gets a file list, not a tree. Model tokens are spent only where a decision is made.

| Card | What | Kind | Verification (scoped) | Expected delta |
|---|---|---|---|---|
| **M0** | Land the tool. Commit `scripts/maintainability.py` and a baseline (`baseline`), add `check --changed` to `pre-push` and CI, gitignore `crates/*/tests/scratch_*.rs`, add the budgets to `docs/TESTING.md` and `/gate`, `/card` and `/feature` (each prescribes commands; the memory rule is that every prescribing doc changes in the same commit). | human, 1 commit | `check` green on the untouched tree | gate exists, warns on regressions only |
| **M1** | Extract inline test modules: `extract-tests --apply` for the six workspace crates, `cargo fmt --all`, full `nextest`, `clippy --all-targets`, `check-comments.sh`. First prove it in a throwaway worktree; then run once on `develop`. Re-baseline. | mechanical, 0 model tokens | full suite (18.8 s) once | 40 modules, 20 882 lines leave `src/*.rs`; `engine.rs` goes from 4 295 lines to ≈ 1 260 |
| **M2** | Park over-budget preambles: `extract-module-docs --apply`; move vendor findings from the two adapter preambles into `fixtures/<kind>/README.md` per the harness audit. Re-baseline. | mechanical | `cargo check --workspace`, `check-comments.sh` | 25 files, ≈ 1 100 comment lines out of `src/` |
| **M3** | Test support: create `tack-test-support`; fill `tests/common` in api/orch/runner/cli; replace the 18 `project()`, 12 `request()`, 6 `runner()` and 4 `claim()` local copies. One card per crate (db, orch, api, runner, cli), each with the `grep` line from Appendix A as its file list. | agent per crate | `nextest -E 'package(<crate>)'` | −3 000 to −4 000 lines, no test removed |
| **M4** | Prune per test binary (28 binaries: api 8, orch 7, cli 6, runner 5, db 2). Input per card: its binary's `measure` rows, its `duplicate-tests` pairs, the `replay`/`stale`/`idempotent` map. Apply §2.2: rows for variants, two layers per invariant, ≤ 40-line bodies, ≤ 60-char names, ≤ 10-line preambles, waits from `0064-fixed-waits.txt` → polls. CI's existing line-coverage floors (core 85, db 70, api 70, orch 70, runner 85) are the guard against over-pruning. | agent per binary, largest first (`execution_repo.rs` 4 323 lines, `lifecycle.rs` 2 002, `crud.rs` 1 856, `chaos_recovery.rs` 1 329) | that binary, then coverage job | −25 000 lines, ≈ −400 tests |
| **M5** | Harness core: card T0 of `harness-maintainability-audit.md`, unchanged. Runs after M1 because it edits files M1 moves. | agent | `package(tack-runner)` + crash matrix | −2 000 lines, four lifecycle copies → one |
| **M6** | Trim comments per file: `comment-worklist --json` split into batches of ≈ 10 files by crate; an agent gets the block list and the §3 table, nothing else. Then empty `docs/dev-notes/`: each note → ADR / fixture README / deleted. | agent per batch | `cargo check --workspace` + `check-comments.sh` only (the `/gate` table already exempts comment-only changes from the suite) | comment share 23 % → ≤ 20 %; 0 blocks over budget |
| **M7** | Docs generation per §4: includes in the book, `gen-api-reference.py`, `cargo doc` in CI, the move to `docs/closed-cycles/`, `context-budget.md` rewritten to a quarter of its size because the files it warns about are gone. | human + one agent for the renderer | `mdbook build docs/book` with link check; `regen-generated.sh` idempotent | −60 000 doc lines from the tree |
| **M8** | Ratchet down: set BUDGETS to the targets (40-line bodies, 30 % share, ratio 0.8), re-baseline, switch `check` from warning on regressions to failing on any file over budget. | human, 1 commit | `check` green | the budgets become the definition of done |

Cost discipline for the agent cards (M3, M4, M6): each prompt carries the card's file
list and the relevant rule table; the agent runs `check --changed` and the one scoped
`nextest` filterset; the full suite runs once, at integration. M1 and M2 cost no tokens at
all and remove 22k lines from `src/`; running them first means every later agent reads
production files that are production files.

Not a card: the frontend. At 27.9k source lines to 13.9k unit and 4.5k E2E lines it is
inside the target ratio already; §2.2's naming and body rules apply to new tests, and
`check` can grow a `.test.tsx` scanner if it drifts.

## 6. Decisions this plan needs

1. ~~Archive location~~ — decided 2026-09-11: `docs/closed-cycles/` (§4).
2. ~~`git-cliff` for the changelog~~ — adopted and wired 2026-09-11 (§4).
3. The budget numbers in §2.2 and §3 — they are defaults with a command behind them, not
   findings; pick the ones you would hold a person to.
4. Whether M4 runs before or after M5. Before is proposed: it shrinks what T0 has to
   migrate.

## 7. What this plan does not do

It does not touch what the code does, what the wire contract says or what CI verifies. It
removes copies, prose and scaffolding; the contract fixtures, the OpenAPI gate, the golden
files, `wave2_gate.rs` and the coverage floors are the tests a person actually wants and
they stay.

---

## Appendix A — commands behind the numbers

All from the repository root. Dry runs only; nothing below writes to the tree.

```sh
python3 scripts/maintainability.py measure --totals       # prod / test / ratio / tests / sleeps
python3 scripts/maintainability.py measure --top 25       # per-file table, largest first
python3 scripts/maintainability.py extract-tests          # what M1 would move (dry run)
python3 scripts/maintainability.py extract-module-docs    # what M2 would move (dry run)
python3 scripts/maintainability.py comment-worklist       # M6's input, largest block first
python3 scripts/maintainability.py duplicate-tests        # M4's input: same claim, two files
MAINTAINABILITY_BASELINE=/tmp/b.json python3 scripts/maintainability.py baseline && \
MAINTAINABILITY_BASELINE=/tmp/b.json python3 scripts/maintainability.py check   # the ratchet, off-tree

# helper copies per crate (M3's file lists)
git ls-files 'crates/*/tests/**/*.rs' | xargs grep -lE 'fn (create_project|seed_project|make_project|new_project|project)\('
git ls-files 'crates/*/tests/**/*.rs' | xargs grep -lE 'fn (request|json_request|post|get|send)\('

# invariant families across layers (M4)
for w in replay stale idempoten; do echo "$w: $(git ls-files 'crates/**/*.rs' | xargs grep -lE "fn [a-z0-9_]*$w" | sed 's#crates/\([^/]*\)/\(src\|tests\).*#\1/\2#' | sort | uniq -c | tr '\n' ';')"; done

# documentation volume
git ls-files 'docs/**' | xargs cat | wc -l                                  # 142 581 incl. generated
find docs/agent-handoffs -type f | wc -l; find docs/agent-handoffs -type f | xargs cat | wc -l   # 230 / 59 986
wc -l TODO.md docs/API-REFERENCE.md docs/TESTING.md docs/book/src/developer/testing.md
cargo nextest list --workspace | grep -oE '^\s*[a-z_-]+::[a-z_0-9]+' | sort -u | awk -F:: '{c[$1]++} END{for(k in c) print k, c[k]}'
```

# Context budget — what to read, and what it costs

You are not paid by the token you read. Reading a file you did not need costs the same as
reading one you did, and it crowds out the file you will need later in the same task. This
page exists because this repository has several files that are cheap to open and expensive
to have opened.

**Measured 2026-08-30; `TODO.md` rows re-measured 2026-09-03 after Part VI was added.** Re-measure with `wc -c <file>` rather than trusting these numbers a
year from now; the ratios are the point, not the digits.

| Source | Lines | ~Tokens | Read it? |
|---|---|---|---|
| `TODO.md` **whole** | 15,000 | **~250k** | **Never.** This is more than a context window for one file that is ~90% closed-cycle history. |
| `TODO.md` active boards (lines 1–~2400) | ~2,370 | ~39k | Yes, when you need **a** board — one Part, never all. Parts IX, VIII, VII, VI, V and IV live at the top in that order; `grep -n "^# Part" TODO.md` gives the start lines, then `sed -n` the one you need (Part VII is lines 60–430, ~370 lines, ~6k; Part VI is lines 431–1400, ~970 lines, ~16k). |
| `docs/agent-handoffs/**` **all** | 12,161 | **~221k** | **Never all.** 48 files. Read the one or two your card's `Context` names. |
| `docs/agent-handoffs/part-iii/III-C2.md` (largest single) | — | ~15k | Only if named. One handoff can cost as much as every active board combined. |
| `docs/agent-handoffs/part-vi/README.md` (dispatch plan) | 559 | ~8k | **Header + your card's block only** (~2k). It tells you what else to read, with sizes; reading it whole defeats its purpose. `TEMPLATE.md` beside it is ~0.6k and replaces digging the template out of the archive. |
| `docs/agent-handoffs/part-vii/README.md` (dispatch plan) | 337 | ~5k | **Header + your card's block only** (~2k). Same shape as Part VI's; `TEMPLATE.md` beside it is a pointer plus three sections. |
| `docs/openapi.json` | 11,829 | **~88k** | **Almost never.** It is generated. To check one path, `python3 -c` or `jq` it. |
| `docs/book/src/roadmap.md` | 3,592 | ~54k | Rarely whole. It records intent, not state. The `# Next` sections at the end (Phases 60 and 61) are the live part. |
| `crates/tack-db/src/migrations.rs` | 1,614 | ~19k | Grep it for the table you care about; adding a migration needs the tail, not the file. |
| `CHANGELOG.md` | 1,250 | ~10k | Never for "what is unreleased" — it holds released sections only; `make changelog` prints the unreleased one from the commits. |
| `docs/API-REFERENCE.md` | 1,433 | ~7k | Grep for the endpoint. |
| `docs/ARCHITECTURE.md` | 326 | ~5k | Yes, whole, when you need crate-level design. It is the cheap one. |
| `docs/plans/human-maintainability.md` | 240 | ~4k | The one section your card cites (§2 tests, §3 comments, §4 docs, §5 cards). |
| `scripts/maintainability.py` | 440 | ~5k | **Never read it; run it.** `--help` lists the subcommands; `measure`, `check --changed`, `comment-worklist` and `duplicate-tests` produce the file lists a card works from. |
| `docs/TESTING.md` | 398 | ~3k | Yes, whole, when writing tests. |
| `docs/CONFIG.md` | 73 | ~1k | Yes, whole. Always cheaper than guessing an env var. |
| `CLAUDE.md` | 138 | ~1k | **Already in your context.** Do not re-read it, and do not re-derive what it says. |

## Extraction recipes

`TODO.md` — the file that costs the most and is opened the most carelessly:

```bash
head -58 TODO.md                      # the header names which Parts are ACTIVE. Trust it over section order.
grep -n "^# \|^## " TODO.md           # the map. Cheap. Do this before any sed.
n=$(grep -n "### V-A2 " TODO.md | cut -d: -f1); sed -n "${n},$((n+60))p" TODO.md   # one card
```

`docs/openapi.json` — never cat it:

```bash
python3 -c "import json;d=json.load(open('docs/openapi.json'));print(list(d['paths']))" | tr ',' '\n' | grep runner
python3 -c "import json;d=json.load(open('docs/openapi.json'));print(json.dumps(d['paths']['/api/items'],indent=2))"
```

Handoffs — find the relevant one before opening any:

```bash
grep -rln "<the thing you care about>" docs/agent-handoffs/ | head -3
ls -t docs/agent-handoffs/*/ | head -5      # most recent decisions
```

Code — locate before reading:

```bash
rg -n "<symbol>" crates/ frontend/src -l | head        # which files
rg -n "<symbol>" crates/ -A 5 | head -40               # the shape, not the file
```

## Rules

1. **Grep before you read. Read a range before you read a file. Read a file before you read a
   directory.** Each step up costs roughly an order of magnitude here.
2. **CLAUDE.md is already loaded.** Architecture, crate boundaries, the auth split, the
   migration rule, the generated-files rule — all of it is in your context before you start.
   Re-deriving it from the source is pure waste, and re-reading it is worse.
3. **Read for a question you can state.** "Understanding the codebase" is not a question. "Which
   handler owns the fencing check" is, and `rg` answers it for ~200 tokens.
4. **A file you opened and did not use is a finding.** Say so in the handoff — it usually means
   the board pointed somewhere stale, and the next agent will pay the same cost.
5. **Do not re-verify what a card's `Context` states as measured.** The boards carry measured
   facts precisely so each agent does not re-measure them. If you believe one is wrong, check
   that one and say so — do not silently re-derive the whole table.
6. **Subagents cost a full context each.** Dispatching one to read three files is more expensive
   than reading three files. Dispatch when the search is genuinely broad and you need only the
   conclusion, not when you already know the path.
7. **A green test run is eight lines.** `cargo nextest run --workspace` prints failures and one
   summary line (~150 tokens); `cargo test --workspace` prints one line per passing test (~84k
   tokens). Never run the latter, never read a green run's output, never re-run it "to see".
   The `/gate` skill has the diff→command table.

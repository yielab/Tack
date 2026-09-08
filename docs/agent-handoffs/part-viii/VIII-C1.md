# VIII-C1 handoff

- Base SHA / branch / final SHA: `abfd24b` / `agent/viii-c1-orch-new-measurement` / `9297623`
- Files changed (must equal ownership list): `.claude/scope-discipline.md` (the `orch_runs_new`/`orch_approvals_new` bullet), `docs/book/src/roadmap.md` (one paragraph repeating the same claim), this handoff. Equals ownership.
- Contract fixtures consumed: none — documentation-only card, no runner-v1 contract involved.
- Behavior implemented: none. No `.rs` file, migration, `docs/openapi.json` or `schema.gen.ts` touched.
- Tests added and exact commands/results: none added (documentation card). Verification instead: a standalone measurement program (below) plus `.githooks/pre-push`, both green.
- Failure/adversarial case proved: n/a (no code path changed).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none outstanding — the one number this card exists to settle was measured (below).
- Secrets/logging review: n/a, no code touched.
- Safe merge order and likely conflicts: no overlap with VIII-A1, VIII-B1 or VIII-B2 — disjoint files. The only *content* overlap is with the wave integrator: `TODO.md:3797-3798` repeats the same falsified claim inside the VIII-C1 evidence table's own row and inside V-B2's archived card context, and I did not edit `TODO.md` (out of scope, per the dispatch prompt and per §VIII.2's chokepoint table). The integrator should apply the same correction there when next touching that region.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `orch_runs_new` and `orch_approvals_new` do **not** exist as tables in a fully-migrated Tack database — they are transient staging-table names used only inside the single-transaction 037/038 rebuilds, renamed away (`ALTER TABLE ... RENAME TO ...`) before that transaction commits. | See *Measured numbers* below — full command and raw output. |
| `.claude/scope-discipline.md`'s claim that these two names "are still in the schema: rebuild leftovers ... that nothing cleaned up" was **false**, and has been removed from that file's evidence list. | `git diff .claude/scope-discipline.md` in this branch. |
| ADR 0060's Measurement section was correct as written; no amendment needed. | Re-ran its stated recipe verbatim in spirit (see below) and got the same shape of result: 48 tables total, neither `_new` name present. |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

**The verdict: `.claude/scope-discipline.md` was wrong. ADR 0060 was right.** Nothing in ADR 0060 needed correcting.

### Why a workaround was needed instead of running ADR 0060's recipe verbatim

ADR 0060's Measurement section states the recipe as pseudocode (`sqlite::memory:` →
`tack_db::migrations::run_all()` → `SELECT name FROM sqlite_master ...`), not a literal
runnable shell command — there is no existing test, binary, or example in this tree that
already does exactly this and prints the table list, and this card must not add a `.rs`
file to get one (documentation-only, per the dispatch prompt and §VIII.1 rule 1's "no new
surface"). So I built the same query as a tiny **out-of-tree** Rust binary, in the
scratchpad (never inside this repo, never committed), that path-depends on this worktree's
`tack-db` crate as a library and calls the exact same public function ADR 0060 names,
`tack_db::migrations::run_all`, against `sqlite::memory:`.

Files (both outside this repo, at
`/tmp/claude-1000/-home-ox-Sites-objetivosMios/af5361b3-09d8-48cc-848e-51a01823b6db/scratchpad/orch-new-measure/`):

`Cargo.toml`:
```toml
[package]
name = "orch-new-measure"
version = "0.1.0"
edition = "2021"

[dependencies]
tack-db = { path = "/home/ox/Sites/objetivosMios/.claude/worktrees/agent-a23d8b45e40873386/crates/tack-db" }
sqlx = { version = "0.9", features = ["runtime-tokio", "sqlite"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`src/main.rs`:
```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");

    tack_db::migrations::run_all(&pool)
        .await
        .expect("run_all migrations");

    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("query sqlite_master");

    let names: Vec<String> = rows.iter().map(|r| r.get::<String, _>(0)).collect();
    println!("TOTAL_TABLES={}", names.len());
    for n in &names {
        println!("{}", n);
    }
    println!("HAS_orch_runs_new={}", names.iter().any(|n| n == "orch_runs_new"));
    println!("HAS_orch_approvals_new={}", names.iter().any(|n| n == "orch_approvals_new"));
}
```

**Command run:**

```
export CARGO_TARGET_DIR=/tmp/tack-agent-targets/viii-c1
cd /tmp/claude-1000/-home-ox-Sites-objetivosMios/af5361b3-09d8-48cc-848e-51a01823b6db/scratchpad/orch-new-measure
cargo run --release
```

**Raw output (tail, after the dependency build):**

```
TOTAL_TABLES=48
_migrations
agent_enrollment_tokens
agent_fleet_members
agent_fleets
agent_profiles
agent_runners
app_meta
attachments
boards
comments
control_planes
custom_field_definitions
custom_field_values
dependencies
execution_artifacts
execution_attempts
execution_cancellation_replays
execution_claim_replays
execution_completion_replays
execution_decisions
execution_event_batch_replays
execution_events
execution_heartbeat_replays
execution_recovery_audits
execution_requests
github_links
item_roles
items
items_fts
items_fts_config
items_fts_data
items_fts_docsize
items_fts_idx
model_profiles
orch_approvals
orch_events
orch_events_daily
orch_links
orch_metrics
orch_metrics_daily
orch_runs
orch_tasks
orch_trace_cursors
project_templates
projects
roles
sprints
workspaces
HAS_orch_runs_new=false
HAS_orch_approvals_new=false
```

`TOTAL_TABLES=48` matches ADR 0060's own stated result ("48 tables total"). Neither
`orch_runs_new` nor `orch_approvals_new` is present — `orch_runs` and `orch_approvals` are,
under their permanent names.

### Why, reading the 037/038 rebuild itself

Read `crates/tack-db/src/migrations.rs` lines ~1226–1327 (`MIGRATION_037_STATEMENTS`,
`MIGRATION_037`, `MIGRATION_038_STATEMENTS`, `MIGRATION_038`, and the doc comment above
them describing the rebuild). Both migrations follow the same six/seven-statement shape,
run inside one transaction by the migration runner:

1. `DROP TABLE IF EXISTS orch_runs_new` — defensive, in case an earlier *interrupted*
   attempt left a stale staging table (this is the case the boot-safety guard and
   III-A3's historical recovery note are about — see below).
2. `CREATE TABLE IF NOT EXISTS orch_runs_new (...)` — the new shape, with the widened
   primary key.
3. `INSERT INTO orch_runs_new (...) SELECT ... FROM orch_runs` — copy every row across,
   by explicit column list, never `SELECT *`.
4. `DROP TABLE orch_runs` — the old table is gone.
5. `ALTER TABLE orch_runs_new RENAME TO orch_runs` — the staging table becomes the
   permanent one, under the permanent name.
6. Recreate the index.

Steps 4–5 are the reason the name never survives a completed migration: by the time this
transaction commits, `orch_runs_new` has been *renamed away* — it does not coexist with
`orch_runs` as a second table, it becomes `orch_runs`. The name `orch_runs_new` is a
staging identifier scoped to the lifetime of one transaction, not a schema object that
persists afterward. Migration 038 does the identical thing for `orch_approvals_new` →
`orch_approvals`.

The **only** state in which `orch_runs_new` would exist in `sqlite_master` is a database
caught mid-transaction by a crash between steps 2 and 5 — and SQLite's transactional DDL
means even that does not happen on a clean commit/rollback; the module doc's "Boot-safety
guard" (referenced in `docs/plans/agnostic-control-plane.md` and `TODO.md`'s Part I
archive, task 42.3) exists to detect exactly that half-applied edge case and refuse to
boot rather than silently working around it. `docs/agent-handoffs/part-iii/III-A3.md`
records one real, historical instance of that recovery, from before this migration was
released — it is not evidence that the staging tables persist in a normal, fully-migrated
database; it is evidence the guard that catches the abnormal case works. I left that
handoff unedited: it is a dated, historical record of a real incident, and this repo's
convention is that handoffs get amendments, never rewrites.

### Documents corrected

- `.claude/scope-discipline.md` — removed the false bullet ("`orch_runs_new` and
  `orch_approvals_new` are still in the schema: rebuild leftovers from migration 037 that
  nothing cleaned up.") from *The evidence*, and updated the now-stale "two of the five
  items" cross-reference later in the file to "two of the four items" (the list went from
  five entries to four; the two entries that cross-reference — `model_profiles` and the
  Parts I/II control plane — are unaffected by the removal).
- `docs/book/src/roadmap.md`, "Two execution models coexist in the schema and the UI"
  (around line 3156) — removed the parenthetical "`orch_runs_new` and
  `orch_approvals_new`, leftovers of migration 037's rebuild" clause from the same
  sentence pattern, leaving the (separately-measured, unrelated to this card) "11
  `orch_*` tables" count as-is, matching the wording already used in
  `.claude/scope-discipline.md`'s *other*, unrelated bullet about the Parts I/II control
  plane, which never carried the `_new` claim in the first place.

### Documents found repeating the claim but **not** corrected, and why

- `TODO.md:3797-3798` (inside the Part I archive, quoted verbatim inside the V-B2 card's
  own historical context in the Part V archive) repeats the identical wrong wording:
  "11 `orch_*` tables (including `orch_runs_new` and `orch_approvals_new`, leftovers of
  migration 037's rebuild)". This is the same sentence pattern as the two documents I did
  correct. **Left unedited per the dispatch prompt's explicit instruction** ("do not edit
  `TODO.md` — the wave integrator owns it") and per §VIII.2's chokepoint table, which
  reserves `TODO.md` for the wave integrator. Flagging it here for the integrator to
  correct in the same pass.
- `docs/adr/0060-docket-control-plane-disposition.md` — this is the document that had the
  claim **right**; no amendment was needed. (Had it needed one, the rule is a dated
  amendment appended at the bottom, never a rewrite of the body — not applicable here.)
- `docs/plans/agnostic-control-plane.md:741,743` — uses `orch_runs_new` correctly, as the
  transient staging-table name inside the 12-step rebuild procedure and inside the
  half-applied-rebuild boot guard's description. Does not claim the name persists. Not a
  repetition of the false claim; left unedited.
- `docs/book/src/roadmap.md:1958` — describes the same boot-safety guard ("`run_all`
  refuses to boot if both `orch_runs` and `orch_runs_new` exist"), which is an accurate
  description of the half-applied-rebuild detection, not a claim that the tables coexist
  in a normal database. Left unedited.
- `docs/agent-handoffs/part-iii/III-A3.md:17` — a dated historical handoff describing a
  real, one-time recovery from stale `orch_runs_new`/`orch_approvals_new` state before
  this migration shipped (see above). Not a claim about current, fully-migrated schema
  state; and handoffs are corrected by amendment, never rewrite — no amendment was needed
  since the text does not make the false claim. Left unedited.
- `crates/tack-db/tests/migrations/orch_migrations.rs`,
  `crates/tack-db/src/migrations.rs` — `.rs` files; out of scope for this card
  regardless of content (must-not list).

## What I read in `../rack-cli`

Nothing. This card's measurement is entirely internal to this repository (Tack's own
migration runner and schema) — it never touches what docket sends or accepts over the
wire, so `../rack-cli` was not relevant and was not opened. `~/.docket` was likewise never
touched.

## The revert proof

Not applicable — this card changes no code and adds no test, so there is nothing to
revert-and-watch-fail. The verification instead is the measurement above (a workaround
program, run twice conceptually: once as the primary source of truth, cross-checked
against ADR 0060's own stated result of 48 tables) plus `.githooks/pre-push` passing with
zero code diff.

## The question I did not answer

None — §VIII.1 rule 1 ("this Part closes gaps; it does not design surfaces") never applied
here; the card's Acceptance was fully satisfiable by measurement and reading, and the
answer to the "which document is wrong" question was unambiguous once measured.

## What a stranger still cannot do

Nothing changed here that a stranger could act on differently — this card corrects two
prose claims about schema shape, not behavior, an endpoint, or a capability. A stranger
reading `.claude/scope-discipline.md` or `docs/book/src/roadmap.md` after this change no
longer learns something false about the schema; that is the entire effect.

## Context spent

- Tokens read before the first edit (cold start): read the Part VIII README header + the
  VIII-C1 block, `TODO.md` §VIII.0–§VIII.2 and §VIII.4's VIII-C1 card only (via `sed`
  ranges, never the whole file), the CLAUDE.md map, `.claude/scope-discipline.md` in full
  (123 lines, the file this card edits), ADR 0060's Measurement section, and the 037/038
  region of `crates/tack-db/src/migrations.rs` (~150 lines). No estimate was given for this
  card specifically to compare against.
- Context size at handoff: not measured precisely; the read set above plus the grep
  results and the standalone measurement program's output.
- Files opened and not used: none — every file read fed either the measurement, the
  correction, or the "found but not corrected" list.
- Read-list lines that were wrong: none noted — the dispatch prompt's read order matched
  what the card needed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

### 2026-09-08 — the "11 `orch_*` tables" count, in the same two files

The coordinator flagged that the same root miscount survives in both files this card
already corrected: "11 `orch_*` tables" in `.claude/scope-discipline.md` and
"11 `orch_*` tables, a `control_planes` table" (reading as 12) in
`docs/book/src/roadmap.md`. ADR 0060's own Measurement section names this explicitly as
*the other half* of the same error — a naive `grep -c 'orch_'` over
`crates/tack-db/src/migrations.rs` counts 11 only because it also matches the literal
strings `orch_runs_new` and `orch_approvals_new` inside the migration source, the same
two names this card already proved never exist as tables after a completed migration.

**Re-measured, not copied from ADR 0060's prose.** Extended the same out-of-tree harness
(`/tmp/claude-1000/-home-ox-Sites-objetivosMios/af5361b3-09d8-48cc-848e-51a01823b6db/scratchpad/orch-new-measure/src/main.rs`,
same `Cargo.toml`, same path-dependency on this worktree's `tack-db`) to also filter the
already-fetched `sqlite_master` table list for the `orch_` prefix and check for
`control_planes` by name.

**Command:**

```
export CARGO_TARGET_DIR=/tmp/tack-agent-targets/viii-c1
cd /tmp/claude-1000/-home-ox-Sites-objetivosMios/af5361b3-09d8-48cc-848e-51a01823b6db/scratchpad/orch-new-measure
cargo run --release
```

**Raw output (new lines only — `TOTAL_TABLES=48` and the full table list are identical to
the first measurement above and are not repeated):**

```
ORCH_PREFIXED_COUNT=9
orch_prefixed: orch_approvals
orch_prefixed: orch_events
orch_prefixed: orch_events_daily
orch_prefixed: orch_links
orch_prefixed: orch_metrics
orch_prefixed: orch_metrics_daily
orch_prefixed: orch_runs
orch_prefixed: orch_tasks
orch_prefixed: orch_trace_cursors
HAS_control_planes=true
DOCKET_SPECIFIC_TOTAL(orch_prefixed+control_planes)=10
```

**Result: 9 tables are literally `orch_`-prefixed; `control_planes` is a 10th
Docket-specific table that does not carry the prefix.** This agrees with ADR 0060's stated
enumeration (`control_planes`, `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`,
`orch_approvals`, `orch_metrics`, `orch_events_daily`, `orch_metrics_daily`,
`orch_trace_cursors` — 10 Docket-specific, 9 of them `orch_*`) exactly, table for table. My
measurement does not disagree with ADR 0060's number here — only with the two documents
that had re-derived "11" from the naive grep instead of from this count. No further
escalation needed.

**Documents corrected (in addition to the two changes recorded above):**

- `.claude/scope-discipline.md` — "11 `orch_*` tables" → "9 `orch_*`-prefixed tables plus
  `control_planes` (10 Docket-specific tables total)", in the same "Parts I and II built an
  entire control plane" bullet the first amendment did not touch.
- `docs/book/src/roadmap.md` — "11 `orch_*` tables, a `control_planes` table" (which read
  as 12 tables total) → "9 `orch_*`-prefixed tables plus a `control_planes` table (10
  Docket-specific tables total)", in the same "Two execution models coexist" paragraph the
  first amendment already touched.

Both now state the count as "10 Docket-specific tables total" so a future reader cannot
re-derive 11 or 12 from the sentence — the ambiguity the coordinator asked to close.

**Left alone, per explicit instruction:** `TODO.md:3399` (own evidence row already citing
the naive-grep command as its source) and `TODO.md:3797` (the V-B2 archive context this
card's first pass already declined to edit) both still say "11". The coordinator confirmed
they will handle `TODO.md` at integration; this amendment does not touch it.

**Gate:** `.githooks/pre-push` re-run after both edits, green, zero code diff (fmt, clippy,
check-comments, check-test-hygiene, generated-files freshness all passed).

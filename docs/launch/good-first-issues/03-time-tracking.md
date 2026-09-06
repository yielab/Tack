# good first issue: time tracking (logged time spent, not just estimates)

**Suggested labels:** `good first issue`, `enhancement`

## The gap, checked directly

An item already carries `estimate: Option<f64>` and `estimate_unit: EstimateUnit`
(`crates/tack-core/src/models.rs:105-106`) — story points, hours, days, or a custom
unit. It also carries `started_at`/`completed_at` timestamps. What it does not have is
anywhere to record *actual* time spent working on it: `grep -rn 'time_spent'
crates/ frontend/src` finds nothing anywhere in the schema, the API, or the UI. There is
no way to compare planned vs. actual effort, which is the entire point of an estimate.

## Why `started_at`/`completed_at` alone don't solve this

Wall-clock elapsed time between those two timestamps isn't the same thing as time
actually worked — an item can sit "In Progress" over a weekend, or across several
separate sessions. Real time tracking needs either a single accumulated `time_spent`
field updated by explicit start/stop actions, or (more useful, more work) a `time_entries`
table recording individual logged sessions per item. Either is a reasonable scope for a
first PR; a single project probably shouldn't do both.

## Where to start

This is close to `CONTRIBUTING.md`'s own worked example for adding a new entity — it
literally uses `TimeEntry` as the walkthrough's example name (see "How To Add a New
Feature" → "Adding a New Entity" in `CONTRIBUTING.md`). Following that walkthrough
directly gets you most of the way:

1. **Model** — add the field(s) to `crates/tack-core/src/models.rs` (either a
   `time_spent: Option<f64>` on `Item`, matched to the existing `estimate_unit` pattern,
   or a new `TimeEntry` struct if you're doing the fuller version).
2. **Migration** — one new migration in `crates/tack-db/src/migrations.rs`, added to
   `run_all()`. Read this repo's one hard rule here first: **one `ALTER` per migration
   file** — the migration runner executes statements individually with no wrapping
   transaction, so a multi-statement migration failing halfway leaves the schema
   half-upgraded with no automatic rollback.
3. **Repository** — a new module under `crates/tack-db/src/repo/` if you're adding
   `time_entries` as its own table (see `crates/tack-db/src/repo/comments.rs` for a
   same-shape existing example: a child record scoped to one item).
4. **Handler + routes** — `crates/tack-api/src/handlers/`, wired into
   `crates/tack-api/src/router.rs`, following any existing item-scoped sub-resource
   handler as a template.
5. **Frontend** — a small addition to the item detail view (see
   `frontend/src/features/item-detail/` for the existing tabs structure) to log/display
   time; regenerate `frontend/src/shared/api/schema.gen.ts` after the API change
   (`cd frontend && npm run gen:api`) rather than hand-editing it.

## What "done" looks like for a first PR

The simpler `time_spent` accumulator is a completely reasonable, mergeable scope for a
first contribution here — it doesn't need sprint burn-down charts or reporting built on
top in the same PR. Add tests at the layer you touch: a core model test if you added
validation, a repository test under `crates/tack-db/tests/`, and a handler test under
`crates/tack-api/tests/handlers/` proving the new field round-trips through the API.

## Before you start

Read `CONTRIBUTING.md` in full — its "How To Add a New Feature" section is written for
exactly this kind of change. This issue does not require reading this repository's
internal planning board (`TODO.md`) to get started.

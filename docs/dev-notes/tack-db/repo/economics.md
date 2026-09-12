# `crates/tack-db/src/repo/economics.rs`

Moved out of the module preamble; trim or delete freely.

Unit economics — read-only aggregate queries over
 `items` + `orch_tasks` (+ `orch_events` for the rework signal). Deliberately its own
 module rather than an extension of `repo/orch.rs`: every query here is
 additive/read-only against tables `repo/orch.rs` already owns, so a separate file
 avoids colliding with unrelated edits to that file.

 Two things worth knowing before extending this module:

 1. **The rework-signal event types (`rework_started`, `verification_failed`,
    `tester_verdict_failed`) only ever arrive via `tack-orch::reconciler`'s trace
    ingestion, which always sets `orch_events.run_id: None`** — docket's trace
    payload carries no `run_id`, only `session_id`, and the ingestion code leaves
    `run_id` unset rather than guessing at a lookup it doesn't have. `orch_events.
    run_id` is not `NULL` for every row in the table — `tack-api::orch_store`'s
    `status_map_skipped_human_override` recording does set it — but for these three
    event types specifically, a per-*attempt* correlation via `orch_events.run_id =
    orch_tasks.remote_run_id` would silently match nothing in practice; it isn't a
    fit for "how often did agent work need rework" here. What *is* populated
    reliably is `orch_events.item_id` (via `reconciler::session_id_task_id` →
    `find_orch_task_by_remote_task_id`), so
    [`Repository::list_item_ids_with_rework_signal`] correlates at the item level
    instead. This is a real, disclosed gap for whoever next needs per-attempt (not
    per-item) rework-signal correlation.
 2. **Only `orch_events`/`orch_metrics` are subject to the retention
    sweep — `orch_tasks` is never purged.** So `tokens_in`/`tokens_out`/
    `cost_usd_estimated`/lead-time figures below are never truncated by
    `TACK_ORCH_EVENT_RETENTION_DAYS`; only the rework-signal correlation (which
    depends on `orch_events`) can silently miss history once a task's raw events
    have aged out. Callers must compare `last_dispatched_at` (below) against their
    own retention cutoff to know whether an item's absence from
    [`Repository::list_item_ids_with_rework_signal`]'s result means "no rework" or
    "unknown — the evidence may already be gone."

# `crates/tack-api/src/handlers/economics.rs`

Moved out of the module preamble; trim or delete freely.

Unit economics: tokens, estimated cost, agent-vs-human lead time, and
 rework rate, sliced by `project_type` and `item_type`. Read-only
 aggregate endpoints over `Repository::
 list_completed_item_economics`/`list_item_ids_with_rework_signal`
 (`tack_db::repo::economics`) — no live docket call, so a plane outage can
 never turn into a 500 here, the same discipline `handlers::orch`'s
 module doc names for `GET /api/fleet`.

 **Gated behind `TACK_ORCH_ENABLE`.** [`economics_routes`] is merged into
 `router.rs`'s `orch_routes()`, which applies `orch::require_orch_enabled` once as a
 layer over the whole sub-router — every route below inherits it without a
 per-handler check, and 404s (not 200-with-empty-data) when the flag is unset.

 **Money is honestly represented on every response this module returns.**
 Token counts (`tokens_in`/`tokens_out`) are always present and rendered
 first; every dollar figure is named `cost_usd_estimated` (never "cost"
 or "spend") and travels with a `pricing_snapshot_at` that is honestly
 `None` today — no pricing-snapshot mechanism exists anywhere in this
 codebase yet. The frontend must render it through
 `shared/agentActivity/format.ts#formatEstimatedCost` — reused verbatim,
 never reimplemented.

 **Three honesty decisions this module makes, spelled out rather than left
 implicit:**

 1. **Minimum sample size — [`MIN_SAMPLE_SIZE`].** Below it, [`LeadTimeStat`] and
    [`ReworkStat`] report `below_min_sample: true` and raw counts/durations, never
    a derived average or rate a reader could mistake for a stable signal.
 2. **Selection bias — [`LEAD_TIME_SELECTION_BIAS_NOTE`].** Carried on every
    [`EconomicsSlice`] that has a lead-time comparison, not linked from a doc.
    Items reach an agent via auto-dispatch (which only fires on specific statuses)
    or because a person chose to hand them off — neither is a random sample of all
    work, so a shorter average agent lead time is at least as consistent with
    "people dispatch the easy stuff" as with "agents are faster." This module
    deliberately never computes a single "agents are Nx faster" ratio: both
    populations' stats are reported side by side and left for the reader to
    compare — the same discipline this module applies to cost ratios, never
    showing a bare percentage/ratio without the caveat attached.
 3. **Retention truncation — [`REWORK_RATE_DEFINITION`] / [`REWORK_TRUNCATION_NOTE`].**
    `orch_tasks` (tokens, cost, dispatch timestamps) is never purged — only
    `orch_events`/`orch_metrics` are, by the retention sweep. So token,
    cost, and lead-time figures below are NOT subject to truncation, and this
    module says so rather than blanket-hedging every number on the page. Only the
    rework signal (which lives in `orch_events`) can go stale: an item whose only
    dispatch attempt predates `TACK_ORCH_EVENT_RETENTION_DAYS` is excluded from the
    rework-rate denominator entirely (`ReworkStat::attempts_excluded_stale`), never
    counted as "no rework happened" — see `tack_db::repo::economics`'s module doc
    for the underlying schema gap (`orch_events.run_id` is always `NULL` today, so
    correlation is item-level, not attempt-level).

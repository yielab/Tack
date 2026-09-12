# `crates/tack-orch/src/reconciler.rs`

Moved out of the module preamble; trim or delete freely.

The orchestration reconciler: one background `tokio` task per registered
 control plane, polling it on an interval and driving the
 `healthy` → `degraded` → `unreachable` state machine.

 # The three-phase shape, and why it is not just discipline

 **A SQLite write transaction is never held across an HTTP call to a control
 plane.** This module enforces that by construction rather than by
 convention — each poll tick has three strictly separated phases, run in
 this order, with no phase able to see into the next:

 1. **Fetch** ([`reconcile_once`]) — every HTTP call this poll needs. No
    database access happens anywhere in this phase; it is not even possible,
    because [`reconcile_once`] and everything it calls never receive a
    store or pool handle. It returns a plain, ownable [`PollEvaluation`].
 2. **Decide** ([`HealthTracker::observe`]) — a pure, synchronous state
    transition over the fetch result. No I/O of any kind.
 3. **Persist** ([`spawn_one`]'s single `store.record_health(...).await`
    call) — one short write, invoked exactly once per tick, strictly after
    phase 1 has fully completed. Nothing in this phase awaits an HTTP call.

 Because phase 1 has already finished (its `.await` has resolved) before
 phase 3 begins, there is no window in which a write is open while an HTTP
 request is in flight — not "we were careful", but "the types do not let you
 interleave them; phase 3 has no `ControlPlane` handle to await on."

 # Adding a new `poll_*` step

 [`reconcile_once`] builds a [`FetchOutcome`] as an explicit, flat list of
 steps — one field per `poll_*` call. A new step is exactly three additions:
 one field on [`FetchOutcome`], one module-private `poll_*` function shaped
 like [`poll_health`]/[`poll_status`], and one line in [`reconcile_once`]'s
 struct literal.

 Two rules constrain where the rest of the work goes:

 - **Persistence does not belong in [`reconcile_once`].** Add it to
   [`spawn_one`]'s loop as its own short call after
   `store.record_health(...).await`, preserving the fetch-then-persist
   separation above. This is why [`reconcile_once`] returns
   `(PollEvaluation, FetchOutcome)` rather than the verdict alone: the
   tuple's second element is the raw fetch, carried out so a later phase can
   read fields `evaluate` never touches.
 - **A data-ingestion failure must not affect the health verdict.** Only
   `/health` and `/status.json` decide reachability. A failed runs,
   approvals, traces or metrics poll is handled by its own persist step and
   is invisible to [`evaluate`].

 `poll_runs` is the one step needing input from outside the control plane:
 docket's `/runs` is filtered by `?project=`, one call per *linked* project,
 and that list comes from `orch_links` in the database. [`spawn_one`] reads
 it (`store.list_linked_projects`) **before** the panic-isolated
 [`reconcile_once`] call — a single short read, never held across an HTTP
 `.await`. The consequence is that a tick's project list can be one tick
 stale relative to a concurrent `orch_links` edit. That staleness is
 accepted and bounded by `TACK_ORCH_POLL_SECS`.

 # Trace cursor

 **The cursor is opaque — this module does not parse it.**
 `ControlPlane::traces` returns [`crate::TracesPage`], whose `next` field is
 the control plane's own minted resume cursor, forwarded verbatim by
 `adapters::docket`. This module stores it and passes it back as `since` on
 the next poll; nothing here decodes it, computes it, or knows its format.

 Do not reintroduce a client-side reconstruction of that cursor. One existed
 and was correct, and it was still wrong: it had to stay byte-for-byte in
 sync with the server's algorithm forever with no compiler check, so a
 server-side change would have drifted it into silently wrong resumption —
 no compile error, no failing test.

 **`orch_events.id` has no natural key to upsert on.** A trace event is a
 position in a JSONL stream, not an entity with a stable id: the payload
 carries no monotonic sequence number or byte offset. [`derive_event_id`] is
 therefore a UUIDv5 (namespace + name, no randomness) over every field of
 the event plus `control_plane_id`/`remote_project`, so the same source
 event always produces the same id and re-ingesting an overlapping cursor
 window is a no-op row-count-wise (`upsert_orch_events`'s `ON CONFLICT(id)`)
 rather than a duplicate.

 **Retention composition is the sharpest edge here.** The retention sweep
 rolls `orch_events` rows older than the cutoff into `orch_events_daily` and
 deletes them. A lost or rewound cursor can re-deliver an event whose row was
 already rolled up and purged; because its id is content-derived,
 re-ingesting it would `INSERT` a fresh row that the *next* sweep cannot
 distinguish from a brand-new event, rolling its count in a second time and
 silently double-counting real cost and token totals. [`persist_events`]
 guards this at ingest: an event whose `occurred_at` already predates
 `now - retention_days` — the same formula [`spawn_retention_sweep`] uses —
 is dropped rather than inserted. The cost is not counting a handful of
 events at the extreme edge of a pathological rewind, which is strictly
 better than corrupting a total.

 # Retention sweep — a separate background task

 Unlike the per-plane `poll_*`/`persist_*` steps above,
 [`spawn_retention_sweep`] is not part of any plane's [`spawn_one`] loop — it
 operates fleet-wide across `orch_events`/`orch_metrics`, independent of
 which planes (if any) are registered. It uses its own narrow trait,
 [`RetentionStore`], rather than growing [`ControlPlaneStore`] with unrelated
 fleet-wide concerns. The rollup-then-purge SQL and its crash-safety argument
 live in `tack_db::Repository::rollup_and_purge_orch_events` /
 `rollup_and_purge_orch_metrics`; this module only schedules it.

 **Nothing calls [`spawn_retention_sweep`] at boot.** It is built and tested,
 but no caller spawns it, so orchestration event/metric retention does not
 actually run. Wiring it into `server.rs` mirrors the reconciler spawn block
 already there.

 # Jitter

 Interval jitter (±20%) is derived deterministically from the plane's
 [`uuid::Uuid`] plus a per-plane tick counter, hashed with
 [`std::collections::hash_map::DefaultHasher`] — **not** the `rand` crate,
 which is not a workspace dependency. This keeps the schedule reproducible in
 tests while still spreading N planes' poll times so they do not stampede the
 gateway in lockstep.

 # Panic isolation

 A panic inside a single poll — a bug in a `poll_*` fn, a bad adapter,
 anything — must not take down that plane's loop, let alone another plane's,
 and must never be visible to a user request (this module makes no *inbound*
 HTTP calls to Tack at all; it only calls *out* to a control plane).
 [`spawn_one`] gets this from `tokio::spawn`'s unwind boundary: each poll tick
 runs inside its own spawned task, and a panic there surfaces as a `JoinError`
 to the non-panicking outer loop, which logs it and treats the tick as a
 failed poll. The loop itself never panics, so it keeps ticking.

 # Persistence interface

 [`ControlPlaneStore`] is a narrow trait rather than `tack_db::Repository`
 used directly, even though `tack-orch` already depends on `tack-db`. Turning
 a `tack_db::repo::orch::ControlPlane` row into a live `Arc<dyn ControlPlane>`
 requires dispatching on `kind` to a concrete adapter, which is a composition
 concern this module must not own. The trait's signatures deliberately mirror
 the repository's (same field meaning, `i64` failure counts, borrowed
 `Option<&str>` for `api_version`) so the glue stays a thin wrapper.

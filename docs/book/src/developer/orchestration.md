# Orchestration Architecture

This chapter covers the Agent-Factory Control Center: the `tack-orch` crate, the
`ControlPlane` trait, the reconciler's poll loop, the dispatcher, and the schema
behind all of it. It assumes you've read the
[Architecture Overview](README.md) and the [Crate Tour](crate-tour.md), which cover
this crate alongside the other five in the main workspace.

For the *why* behind this feature, see
[Roadmap → Agent-Factory Control Center](../roadmap.md#next--agent-factory-control-center-phases-3338-august-2026).
This page documents what's actually implemented — not an aspirational end state.
The reciprocal docket-side work (the `POST /tasks`, `GET /traces`, `POST /pods`
endpoints this crate depends on) lives in docket, a separate public project.
Verify docket's HTTP surface against its own source directly rather than its
`ROADMAP.md`, which has repeatedly lagged what's actually shipped (see
[docket's `ROADMAP.md` lags its own source](#dockets-roadmapmd-lags-its-own-source)).

## The one-line architecture

Tack holds desired state, an external agent-fleet backend executes it, and a
reconciler in the `tack-orch` crate closes the loop. **Both halves of the loop are
real**: intent flows **push** (Tack → docket, synchronous, returns a task/run id),
and progress flows **pull** (a jittered poll loop, Kubernetes-style):

```text
┌───────────────────────────── Tack (control center) ──────────────────────────────┐
│  Fleet · Approvals inbox · Agent Activity · Economics · Provisioning wizard      │
│  tack-api    POST /api/items/{id}/dispatch            (dispatcher.rs)            │
│              POST /api/sprints/{id}/dispatch          (sprint_dispatch.rs)       │
│              GET  /api/sprints/{id}/dispatch/dry-run                             │
│              POST /api/approvals/{token}              (separate token gate)      │
│              POST /api/templates/{id}/provision        (provisioning.rs)         │
│              GET  /api/economics/{summary,items}       (economics.rs)            │
│                                                                                    │
│  tack-orch::dispatcher / sprint_dispatch (in tack-api)                           │
│      item/sprint → enqueue_task → orch_tasks → status_map (workflow engine)      │
│                                                                                    │
│  tack-orch::reconciler   one tokio task per registered plane                     │
│      poll: health, status, runs, approvals, metrics, traces                      │
│        → orch_* tables → terminal status_map (human-wins) → BoardEvent           │
│                                                                                    │
│  tack-orch::ControlPlane (trait)                                                 │
│      └── adapters::docket::DocketAdapter   (the only implementor today)          │
└─────────────────────────────────────┬──────────────────────────────────────────────┘
                                      │ HTTP, Bearer on authenticated routes only
┌─────────────────────────────────────▼──────────────────────────────────────────────┐
│  docket serve            /health  /status.json  /metrics          (unauthenticated)│
│                           /runs  /approvals  /tasks  /traces        (Bearer)       │
│                           POST /tasks/{project}   POST /dispatch/{project}         │
│                           POST /approvals/{token}   POST /pods      (Bearer)       │
└──────────────────────────────────────────────────────────────────────────────────┘
```

Everything above the line is real, shipped, and tested. The one thing that never
exists on the wire: `ControlPlane::dispatch` (`POST /dispatch/{project}`, pipeline
`variables`) is implemented but never called by anything in Tack today — dispatch
only ever uses `enqueue_task`/`POST /tasks/{project}`. See
[The dispatcher](#the-dispatcher-tack-apisrcdispatcherrs) for why.

## What docket exposes today

Verified against docket's own source directly, not against its `ROADMAP.md` (see
the note below).

| Route | Auth | Notes |
|---|---|---|
| `GET /status.json`, `GET /metrics`, `GET /health` | none | No pause/resume surface exists anywhere in this table, in either direction — see [What's genuinely missing](#whats-genuinely-missing-not-a-tack-gap). |
| `GET /runs?project=`, `GET /runs/{id}` | Bearer | |
| `GET /approvals` | Bearer | Records carry `context: {taskId, pipelineIndex}` — the correlation key back to an item. |
| `GET /tasks/{project}` | Bearer | The pod's task queue as JSON. |
| `GET /traces/{project}?since=` | Bearer | Cursor-paged; **event payloads are snake_case**, the one docket endpoint that differs from the otherwise-camelCase convention. |
| `POST /tasks/{project}` | Bearer | Body `{description, priority, trusted}` → `{ok, task, project, status, approvalToken?}`. Honours the `pre_input` policy gate: `block` → 4xx naming the policy id; `require_approval` → task returned as `waiting_approval` with a token, never a 200 pretending it's queued. |
| `POST /dispatch/{project}` | Bearer | Runs an *existing* queue through the pipeline; body = pipeline `variables`; returns `{ok, run, project}`. Not used by Tack today (see above). |
| `POST /approvals/{token}` | Bearer | `{action: "grant"\|"deny"}` → docket's resulting `state`. `channel="tack"` is sent on every decision Tack makes (already a first-class member of docket's `APPROVAL_CHANNELS`). |
| `POST /pods` | Bearer | `{project, path, blueprint, pod, budget, verifyCmd}` (all but `project` optional) → `201 {ok, project, blueprint, members: [{id, role, model}]}`. Atomic on docket's side — every failure mode either raises before anything is touched (`409` for an existing pod) or tears down everything it started before raising (`500`). **No HTTP route to delete/un-provision a pod exists.** |

### docket's `ROADMAP.md` lags its own source

docket's own `ROADMAP.md` has marked endpoints this crate depends on (`POST
/tasks`, `GET /traces`, `POST /pods`) as `TODO` after they had already shipped in
source — not a timing artifact; in at least one case `ROADMAP.md`'s own last commit
postdated the shipping commit. **docket's source is the authority on what it
exposes over HTTP, never its `ROADMAP.md`.** Re-verify against source before
trusting any staleness claim, including the ones on this page.

## The `ControlPlane` trait

`crates/tack-orch/src/lib.rs` defines:

```rust
#[async_trait::async_trait]
pub trait ControlPlane: Send + Sync {
    fn kind(&self) -> &'static str; // "docket"
    fn capabilities(&self) -> Capabilities; // static, no I/O — what this adapter can do, independent of whether it's reachable right now
    async fn health(&self) -> Result<Health, OrchError>;
    async fn status(&self) -> Result<FleetStatus, OrchError>;
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError>;
    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError>;
    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError>;
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError>;
    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError>;
    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError>;
    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError>;
    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError>;
    async fn decide_approval(&self, token: &str, grant: bool) -> Result<ApprovalState, OrchError>;
    async fn provision_pod(&self, params: ProvisionPodParams) -> Result<ProvisionedPod, OrchError>;
}
```

**Not frozen.** Treat this trait's current shape as current, not eternal — change
it again if a real design need shows up, and update every implementor and caller
in the same change.

**Two design points worth understanding together:**

1. **`traces` returns docket's own opaque cursor, not a client-reconstructed one.**
   `TracesPage { events, next: Option<String> }` — `next` is opaque, never parsed
   or reconstructed by Tack, just persisted and passed back verbatim. Reimplementing
   docket's `"<ts>Z:<n>"` cursor algorithm client-side would be correct today and
   guaranteed to silently drift the moment docket changes that algorithm, with no
   compile error to catch it. Live-verified: a `since` value fed straight back from
   a real `docket serve` produces zero new events and an unchanged `next` — proof
   the forwarded cursor is one docket itself re-mints identically.
2. **`OrchError::PolicyBlocked { policy_id, message }` distinguishes a deliberate
   policy refusal from a generic transport failure.** The typed variant carries a
   real `policy_id`, parsed once (`adapters::docket::parse_policy_block`) with a
   `"unknown"` fallback if docket's wording ever drifts — never panicking, the same
   "degrade, don't fail the poll" discipline the remote-state enums use. Matching on
   a string prefix of the error message (the alternative) would silently turn a
   reworded docket error into a generic failure instead.

### `OrchError`

```rust
pub enum OrchError {
    Http(String),                              // transport-level failure
    Auth,                                       // 401/403
    Decode(String),                             // malformed response body
    NotFound(String),                           // resource doesn't exist on the remote side
    Unavailable(String),                        // plane configured but not currently reachable
    Disabled,                                   // gated behind a flag/config, or a write method the adapter doesn't implement
    PolicyBlocked { policy_id: String, message: String },  // pre_input refused it, on purpose
    AlreadyDecided(String),                     // the approval was already granted/denied
    AlreadyExists(String),                      // docket's PodAlreadyExistsError, 409
}
```

### Remote enums never fail a poll on an unrecognised value

`RunState`, `RunSource`, `TaskStatus`, and `ApprovalState` are generated by a
`remote_string_enum!` macro (`lib.rs`) rather than a plain `#[derive(Deserialize)]`.
Each gets a hand-written `Serialize`/`Deserialize` (via `String`, not
`#[serde(other)]` — that attribute only supports a payload-less fallback, and the
original string needs to survive) plus an `Unknown(String)` variant that round-trips
byte-for-byte. A docket upgrade that adds a new run state or task status degrades to
"shown as-is" on the next poll, never to a `Decode` error that kills that plane's poll
loop. If you add a new remote-state enum, use the macro rather than hand-rolling
another one.

## Money: `*_usd_estimated`, always

Every dollar-valued field in `tack-orch`, the repository layer, and the API DTOs is
named with an `_usd_estimated` suffix (`cost_usd_estimated` on `orch_tasks`,
`FleetEntry`, `FleetAgent`, `RemoteEvent`, …), even where the upstream wire field is
the bare `costUsd`/`cost_usd` — the `#[serde(rename = "...")]` maps the name, not the
meaning. `orch_links.budget_usd` is the one deliberate exception: it's a cap a human
typed in, not a number derived from token counts, so it's never suffixed. See the
[user guide's explanation](../user-guide/orchestration.md#why-every-dollar-figure-says-estimated)
for the user-facing framing of this same rule.

## The reconciler

`crates/tack-orch/src/reconciler.rs` runs one `tokio` task per registered control
plane. Each tick is three strictly separated phases, enforced by the types rather than
by convention:

1. **Fetch** (`reconcile_once`) — every HTTP call the tick needs, across **six**
   steps today (health, status, runs, approvals, metrics, traces — see the
   extension recipe below for the pattern each new step follows). No database
   handle is reachable from this phase at all.
2. **Decide** (`HealthTracker::observe`) — a pure, synchronous state transition over
   the health/status fetch results only. No I/O. Ingestion data (runs, approvals,
   metrics, traces) rides alongside in the same `(PollEvaluation, FetchOutcome)`
   tuple but is deliberately **not** read by `evaluate` — a new poll step's failure
   must never influence the plane's reachability verdict.
3. **Persist** (`spawn_one`'s loop) — `store.record_health(...)` first, then each
   ingestion step's own upsert call (`upsert_runs`, `upsert_approvals`,
   `upsert_metrics`, `persist_events`), each its own short write, strictly after
   phase 1's `.await` has already resolved.

Because phase 1 has fully completed before phase 3 begins, there is no window where a
write transaction is open while an HTTP request to docket is in flight — this is the
concrete mechanism behind the first non-negotiable below.

### Health state machine

```rust
pub const DEGRADED_AFTER_FAILURES: i64 = 3;
pub const UNREACHABLE_AFTER_FAILURES: i64 = 10;
pub const MAX_BACKOFF_SECS: u64 = 300;
```

- **healthy → degraded** after 3 consecutive `/health`+`/status.json` failures.
- **degraded → unreachable** after 10 consecutive failures.
- **Recovery is immediate.** A single successful poll resets `consecutive_failures` to
  zero and re-evaluates state from scratch, regardless of how long the prior outage
  ran.
- An `apiVersion` mismatch is an **independent** signal from reachability —
  `HealthTracker::observe` takes the more severe of the two (`HealthState` derives
  `Ord` with `Healthy < Degraded < Unreachable`).
- Poll backoff doubles with each consecutive failure, capped at `MAX_BACKOFF_SECS`.
- Log severity only fires on a *transition* (`warn` entering degraded/unreachable,
  `info` recovering, `debug` on an unchanged repeat failure) — a sustained outage
  produces at most two `warn` lines no matter how long it lasts.

### Jitter is deterministic, not `rand`

`jittered_secs(plane_id, tick, base_secs)` hashes `(plane_id, tick)` with
`std::collections::hash_map::DefaultHasher` and maps the result to a ±20% fraction —
deliberate, not a placeholder: `rand` is not a workspace dependency, and this keeps
the schedule reproducible in tests. It still gives the real anti-stampede property —
N registered planes don't all wake up on the same tick.

### Panic isolation

`spawn_one` wraps each tick's fetch phase in its own `tokio::spawn`: a panic there
becomes a `JoinError` the outer loop catches, logs, and treats as a failed poll. No
other plane's task is affected, and nothing in this module ever makes an *inbound*
HTTP call, so a panic here can never touch a live user request.

### Extending the poll loop: adding a `poll_*` step

Exactly three edits:

```rust
// 1. Add a field to FetchOutcome:
struct FetchOutcome {
    health: Result<Health, OrchError>,
    status: Result<FleetStatus, OrchError>,
    runs: Vec<(String, Result<Vec<RemoteRun>, OrchError>)>,
    approvals: Result<Vec<RemoteApproval>, OrchError>,
    metrics: Result<Vec<MetricSample>, OrchError>,
    traces: Vec<TracesPollResult>,     // <- your new field
}

// 2. A fetch-only poll_* fn, module-private, no DB access:
async fn poll_traces(control_plane: &Arc<dyn ControlPlane>, /* … */) -> /* … */ { /* … */ }

// 3. One line inside reconcile_once's struct literal.
```

Your own persistence call goes in `spawn_one`'s loop, as its own short call placed
*after* `store.record_health(...).await` — never inside `reconcile_once`. **Do not
let a new poll step's failure influence `evaluate`'s reachability verdict.**

### `orch_events` has no natural key

Unlike `orch_runs` (keyed by `run_id`) and `orch_approvals` (keyed by `token`), a
docket trace event is a position in a JSONL stream, not an entity with a stable id.
`orch_events.id` is caller-assigned specifically so it can be derived as a **pure
function of the source event** — keyed on `(control_plane_id, remote_project, seq)`
when docket's record carries a sequence number, hashed from a canonicalised record
otherwise. Never keyed on ingestion time, poll tick, or insert order — all three make
replay non-idempotent. A rewound or reset cursor re-ingests an overlapping window
with **zero** duplicate rows, tested directly.

### Terminal `status_map` reconciliation

The reconciler is also where the terminal half of `status_map` (`on_succeeded`,
`on_failed`, `on_cancelled`) is applied — see
[`RepoControlPlaneStore::upsert_runs`](#the-persistence-interface) below, which calls
into it right after resolving a run's correlated item. `on_running`/
`on_waiting_approval` are **not** reconciler-driven — the reconciler never polls
docket's `/tasks` endpoint at all, only `/runs` and `/approvals`, so those two keys
are applied once, synchronously, at dispatch time (see the dispatcher below) and
never revisited by a poll.

### The Prometheus text parser

`crates/tack-orch/src/adapters/prometheus.rs` — a small, dependency-free parser for
docket's `/metrics` endpoint: `pub fn parse(input: &str) -> Vec<MetricSample>`. Never
errors, never panics; a malformed line is dropped, not a whole-document failure.
Reused verbatim by `poll_metrics`'s ingestion — do not write a second parser.

## The dispatcher (`tack-api/src/dispatcher.rs`)

The write path that turns Tack into a control center rather than a dashboard. Given
an item that has just entered (or is being manually pushed into) a dispatch-eligible
status, `dispatch_item(state, item_id: Uuid, trusted: bool) ->
Result<DispatchOutcome, ApiError>`:

1. Resolves the item's project → `orch_links` → `status_map`. An unlinked project,
   or an empty `dispatch_from`, are ordinary states (`DispatchOutcome::NoDispatchPolicy`),
   not errors.
2. Refuses — without calling docket — if the item's status isn't in `dispatch_from`
   (`DispatchOutcome::NotEligible`).
3. Idempotency: if the item's most recent `orch_tasks` attempt is still
   `pending`/`running`/`waiting_approval`, does **not** call docket again
   (`DispatchOutcome::AlreadyInFlight`).
4. Calls `ControlPlane::enqueue_task` — `POST /tasks/{project}`'s three outcomes,
   live-verified against a real docket instance:
   - **block** → `DispatchOutcome::Blocked { policy_id, message }`, no `orch_tasks`
     row at all.
   - **allow** / **require_approval** are indistinguishable at `enqueue_task`'s
     return-type level (both `Ok(task_id)`, since the trait's `Result<String,
     OrchError>` return can't carry docket's real `status`/`approvalToken` fields
     without widening it again) — `dispatch_item` makes one follow-up `list_tasks`
     call to resolve which actually happened. Widening `enqueue_task`'s return type
     to avoid that extra round trip is a real, deliberately-deferred future change,
     not an oversight.
5. Persists `orch_tasks` (task id + attempt + `trusted`), then applies the
   `status_map`-named target (`on_waiting_approval` or `on_running`) **through the
   workflow engine, never raw SQL** — a transition the engine refuses is recorded as
   a `status_map_rejected` `orch_events` row and the item is left untouched.

### `trusted` has no default, anywhere in this call chain

`dispatch_item`'s `trusted: bool` parameter is **required and non-`Option`** —
deliberately, because docket's own `core/dispatch.py::enqueue_task` treats an
*omitted* `trusted` as "trusted iff the caller is `operator`," which is always true
for every existing caller (confirmed against a real docket instance). A required
Rust `bool` can't stop a caller from passing the wrong *value*, but it makes the *omission* — the
actual vulnerability — a compile error. `handlers::orch::dispatch_item` (the manual
"Dispatch" button's HTTP entry point) resolves a default from the item's persisted
`source.is_trusted()` (see [`ItemSource`](#the-trust-boundary-itemsource) below);
the auto-dispatch hook and sprint dispatch both do the same. There is no convenience
wrapper anywhere that defaults `trusted` — adding one would silently recreate the
hole this signature exists to close.

### Idempotency and `attempt`

`attempt = 1 + the highest existing orch_tasks.attempt for this item` (0 if none).
Two layers make "double-dispatch creates one task, not two" hold under real
concurrency:

1. **`DispatchLocks`** — a process-wide `static LazyLock<Mutex<HashSet<Uuid>>>` in
   `dispatcher.rs`, deliberately **not** a field on `AppState` (dozens of
   pre-existing `AppState { .. }` struct literals across test files would have
   needed updating). A second concurrent request for the same item gets an
   immediate `409` rather than racing the first.
2. The `orch_tasks` read itself catches the sequential case.

This holds because Tack is a single-process, single-SQLite-writer binary; it would
not hold across multiple replicas.

### The trust boundary: `ItemSource`

Migration 029 added `items.source TEXT NOT NULL DEFAULT 'unknown'`, backed by
`tack_core::models::ItemSource` (`Manual` / `Github` / `Linear` / `JsonImport` /
`CsvImport` / `Unknown`) with `is_trusted()` — `true` iff `Manual` — as the **single**
place the trust rule is encoded. `source` is written exactly once, in
`Repository::create_item_with_source`, and `update_item` has no code path that
touches the column at all (not "chose not to," the column name doesn't appear in
its SQL — enforced by a test that edits an untrusted item's title/description
through `update_item` and asserts `source` is unchanged).

Two independently-chosen defaults, deliberately not the same value:

- **The migration's SQL default (`'unknown'`) is backfill-only** — every
  pre-migration row (including items GitHub-imported before this feature
  existed) resolves to untrusted, since there's no record of which ones were
  manually typed.
- **`Item`'s `#[serde(default)]` also resolves to `Unknown`** — an old export or a
  hand-built import payload that omits `source` can't claim `Manual` trust just by
  leaving the field out.

| Path | `ItemSource` |
|---|---|
| `POST /api/projects/{id}/items` (UI, `tack add`, MCP tool) | `Manual` |
| GitHub import | `Github` |
| Linear import | `Linear` |
| JSON/YAML project import | **Preserves** the source item's own `source` from the payload (a full snapshot restore already implies at least `create_item`-level privilege; the `#[serde(default)]` above still stops a payload that omits the field from claiming `Manual`) |
| CSV import | `CsvImport` |

## Sprint dispatch (`tack-api/src/sprint_dispatch.rs`)

`POST /api/sprints/{id}/dispatch` / `GET /api/sprints/{id}/dispatch/dry-run` —
DAG-ordered, no precedent in either codebase. Both routes share one planning
function, `plan_sprint_dispatch`, so a dry-run preview and a real run are
**guaranteed** to agree, not just usually agree.

- **Readiness** = every direct dependency is in a Done-category status *right now*
  (checked against the live item table, not against a `RunState`) — including
  dependencies outside the sprint, or outside the project.
- **Ordering** comes from `tack-core::dependency::DependencyGraph::topological_order`
  — Kahn's algorithm, made **deterministic** (not merely "a valid order") via a
  `BinaryHeap` tie-break on each node's position in the input slice, since this
  codebase's default hasher is randomized per instance. An impossible cycle (should
  be unreachable — `validate_new_edge` prevents it at creation) returns
  `CoreError::DependencyCycle` rather than looping or truncating.
- **Concurrency**: a `tokio::sync::Semaphore` bounds in-flight `dispatch_item` calls
  at `max_in_flight` (query param, clamped `[1, 20]`, default 5), submitted in
  topological order.
- **Partial failure**: a policy block or error on one item never aborts the rest.
  Downstream-blocked items report `waiting_on_dependencies` on their own next
  evaluation — no separate bookkeeping needed, since a blocked item never reaches a
  Done-category status.

**A known WIP-limit race, fixed below.** Concurrent dispatch of *different* items
into the *same* WIP-limited column was, before the fix, a genuine race —
`apply_mapped_status` read a column's item count and wrote the new status as two
separate, unlocked steps. `max_in_flight` made this an everyday occurrence rather
than a rare two-human collision.

## Fixing the WIP-limit race: `update_item_status_checked`

`crates/tack-db/src/repo/items.rs`'s `Repository::update_item_status_checked` wraps
the count-check-then-write sequence in one `BEGIN IMMEDIATE` SQLite transaction —
acquiring the write lock **at the count read**, not on the first write. This matters
concretely: a plain deferred `self.pool().begin()` (the pre-existing convention
elsewhere in `repo/orch.rs`) only takes the lock when the *first write statement*
runs, so two concurrent transactions could both read the count before either takes
the lock. `BEGIN IMMEDIATE` (`sqlx::Pool::begin_with`) makes the second concurrent
caller's transaction simply block until the first commits, so it's guaranteed to
read post-commit state.

Returns `Option<StatusUpdateOutcome>` (`Applied(Box<Item>)` / `Rejected(CoreError)`,
`None` only if the item vanished). Reuses the exact same `WorkflowConfig::
check_wip_limit` the old unguarded code called — no duplicated comparison logic.

**Two call sites now use it**, closing the race everywhere it could manifest, not
just on the dispatch path where it was first found: `dispatcher::apply_mapped_status`
(reproduced 12/12 concurrent dispatches over-filling a WIP-5 column before the fix)
and `handlers::items::update_item` (the human board-drag path). Both reproduced the
identical race live before being fixed — see their own test modules
(`crates/tack-api/tests/security/wip_limit_race.rs` and `board_drag_wip_race.rs`)
for the exact repro methodology (genuinely concurrent requests via `tokio::spawn`
on a multi-thread runtime, asserting the pre-fix code over-fills the column before
touching anything).

`Repository::count_items_by_status` still exists and is still used where an
unguarded read is fine (e.g. the Fleet aggregate) — only the *check-then-write*
sequence needed the atomic replacement.

## The persistence interface

`crates/tack-api/src/orch_store.rs`'s `RepoControlPlaneStore` implements
`reconciler::ControlPlaneStore` (the reconciler's narrow persistence trait) and is
where the reconciler and the rest of `tack-api` actually meet. Beyond
`list_registered`/`record_health`, it's grown the ingestion upsert calls (`upsert_runs`,
`upsert_approvals`, `upsert_metrics`, `persist_events`) and — the one with real
business logic in it — the terminal `status_map` reconciliation:

`upsert_runs` calls `reconcile_terminal_status_map` right after resolving a run's
correlated item, for any run that just transitioned to `succeeded`/`failed`/
`cancelled`. Three private methods:

1. **`reconcile_terminal_status_map`** — maps the run's terminal `RunState` to the
   matching `status_map` key, no-ops on an absent key or an already-matching status,
   then either applies the transition (through `dispatcher::apply_mapped_status`,
   reused verbatim, not reimplemented) or records the skip.
2. **`card_has_diverged`** — the human-move detector, below.
3. **`record_status_map_skipped`** — writes a `status_map_skipped_human_override`
   `orch_events` row using the same free-form-`event_type` convention
   `status_map_rejected` already established.

### How "has a human moved it" is determined without a schema change

There's no persisted "who/what last set this status" column — deliberately, to
avoid a migration for this feature. `card_has_diverged` instead compares
`item.status` against **the one `status_map` key the item's latest `orch_tasks`
attempt actually used**: `on_waiting_approval`'s value if that attempt's
`remote_status` is `waiting_approval`, else `on_running`'s value.

**This has to be exactly one key, not a union of every plausible marker.** An
earlier draft checked membership in `{dispatch_from ∪ on_running ∪
on_waiting_approval}` — wrong, and caught by a test built around the roadmap's own
worked `status_map` example, where `on_waiting_approval` and `on_failed` are **both**
`"Blocked"`. Under the union check, a human dragging a card to "Blocked" (exactly
the scenario this feature exists to protect) reads as "unchanged, still parked at
`on_waiting_approval`'s value" — and the terminal transition fires anyway, silently
overwriting the human's decision. Resolving to a single expected value from the
attempt's own last known `remote_status`, rather than a set, is what fixes it. See
`crates/tack-api/tests/orchestration/reconciler/terminal_status.rs`'s
`a_human_move_since_dispatch_blocks_on_succeeded_even_when_the_value_collides_with_on_waiting_approval`.

**Accepted limit:** this cannot detect a human re-choosing the exact status the
automation already believed the item was in (dragging a card to "In Progress" for
your own reason while `on_running` already parked it there) — no value-based check
can, without a real change-log.

## The database layer

`crates/tack-db/src/repo/orch.rs` is the repository module for the orchestration
tables; `repo/economics.rs` is a **separate** module for the two unit-economics
queries (kept separate to reduce merge collisions during concurrent development,
not a structural necessity). Notable design points:

- **The stored control-plane token never leaves this layer in a read DTO.**
  `get_control_plane_token` is doc-commented "INTERNAL ONLY"; every other read
  returns `token_set: bool`, never the value. Compile-time guarantee: there is no
  field to accidentally serialize.
- `UpdateControlPlane.token: Option<Option<String>>` gives tri-state `PATCH`
  semantics — absent leaves it, `Some(None)` clears it, `Some(Some(t))` replaces it.
- Batch upserts (`upsert_orch_tasks`/`_runs`/`_events`/`_approvals`/`_metrics`) use
  **one transaction per call**, not one per row.
- `orch_runs.item_id` and `orch_approvals.item_id` are write-once through the upsert
  path (`COALESCE(excluded.item_id, item_id)`) — a poll that doesn't yet know an
  item's attribution can never clobber one a previous poll already learned.
- `remote_status`/`state`/`source`/`event_type` columns are plain `String` — no
  matching against the Rust enums happens at this layer. An unrecognised value
  round-trips byte-for-byte.
- **`orch_events.run_id` is always `NULL` in the whole codebase.** Both call sites
  that construct a `NewOrchEvent` (trace ingestion, `status_map_rejected`) hardcode
  `run_id: None` — docket's trace payload carries no run id, only a `session_id` the
  reconciler doesn't correlate to a specific dispatch attempt. `orch_events.item_id`
  is the only reliable correlation key today; anything that needs per-*attempt* (not
  per-item) correlation can't get it from this table yet.

## Migrations 019–031

Landed following the `018_github_links` precedent — a `const [&str; N]` slice of
statements per migration, registered in the migration list. Every foreign key to
`items`/`projects`/`control_planes` is `ON DELETE CASCADE`. Timestamps are RFC3339
`TEXT`; UUIDs are `TEXT`.

| Migration | Table / change | Key columns |
|---|---|---|
| 019 | `control_planes` | `id` PK, `name`, `kind` (default `'docket'`), `base_url`, `token` (nullable, write-only over the API), `api_version`, `health`, `last_seen_at`, `consecutive_failures`. No FKs — root of the graph. |
| 020 | `orch_links` | `project_id` PK (FK → `projects`), `control_plane_id` (FK), `remote_project`, `pipeline_file`, `blueprint`, `auto_dispatch`, `budget_usd` (a cap, deliberately not `_estimated`), `status_map` (JSON `TEXT`). |
| 021 | `orch_tasks` | **Composite PK `(item_id, remote_task_id)`** — an item can be redispatched, each attempt gets its own row. `remote_run_id` (indexed, no hard FK), `remote_status`, `attempt`, `tokens_in`/`tokens_out`, `cost_usd_estimated` (nullable), `dispatched_at`, `trusted`. |
| 022 | `orch_runs` | `run_id` PK, `control_plane_id` (FK), `item_id` (nullable — `NULL` = mirrored, unattributed), `remote_project`, `source`, `state`, `started_at`, `ended_at`, `error`. |
| 023 | `orch_events` | Append-only telemetry. `id` PK (caller-assigned), `control_plane_id` (FK), `item_id` (nullable FK), `run_id` (always `NULL` today — see above), `event_type` (raw string), `payload` (JSON `TEXT`). Indexes `(item_id, occurred_at)`, `(occurred_at)`. |
| 024 | `orch_approvals` | `token` PK (docket's own correlation id, not a credential), `control_plane_id` (FK), `item_id` (nullable — `NULL` = uncorrelated, still shown fleet-wide), `remote_task_id`, `agent`, `action`, `state`, `requested_at`, `decided_at`. |
| 025 | `orch_metrics` | Mirror of docket's Prometheus `/metrics` scrape — one row per scrape per metric per label set. |
| 026 | `orch_events_daily` | Per-day aggregate of purged `orch_events`. Keyed `(day, control_plane_id, event_type)` — **drops `item_id`**, so per-item history truncation past the retention window is not recoverable from the aggregate. |
| 027 | `orch_metrics_daily` | Per-day aggregate of purged `orch_metrics`; non-finite samples counted but excluded from sum/min/max. |
| 028 | `orch_trace_cursors` | Resumption cursor per `(control_plane_id, remote_project)`, stored as an **opaque string** — docket's own cursor value, not a client reconstruction. |
| 029 | `items.source` | `ALTER TABLE items ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown'` — the prompt-injection trust boundary. See [The trust boundary](#the-trust-boundary-itemsource). |
| 030 | `project_templates.orchestration` | `ALTER TABLE project_templates ADD COLUMN orchestration TEXT` (nullable, no default — `NULL` = no block, distinct from `Some("{}")`). Backwards compatible; every existing template row and every existing INSERT keeps working unchanged. |
| 031 | `idx_items_completed_at` | Partial index `ON items(completed_at) WHERE completed_at IS NOT NULL` — both unit-economics queries filter on this column instance-wide (not scoped to one project, since the whole point is slicing across projects), so no existing `(project_id, …)` composite index helps without it. |

Agent state is **not** denormalized onto `items` — the board query LEFT JOINs the
latest `orch_tasks` row.

## The API surface

Every route below lives in `orch_routes()` (`crates/tack-api/src/router.rs`), gated
by a single middleware layer, `orch::require_orch_enabled`, rather than a per-handler
check:

```rust
pub async fn require_orch_enabled(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !state.config.orch_enable {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(next.run(req).await)
}
```

With `TACK_ORCH_ENABLE` unset, every route below 404s — indistinguishable from not
existing. The ordinary Bearer-token gate (`require_token`) wraps this sub-router too.

| Method | Path | Notes |
|---|---|---|
| `POST` | `/api/control-planes` | Register. `token` write-only. |
| `GET` | `/api/control-planes` | List (no tokens). |
| `GET` | `/api/control-planes/{id}` | One plane. |
| `PATCH` | `/api/control-planes/{id}` | `token: Option<Option<String>>` tri-state. |
| `DELETE` | `/api/control-planes/{id}` | Deregister. |
| `GET` | `/api/projects/{id}/orch-link` | `{linked: false, link: null}` for an unlinked project — `200`, not `404`. |
| `PUT` | `/api/projects/{id}/orch-link` | Create/replace. Validates every `status_map` status name against the project's live `WorkflowConfig`. |
| `GET` | `/api/fleet` | The Fleet view's aggregate. |
| `GET` | `/api/metrics` | Tack's own work-tracking metrics merged with the mirrored docket ones (Prometheus text), unauthenticated like docket's own. |
| `GET` | `/api/items/{id}/agent-activity` | One item's dispatch/hop/token timeline. |
| `GET` | `/api/projects/{id}/agent-activity` | Project-level roll-up of the same. |
| `POST` | `/api/items/{id}/dispatch` | Enqueue a governed task; applies `on_running`/`on_waiting_approval`. |
| `POST` | `/api/sprints/{id}/dispatch` | DAG-ordered sprint dispatch. `max_in_flight` query param. |
| `GET` | `/api/sprints/{id}/dispatch/dry-run` | Same planning function, zero writes, zero HTTP calls to docket. |
| `GET` | `/api/approvals` | Fleet-wide pending-approvals inbox, oldest first, includes uncorrelated rows. |
| `POST` | `/api/approvals/{token}` | Grant/deny. Gated additionally by `X-Tack-Approval-Token` against `TACK_ORCH_APPROVAL_TOKEN` — `403` unconditionally if that variable is unset, no fallback. |
| `GET` | `/api/projects/{id}/orch-budget` | This project's `budget_usd` vs. token-derived spend. |
| `GET` | `/api/projects/{id}/orch-policy` | Denial rate / policy hits / tool-call volume — **scoped to the control plane, not the project** (`scoped_to_control_plane_only: true` always present on the wire). |
| `POST` | `/api/templates/{id}/provision` | Create a Tack project *and* a docket pod from a template's `orchestration` block, with rollback-before-the-pod-exists (never after — docket has no delete route). |
| `GET` | `/api/economics/summary` | Tokens, estimated cost, lead time, rework rate — overall / by `project_type` / by `item_type`. |
| `GET` | `/api/economics/items` | Per-item detail; `?format=csv` for export. |

### `GET /api/fleet`

One row per Tack project with an `orch_links` row, joining the link, its control
plane's reconciler-observed health, and cost/token/approval sums from `orch_tasks`/
`orch_approvals` for that project's items. `cost_usd_estimated` is `null` whenever
`health == "unreachable"` — never coerced to `0`. `pending_approval_count` is
project-scoped; an uncorrelated approval (`item_id IS NULL`) is excluded here and
surfaces only in the fleet-wide `/api/approvals` inbox.

Every handler in `handlers/orch.rs`/`handlers/economics.rs`/`handlers/provisioning.rs`
that isn't a dispatch/provisioning route reads Tack's own database, populated
out-of-band by the reconciler — a docket outage can only ever leave `health`/
`last_seen_at` stale, never turn into a `500`.

## The five non-negotiables

Enforced by the shape of the code, not just documented — each one below names where.

1. **Never hold a SQLite write transaction across an HTTP call to docket.** The
   reconciler's fetch → decide → persist phase separation makes this structurally
   true. The dispatcher's own fetch → HTTP → short-write sequence (used by every
   caller: manual dispatch, auto-dispatch, sprint dispatch) does the same.
2. **Status changes go through the workflow engine, never raw SQL.** WIP limits,
   explicit transitions, `started_at`/`completed_at`, and parent auto-propagation all
   fire on an orchestration-driven status change exactly as they do on a user drag —
   `apply_mapped_status` calls the same `Repository::update_item_status_checked`
   the human board-drag path now calls (see the WIP-race fix above). `PUT /orch-link`'s
   `status_map` validates every named status against the project's live
   `WorkflowConfig` before it's ever stored.
3. **Everything is off by default.** `TACK_ORCH_ENABLE` unset ⇒ `spawn_reconcilers`
   returns before even querying for registered planes, and `require_orch_enabled`
   404s every route.
4. **The stored control-plane token never leaves the repository layer**, and is
   **scrubbed from every backup bundle.** `scrub_snapshot_secrets`
   (`crates/tack-api/src/remote_backup.rs`) nulls `control_planes.token` before the
   `VACUUM`, alongside the pre-existing `app_meta` secret scrub — the same class of
   leak the S3 secret key had before its own exclusion shipped. Tested with a
   raw-bytes assertion on the extracted snapshot, not just a DTO-shape check.
5. **A check-then-write status update is never split across two unlocked steps.**
   `Repository::update_item_status_checked`'s `BEGIN IMMEDIATE` transaction is the
   one place a WIP-limit check and its corresponding write happen — every caller
   (dispatch, sprint dispatch, board drag) goes through it, closing a real,
   live-reproduced race (see above). Any future status-writing code path must go
   through it too.

## Concurrency control: `version`, `ETag`, `If-Match` — and what it doesn't cover

A `version INTEGER` column on `items`, `orch_links`, and `control_planes`
(migrations 034–036) backs optimistic concurrency, and
`handlers::items::{get_item,update_item}` round-trips it as
an RFC 7232 `ETag`/`If-Match` pair: `GET /api/items/{id}` returns `ETag: "<id>-<version>"`,
and `PATCH /api/items/{id}` with a matching `If-Match` claims the next version atomically
before touching any other field; a stale or mismatched `If-Match` is a `412`, never a
silent overwrite. **An absent `If-Match` header behaves exactly as it always has** — this
is additive, not a new requirement on any existing caller.

### The MCP write path now sends it

`tack-cli`'s HTTP client (`client.rs`) can attach an `If-Match` header to a request, so
`tack mcp`'s `update_item`/`move_item` tools are no longer unconditionally
last-write-wins — the one write path most exposed to the exact race `If-Match` exists to
catch (an autonomous agent editing a card a human is also looking at). Both tools:

1. `GET /items/{id}` first, via `TackClient::get_with_etag`, to read the current `ETag`.
2. `PATCH /items/{id}` with that value as `If-Match`, via `TackClient::patch_if_match`.
3. On `412`, return a *distinct* tool error naming the race and telling the agent to
   re-read and retry — not the generic `{status}: {message}` shape every other error uses.
   An agent that can't tell "you raced" from "the server broke" retries blindly and
   clobbers whatever won.

If the server ever answers a `GET` with no `ETag` (an older server, or a route that never
gains version tracking), the client sends no `If-Match` and the write proceeds unguarded —
the fallback is silent and total, not a partial degrade.

### CORS had to catch up separately

`If-Match` and `ETag` are meaningless to a browser client unless the CORS layer explicitly
allows/exposes them — `tower_http`'s preflight response only lists what
`CorsLayer::allow_headers`/`expose_headers` were built with, regardless of what a real
request needs. `router.rs`'s `CorsLayer` allows `if-match` on requests and exposes
`ETag` on responses; it also allows `x-tack-approval-token` —
`frontend/src/features/approvals/api.ts` sends that header on every grant/deny decision,
and it has only ever worked because production is same-origin via `embed-spa`. Any
cross-origin deployment through `TACK_ALLOWED_ORIGINS` would fail every approval
preflight silently if that header weren't allowed. See
`crates/tack-api/tests/security/cors.rs` for the CORS coverage.

### Two writers this control deliberately does not cover

Both of the following mutate `items.status` (or a row's `version`) with no HTTP request in
flight at all, which means no `If-Match` was ever possible — not an oversight, a
consequence of where the call happens:

- **The reconciler's terminal-status transition.**
  `RepoControlPlaneStore::upsert_runs` (`orch_store.rs`) calls
  `dispatcher::apply_mapped_status` directly, in-process, from a background `tokio` task —
  there is no `HeaderMap` to read an `If-Match` from because there is no request. This is
  **the largest single mutator of `items.status` in the whole system** (every terminal
  docket run that has a `status_map` target passes through it) and it sits entirely outside
  this optimistic-concurrency control. A human moving a card at the same moment a poll
  resolves a terminal status is instead handled by a *different* mechanism —
  `card_has_diverged`, described above — which compares the item's status against the one
  value the automation itself expects, not a version number.
- **`propagate_parent_completion`.** A child's `PATCH` (which *does* carry `If-Match` for
  the child) can cascade into `Repository::check_and_update_parent_status` bumping the
  *parent's* `version` — a row no caller in that request ever named, let alone sent a
  precondition for. A client holding the parent's old `ETag` from an earlier `GET` will see
  its next `If-Match` on the parent 412 the moment any child completes the set, and that is
  correct: the parent genuinely changed underneath it.

**What this means for a client:** a `412` proves *a* concurrent write happened to the exact
row named in the request; it is not a total ordering over every writer in the system, and a
`200` on some other row is not proof that row is still what a stale `GET` believes it to
be. Anything that needs a stronger guarantee than "the row I'm PATCHing hasn't changed
since I last read it" needs a mechanism that doesn't exist yet.

## Adding a new control-plane backend

1. Implement `ControlPlane` for your type in a new `adapters::<name>` module,
   returning `OrchError::Disabled` for any write method you don't support yet.
2. Add a `match` arm for your `kind` string in `RepoControlPlaneStore::list_registered`
   (`crates/tack-api/src/orch_store.rs`).
3. Nothing in `reconciler.rs`, `dispatcher.rs`, `sprint_dispatch.rs`,
   `handlers/orch.rs`, or the Fleet frontend needs to change — they all consume
   `Arc<dyn ControlPlane>`.

## What's genuinely missing (not a Tack gap)

Two things docket itself doesn't expose over HTTP, confirmed by reading `serve.py`
directly rather than assumed — Tack builds no workaround for either:

- **Pause/resume has zero HTTP surface, in either direction.** `docket profile <id>
  --resume` is CLI-only; `/status.json` and `/metrics` were both checked line by
  line and neither field (`paused`/`pausedReason`) is emitted anywhere. The one
  indirect signal (a `paused_refused` trace event) can't be reliably attributed to
  one linked Tack project with today's ingestion (`orch_events` has no
  `remote_project` column). Tack builds no pause control or indicator — see the
  [user guide](../user-guide/orchestration.md#budget-pause-and-policy).
- **`docket pipeline validate` is CLI-only.** No HTTP route exists for it, so
  `handlers::templates::validate_template_orchestration` only checks that
  `orchestration.pipeline_yaml` parses as YAML, never that it's a valid docket
  pipeline. Recorded as a gap in docket's own roadmap.

## Testing this feature

- `cargo nextest run --workspace -E 'package(tack-orch)'` — `ControlPlane`/DTO unit tests, the `DocketAdapter`
  integration tests against `wiremock` fixtures (captured from a real
  `docket serve`, plus deliberately malformed/unknown-enum fixtures), the
  Prometheus parser's own tests, and the reconciler's state-machine tests (driven by
  a fake `ControlPlane`/`ControlPlaneStore`, no real network or database).
- `cargo nextest run --workspace -E 'package(tack-db)'` — migration table-existence/upgrade-in-place/FK-orphan
  tests, the repository's CRUD/idempotency tests, and `status_update_checked_test.rs`
  (the atomic WIP-check transaction's own correctness, isolated from any HTTP call).
- `cargo nextest run --workspace -E 'package(tack-api)'` — route-gating and token-leak tests
  (`every_orch_route_404s_when_disabled`, and a suite asserting the literal
  control-plane token string never appears in any response body); the dispatch,
  sprint-dispatch, terminal-status, approvals, budget/policy, provisioning, and
  economics tests, which live under `crates/tack-api/tests/orchestration/` and
  `handlers/`; and the two WIP-race regression suites under
  `crates/tack-api/tests/security/` that reproduce the race
  live before asserting the fix.

None of these require a live docket instance for CI — the adapter tests run against
`wiremock`, and everything downstream runs against an in-memory database and a
hand-written fake `ControlPlane`. For a live sanity check beyond the committed
suite, verify against a real, isolated `docket serve` (scratch `DOCKET_HOME`, never
`~/.docket`) — see
[Local Integration Setup](../user-guide/orchestration-local-setup.md) for how.

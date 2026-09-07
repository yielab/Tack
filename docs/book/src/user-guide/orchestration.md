# Orchestration & the Fleet View

Tack is the **control center for a factory of products** built by governed agent
fleets — today, that means [docket](https://github.com/yielab/docket), a separate
tool that runs pods of Lead/Implementer/Reviewer/Tester agents against a project,
with its own budgets, approvals, and audit log. This page covers what a **control
plane** is, how to register one, what dispatch does, how approvals and budgets work,
and — read this carefully — why every dollar figure on every page says "estimated."

**This feature is off by default.** With `TACK_ORCH_ENABLE` unset, no reconciler
task runs and every route this page describes returns `404`, exactly as if none of
it existed.

Tack **both watches and drives** your agent fleet: it mirrors what docket reports
(runs, approvals, metrics, traces) *and* it can push work to docket — dispatching a
single item or an entire dependency-ordered sprint. If you want to try this against a
real, local `docket serve` from a clean start, see
[Local Integration Setup](orchestration-local-setup.md) — this page assumes
orchestration is already enabled and a control plane is already reachable, and
focuses on what each surface means and why it's built the way it is.

## What a control plane is

A **control plane** is an agent-fleet backend Tack can poll and dispatch to — a
running `docket serve` instance, in practice. You can register more than one (for
example, one per environment), and you can link a control plane to a specific Tack
project so Tack knows which pod's agents are working on that project's board.

Tack never guesses at a control plane's state. A background task called the
**reconciler** polls each registered plane on an interval and records what it finds
— whether it's reachable, what version it reports, its runs, its approvals, its
metrics, and (once you've linked a project to it) its activity. If a plane goes
quiet, the Fleet view says so rather than showing you stale numbers as if they were
current.

## Enabling orchestration

The whole feature is gated behind four settings:

| Variable | Default | Effect |
|---|---|---|
| `TACK_ORCH_ENABLE` | `false` | Turns on the reconciler background task and every orchestration route (control planes, links, fleet, dispatch, approvals, budget/policy, provisioning, economics). Unset ⇒ none of it exists — every route 404s exactly as if it were never built. |
| `TACK_ORCH_POLL_SECS` | `10` | How often (in seconds) the reconciler polls each registered plane, before per-plane exponential backoff and ±20% jitter are applied. |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `90` | Days of mirrored `orch_events`/`orch_metrics` history kept before the retention sweep rolls old rows into per-day aggregates and deletes the originals. See [Retention](#retention-what-survives-past-the-window) below. |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | A **separate** shared secret, distinct from `TACK_API_TOKEN`, required to grant or deny a docket approval. See [Approvals inbox](#approvals-inbox). |

See [Configuration](configuration.md) for the full reference table. Set
`TACK_ORCH_ENABLE=true` and restart the server to turn the feature on. Nothing else
changes until you register a control plane.

## Registering a control plane and linking a project

You can do both from the UI or the API.

**UI:** open a project's **Settings → Orchestration** tab. The form there
(`LinkForm`) lets you pick a registered control plane, name the remote project
(docket's own project identifier, not Tack's UUID), and set a budget cap. It does
not yet let you create the control plane itself or edit `status_map`/
`auto_dispatch`/`blueprint` — those still go through the API.

**API:**

```bash
curl -X POST https://tack.test/api/control-planes \
  -H "Content-Type: application/json" \
  -d '{
    "name": "docket-prod",
    "base_url": "https://docket.example.com",
    "token": "<bearer token, optional>"
  }'
```

- `kind` defaults to `"docket"` — the only backend Tack understands today.
- `token` is the Bearer credential the control plane expects on its authenticated
  routes. It's optional: an unauthenticated docket instance works fine without one.
- **The token is write-only.** Once stored, it never comes back out of the API —
  every response shows `token_set: true/false` instead of the value. `PATCH` with a
  new `token` replaces it; `PATCH` with `"token": null` clears it; omitting the field
  entirely on a `PATCH` leaves the stored token untouched.

```bash
curl -X PUT https://tack.test/api/projects/<project-id>/orch-link \
  -H "Content-Type: application/json" \
  -d '{
    "control_plane_id": "<control-plane-id>",
    "remote_project": "myapp",
    "budget_usd": 50.0,
    "auto_dispatch": false,
    "status_map": {
      "dispatch_from": ["Ready"],
      "on_running": "In Progress",
      "on_waiting_approval": "Blocked",
      "on_succeeded": "Done",
      "on_failed": "Blocked",
      "on_cancelled": "Ready"
    }
  }'
```

A project has at most one link. Every status name inside `status_map` is validated
against **your project's actual workflow** the moment you save — a typo is rejected
with a `400` naming the bad status, never silently accepted as a no-op. See
[status_map](#status_map-mapping-docket-states-onto-your-board) below.

## Dispatch

Dispatch is what turns Tack from a dashboard into a control center: moving a card (or
running a sprint) can send a governed task to docket.

### Dispatching a single item

```bash
curl -X POST https://tack.test/api/items/<item-id>/dispatch
```

Also reachable from the item detail drawer ("Dispatch to agents") and the board
card's context menu. The response's `outcome` field is one of:

| `outcome` | Meaning |
|---|---|
| `no_dispatch_policy` | The linked project's `status_map.dispatch_from` is empty — nothing configured to dispatch on. Not an error. |
| `not_eligible` | The item's current status isn't in `dispatch_from`. Not an error — includes `current_status` and `dispatch_from` so the caller can see why. |
| `already_in_flight` | The item's most recent attempt is still `pending`/`running`/`waiting_approval`. Docket is **not** called again — this is the idempotency guard, not a failure. |
| `blocked` | docket's `pre_input` policy hook refused the task before creating it. **No `orch_tasks` row is created; the item is left untouched.** Carries a real `policy_id` (e.g. `"block-cmd"`) and `message` — never a generic failure. |
| `waiting_approval` | docket queued the task but a policy demands human sign-off before it runs. Carries an `approval_token`. |
| `dispatched` | docket accepted the task and it's running. |

**Read this table before building anything on top of it.** `blocked` and
`waiting_approval` are deliberately **distinct outcomes** with distinct shapes
(`policy_id` vs. `approval_token`), not two flavors of the same "something's
pending" state — conflating them is a real correctness bug. A `blocked` task never
existed on docket's side at all; a `waiting_approval` task exists, is queued, and is
one grant away from running. An integration that shows both the same way ("dispatch
paused") hides which one is actually true.

Every one of these outcomes is an HTTP `200` — branch on the `outcome` field in the
JSON body, not on the status code.

### Dispatching a sprint (DAG-ordered)

```bash
# Preview only — no dispatch, no writes to docket
curl "https://tack.test/api/sprints/<sprint-id>/dispatch/dry-run?max_in_flight=5"

# The real run
curl -X POST "https://tack.test/api/sprints/<sprint-id>/dispatch?max_in_flight=5"
```

`max_in_flight` is a **query parameter** on both routes (not a JSON body field),
clamped to `[1, 20]`, default 5. Sprint dispatch topologically sorts the sprint's
items by their dependency graph and holds each item until every one of its
dependencies has reached a Done-category status — a dependency outside the sprint
(even outside the project) is checked the same way. Independent items dispatch
concurrently, bounded by `max_in_flight`.

Every item in the sprint always appears in the response with an `order` and a
`decision` (`waiting_on_dependencies` / `no_dispatch_policy` / `not_eligible` /
`already_in_flight` / `would_dispatch` in a dry run; `blocked` / `waiting_approval` /
`dispatched` / `error` in a real run) — nothing is silently excluded. A policy block
or an error on one item never aborts the rest of the sprint; downstream items simply
report `waiting_on_dependencies` on their next evaluation, with no separate
bookkeeping. The dry-run and the real run share one planning function, so the
preview's order and skip decisions are guaranteed to match what actually happens —
the UI's "Run sprint" dialog always shows this preview before a confirm click can
fire the real dispatch.

### The trust boundary: `source` and `trusted`

Every item carries a **sticky `source`** — `manual`, `github`, `linear`,
`json_import`, or `csv_import` — set exactly once, at creation, and never changed by
an update. Only `manual` items are trusted; every import path defaults to untrusted.

**Why this matters.** An imported issue's title and description were written by
whoever filed it — anyone on the internet, for a public GitHub repo. Dispatch turns
that text into a pipeline `variables` payload that a real agent with tool access
reads and acts on. Treating imported text with the same trust as something you typed
yourself is a prompt-injection hole: a malicious issue title becomes a literal
instruction to an agent that can run `bash`.

So every dispatch call passes `trusted` **explicitly** — `true` for a manually
created item, `false` for anything imported. This is deliberate and structural, not
a convention: **docket's own `enqueue_task` treats an *omitted* `trusted` value as
"trusted iff the caller is `operator`" — which is always true for every existing
caller.** Omitting the flag silently grants operator-level trust to attacker-authored
text. Tack's dispatcher requires the flag on every call (there is no version of the
function that lets you skip it), specifically so this can never happen by omission.

Auto-dispatch (see below) passes the item's persisted `source.is_trusted()` value
automatically; the manual "Dispatch to agents" button does the same. You never need
to think about this as an operator — it's enforced structurally — but if you're
building against the API directly, know that the safety property here is "the flag
is always present," not "the flag is usually correct."

### Auto-dispatch

When a project's link has `auto_dispatch: true`, moving an item into a
`dispatch_from` status fires the same dispatch path automatically, off the request
path (so a slow or failing dispatch never delays or fails the status-change request
itself). A failure is logged and recorded as an `orch_events` row
(`auto_dispatch_failed` / `auto_dispatch_blocked`) rather than silently swallowed —
check an item's Agent Activity tab if a card you expected to dispatch didn't seem to.

## `status_map`: mapping docket states onto your board

`status_map` is the configuration that connects docket's task/run lifecycle to your
project's own workflow statuses:

```json
{
  "dispatch_from": ["Ready"],
  "on_running": "In Progress",
  "on_waiting_approval": "Blocked",
  "on_succeeded": "Done",
  "on_failed": "Blocked",
  "on_cancelled": "Ready"
}
```

`dispatch_from` is the only required key; every other key is optional, and an absent
key means "don't touch the item's status on that transition." **Every status name is
validated against your project's `WorkflowConfig` when you save the link** — not
against a generic default — so a construction project's real column names
(`Permit`/`Procurement`/`Build`/…) are what gets checked, not "In Progress"/"Done."
A typo is rejected with a `400` immediately, never silently accepted as a dead
mapping.

Two different moments apply `status_map`, and they work differently:

- **Dispatch-time** (`on_running`, `on_waiting_approval`) is applied synchronously,
  inside the same request that dispatches the item, right after docket accepts the
  task.
- **Terminal** (`on_succeeded`, `on_failed`, `on_cancelled`) is applied by the
  reconciler, once a mirrored `orch_runs` row reaches that terminal state on a later
  poll.

**Both paths go through the workflow engine, never raw SQL** — the same WIP-limit
and explicit-transition checks a human dragging a card gets. If the mapped
transition is illegal (a construction project's linear workflow refusing an
out-of-order jump, or a WIP-limited column that's full), the transition is **skipped
and recorded**, never forced: an `orch_events` row (`status_map_rejected` at
dispatch time) names the workflow engine's own rejection reason, and the item is
left exactly where it was. Docket still ran the task — Tack just couldn't reflect it
on the board — so check an item's event history if its status looks stuck relative
to what you know docket did.

### A human who moved the card wins

If a run reaches a terminal state and the item's status has drifted from where
Tack's own automation last parked it — a human dragged the card to something else in
the meantime — **the mapped terminal status is not applied.** Docket's outcome is
still fully mirrored into `orch_runs` (nothing about the run itself is lost), but the
board-visible item status is left alone, and the skip is recorded as a
`status_map_skipped_human_override` event.

This is a deliberate choice, not an oversight: an agent finishing its work is a real
signal, but a human who explicitly dragged a card to, say, "Blocked" made a decision.
Silently reverting it the instant a run happens to succeed is exactly the kind of
thing that erodes trust in the board. The accepted limit: this can't detect a human
re-choosing the *exact* status the automation already believed the item was in (e.g.
dragging a card to "In Progress" for your own reason while `on_running` already put
it there) — no value-based check can, without a change-log of who-set-what-when.

## The Fleet view

Once a control plane is registered and at least one project is linked, the **Fleet**
page (sidebar, under Workspace) shows one row per linked project:

| Column | What it shows |
|---|---|
| **Project** | The Tack project name and which control plane / kind it's linked to. |
| **Pod health** | `healthy`, `degraded`, or `unreachable`, driven entirely by the reconciler's poll history. |
| **Roster** | Always empty today — see [What's still a placeholder](#whats-still-a-placeholder). |
| **Last activity** | When something last happened on this project's agent tasks. |
| **Burn vs budget** | Tokens used against your configured cap, with an estimated dollar figure underneath. |
| **Gateway** | Always `unknown` today — see below. |
| **Approvals** | Pending gated actions for this project — read the fleet-wide [Approvals inbox](#approvals-inbox) for the full picture, including uncorrelated approvals this column can't show. |

### Reading pod health

Health is the reconciler's running verdict, not a live check:

- **Healthy** — the plane answered its last poll normally.
- **Degraded** — three consecutive poll failures. The row keeps its last-known
  numbers, distinguished by an amber health chip and a "last seen" caption.
- **Unreachable** — ten consecutive poll failures. Token counts, cost, gateway
  state, and approval count are replaced with a dash and a caption explaining why,
  rather than a confident-looking (and wrong) zero. Roster is greyed and labelled
  "may be out of date."

Recovery is immediate — a single successful poll takes a plane straight back to
healthy. Killing the control plane produces exactly this sequence: healthy →
degraded → unreachable, with **no** failed request or error on the Tack side at any
point.

## Agent Activity

Every item that has ever been dispatched has an **Agent Activity** tab (item detail
drawer) showing hops, tool calls, verdicts, rework cycles, approvals, and
tokens/estimated cost, grouped by dispatch attempt, newest first. A compact state
chip (queued / running / waiting-approval / failed / done) appears on Board, List,
and Table cards for any item with agent activity — nothing renders for an item that
has never been dispatched.

`GET /api/items/{id}/agent-activity` and `GET /api/projects/{id}/agent-activity`
back this directly if you're building your own view.

## Approvals inbox

A fleet-wide page (sidebar → Approvals) listing every pending approval across every
linked pod, **oldest first** — docket's approvals fail closed on timeout, so latency
here has a real cost.

**Reading the inbox** needs only the ordinary orchestration gate. **Deciding one**
(`POST /api/approvals/{token}`) needs a **separate credential**: the request must
carry an `X-Tack-Approval-Token` header matching `TACK_ORCH_APPROVAL_TOKEN`. With
that variable unset, every decision request gets `403` unconditionally — there is no
"no secret configured, allow it" fallback, unlike the ordinary API-token gate. This
is deliberate: releasing a gated agent action is a materially higher-privilege act
than editing a card, and the safe default has to be "nothing on this server can
release a paused agent," not "trust the network boundary" (which is the ordinary
Bearer gate's own safe default). The inbox's list response carries
`grant_available: bool` so the UI can hide decision controls without a second probe,
never the secret itself.

Every decision made from Tack is recorded on docket's side with `channel: "tack"` —
distinguishable in docket's own audit chain (`docket audit verify`) from a CLI grant,
an HTTP grant from something else, or a Telegram decision. Grant/deny is never a
single click: the UI opens a confirmation naming the requesting agent, the action
text, the correlated item (or "Uncorrelated"), and how long it's been waiting, with
the words "This cannot be undone."

**Uncorrelated approvals surface here and nowhere else.** An approval docket can't
tie back to a specific Tack item (`item_id IS NULL` — usually something dispatched
outside Tack entirely) doesn't appear in the Fleet view's per-project approval count,
but it does appear in this inbox, labelled "Uncorrelated," so nothing gated is ever
invisible.

## Budget, pause, and policy

Project **Settings → Orchestration** carries two panels beyond the link form:

- **Budget** — this project's configured `budget_usd` cap against Tack's own
  token-derived spend estimate (`GET /api/projects/{id}/orch-budget`). The
  progress fraction is explicitly captioned as "an estimate of a fraction of an
  estimate" everywhere it renders, and it is **not** clamped at 100% — an
  over-cap project is exactly the state an operator most needs to see.
- **Policy** — denial rate, policy hits by id, approvals by channel, and tool-call
  volume, all sourced from docket's `/metrics` (`GET /api/projects/{id}/orch-policy`).
  **This data is scoped to the control plane, not the linked project** —
  docket's own metrics endpoint aggregates every project on a pod together, with no
  per-project label to filter on. The response carries
  `scoped_to_control_plane_only: true` and the panel renders that caveat above every
  number, not as a footnote. Chain verification is not reimplemented in Tack; the
  panel links out to `docket audit verify`.

**There is no pause control or pause indicator anywhere in Tack, and that's by
design given what docket currently exposes, not a missing feature Tack chose not to
build.** docket's budget auto-pause (`docket profile <id> --resume` is the only way
to clear it) has **zero HTTP surface, in either direction** — no route to trigger it,
and no route to even read whether a given agent is currently paused. `GET
/status.json` and `GET /metrics` were both checked line by line and neither emits a
`paused`/`pausedReason` field at all, even though docket tracks both internally. The
one indirect signal, a `paused_refused` trace event, exists but can't be reliably
attributed to *which* linked Tack project produced it with today's ingestion. If a
project's spend looks stalled, the budget panel names the real remedy —
`docket profile <pod-id> --resume` — as a static caption; it never claims to know
whether that's actually what happened.

## Provisioning: one click, a project and its pod

A **template's** `orchestration` block (blueprint, pipeline reference, budget,
default `status_map`, pod shape) can be set via `POST /api/templates` and then used
to provision a Tack project *and* a docket pod together:

```bash
curl -X POST https://tack.test/api/templates/<template-id>/provision \
  -H "Content-Type: application/json" \
  -d '{
    "name": "New Product",
    "control_plane_id": "<control-plane-id>",
    "remote_project": "new-product",
    "pod": {"blueprint": "software", "pod_shape": "full", "budget_usd": 50.0}
  }'
```

Also reachable from the **Provision** page (sidebar) as a four-step wizard:
project/template → pod & control plane → review → result. A confirmation dialog
("This creates real infrastructure and cannot be automatically undone.") gates the
actual call, the same pattern the approvals inbox and sprint dispatch use — this
route is deliberately **not** gated behind the separate approval token, since it's
ordinary use of the same privilege class as dispatch, not a release of a guardrail's
deliberate block.

**Rollback is one-directional, by necessity.** docket's own `POST /pods` is atomic
on its own side (fully created or nothing created, with `409` for "already exists"),
and has **no HTTP route to delete a pod**. So the flow validates everything it can
(control plane exists, `status_map` names real statuses in the project's own
just-created workflow) *before* calling docket, and only rolls the Tack project back
if the call to docket itself fails. If the pod is created successfully but the final
`orch_links` write fails, the project is **not** deleted (that would strand the only
record that the pod exists) — the response instead reports
`pod_created_link_failed`, naming the pod, and pointing at the manual link form so
you can finish the connection without provisioning a second pod.

**Known gap:** a template's `orchestration.pipeline_yaml` has no delivery path to
docket at provisioning time — `POST /pods` has no pipeline field, and links only
store a pipeline *by name* (`pipeline_file`), not inline YAML. Provisioning still
succeeds; the response's `warnings[]` names the gap rather than silently dropping the
pipeline or inventing a delivery mechanism.

**Pipeline validation is a YAML parse only, not a schema check.** When you set
`orchestration.pipeline_yaml` on a template, Tack checks that it parses as YAML and
nothing more — it is **not** validated as a real docket pipeline (duplicate step
ids, bad rework edges, unknown variables, etc.). docket's own `pipeline validate` is
the tool that actually knows that schema, and it's CLI-only today — no HTTP route
exists for it. Tack deliberately does not reimplement docket's pipeline schema in
Rust (a second copy would drift the moment docket adds a step kind), and does not
shell out to a local `docket` binary (every other control-plane interaction goes
over HTTP, since docket is not assumed to run on the same host as Tack). The error
message on an invalid block says exactly this: "this only checks it parses as YAML,
not that it is a valid docket pipeline."

## Unit economics

The **Economics** page (sidebar) answers "what did each product line cost, in
tokens and estimated dollars, per shipped item, and how often did agents need
rework?" — sliced overall, by project type, and by item type
(`GET /api/economics/summary`; per-item detail and CSV/JSON export at
`GET /api/economics/items`).

A few rules this page enforces, not just documents:

- **Agent vs. human population is disjoint.** A completed item counts as "agent" if
  it has at least one dispatch on record (regardless of who ultimately finished it),
  "human" otherwise. Agent lead time is measured `dispatched_at → completed_at`;
  human lead time is `started_at → completed_at`.
- **Below a minimum sample size (5), no average is shown.** Lead time falls back to
  raw per-item hours, rework rate falls back to a raw count, and
  **cost-per-completed-item — the headline number of this page — is withheld
  entirely** rather than computed from a handful of items.
- **No bare "agents are Nx faster" ratio is ever computed.** Agent and human lead
  time are shown side by side with an explicit selection-bias note underneath, for
  the reader to weigh — an item dispatched to an agent and an item a human picked up
  are not a controlled comparison.
- **Rework rate's definition travels with the number.** It's the share of
  dispatched, *completed* items with at least one `rework_started`,
  `verification_failed`, or `tester_verdict_failed` event — an item that was
  dispatched, reworked repeatedly, and never finished is invisible to this rate,
  which means it likely **understates** rework, not overstates it.
- **Retention truncation only affects rework rate, never tokens/cost/lead time.**
  `orch_tasks` (the source for tokens, cost, and lead time) is never purged by the
  retention sweep — only `orch_events`/`orch_metrics` are. So a dispatch whose
  rework-signal events have aged out of the retention window is excluded from the
  rework-rate denominator entirely (never silently counted as "no rework"), while its
  token and cost figures remain fully intact.

**Not built:** per-role/per-model outcome-quality export (task 38.3). docket's
per-agent role/model roster is a live `/status.json` snapshot, never persisted per
dispatch — building this needs a schema change to capture role/model at dispatch
time, which is a real follow-up, not an oversight.

## Retention: what survives past the window

`orch_events` and `orch_metrics` grow unbounded otherwise (docket has the identical
open gap in its own traces directory). A daily job rolls raw rows older than
`TACK_ORCH_EVENT_RETENTION_DAYS` into per-day aggregates (`orch_events_daily`,
`orch_metrics_daily`) **before** deleting them — so a 91-day-old event is gone, but
its day's totals survive.

**The aggregate is coarser than the raw data, and that has a real consequence.**
`orch_events_daily` is keyed by `(day, control_plane_id, event_type)` — it drops
`item_id` entirely. Once an item's dispatch history ages past the retention window,
its per-item event history (and, as a direct result, its rework-rate signal — see
[Unit economics](#unit-economics) above) is **not recoverable at any granularity**,
not just "coarser." `orch_tasks` (tokens, cost, dispatch timestamps) is untouched by
this sweep and has no retention limit of its own.

## Why every dollar figure says "estimated"

**Tack never sees a bill, and neither does docket.** Both explicitly refuse to
relabel a token-based estimate as spend. What Tack receives is **token counts** —
how many tokens a task used, in and out — and multiplies them by a pricing table it
has on file, labelling the result an estimate.

- **Token counts are the primary figure** — a plain count, real whenever the plane
  is reachable.
- **The money figure is a derived estimate**, stored as `cost_usd_estimated` and
  rendered with the word "estimated," plus a pricing-snapshot date whenever that
  date is known.
- **A missing or stale cost figure is shown as unavailable, never `$0.00`.** A
  confident-looking zero next to a dead control plane would read as "this pod cost
  nothing," which is worse than admitting Tack doesn't currently know.

`budget_usd` — the cap you set on a linked project — is the one dollar figure on
these pages that is **not** an estimate. It's a number you typed in, so it's shown
plainly, never suffixed "estimated."

If you're building anything that consumes these numbers, carry the same discipline:
never present `cost_usd_estimated` as if it were an invoiced amount.

## What's still a placeholder

Honestly-reported gaps, not bugs:

- **`gateway` always reads `"unknown"`.** No persisted gateway column exists, and
  the reconciler doesn't poll or store one. Independent of Tack's own gap: docket's
  own `gateway_active()` is hardcoded to return `false` in the current docket
  version — there's no daemon gateway any more — so even a fully-wired poll would
  read `"inactive"` universally today.
- **`roster` is always `[]`.** No agent-roster table exists; the column is in the
  UI and the API for when one does.
- **`pricing_snapshot_at` is always `null`.** No pricing-snapshot mechanism exists
  anywhere in the codebase yet.
- **Docket's own per-agent `budgetUsd`/`costUsd`** (from `/status.json`) is fetched
  by the reconciler every poll but never persisted — only used transiently to
  compute plane health. The budget panel shows Tack's own token-derived estimate
  against your configured cap instead, not docket's own figure.

## Known gap: repeated Sprints-view UI bug

`getByRole('button', { name: 'Run sprint' })` has been observed resolving to more
than one element on the Sprints view across several test runs — live-reproduced, not
flake. Not yet root-caused; if "Run sprint" seems to appear
twice or a dry-run preview behaves oddly, this is a known, tracked issue, not
something wrong with your setup.

## See also

- [Local Integration Setup](orchestration-local-setup.md) — take this from a clean
  install to Tack driving a real local `docket serve`.
- [Configuration](configuration.md) — every `TACK_ORCH_*` environment variable.
- [Administration & Security](administration.md) — token handling for the API token
  and the cloud-backup secret key; the control-plane token follows the same
  write-only-over-the-API discipline. The docket Bearer token is now scrubbed from
  every `GET /api/backup` snapshot (nulled in place, row otherwise intact) — the same
  gap the S3 secret key had before its own exclusion shipped, now closed.
- [Roadmap](../roadmap.md) — the full Agent-Factory Control Center plan (phases
  33–38) and the multi-agent dispatch history behind it.
- For the internals — the reconciler, the `ControlPlane` trait, the dispatcher, and
  how to add a new control-plane backend — see the
  [developer guide](../developer/orchestration.md).

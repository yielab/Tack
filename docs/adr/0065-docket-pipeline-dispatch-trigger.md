# ADR 0065: The Docket pipeline-dispatch trigger gets a caller, its own token, and no claim on an item

**Decide:** approve wiring `ControlPlane::dispatch` — today a method that returns
`Disabled` unconditionally — to docket's real `POST /dispatch/{project}` route, and give
it the one thing it has never had: **a caller**. Approve that the caller is one operator
route, `POST /api/projects/{id}/orch-dispatch`, behind **its own privileged token**
(`TACK_ORCH_DISPATCH_TOKEN`, fail-closed when unset), with `tack orch dispatch` as its
in-tree client. Approve that this trigger **claims no Tack item**: it starts a docket
pipeline run for a linked project, writes no `orch_tasks` row of its own, and therefore
never enters the one-scheduling-owner contest ADR 0060 settled.

**Why now:** the method is a permanently-`Disabled` arm of a trait this tree already names
as its canonical overengineering example. Either it gets a caller or it gets deleted —
leaving it is precisely the "mechanism with no caller" defect `.claude/scope-discipline.md`
was written about. The gap is entirely on Tack's side: docket's route is real,
authenticated, and already live-verified.

**If you do nothing:** `dispatch` stays a typed dead end — a trait method every reader has
to discover is unreachable, one more reason the `ControlPlane` abstraction reads as
speculative, and the only docket capability Tack can see but not use.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | `DocketAdapter::dispatch` is implemented against `POST /dispatch/{project}` and returns docket's run id. | The route exists and is authenticated. Nothing about it is speculative — only Tack's side was missing. |
| 2 | Its caller is one operator route, `POST /api/projects/{id}/orch-dispatch`, which resolves the docket project through the existing `orch_links` row. | Tack's operator surface is project-scoped and the link already exists. A control-plane-scoped route would need a second way to name a docket project. |
| 3 | It carries **its own token**, `TACK_ORCH_DISPATCH_TOKEN`, distinct from `TACK_API_TOKEN`, **fail-closed when unset**. | It triggers paid model spend on a remote fleet. That is exactly the privileged-action posture `TACK_ORCH_APPROVAL_TOKEN` and `TACK_EXECUTION_DECISION_TOKEN` already set. |
| 4 | It stays behind `TACK_ORCH_ENABLE` (default off), like every other orch route. | It reaches the network. No new gate shape is invented. |
| 5 | **It claims no Tack item.** No `orch_tasks` row, no `execution_requests` row, no item id in the request; `decide_scheduling_owner` is not consulted and does not apply. | One scheduling owner is an *item-level* invariant. A project-level pipeline run cannot collide with it, and pretending it could would mean inventing an item id the caller never supplied. |
| 6 | The `variables` body is passed through as an opaque JSON object and **never logged**. | Logs carry ids only. Tack does not know what a docket pipeline's variables mean and must not invent validation for them. |
| 7 | The resulting run is ingested by the **existing** reconciler `/runs` poll — no new ingestion path, no new table. | It is an ordinary docket run the moment it exists. A second ingestion path is a second source of truth. |
| 8 | `tack orch dispatch <project>` is the route's in-tree caller. | Scope-discipline rule 1: a route whose only caller is a hypothetical operator's `curl` is still a mechanism without a caller. The CLI is the cheapest honest one, and `tack-cli` is already an HTTP-only client. |

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail for whoever implements or later audits one
of these calls; nothing above depends on anything below it.

---

- **Status:** proposed 2026-09-08.
- **Date:** 2026-09-08
- **Relationship to earlier ADRs:** does not supersede anything. **Extends ADR 0060**
  ("the Docket control plane stays a maintained, optional legacy bridge") by closing one of
  the gaps that decision left open. ADR 0060's disposition — maintained, optional,
  never the owner of a runner-v1 request — is unchanged in substance. ADR 0050's
  "runner-v1 is the plan of record" is unchanged: decision 5 keeps this trigger outside
  the scheduling contest entirely rather than giving Docket a new claim.
- **Wire contract:** `docs/contracts/runner-v1/` is **unchanged**. This trigger lives
  wholly on the legacy Docket bridge and touches no runner-v1 type, fixture or route.

## What is true today, measured

Every claim below was read from the tree on 2026-09-08, not assumed.

| Fact | Where |
|---|---|
| `ControlPlane::dispatch` returns `OrchError::Disabled` unconditionally | `crates/tack-orch/src/adapters/docket.rs`, module doc "Write methods" |
| The reason recorded for that is "no consumer in Tack yet", not a missing docket route | same |
| docket's route is real: `POST /dispatch/{project}`, bearer-authenticated | `../rack-cli/src/docket/serve.py`, `do_POST` |
| `enqueue_task`, `decide_approval` and `provision_pod` on the same adapter **are** implemented | `adapters/docket.rs` |
| A `pre_input` policy block arrives as HTTP 400 and maps to `OrchError::PolicyBlocked` | `adapters/docket.rs`, `parse_policy_block` |
| Two privileged actions already carry their own fail-closed token | `TACK_ORCH_APPROVAL_TOKEN`, `TACK_EXECUTION_DECISION_TOKEN` in `docs/CONFIG.md` |
| Orch routes 404 with `TACK_ORCH_ENABLE` unset; no reconciler spawns | `docs/CONFIG.md` line 35 |

## What this deliberately does not do

- **No UI.** The route's caller is the CLI. A settings panel for a project-level pipeline
  trigger is a separate decision with its own design, and building it here would widen a
  hardening pass into a feature.
- **No item linkage.** Decision 5 is a hard boundary, not a first step. A future card that
  wants a Docket pipeline run attached to a Tack item must re-open this ADR, because
  attaching one puts it back inside the one-scheduling-owner invariant.
- **No retry, schedule or cancel.** `DocketAdapter::capabilities` reports `cancel: false`;
  a dispatched pipeline run cannot be called back through this route, and the route must
  not imply otherwise.
- **No new ingestion.** Decision 7. If the reconciler does not already surface the run, that
  is a reconciler finding, not work for this route.

## The risk this accepts

A privileged token that triggers paid spend on a remote fleet is the highest-consequence
route the Docket bridge will have. Three things bound it: the token is separate and
fail-closed, so an operator token leak does not reach it; `TACK_ORCH_ENABLE` is off by
default, so an install that never opted into Docket has no such route at all; and decision
6 keeps the request body out of the logs, so the trigger cannot become a way to write
arbitrary text into operator-visible output.

The residual risk is a **correctly-authenticated operator dispatching the wrong project's
pipeline**, which costs money and cannot be cancelled (`cancel: false`). That is accepted
rather than mitigated: the mitigation would be a confirmation step, which belongs to the
caller, and `tack orch dispatch` is where it should live if it is ever wanted.

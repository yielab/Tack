# `crates/tack-api/src/handlers/provisioning.rs`

Moved out of the module preamble; trim or delete freely.

Provisioning flow: the end-to-end path from "I want a new product" to a
 Tack project wired to a live docket pod.

 `POST /api/templates/{id}/provision` — deliberately a **separate route**
 from the plain `POST /api/projects/from-template/{id}` rather
 than an extension of it, even though `router.rs`'s original placeholder
 comment suggested reusing that endpoint via a `provision_pod:
 true` body flag. Reasons for diverging from that suggestion, disclosed
 rather than silently overridden:

 1. **No response-shape change to a widely-used endpoint.** The plain
    endpoint returns a bare `Project` and at least one existing frontend
    call site (`features/templates/Templates.tsx`) reads `.id` straight
    off the response. Provisioning's response needs to carry pod +
    link information alongside the project — widening the existing
    shape (even additively) is more risk for zero benefit when a new
    route costs one line in `router.rs` and one in `openapi.rs`.
 2. **A cleaner privilege/gating story.** This route lives inside
    `orch_routes()`, so `require_orch_enabled` 404s it for free
    — the plain endpoint stays reachable with
    orchestration off, exactly as it always has. Folding provisioning
    into the same handler would have meant hand-rolling that same check
    inline instead of getting it from the router layer.

 The actual project-creation work is **not duplicated**: this module
 calls `handlers::templates::build_project_from_template` — the same
 internal function the plain endpoint calls — then goes on to provision
 a pod and write the `orch_links` row.

 # Rollback design

 Three systems, two of them external, one HTTP call that cannot be
 undone once it succeeds. Verified directly against
 `~/Sites/rack-cli/src/docket/serve.py::_handle_post_pods` and
 `core/pod_provisioning.py` before designing any of this — not against
 docket's `ROADMAP.md`, which does not reliably reflect what has shipped
 there.

 **The real `POST /pods` contract:**
 - Request: `{project, path, blueprint, pod, budget, verifyCmd}` — every
   field but `project` optional.
 - Success: `201 {"ok": true, "project", "blueprint", "members": [{"id",
   "role", "model"}]}`.
 - `401` — bad/missing Bearer token.
 - `400` — a validation failure docket catches *before touching
   anything*: unknown blueprint, invalid `verifyCmd` (NUL/newline/too
   long), a `pod` value other than `"full"`, a missing `project`.
 - `409` — `PodAlreadyExistsError`, raised **before anything is
   touched** (`pod_member_ids(project)` is checked first, before even the
   blueprint name is resolved) — docket's own "skip, don't clobber"
   idempotence contract.
 - `500` — `PodProvisionError`. Critically, **docket's own module
   docstring states this is raised only *after* rollback has already
   run**: `provision_members` tears down every member (and any
   pod-level port range / scratch dir) created during *that* failing
   call before raising. So by the time Tack ever sees a non-2xx from
   this route, **docket guarantees nothing was left behind on its
   side** — every failure mode is atomic (fully created or nothing
   created), the one exception being the ordinary "your request was
   already satisfied" case (409).

 **What follows from that:** the only resource this flow can ever leave
 half-created is Tack's *own* side (a project row) — docket's side is
 either "pod exists" or "pod doesn't exist," never "pod half exists."
 And **docket has no HTTP route to delete/un-provision a pod at all**
 (confirmed by reading every `do_GET`/`do_POST` branch in `serve.py` —
 there is no `do_DELETE`, no `/pods/{id}` route of any method). So the
 one irreversible step in this whole flow is a *successful* `POST
 /pods` call — everything before it is cheap to undo, and nothing after
 it can ever be undone through this API.

 That fixes the ordering this module uses, deliberately:

 1. **Create the Tack project first.** Cheap, local, fully reversible
    (`Repository::delete_project`).
 2. **Validate everything provisioning needs** (the referenced control
    plane exists, `status_map` names real statuses in the *project's own*
    workflow, `pod_shape` is well-formed) — still before any call to
    docket. Any failure here rolls the project back.
 3. **Call `POST /pods`.** Any failure here (400/401/409/500) means, per
    the contract above, **nothing new exists on docket's side** — roll
    the project back too, and say so explicitly in the error.
 4. **Write `orch_links`.** This is the one step that runs *after* the
    irreversible action succeeded. A failure here is **not** treated as
    a request failure and the project is **never** deleted at this
    point — deleting it would strictly worsen things: the project row is
    now the *only* record Tack has that this pod exists at all, and
    docket cannot be asked to remove it. Instead this handler returns a
    normal `200` whose `provisioning` field is
    [`ProvisioningOutcome::PodCreatedLinkFailed`], naming the exact
    control plane + remote project the operator now owns and pointing
    at the existing manual-link UI
    (`features/settings/orchestration/LinkForm.tsx`) to finish the job —
    which only needs a `PUT /orch-link` call, never a second `POST
    /pods`.

 **What this module deliberately does not attempt:** retrying a failed
 `orch_links` write automatically, or inventing a way to "adopt" a 409
 (an already-existing remote project name) as this attempt's own pod.
 Both would require Tack to track cross-request provisioning state it
 has nowhere reliable to put; a 409 is treated as a hard
 failure (project rolled back, operator told to pick a different remote
 project name or use the existing manual-link flow if they know the
 existing pod is theirs).

 # Privilege — deliberately *not* gated behind `TACK_ORCH_APPROVAL_TOKEN`

 The separate approval-decision credential exists for one specific
 reason: *overriding a guardrail policy's deliberate block* is a
 categorically different, narrower privilege than "using the
 orchestration API at all" — its safe default had to be "nothing can
 release a gated action" precisely because that action is a human
 override of a considered "no."

 Provisioning is consequential (it creates real infrastructure and can
 spend real budget) but it is not that kind of override — it is ordinary
 use of the same privilege class as manual dispatch
 (`POST /items/{id}/dispatch`) and sprint-wide dispatch
 (`POST /sprints/{id}/dispatch`), both of which can also spend
 real budget across many items in one call and are gated only by the
 ordinary `TACK_API_TOKEN` + `TACK_ORCH_ENABLE` pair. Requiring a second
 credential *only* for provisioning, while sprint-wide dispatch needs
 none, would be an inconsistent privilege boundary, not a more careful
 one. The "require confirmation" instruction is met on the frontend
 instead (`frontend/src/features/provisioning/ProvisioningWizard.tsx`):
 a dedicated confirmation step naming the real docket project name,
 blueprint, and budget cap, with no single-click path from "open the
 wizard" to "a pod exists" — the same non-reversible-action pattern used
 elsewhere for approval decisions and sprint dispatch.

# `crates/tack-orch/src/adapters/docket.rs`

Moved out of the module preamble; trim or delete freely.

`DocketAdapter` — the [`ControlPlane`] implementation for docket.

 # Constructor

 ```ignore
 let adapter = DocketAdapter::new("http://127.0.0.1:7331", Some(token))?;
 let plane: std::sync::Arc<dyn tack_orch::ControlPlane> = std::sync::Arc::new(adapter);
 ```

 `new` takes the docket base URL (any trailing slash is normalized away)
 and an optional Bearer token. A `None` token is a legitimate
 configuration — every unauthenticated route (`/health`, `/status.json`,
 `/metrics`) still works, and calling an authenticated route without one
 degrades to whatever docket itself returns for a missing
 `Authorization` header (a real 401, mapped to [`OrchError::Auth`]) rather
 than being special-cased client-side, since that is both simpler and
 exactly what a caller would observe by hand.

 # Auth split

 `/status.json`, `/metrics`, and `/health` never carry
 a Bearer token, even if one is configured — every other route
 (`/runs`, `/runs/{id}`, `/approvals`, `/tasks/{project}`,
 `/traces/{project}`, plus the write routes) does. This is
 enforced structurally by [`DocketAdapter::get_unauthed`] vs.
 [`DocketAdapter::get_authed`] never sharing a code path that attaches
 the header — a future edit can't accidentally leak the token onto an
 unauthenticated request by adding one branch to a shared function.

 # Write methods

 **`enqueue_task`, `decide_approval`, `provision_pod`, and `dispatch` are
 all implemented** — see below. [`ControlPlane::dispatch`] POSTs to
 `POST /dispatch/{project}`, a distinct pipeline-run trigger (body =
 arbitrary `variables`) from `enqueue_task`'s pod-queue route, and returns
 docket's own run id. Docket hands that id back before the pipeline
 itself actually runs — the dispatch (including any `pre_input` guardrail
 evaluation inside it) executes afterwards, off the request thread — so a
 `block` verdict is never observable as this method's `Err` the way it is
 for `enqueue_task`; it can only surface later, as the run's own failed
 state. See [`DocketAdapter::dispatch`]'s own doc comment for detail.

 `decide_approval`'s implementation sends a fixed `channel: "tack"` on
 every decision (verified against `approval.APPROVAL_CHANNELS` in
 docket's `core/approval.py`, which already lists
 `"tack"` alongside `cli`/`http`/`mcp`/`telegram`/`timeout`) — see the
 trait doc for why this isn't a parameter. The **separate**
 `TACK_ORCH_APPROVAL_TOKEN` gate sits one layer up, in
 `tack-api`'s HTTP handler — this adapter has no concept of it, the same
 way it has no concept of Tack's ordinary `TACK_API_TOKEN`.

 `enqueue_task`'s implementation deliberately does **not** parse
 `status`/`approvalToken` off `POST /tasks/{project}`'s response body,
 even though both are present on the wire (see "Verified live" below) —
 [`ControlPlane::enqueue_task`]'s signature (`Result<String, OrchError>`)
 has nowhere to carry them out, and widening it is a separate, larger
 change than this method needs (see the trait doc's own note on this).
 `tack-api`'s `dispatcher` module recovers that
 information with one follow-up call to the already-fully-implemented
 [`ControlPlane::list_tasks`], matching the just-created task by the id
 this method returns. See that module's doc comment for the full
 reasoning; this adapter only needs to get the id right.

 A `pre_input` policy **block** (HTTP 400) is mapped to
 [`OrchError::PolicyBlocked`] — the policy id is
 parsed out of docket's own error text
 (`"task rejected by guardrail policy '<id>' at enqueue: <message>"`) by
 [`parse_policy_block`].

 # Verified live against a real docket server

 Every route this adapter uses was exercised against a real, isolated
 `docket serve` instance — not just read from `serve.py`/`core/dispatch.py`
 source. First captured against docket `0.2.0b1`; **recaptured against
 `v0.2.0-beta.2`**, since `serve.py`, `core/dispatch.py`,
 `core/approval.py` and `core/policy.py` each grew by hundreds to
 thousands of lines between those two releases and nothing below was
 assumed to still hold. Every bullet states what changed, if anything, or
 that it was confirmed unchanged; a claim this crate could not re-run is
 marked as such rather than left to read as current.

 - **`POST /tasks/{project}`'s success response — confirmed unchanged at
   `v0.2.0-beta.2`.** `{"ok": true, "task": "<id>", "project": "...",
   "status": "pending"|"waiting_approval", "approvalToken"?: "..."}`, not
   `{"taskId": "..."}`. The task id is under the key `"task"`, and a
   `require_approval` verdict adds `"approvalToken"` alongside `"status":
   "waiting_approval"` rather than a separate response shape.
   [`NewRemoteTask`] (the request body this adapter would send) is
   unaffected — only the response this adapter doesn't parse yet (because
   [`ControlPlane::enqueue_task`] is disabled) differs from docket's own
   docs. Whoever wires this up must not deserialize a `taskId` field — it
   doesn't exist on the wire.
 - **The `pre_input` gate's three outcomes, and the `trusted` boundary —
   confirmed unchanged at `v0.2.0-beta.2`.** A `block` verdict returns
   HTTP 400 with `{"ok": false, "error": "task rejected by guardrail
   policy '<id>' at enqueue: <message>"}`; a `require_approval` verdict
   returns HTTP 200 with the task's real `status` (`"waiting_approval"`,
   never `"pending"`) and its `approvalToken`; and passing `trusted:
   false` explicitly in the request body genuinely flips a
   `prompt-injection`-id policy from silently skipped to evaluated — while
   omitting `trusted` entirely reproduces every existing caller's behavior
   (operator trust, the policy skipped) exactly as
   `core/dispatch.py::enqueue_task`'s docstring says. `core/dispatch.py`'s
   own module doc is explicit that `pre_input` is evaluated **once, at
   enqueue, and never re-evaluated** for a task already on the queue —
   confirmed by the fact that a `/dispatch/{project}` run against a task
   this gate had already passed never re-trips it (see the `dispatch`
   bullet below).
 - **`POST /approvals/{token}` — grant confirmed unchanged; `deny` and the
   409/404 split are now closed, both live.** Grant genuinely resumes a
   gated task (`waiting_approval` → `pending`, confirmed via a follow-up
   `GET /tasks/{project}`) and returns `{"ok": true, "token": "...",
   "state": "granted"}`. **`deny`, `ApprovalNoop` and the unknown-token
   404 were never captured live before this — all three now are:** `deny`
   returns `{"ok": true, "token": "...", "state": "denied"}` and
   terminalizes the gated task immediately with no agent turn spent
   (`reason: "approval denied"`); replaying a decision against an
   already-decided token returns `409 {"ok": false, "error": "Already
   granted: <token>"}` (the `ApprovalNoop` path `core/approval.py` raises);
   a genuinely unknown token 404s with `{"ok": false, "error": "Approval
   not found: <token>"}`. `decide_approval`'s classification below now
   matches a live capture on every branch it handles, not a source
   reading for two of the four.
 - **`POST /pods` — confirmed unchanged at `v0.2.0-beta.2`**, against an
   isolated `docket serve` (`DOCKET_HOME` pointed at a scratch dir,
   `~/.docket`'s mtime confirmed unchanged before and after): a fresh
   `POST /pods` returns `201 {"ok": true, "project", "blueprint",
   "members": [{"id", "role", "model"}]}` exactly as
   [`ProvisionedPod`]/[`ProvisionedPodMember`] model it; a second call for
   the same `project` returns `409 {"ok": false, "error": "'<project>'
   already exists"}`; an unknown blueprint, a missing `project`, and a
   `pod` value other than `"full"` each return `400` with a plain `{"ok":
   false, "error": "..."}` body (same shape [`ErrorBody`] already extracts
   for `enqueue_task`/`decide_approval`); a request with no
   `Authorization` header returns `401`. Ran this crate's own compiled
   [`DocketAdapter::provision_pod`] against the live server again (not
   just a hand-built `curl`), confirming the happy path and the 409 both
   still decode correctly end to end.
 - **`POST /dispatch/{project}` — verified live for the first time.** This
   method shipped with no server ever having answered it. Ran this crate's
   own compiled [`DocketAdapter::dispatch`] (and [`ControlPlane::get_run`]
   to observe the outcome) against the isolated server: `dispatch`
   returns the id under `"run"`, exactly as [`DispatchResponse`] decodes
   it, and a request with no `Authorization` header is rejected before
   anything is created. A missing body is treated as `{}` — a
   no-variables dispatch, not an error. Provisioning a real pod, enqueuing
   one task, then dispatching it with no provider credential configured
   anywhere the isolated server could reach (`ANTHROPIC_API_KEY` and
   every equivalent absent from its process environment) reproduces the
   whole path for zero cost: the response returns `{"ok": true, "run":
   "<id>", ...}` before the pipeline has done anything, and only a
   follow-up `GET /runs/{id}` shows the Lead's one hop failing with `no
   endpoint configured for model '<model>'` — a local resolution failure,
   never a rejected call to a real provider, confirmed by `costUsd`
   staying `0.0` on the task record throughout. **One assumption this
   crate carried into the Part turned out false: an unknown/unprovisioned
   project does not 404 on this route.** `/dispatch/{project}` never
   checks whether `project` has a pod before creating the run record —
   dispatching a project docket has never heard of returns the same `200`
   happy-path response, and the failure (`DispatchError: no pod found for
   '<project>'`) only surfaces on the same asynchronous `GET /runs/{id}`
   path. [`OrchError::NotFound`]'s mapping in `dispatch` below is
   defensive, not dead — docket returns `404` from other authenticated
   routes on this same server — but no server-side branch in
   `/dispatch/{project}` itself produces one; nothing here decodes it
   incorrectly, so no ownership widening was needed to record this.

   **ADR 0065's central claim — a guardrail block is not synchronously
   observable on this route — held, and this capture sharpens it.** No
   verdict of any kind reached the caller synchronously in either capture
   above (a hard local failure, not just a guardrail verdict): the run id
   is returned before the outcome exists, full stop. What this capture
   could **not** reproduce is a genuine `pre_input` **block** occurring
   inside a dispatch — and per `core/dispatch.py`'s own module doc (see
   the `pre_input` bullet above), it structurally cannot: that hook
   evaluates once, at a task's own enqueue, never again when the task is
   later dispatched. The one guardrail hook that *can* fire asynchronously
   on this route is `pre_output`, scanning a real hop's real output — and
   reproducing that live needs a real agent turn, which the no-credential
   constraint this capture depends on forecloses. Recorded as
   `not_measured`: a `pre_output` block on a real `/dispatch/{project}`
   run, blocked on a working (and therefore costed) provider credential.

 # `list_tasks` / `traces`

 Both routes exist in docket
 (`GET /tasks/{project}`, `GET /traces/{project}?since=`),
 verified directly against `serve.py`'s `do_GET`, not against
 docket's own `ROADMAP.md` (which still lists them `TODO` — a real
 staleness bug in that project's own docs). If either
 route 404s against a real docket instance today, that means the plane is
 running an older docket build, not that the endpoint is hypothetical —
 [`OrchError::NotFound`] still surfaces exactly as before so callers
 (`reconciler::poll_traces`) can tell "this plane doesn't have the
 capability yet" apart from a real outage ([`OrchError::Auth`]/
 [`OrchError::Http`] are unaffected by any of this).

 **`traces`'s wire-format trap, verified against `serve.py`'s
 `_traces_page`/`do_GET` directly:** the real
 response is `{"events": [...], "next": "<cursor>"}`, but `events` is an
 array of **raw JSON strings**, not parsed objects — `_traces_page`
 returns the verbatim JSONL lines `core.trace.export_lines` read off
 disk, and `do_GET` calls `json.dumps` on that list of strings without
 ever parsing them back into objects first. So every element of `events`
 must be JSON-decoded a *second* time to reach the real event record
 ([`TracesResponse`] below reflects this: `events: Vec<String>`, not
 `Vec<RemoteEvent>`). `next` is docket's own minted resume cursor
 (`serve.py`'s module comment above `_traces_page` documents the
 compound `"<ts>Z:<n>"` format and why a bare last-seen timestamp isn't
 enough) — this adapter reads `next` and
 returns it verbatim as [`TracesPage::next`], opaque to this crate. Do not
 reintroduce a client-side reconstruction of docket's cursor algorithm —
 see `reconciler.rs`'s module doc for why one was tried and removed.

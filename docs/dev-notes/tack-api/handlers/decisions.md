# `crates/tack-api/src/handlers/decisions.rs`

Moved out of the module preamble; trim or delete freely.

Operator decision-resolution repository/service/handler
 module. Registered in `handlers.rs` and merged into the operator router
 in `router.rs`'s `operator_execution_routes` — **before** the
 `require_token` layer is applied and **with** `inject_operator_principal`
 layered directly on top, exactly like every other route that function
 merges.

 # `TACK_EXECUTION_DECISION_TOKEN`

 [`require_decision_token`] mirrors
 `handlers::orch::require_approval_token` exactly, closing a real
 contract-vs-implementation gap:
 `docs/contracts/runner-v1/protocol.json` names decision
 resolution a `"separately_scoped_operator_credential"` (distinct from the
 plain `operator_session_or_api_token` every other operator route uses),
 and `errors/forbidden.json`'s frozen example carries
 `"required_scope":"operator:decisions"`. `TACK_EXECUTION_DECISION_TOKEN`
 is that second, independent credential — checked here, on top of (not
 instead of) the `x-tack-principal` check below, fail-closed when unset
 exactly like `TACK_ORCH_APPROVAL_TOKEN`. See `require_decision_token`'s
 own doc comment for the full rationale and `CLAUDE.md`'s config table for
 the environment variable.

 # Security boundary: runner may raise/read, never resolve

 This module reads exactly one identity signal: the `x-tack-principal`
 header (see [`principal`]). It never reads `Authorization` at all — no
 code path here can authenticate, or even inspect, a runner bearer
 credential. That is the entire enforcement mechanism for "a runner may
 raise and read its own attempt's decision (`POST .../decisions`, `POST
 .../decisions/poll`, both in `handlers/runner_protocol.rs`) but
 never resolve it": resolution lives on a structurally separate route
 family (mounted on `/api` behind `require_token`, a sibling of
 `/api/runner/v1` exactly as `CLAUDE.md`'s "Two authentication surfaces,
 separated structurally" describes for every other operator/runner pair),
 not an exemption entry on the runner surface, and the runner credential
 carries zero privilege here even if presented — proven in
 `crates/tack-api/tests/runner_protocol/decisions.rs`'s
 `self_resolution_via_a_valid_runner_bearer_credential_is_denied_and_writes_nothing`
 test.

 `docs/contracts/runner-v1/protocol.json`'s `authentication` block names
 `decision_resolution` a "separately_scoped_operator_credential" — distinct
 wording from the plain `operator_session_or_api_token` every other
 operator route uses, and `errors/forbidden.json`'s example carries
 `"required_scope":"operator:decisions"`. Tack's actual operator-auth model
 (`middleware::require_token`) is a single shared bearer token with no
 scope/claim system at all (see `middleware.rs`'s own
 `operator_principal_value` doc comment: "a single shared bearer token, not
 per-user sessions"). This route is mounted behind the same `require_token`
 gate every other operator route uses — the stricter scoped-credential
 reading the contract describes remains an open contract-vs-implementation
 gap, not silently resolved.

 # No item-status mapping

 `execution_requests.status_map_policy_id` (migration 044) is a bare
 nullable `TEXT` column with **zero interpreter anywhere in this
 codebase** — grep confirms it is threaded verbatim through every layer
 (CLI args, request snapshot, DB column) and never once read back to
 decide anything. Nothing defines what a policy id resolves to: which
 decision kinds/answers map to which item statuses, or even what shape a
 "policy" is. Inventing that mapping now would mean fabricating an
 unrequested, uncontracted format.
 This module therefore treats "status mapping only
 after commit through the workflow engine" as a **structural
 guarantee with nothing to hang a policy off of yet**: no function in this
 file ever writes `items.status`, directly or indirectly, full stop —
 `resolve` never touches the `items` table at all, and
 `crates/tack-api/tests/runner_protocol/decisions.rs`'s own expiry tests
 assert the item's status is unchanged across both the fail-closed-conflict
 and bulk-sweep paths. Wiring a
 real mapping is future work that first needs a policy schema/format
 decision from whoever owns `status_map_policy_id`'s contract.

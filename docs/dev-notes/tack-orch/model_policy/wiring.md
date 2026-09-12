# `crates/tack-orch/src/model_policy/wiring.rs`

Moved out of the module preamble; trim or delete freely.

Live wiring between the pure [`super::resolve_model_policy`] and real
 `agent_profiles.limits` / `agent_fleets.default_policy` rows.

 # Where each tier's data actually lives today

 - **Request override**: `execution_requests.requested_model_provider`/
   `requested_model_id` (migration 044) — already read directly by the
   caller (there is nothing for this module to fetch).
 - **Agent profile default**: [`parse_model_default_convention`] reads an
   optional `{"default_model": {"provider": ..., "model_id": ...}}` (or
   `{"default_model": "auto"}`) key out of `agent_profiles.limits`
   (migration 042) — a JSON blob already fully operator-settable via
   `POST /api/agent-profiles`' existing `limits` field
   (`crates/tack-api/src/handlers/runner_admin.rs`). No schema change.
 - **Fleet default**: the same convention, read out of
   `agent_fleets.default_policy` (migration 039) — likewise already
   operator-settable via `POST /api/runner-fleets`.
 - **Project default**: `projects.default_model` (migration 062) — the
   exact JSON serialization of a `tack_core::models::ProjectModelDefault`,
   set via `PATCH /api/projects/{id}`. Unlike the two tiers above, this
   column is never an untyped, unenforced convention: the API's JSON
   extractor deserializes a request body directly into that typed enum,
   so [`parse_project_default_model`] never needs to treat a malformed
   shape as "no opinion" the way [`parse_model_default_convention`] does
   for an opaque `limits`/`default_policy` blob — a decode failure here
   means the column holds something no write path produced, and is
   reported as a real error instead.

 This mirrors `crate::scheduler::wiring`'s own established shape
 (`priority_from_metadata` reading a documented, non-binding convention
 out of `execution_requests.metadata` because no real `priority` column
 exists) — a documented stopgap, not a second frozen contract.

 # Capability intersection before claim

 This module does not itself check a resolved model against any runner's
 declared capability — that check already exists, untouched, in
 `crate::scheduler::select::select_runner` and is wired to
 live data by `crate::scheduler::wiring::choose_request_for_runner`.
 Once a [`ResolvedModelPolicy`](super::ResolvedModelPolicy)'s
 selector is persisted as an `execution_requests` row's
 `requested_model_provider`/`requested_model_id` (or left `NULL` for
 `AutoSelect`), the existing, unmodified claim path enforces "unavailable
 choice never leases" automatically — proven end-to-end, using only
 already-existing repository methods, in
 `crates/tack-orch/tests/scheduling/policy.rs`.

# `crates/tack-api/src/handlers/orch.rs`

Moved out of the module preamble; trim or delete freely.

Control-plane link API: register docket
 control planes, link a Tack project to one, and read the Fleet view's
 aggregate.

 **Off by default, toggleable from the UI.** Every route in this module is
 gated behind the *effective* orchestration setting via
 [`require_orch_enabled`], applied once as a layer on the orch sub-router
 in `router.rs` rather than repeated per-handler. The effective value is
 an `app_meta`-stored flag (editable at runtime via
 `GET`/`PUT /api/settings/orchestration`, `handlers/settings.rs`'s
 [`effective_orch_enabled`](crate::handlers::settings::effective_orch_enabled)),
 falling back to `TACK_ORCH_ENABLE` as a deployment default when the UI has
 never set one — mirroring the Cloud Backup precedent exactly. With
 orchestration disabled, every
 route here returns `409 Conflict` with a stable `error.code:
 "orchestration_disabled"` and a message naming where to enable it — not a
 404. A 404 made "disabled" indistinguishable from "route doesn't exist",
 which hid the feature from its own operator. This is not a security
 boundary being removed (the Bearer-token gate and
 the separate `TACK_ORCH_APPROVAL_TOKEN` check are unchanged).

 **Token discipline** mirrors the S3 backup secret precedent
 (`handlers/settings.rs`'s `secret_key_set`): the docket Bearer token is
 write-only over this API. [`ControlPlaneResponse`] never carries it — only
 `token_set: bool`. A `PATCH` with the `token` field **absent** leaves the
 stored token untouched; an explicit `"token": null` clears it; a string sets
 or replaces it. See [`UpdateControlPlaneRequest`] and [`deserialize_some`].

 **A control-plane failure never fails a user request here.** Every handler
 reads Tack's own database, populated out-of-band by the reconciler
 (`tack-orch`) — a docket outage can only leave `health`/`last_seen_at` stale,
 never turn into a 500 on a user's request.

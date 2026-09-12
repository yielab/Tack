# `crates/tack-runner/src/transport.rs`

Moved out of the module preamble; trim or delete freely.

The real HTTP transport for runner protocol v1.

 Without this module, [`crate::UnavailableProtocolClient`] is the only
 production [`RunnerProtocolClient`] in the tree and `reqwest` is not
 a dependency of this crate — so a packaged `tack-runner` binary could not
 enroll, claim, heartbeat or report against a live server.

 Two seams live here and they are deliberately separate:

 - [`HttpPullProtocol`] implements [`PullProtocol`] (the eight engine-facing
   operations) **and** [`AttemptDataProtocol`] (events, decisions,
   decision polling, artifact manifests, artifact content). Together they
   cover all fourteen `/api/runner/v1` routes.
 - [`HttpRunnerClient`] implements [`RunnerProtocolClient`]: the daemon
   loop that enrolls (or resumes a persisted session), replays unresolved
   journal records, then claims and heartbeats until shutdown.

 ## Authority

 `docs/contracts/runner-v1/` is the authority for every payload here. Where
 a response body and a fixture could disagree this module follows the
 fixture; the one place the wire carries information no fixture fixes — the
 artifact content upload URL — it follows the server's own
 `upload.path`/`upload.method` rather than reconstructing a path, because
 `artifact.response.json` records that grant *as data*.

 ## Retry discipline

 Two independent conditions must both hold before anything is resent:

 1. the failure is retryable — delegated to
    [`ProtocolClientError::is_retryable`], itself derived from
    `StableErrorCode::retryable` and thus from `errors/*.json`; and
 2. the operation is **replayable by construction** — its payload carries
    an idempotency key (`claim_request_id`, `heartbeat_id`, `completion_id`,
    `cancellation_request_id`, `recovery_key`, `decision_id`, `artifact_id`,
    an event `checkpoint`) or it is a pure read.

 [`Idempotency::SingleUse`] marks the one operation that fails both tests:
 **enrollment**. Its token is redeemed exactly once server-side, so a
 response lost in transit leaves an ambiguous state in which the server may
 hold a credential the runner never received. Resending would burn a token
 and could not recover the credential anyway. It is reported as a typed
 transport failure instead — the same "never blind-retry an ambiguous
 post-spawn state" principle, applied here to credentials instead of
 process spawning.

 ## Secrets

 The enrollment token travels only in the enrollment request body; the
 runner credential travels only in an `Authorization: Bearer` header. Neither
 is ever logged, put in an error, or included in a `Debug` rendering —
 [`RunnerCredential`] and [`crate::EnrollmentCredential`] redact
 structurally, and this module never calls `expose()` outside the exact
 place the byte is written onto the wire. `secrets_never_appear_in_logs_or_errors`
 asserts it.

# `crates/tack-api/src/handlers/runner_protocol/artifact_download.rs`

Moved out of the module preamble; trim or delete freely.

Operator-facing verified-artifact content download.

 This handler is **not** part of `runner_protocol`'s own `routes()` — that
 router is deliberately runner-credential-only and sits structurally
 outside `require_token` (see `router.rs#runner_protocol_routes`'s own doc
 comment). An operator download instead lives under the operator `/api`
 surface: `router.rs#operator_execution_routes` mounts [`routes`] as
 `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`,
 sharing the `TACK_STORAGE_DIR`-derived artifact root with
 `runner_protocol_routes`. `crates/tack-api/tests/wiring/artifact.rs`
 proves the mount through the real `build_router` and was verified
 load-bearing by unmounting it and watching the test 404.

 Nested under `runner_protocol/` (a submodule of an already-registered
 file) purely so it is reachable without touching `handlers/mod.rs` — see
 `runner_protocol.rs`'s own `mod artifact_download;` comment. It is not
 part of the runner protocol itself; `principal()` below reads
 `x-tack-principal`, the *operator* auth header (mirroring
 `executions.rs`'s own `principal()`), never a runner bearer credential.

 Streams the file back chunk-by-chunk (`futures::stream::unfold` over a
 `tokio::fs::File`, no whole-file read into memory) — the read-side half
 of the streaming design `artifact_storage.rs` uses on the write side.

 Module-level `dead_code` allow: every item here is reachable in
 production (mounted via `router.rs#operator_execution_routes`) and
 exercised directly through `runner_protocol/artifact_events.rs`'s own
 `#[path]`-loaded copy of `runner_protocol.rs` (which loads this file the
 same way). `runner_protocol/lifecycle.rs`, its sibling module in the same
 test binary, loads a second, independent copy of that same tree via its
 own `#[path]` (see each file's `#[allow(clippy::duplicate_mod)]`) and
 never calls into this module from it — the two `#[path]` copies are
 distinct items, so `lifecycle`'s copy alone would otherwise flag every
 item here as unused. This mirrors the same precedent in
 `runner_protocol.rs` itself (`RunnerV1ErrorEnvelope` in `executions.rs`,
 and the individually-annotated `Limits` fields).

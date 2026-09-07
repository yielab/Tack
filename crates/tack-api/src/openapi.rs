//! Machine-generated OpenAPI 3.1 contract for the Tack HTTP API.
//!
//! The document is assembled at compile time by `utoipa` from the
//! `#[utoipa::path(...)]` annotations on the handlers plus the `#[derive(ToSchema)]`
//! DTOs in `tack-core` (behind its `openapi` feature) and the local response
//! envelopes below. It is served verbatim at `GET /api/openapi.json` and a copy
//! is committed to `docs/openapi.json`; a drift-gate test
//! (`tests/openapi_contract.rs`) fails CI if the two diverge.
//!
//! ## Known imprecise / manual spots
//! - **`Json<serde_json::Value>` handlers.** Most handlers return untyped
//!   JSON rather than a concrete DTO. Where a concrete DTO exists it is declared as
//!   the response `body`; where the JSON is genuinely ad-hoc — `{"deleted": true}`
//!   / `{"updated": true}`, import counters, backup manifests, the masked backup
//!   settings view — the response is modelled as a free-form `Object`. Those are
//!   accurate about *what the endpoint returns today*, not aspirational.
//! - **Multipart upload.** `POST /api/items/{item_id}/attachments` takes
//!   `multipart/form-data`; utoipa cannot infer that from the `Multipart`
//!   extractor, so its request body is hand-declared.
//! - **Undocumented on purpose.** The WebSocket upgrade
//!   `GET /api/projects/{id}/boards/live` and the SPA fallback are omitted.
//! - **Untagged enum variants.** `ItemType::Custom(String)`,
//!   `EstimateUnit::Custom(String)` and `BoardGrouping::CustomField(Uuid)` are
//!   externally-tagged, so they render as a `oneOf` mixing bare strings with
//!   single-key objects — faithful to the serde output but noisier than a plain
//!   string enum.

use serde::Serialize;
use utoipa::openapi::header::HeaderBuilder;
use utoipa::openapi::path::{
    HttpMethod, OperationBuilder, Parameter, ParameterBuilder, ParameterIn, PathItem, PathsBuilder,
};
use utoipa::openapi::request_body::RequestBodyBuilder;
use utoipa::openapi::response::ResponseBuilder;
use utoipa::openapi::schema::{KnownFormat, SchemaFormat, SchemaType, Type};
use utoipa::openapi::{
    ContentBuilder, Info, ObjectBuilder, Ref, RefOr, Required, Response, Schema,
};
use utoipa::{OpenApi, PartialSchema, ToSchema};

use tack_core::models::{
    Attachment, Board, BoardColumn, BoardGrouping, BoardView, Comment, CommentType, CreateBoard,
    CreateComment, CreateCustomField, CreateDependency, CreateItem, CreateProject,
    CreateProjectTemplate, CreateRole, CreateSprint, CustomFieldDefinition, CustomFieldType,
    CustomFieldValue, Dependency, DependencyType, EstimateUnit, Item, ItemRole, ItemSource,
    ItemType, OrchBlueprint, Priority, Project, ProjectTemplate, ProjectType, Role,
    SetCustomFieldValue, Sprint, SprintStatus, TemplateOrchestration, TemplateStatusMap,
    UpdateBoard, UpdateCustomField, UpdateItem, UpdateProject, Workspace,
};
use tack_core::workflow::{StatusCategory, StatusDef, Transition, WorkflowConfig, WorkflowType};

use crate::handlers;

/// Structured error envelope returned by every failing endpoint — see
/// `crate::error::ApiError`. The body is always `{ "error": { status, message } }`.
#[derive(Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    /// HTTP status code, duplicated in the body for convenience.
    #[schema(example = 404)]
    pub status: u16,
    /// Human-readable, end-user-facing message.
    #[schema(example = "Item not found")]
    pub message: String,
    /// Stable, machine-readable error code. Present on a narrow set of
    /// responses where a caller needs to branch on *why* without parsing
    /// `message` — e.g. `orchestration_disabled` on the 409 every
    /// orchestration route returns while the feature is switched off (see
    /// `handlers::orch::require_orch_enabled`). Absent on ordinary errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "orchestration_disabled")]
    pub code: Option<String>,
}

/// Pagination envelope for the item-list endpoint. `total` is the
/// unpaginated match count so clients can render "N of M".
#[derive(Serialize, ToSchema)]
pub struct PaginatedItems {
    pub data: Vec<Item>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

/// Detail envelope for `GET /api/items/{id}` — the item plus its assigned roles
/// and dependency edges.
#[derive(Serialize, ToSchema)]
pub struct ItemDetail {
    pub item: Item,
    pub roles: Vec<Role>,
    pub dependencies: Vec<Dependency>,
}

// ─────────────────────────────────────────────────────────────────────────
// Runner protocol v1 + the operator decision/artifact-download routes.
//
// `handlers::runner_protocol` and the decision-resolution/
// artifact-download routes in `handlers::decisions`/
// `handlers::runner_protocol::artifact_download` return raw
// `Json<serde_json::Value>` with no `#[utoipa::path(...)]` annotation
// (unlike `handlers::executions`/`handlers::runner_admin`, which carry
// native annotations and are listed directly in `ApiDoc`'s `paths(...)`
// below). Building small `utoipa::OpenApi`-implementing document fragments
// here (`RunnerProtocolApiDoc`, `ExecutionOperatorExtrasApiDoc`) and
// composing them into `ApiDoc` via `#[openapi(nest(...))]` below is how
// this spec documents that route surface — `OpenApi::nest`'s
// path-prefixing is exactly what turns each fragment's relative paths
// (e.g. `/enroll`, `/attempts/{attempt_id}/decisions/{decision_id}/resolve`)
// into the real mounted paths (`/api/runner/v1/enroll`,
// `/api/attempts/{attempt_id}/decisions/{decision_id}/resolve`).
//
// Bodies use the same free-form `serde_json::Value` schema this file
// already uses for every other ad hoc JSON handler (see the module doc's
// "`Json<serde_json::Value>` handlers" note above) — this is *not* a
// second, hand-maintained shape for the runner-v1 wire format. That
// contract remains solely governed by `docs/contracts/runner-v1/`, never a
// hand-written feature DTO; the per-operation `description` below points
// back to it instead of re-specifying field shapes OpenAPI can't
// independently verify against the frozen fixtures. `x-tack-principal` is
// deliberately **not** documented as a request header anywhere below: it
// is stripped and server-injected
// (`crate::middleware::inject_operator_principal`), never something a
// caller may set, so documenting it as a settable input would misrepresent
// the security model.
// ─────────────────────────────────────────────────────────────────────────

fn json_value_schema() -> RefOr<Schema> {
    <serde_json::Value as PartialSchema>::schema()
}

fn json_content() -> utoipa::openapi::Content {
    ContentBuilder::new()
        .schema(Some(json_value_schema()))
        .build()
}

/// Every operation this is used by (`RunnerProtocolApiDoc`'s
/// runner-protocol-v1 exchanges, plus `ExecutionOperatorExtrasApiDoc` below)
/// returns `tack_orch::execution::ProtocolErrorEnvelope` on failure — never
/// `ErrorEnvelope`, the ordinary `{status, message, code?}` shape
/// `crate::error::ApiError` maps every plain `/api` handler's failure to.
/// `RunnerV1ErrorEnvelope`
/// (`handlers::executions`) already exists as the correct, doc-only mirror
/// of `ProtocolErrorEnvelope` — reused here rather than declaring a second
/// one.
fn error_envelope_content() -> utoipa::openapi::Content {
    ContentBuilder::new()
        .schema(Some(Ref::from_schema_name("RunnerV1ErrorEnvelope")))
        .build()
}

fn ok_response(description: &str) -> Response {
    ResponseBuilder::new()
        .description(description)
        .content("application/json", json_content())
        .build()
}

fn error_response(description: &str) -> Response {
    ResponseBuilder::new()
        .description(description)
        .content("application/json", error_envelope_content())
        .build()
}

fn string_path_param(name: &'static str, description: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .description(Some(description))
        .schema(Some(
            ObjectBuilder::new().schema_type(SchemaType::Type(Type::String)),
        ))
        .build()
}

/// Same as [`string_path_param`] but for a path segment that is numeric on
/// the wire (e.g. `attempt_number`) — never a stringly-typed id.
fn integer_path_param(name: &'static str, description: &str) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .description(Some(description))
        .schema(Some(
            ObjectBuilder::new().schema_type(SchemaType::Type(Type::Integer)),
        ))
        .build()
}

/// A documented request header parameter — e.g. the operator's
/// `x-tack-decision-token` or a runner's `x-tack-fencing-token`, both of
/// which travel out-of-band of the JSON/binary body.
fn header_param(name: &'static str, description: &str, required: bool) -> Parameter {
    ParameterBuilder::new()
        .name(name)
        .parameter_in(ParameterIn::Header)
        .required(if required {
            Required::True
        } else {
            Required::False
        })
        .description(Some(description))
        .schema(Some(
            ObjectBuilder::new().schema_type(SchemaType::Type(Type::String)),
        ))
        .build()
}

fn json_request_body(description: &str) -> utoipa::openapi::request_body::RequestBody {
    RequestBodyBuilder::new()
        .description(Some(description))
        .required(Some(Required::True))
        .content("application/json", json_content())
        .build()
}

/// A raw-bytes schema (`type: string, format: binary`) — the standard
/// OpenAPI convention for a non-JSON body. Used by both
/// `PUT .../artifacts/{artifact_id}/content`'s request (runner upload) and
/// `GET .../artifacts/{artifact_id}/content`'s response (operator
/// download); neither is JSON, so `json_content()` above does not apply.
fn binary_content() -> utoipa::openapi::Content {
    ContentBuilder::new()
        .schema(Some(RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Type(Type::String))
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
                .build(),
        ))))
        .build()
}

/// Standard error responses shared by both the operator and runner-v1
/// operations below — every stable v1 error code
/// (`docs/contracts/runner-v1/errors/*.json`) maps to one of these HTTP
/// statuses; `ok_description`/`request_body` are the only per-operation
/// specifics.
fn json_operation(
    tag: &str,
    summary: &str,
    description: &str,
    params: Vec<Parameter>,
    request_body: Option<&str>,
    ok_description: &str,
) -> OperationBuilder {
    let mut op = OperationBuilder::new()
        .tag(tag)
        .summary(Some(summary))
        .description(Some(description))
        .response("200", ok_response(ok_description))
        .response("400", error_response("invalid_request"))
        .response("401", error_response("unauthorized"))
        .response("403", error_response("forbidden / runner_revoked"))
        .response("404", error_response("not_found"))
        .response(
            "409",
            error_response("conflict / idempotency_conflict / invalid_transition / stale_lease"),
        );
    if !params.is_empty() {
        op = op.parameters(Some(params));
    }
    if let Some(desc) = request_body {
        op = op.request_body(Some(json_request_body(desc)));
    }
    op
}

// `handlers::executions`/`handlers::runner_admin`'s handlers each carry
// their own `#[utoipa::path(...)]` annotation referencing real,
// `ToSchema`-derived request/response DTOs, exactly like every other
// domain in this file (`handlers::orch`, `handlers::items`, …) — listed
// directly in `ApiDoc`'s `paths(...)`/`components(schemas(...))` below
// instead of through a separate nested fragment with every body typed as
// free-form JSON (`json_operation`'s `json_content()`), which would leave
// this domain's schemas empty (`{}`) in `docs/openapi.json`.

const RUNNER_TAG: &str = "runner-protocol-v1";
const RUNNER_PROTOCOL_NOTE: &str = "Authenticated by a hashed `Authorization: Bearer` runner \
    credential (`runner_bearer_credential` per docs/contracts/runner-v1/protocol.json) — never \
    the operator token, and never substitutable for it. Every field, limit and error shape is \
    frozen by docs/contracts/runner-v1/ (protocol.json, limits.json, lifecycle-transitions.json, \
    and this exchange's paired *.request.json/*.response.json fixtures); this document \
    deliberately does not re-specify them as a second, driftable shape.";

/// `PUT .../attempts/{attempt_id}/artifacts/{artifact_id}/content` —
/// the artifact content-upload route: mounted in `router.rs` and served in
/// production. Not part of the `op(...)` closure above:
/// its request is raw bytes, not JSON, and it carries a header parameter no
/// other exchange in this fragment needs. See
/// `handlers::runner_protocol::put_artifact_content` for the real
/// implementation this mirrors.
fn artifact_content_upload_operation() -> OperationBuilder {
    let params = vec![
        string_path_param("attempt_id", "Attempt ID, issued at claim time (opaque)"),
        string_path_param(
            "artifact_id",
            "Artifact ID from this attempt's prior manifest submission \
             (`POST .../artifacts`, opaque)",
        ),
        header_param(
            "x-tack-fencing-token",
            "The attempt's current fencing token. The request body is raw bytes, so — unlike \
             every other runner-protocol write — the fencing token cannot travel inside a JSON \
             body, so it travels as a header instead. docs/contracts/runner-v1/ fixes the \
             manifest exchange's payload shape, not this upload URL (see this fragment's own \
             doc comment).",
            true,
        ),
    ];
    OperationBuilder::new()
        .tag(RUNNER_TAG)
        .summary(Some(
            "Upload one manifested artifact's verified raw content",
        ))
        .description(Some(
            "Follows a successful manifest submission. Content is immutable once verified: a \
             second PUT for the same artifact_id is rejected (409 conflict) before consuming \
             any of its body. Bytes are streamed to storage and checked against the manifest's \
             declared size and sha256 before being committed — any mismatch \
             (artifact_checksum_mismatch) or interrupted stream discards the partial write, \
             never a partially-committed artifact. Rejected (409 conflict) unless the owning \
             attempt is currently `running` or `waiting_decision`.",
        ))
        .parameters(Some(params))
        .request_body(Some(
            RequestBodyBuilder::new()
                .description(Some(
                    "The artifact's raw bytes, matching the size/sha256 declared in its prior \
                     manifest entry. `Content-Type`, if set, must match the manifest's declared \
                     `media_type`.",
                ))
                .required(Some(Required::True))
                .content("application/octet-stream", binary_content())
                .build(),
        ))
        .response(
            "200",
            ok_response(
                "Content verified and committed: {protocol_version, attempt_id, artifact_id, \
                 state: \"content_verified\", size_bytes, sha256}",
            ),
        )
        .response(
            "400",
            error_response(
                "invalid_request (Content-Type mismatch, or the upload stream ended early)",
            ),
        )
        .response("401", error_response("unauthorized"))
        .response("403", error_response("forbidden / runner_revoked"))
        .response(
            "409",
            error_response(
                "conflict (content already recorded and is immutable; or the attempt is not \
                 currently running/waiting_decision) / artifact_checksum_mismatch / stale_lease",
            ),
        )
        .response(
            "413",
            error_response("payload_too_large (artifact_content_bytes_max)"),
        )
}

/// The runner-protocol v1 routes, documented relative to
/// `handlers::runner_protocol::routes` — nested at
/// `docs/contracts/runner-v1/protocol.json`'s `base_path`
/// (`/api/runner/v1`) below (see `router.rs::runner_protocol_routes`).
struct RunnerProtocolApiDoc;

impl OpenApi for RunnerProtocolApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        let attempt_id =
            || string_path_param("attempt_id", "Attempt ID, issued at claim time (opaque)");
        let op = |summary: &str, params: Vec<Parameter>, ok: &str| {
            json_operation(
                RUNNER_TAG,
                summary,
                RUNNER_PROTOCOL_NOTE,
                params,
                Some("protocol_version: 1, plus this exchange's fixture-frozen fields"),
                ok,
            )
        };
        let paths = PathsBuilder::new()
            .path(
                "/enroll",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Exchange a single-use enrollment token for a runner identity and bearer credential",
                        vec![],
                        "Runner enrolled; the raw bearer credential is returned exactly once",
                    ),
                ),
            )
            .path(
                "/refresh",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Refresh reported capabilities and optionally rotate the runner's bearer credential",
                        vec![],
                        "Capabilities accepted; a rotated credential, if requested, is returned exactly once",
                    ),
                ),
            )
            .path(
                "/claim",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Claim the next eligible execution request for this runner or its fleet",
                        vec![],
                        "A fenced lease and the immutable request snapshot, or no_eligible_work",
                    ),
                ),
            )
            .path(
                "/heartbeat",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report liveness, capacity, and active-attempt state in one fenced batch",
                        vec![],
                        "Renewed lease facts per reported attempt",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/accept",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt entering `preparing`", vec![attempt_id()], "Transition accepted or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/start",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt entering `running`", vec![attempt_id()], "Transition accepted or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/events",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Append a fenced, checkpointed batch of execution events",
                        vec![attempt_id()],
                        "Batch committed (accepted/duplicate event ids, committed checkpoint)",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/decisions",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Create a decision for later out-of-band operator resolution",
                        vec![attempt_id()],
                        "Decision recorded",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/decisions/poll",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Poll for decision resolutions since a given timestamp",
                        vec![attempt_id()],
                        "Resolved decisions since `after`, plus the new `next_after` cursor",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/artifacts",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Submit an artifact manifest (content upload is the separate \
                         PUT .../artifacts/{artifact_id}/content operation below; content \
                         download is a distinct, operator-facing route — see \
                         `execution-operator`'s \"Download a verified artifact's raw content\")",
                        vec![attempt_id()],
                        "Manifest accepted; per-artifact upload URLs issued",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/artifacts/{artifact_id}/content",
                PathItem::new(HttpMethod::Put, artifact_content_upload_operation()),
            )
            .path(
                "/attempts/{attempt_id}/completion",
                PathItem::new(
                    HttpMethod::Post,
                    op("Report the attempt's terminal outcome", vec![attempt_id()], "Completion committed or replayed"),
                ),
            )
            .path(
                "/attempts/{attempt_id}/cancellation-observation",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report the observed effect of a requested cancellation",
                        vec![attempt_id()],
                        "Cancellation observation committed or replayed",
                    ),
                ),
            )
            .path(
                "/attempts/{attempt_id}/recovery-observation",
                PathItem::new(
                    HttpMethod::Post,
                    op(
                        "Report a post-restart recovery observation for an attempt (additive v1 \
                         operation; exact path fixed by protocol.json)",
                        vec![attempt_id()],
                        "Recovery observation committed or replayed; server-authoritative disposition returned",
                    ),
                ),
            );
        utoipa::openapi::OpenApi::new(Info::new("runner-protocol-v1", "1"), paths)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Two operator-scoped routes (`handlers::decisions`,
// `handlers::runner_protocol::artifact_download`) mounted onto the real
// `/api` operator surface by
// `router.rs#operator_execution_routes` (see that function's own doc
// comment on the decision-resolve route's second, independent gate, and
// the artifact-download route's shared storage root), so — exactly like
// `RunnerProtocolApiDoc`
// above for its un-annotated `Json<Value>` handlers — this is a
// hand-built `OpenApi` fragment, not a `#[utoipa::path(...)]` annotation on
// the real handler function. Every request/response shape below is a
// schema-only mirror, hand-verified against `handlers::decisions::
// resolve_decision`/`ResolveDecisionResponse` and `handlers::
// runner_protocol::artifact_download::download_artifact_content`'s actual
// source — never generated from them, and never constructed by real code
// (see `RunnerV1ErrorEnvelope`'s doc comment above for the identical,
// already-established precedent).
// ─────────────────────────────────────────────────────────────────────────

/// Schema-only mirror of the `answer` object `POST
/// .../decisions/{decision_id}/resolve` both accepts (`handlers::decisions::
/// validate_answer`) and echoes back on success. `option_id` must be a
/// non-empty string; when the decision's own `options` list is non-empty,
/// resolution also requires `option_id` to be one of them (checked
/// server-side, not expressible in this schema). `text` may be omitted,
/// `null`, or a string — never anything else.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct DecisionAnswerSchema {
    pub option_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// `POST .../decisions/{decision_id}/resolve` request body.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct ResolveDecisionRequest {
    pub answer: DecisionAnswerSchema,
}

/// Schema-only mirror of `resolved_by`'s two observed shapes:
/// `{"kind": "operator", "subject_id": <x-tack-principal>}` for a live
/// resolve, or `{"kind": "system", "subject_id": "expiry"}` for a
/// fail-closed lazy expiry. Deliberately not a closed/tagged enum — nothing
/// in `docs/contracts/runner-v1/` fixes this shape (decision resolution has
/// no runner-v1 fixture at all; see `handlers::decisions`'s own module doc,
/// "No item-status mapping"), so this stays an open two-field object rather
/// than an invented contract.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct DecisionResolvedBySchema {
    #[schema(example = "operator")]
    pub kind: String,
    pub subject_id: String,
}

/// `POST .../decisions/{decision_id}/resolve` response body — mirrors
/// `handlers::decisions::ResolveDecisionResponse` exactly.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct ResolveDecisionResponseSchema {
    pub protocol_version: u32,
    pub decision_id: String,
    /// Always `"resolved"` on a 200 — an expired/not-found/conflicting
    /// decision is a distinct error response, never a 200 with a different
    /// state string.
    #[schema(example = "resolved")]
    pub state: String,
    pub answer: DecisionAnswerSchema,
    pub resolved_at: String,
    pub resolved_by: DecisionResolvedBySchema,
    /// `true` when this response is a byte-identical idempotent replay of
    /// an already-committed resolution rather than a fresh write.
    pub replayed: bool,
}

/// `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`.
/// See `handlers::decisions`'s own module doc for the
/// full security rationale this description summarizes.
fn resolve_decision_operation() -> OperationBuilder {
    let params = vec![
        string_path_param("attempt_id", "Attempt ID the decision belongs to (opaque)"),
        string_path_param(
            "decision_id",
            "Decision ID, scoped to `attempt_id`: a decision_id that exists but belongs to a \
             different attempt resolves as 404 not_found, indistinguishable from one that never \
             existed at all — an attacker guessing another attempt's decision_id learns nothing.",
        ),
        header_param(
            handlers::decisions::DECISION_TOKEN_HEADER,
            "TACK_EXECUTION_DECISION_TOKEN — a second, independent operator credential *on top \
             of* the ordinary operator auth every other `/api` route uses (never a substitute \
             for it). Fail-closed: every call is rejected with 403 whenever the server has not \
             configured TACK_EXECUTION_DECISION_TOKEN at all — there is no \"no secret \
             configured, allow everything\" fallback the way the plain Bearer gate has for an \
             unset TACK_API_TOKEN. Mirrors TACK_ORCH_APPROVAL_TOKEN exactly.",
            true,
        ),
    ];
    OperationBuilder::new()
        .tag("execution-operator")
        .summary(Some(
            "Resolve a pending decision with an operator-supplied answer",
        ))
        .description(Some(
            "Operator-only, and more tightly scoped than the rest of the operator surface. \
             Authenticates via the `x-tack-principal` header alone — this route never reads \
             `Authorization` at all, so a valid runner bearer credential (even one issued for \
             this exact attempt) cannot reach it; `handlers::decisions`'s own tests \
             (`self_resolution_is_denied_*`) prove a runner credential carries zero privilege \
             here even when presented. A runner may raise a decision and poll for its \
             resolution (`POST .../decisions`, `POST .../decisions/poll`, both under \
             `runner-protocol-v1`) but never resolve one itself. \
             \
             Idempotent: replaying the identical `answer` for an already-resolved decision \
             returns the prior resolution (`replayed: true`) rather than erroring; a *different* \
             `answer` is a 409 idempotency_conflict. A decision past its `expires_at` can never \
             be resolved (409 decision_expired) — including one that transitions from `pending` \
             to `expired` lazily on this very call, which still refuses the submitted answer. \
             This route never writes an item's status, directly or indirectly (see \
             `handlers::decisions`'s \"No item-status mapping\" section).",
        ))
        .parameters(Some(params))
        .request_body(Some(
            RequestBodyBuilder::new()
                .description(Some("The operator's answer to this pending decision."))
                .required(Some(Required::True))
                .content(
                    "application/json",
                    ContentBuilder::new()
                        .schema(Some(Ref::from_schema_name("ResolveDecisionRequest")))
                        .build(),
                )
                .build(),
        ))
        .response(
            "200",
            ResponseBuilder::new()
                .description(
                    "Decision resolved — either a fresh write or a byte-identical idempotent \
                     replay of one (`replayed` distinguishes the two).",
                )
                .content(
                    "application/json",
                    ContentBuilder::new()
                        .schema(Some(Ref::from_schema_name("ResolveDecisionResponseSchema")))
                        .build(),
                )
                .build(),
        )
        .response(
            "400",
            error_response(
                "invalid_request (missing/malformed answer, or answer.option_id is not one of \
                 this decision's own recorded options)",
            ),
        )
        .response(
            "401",
            error_response(
                "unauthorized — no x-tack-principal; a runner bearer credential never \
                 satisfies this, by construction",
            ),
        )
        .response(
            "403",
            error_response(
                "forbidden — x-tack-decision-token missing, unconfigured server-side, or \
                 mismatched (details.required_scope = \"operator:decisions\")",
            ),
        )
        .response(
            "404",
            error_response(
                "not_found — no decision exists for this exact (attempt_id, decision_id) pair",
            ),
        )
        .response(
            "409",
            error_response("decision_expired / idempotency_conflict"),
        )
        .response(
            "413",
            error_response(
                "payload_too_large (answer exceeds decision_answer_bytes_max, 32768 bytes)",
            ),
        )
}

/// `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`
fn download_artifact_content_operation() -> OperationBuilder {
    let params = vec![
        string_path_param("request_id", "Execution request ID (opaque)"),
        integer_path_param(
            "attempt_number",
            "1-based attempt number within the execution request",
        ),
        string_path_param(
            "artifact_id",
            "Artifact ID, scoped to the attempt that reported it (opaque)",
        ),
    ];
    OperationBuilder::new()
        .tag("execution-operator")
        .summary(Some("Download a verified artifact's raw content"))
        .description(Some(
            "Operator-only (`x-tack-principal`); never reachable via a runner bearer \
             credential — this route is mounted under the operator `/api` surface, not \
             `runner-protocol-v1`. Streams the stored bytes chunk-by-chunk (never buffers the \
             whole file in memory) and shares the same TACK_STORAGE_DIR-derived artifact root \
             as `runner-protocol-v1`'s own content-upload route.",
        ))
        .parameters(Some(params))
        .response(
            "200",
            ResponseBuilder::new()
                .description(
                    "The artifact's raw bytes. `Content-Type` is the artifact's declared \
                     `media_type`, or `application/octet-stream` when none was declared.",
                )
                .content("application/octet-stream", binary_content())
                .header(
                    "Content-Length",
                    HeaderBuilder::new()
                        .description(Some("Size of the artifact content, in bytes."))
                        .schema(ObjectBuilder::new().schema_type(SchemaType::Type(Type::Integer)))
                        .build(),
                )
                .header(
                    "Content-Disposition",
                    HeaderBuilder::new()
                        .description(Some(
                            "`attachment; filename=\"<name>\"` — falls back to a bare \
                             `attachment` if the artifact's own `name` cannot be encoded as a \
                             valid header value (e.g. contains control characters).",
                        ))
                        .schema(ObjectBuilder::new().schema_type(SchemaType::Type(Type::String)))
                        .build(),
                )
                .build(),
        )
        .response(
            "401",
            error_response("unauthorized — no authenticated operator principal"),
        )
        .response(
            "404",
            error_response("not_found (details.artifact_id) — no artifact manifest matches this (request_id, attempt_number, artifact_id) triple"),
        )
        .response(
            "409",
            error_response(
                "conflict (details.artifact_id) — the artifact manifest exists but its content \
                 has not been verified yet; distinct from not_found, never silently treated as \
                 \"gone\" or zero bytes",
            ),
        )
}

/// The decision-resolution route and operator artifact-download
/// route (see this section's own doc comment above), both nested at `/api`
/// below — the same base every ordinary operator route in `ApiDoc.paths(...)`
/// uses, since both are merged into the real `api` router *before*
/// `require_token` (`router.rs#operator_execution_routes`), not into the
/// runner-credential-only `/api/runner/v1` surface.
struct ExecutionOperatorExtrasApiDoc;

impl OpenApi for ExecutionOperatorExtrasApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        let paths = PathsBuilder::new()
            .path(
                "/attempts/{attempt_id}/decisions/{decision_id}/resolve",
                PathItem::new(HttpMethod::Post, resolve_decision_operation()),
            )
            .path(
                "/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content",
                PathItem::new(HttpMethod::Get, download_artifact_content_operation()),
            );
        utoipa::openapi::OpenApi::new(Info::new("execution-operator-extras", "1"), paths)
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tack API",
        description = "REST + WebSocket API for Tack, a lightweight, workflow-agnostic \
            project-management tool. This contract is generated from the Rust handlers \
            and domain models; it is the single source of truth for the wire format. \
            All failing responses share the `{ \"error\": { \"status\", \"message\" } }` \
            envelope, with an additional `code` field on a narrow set of responses \
            (e.g. `orchestration_disabled`) where a caller needs to branch on the \
            reason without parsing `message`.",
        license(name = "MIT", identifier = "MIT"),
        contact(name = "Tack", email = "info@yielab.com"),
    ),
    paths(
        // ── System / debug ────────────────────────────────────────────────
        crate::debug::health,
        crate::debug::debug_info,
        crate::debug::db_stats,
        // ── Projects ──────────────────────────────────────────────────────
        handlers::projects::create_project,
        handlers::projects::list_projects,
        handlers::projects::get_project,
        handlers::projects::update_project,
        handlers::projects::delete_project,
        // ── Export / import ───────────────────────────────────────────────
        handlers::export::export_project,
        handlers::export::import_project,
        handlers::export::import_csv,
        handlers::import_github::import_github,
        handlers::import_linear::import_linear,
        // ── Items ─────────────────────────────────────────────────────────
        handlers::items::create_item,
        handlers::items::list_items,
        handlers::items::get_item_tree,
        handlers::items::search_items,
        handlers::items::search_items_global,
        handlers::items::get_item,
        handlers::items::update_item,
        handlers::items::delete_item,
        // ── Sprints ───────────────────────────────────────────────────────
        handlers::sprints::create_sprint,
        handlers::sprints::list_sprints,
        handlers::sprints::get_sprint,
        handlers::sprints::update_sprint,
        handlers::sprints::update_sprint_status,
        // ── Roles ─────────────────────────────────────────────────────────
        handlers::roles::create_role,
        handlers::roles::list_roles,
        handlers::roles::delete_role,
        handlers::roles::assign_role,
        handlers::roles::remove_role,
        // ── Comments ──────────────────────────────────────────────────────
        handlers::comments::create_comment,
        handlers::comments::list_comments,
        // ── Dependencies ──────────────────────────────────────────────────
        handlers::dependencies::create_dependency,
        handlers::dependencies::list_dependencies,
        handlers::dependencies::delete_dependency,
        // ── Attachments ───────────────────────────────────────────────────
        handlers::attachments::upload_attachment,
        handlers::attachments::list_attachments,
        handlers::attachments::download_attachment,
        handlers::attachments::delete_attachment,
        // ── Templates ─────────────────────────────────────────────────────
        handlers::templates::create_template,
        handlers::templates::list_templates,
        handlers::templates::get_template,
        handlers::templates::delete_template,
        handlers::templates::create_project_from_template,
        handlers::templates::save_project_as_template,
        // ── Custom fields ─────────────────────────────────────────────────
        handlers::custom_fields::create_field,
        handlers::custom_fields::list_fields,
        handlers::custom_fields::get_field,
        handlers::custom_fields::update_field,
        handlers::custom_fields::delete_field,
        handlers::custom_fields::set_field_value,
        handlers::custom_fields::get_field_value,
        handlers::custom_fields::delete_field_value,
        handlers::custom_fields::get_all_field_values,
        // ── Boards ────────────────────────────────────────────────────────
        handlers::boards_multi::create_board,
        handlers::boards_multi::list_boards,
        handlers::boards_multi::get_board,
        handlers::boards_multi::update_board,
        handlers::boards_multi::delete_board,
        handlers::boards_multi::get_board_view,
        // ── Backup / restore ──────────────────────────────────────────────
        handlers::backup::get_backup,
        handlers::backup::post_restore,
        handlers::backup::post_remote_backup,
        handlers::backup::get_remote_backups,
        handlers::backup::post_remote_restore,
        handlers::backup::post_remote_verify,
        // ── Settings ──────────────────────────────────────────────────────
        handlers::settings::get_backup_settings,
        handlers::settings::put_backup_settings,
        handlers::settings::get_orch_settings,
        handlers::settings::put_orch_settings,
        // ── Embedded runner control (ADR 0061 decisions 2 and 6) ────────────
        handlers::local_runner::get_local_runner,
        handlers::local_runner::put_local_runner,
        handlers::local_runner::list_local_runner_secrets,
        handlers::local_runner::put_local_runner_secret,
        handlers::local_runner::delete_local_runner_secret,
        // ── Orchestration (Agent-Factory Control Center) ────────────────────
        handlers::orch::create_control_plane,
        handlers::orch::list_control_planes,
        handlers::orch::get_control_plane,
        handlers::orch::update_control_plane,
        handlers::orch::delete_control_plane,
        handlers::orch::get_orch_link,
        handlers::orch::put_orch_link,
        handlers::orch::get_fleet,
        handlers::orch::get_orch_budget,
        handlers::orch::get_metrics,
        handlers::orch::get_orch_policy,
        handlers::orch::get_item_agent_activity,
        handlers::orch::get_project_agent_activity,
        handlers::orch::dispatch_item,
        handlers::orch::dispatch_sprint,
        handlers::orch::dry_run_sprint_dispatch,
        handlers::orch::list_pending_approvals,
        handlers::orch::decide_approval,
        handlers::provisioning::create_project_with_pod,
        handlers::economics::get_economics_summary,
        handlers::economics::get_economics_items,
        // ── Harness-agnostic runner fleet: operator execution API ──────────
        handlers::executions::create_execution,
        handlers::executions::list_executions,
        handlers::executions::get_execution,
        handlers::executions::list_execution_attempts,
        handlers::executions::list_execution_attempt_events,
        handlers::attempt_lists::list_execution_attempt_artifacts,
        handlers::attempt_lists::list_execution_attempt_decisions,
        handlers::executions::request_cancellation,
        handlers::executions::requeue_needs_operator,
        // ── Harness-agnostic runner fleet: operator fleet/runner/profile API ──
        handlers::runner_admin::create_fleet,
        handlers::runner_admin::list_fleets,
        // Fleet-membership write route: populates
        // `agent_fleet_members`, the roster a fleet-selector scheduling
        // request resolves against.
        handlers::runner_admin::add_fleet_member,
        handlers::runner_admin::remove_fleet_member,
        handlers::runner_admin::list_runners,
        handlers::runner_admin::revoke_runner,
        handlers::runner_admin::create_pending_runner,
        handlers::runner_admin::revoke_enrollment_token,
        handlers::runner_admin::create_profile,
        handlers::runner_admin::list_profiles,
        handlers::runner_admin::create_model_profile,
        handlers::runner_admin::list_model_profiles,
    ),
    components(schemas(
        // Local response/request envelopes
        ErrorEnvelope,
        ErrorBody,
        handlers::executions::RunnerV1ErrorEnvelope,
        handlers::executions::RunnerV1Error,
        PaginatedItems,
        ItemDetail,
        handlers::boards_multi::BoardViewResponse,
        handlers::boards_multi::BoardColumnWithItems,
        tack_core::models::UpdateSprint,
        handlers::sprints::UpdateSprintStatus,
        handlers::templates::CreateProjectFromTemplate,
        handlers::templates::SaveAsTemplateRequest,
        handlers::import_github::GitHubImportRequest,
        handlers::import_linear::LinearImportRequest,
        handlers::backup::RestoreRemoteRequest,
        handlers::settings::UpdateBackupSettings,
        handlers::settings::UpdateOrchSettings,
        handlers::local_runner::UpdateLocalRunner,
        handlers::local_runner::SetLocalRunnerSecret,
        handlers::orch::ControlPlaneResponse,
        handlers::orch::CapabilitiesResponse,
        handlers::orch::SupportLevel,
        handlers::orch::EventScopeLevel,
        handlers::orch::DecisionSupportLevel,
        handlers::orch::UsageSupportLevel,
        handlers::orch::ModelSelectionLevel,
        handlers::orch::SupportCapability,
        handlers::orch::EventScopeCapability,
        handlers::orch::DecisionsCapability,
        handlers::orch::UsageCapability,
        handlers::orch::ModelSelectionCapability,
        handlers::orch::CreateControlPlaneRequest,
        handlers::orch::UpdateControlPlaneRequest,
        handlers::orch::OrchLinkResponse,
        handlers::orch::OrchLinkView,
        handlers::orch::UpsertOrchLinkRequest,
        handlers::orch::StatusMap,
        handlers::orch::FleetEntry,
        handlers::orch::FleetListResponse,
        handlers::orch::FleetRosterMember,
        handlers::orch::OrchBudgetResponse,
        handlers::orch::OrchPolicyResponse,
        handlers::orch::ToolCallEntry,
        handlers::orch::PolicyHitEntry,
        handlers::orch::ApprovalChannelEntry,
        handlers::orch::ItemAgentEventResponse,
        handlers::orch::ItemAgentRunResponse,
        handlers::orch::ItemAgentAttemptResponse,
        handlers::orch::ItemAgentApprovalResponse,
        handlers::orch::ItemAgentActivityResponse,
        handlers::orch::AgentBadgeRowResponse,
        handlers::orch::AgentBadgeResponse,
        handlers::orch::DispatchedTaskResponse,
        handlers::orch::DispatchItemResponse,
        handlers::orch::SprintDispatchItemResponse,
        handlers::orch::SprintDispatchSummary,
        handlers::orch::DryRunSprintDispatchResponse,
        handlers::orch::SprintDispatchResponse,
        handlers::orch::PendingApprovalResponse,
        handlers::orch::PendingApprovalListResponse,
        handlers::orch::ApprovalDecisionAction,
        handlers::orch::DecideApprovalRequest,
        handlers::orch::DecideApprovalResponse,
        handlers::provisioning::ProvisionPodRequest,
        handlers::provisioning::CreateProjectWithPodRequest,
        handlers::provisioning::ProvisionedPodMemberResponse,
        handlers::provisioning::ProvisioningOutcome,
        handlers::provisioning::CreateProjectWithPodResponse,
        handlers::economics::LeadTimeStat,
        handlers::economics::ReworkStat,
        handlers::economics::EconomicsSlice,
        handlers::economics::EconomicsSummaryResponse,
        handlers::economics::EconomicsPopulation,
        handlers::economics::EconomicsItemResponse,
        handlers::economics::EconomicsItemsResponse,
        // ── Harness-agnostic runner fleet: operator execution API DTOs ──────
        handlers::executions::CreateExecution,
        handlers::executions::CreateExecutionResponse,
        handlers::executions::ExecutionSummary,
        handlers::executions::ExecutionListResponse,
        handlers::executions::ExecutionDetailResponse,
        handlers::executions::AttemptSummary,
        handlers::executions::AttemptListResponse,
        handlers::executions::EventSummary,
        handlers::executions::EventListResponse,
        handlers::attempt_lists::ArtifactSummary,
        handlers::attempt_lists::ArtifactListResponse,
        handlers::attempt_lists::DecisionOptionSummary,
        handlers::attempt_lists::DecisionSummary,
        handlers::attempt_lists::DecisionListResponse,
        handlers::executions::CancellationRequestedResponse,
        handlers::executions::RecoveryConfirmation,
        handlers::executions::RequeueResponse,
        // ── AttemptSummary.model_provenance/usage_economics real shape
        // — schema-only mirrors of `tack_orch::
        // usage_provenance`, which has no `ToSchema`; see their doc
        // comments in `handlers::executions` for why they live there. ──────
        handlers::executions::ModelProvenanceSchema,
        handlers::executions::MeasurementSourceSchema,
        handlers::executions::UsdMeasurementSchema,
        handlers::executions::RunnerTimeCostSchema,
        handlers::executions::UsageEconomicsSchema,
        // ── Operator-scoped decision resolution — schema-only mirrors of
        // `handlers::decisions`; see `ExecutionOperatorExtrasApiDoc`'s
        // own doc comment above. ────────────────────────────────────────
        DecisionAnswerSchema,
        ResolveDecisionRequest,
        DecisionResolvedBySchema,
        ResolveDecisionResponseSchema,
        // ── Harness-agnostic runner fleet: operator fleet/runner/profile API
        // DTOs ───────────────────────────────────────────────────────────
        handlers::runner_admin::CreateFleet,
        handlers::runner_admin::CreateFleetResponse,
        handlers::runner_admin::FleetSummary,
        handlers::runner_admin::FleetListResponse,
        handlers::runner_admin::AddFleetMember,
        handlers::runner_admin::FleetMemberResponse,
        handlers::runner_admin::RunnerSummary,
        handlers::runner_admin::RunnerListResponse,
        handlers::runner_admin::RevokeRunnerResponse,
        handlers::runner_admin::CreatePendingRunner,
        handlers::runner_admin::CreatePendingRunnerResponse,
        handlers::runner_admin::RevokeEnrollmentTokenResponse,
        handlers::runner_admin::CreateProfile,
        handlers::runner_admin::CreateProfileResponse,
        handlers::runner_admin::AgentProfileSummary,
        handlers::runner_admin::AgentProfileListResponse,
        handlers::runner_admin::CreateModelProfile,
        handlers::runner_admin::CreateModelProfileResponse,
        handlers::runner_admin::ModelProfileSummary,
        handlers::runner_admin::ModelProfileListResponse,
        // Core domain models + DTOs
        Workspace,
        Project,
        ProjectType,
        Item,
        ItemType,
        ItemSource,
        Priority,
        EstimateUnit,
        Dependency,
        DependencyType,
        Role,
        ItemRole,
        Comment,
        CommentType,
        Attachment,
        Sprint,
        SprintStatus,
        BoardView,
        BoardColumn,
        Board,
        BoardGrouping,
        ProjectTemplate,
        TemplateOrchestration,
        TemplateStatusMap,
        OrchBlueprint,
        CustomFieldDefinition,
        CustomFieldType,
        CustomFieldValue,
        CreateProject,
        UpdateProject,
        CreateItem,
        UpdateItem,
        CreateSprint,
        CreateRole,
        CreateComment,
        CreateDependency,
        CreateProjectTemplate,
        CreateCustomField,
        UpdateCustomField,
        SetCustomFieldValue,
        CreateBoard,
        UpdateBoard,
        WorkflowConfig,
        WorkflowType,
        StatusDef,
        StatusCategory,
        Transition,
    )),
    tags(
        (name = "system", description = "Health and debug probes."),
        (name = "projects", description = "Projects: the top-level container for work."),
        (name = "items", description = "Items: the universal work unit (epics, tasks, bugs, …)."),
        (name = "sprints", description = "Sprints / iterations within a project."),
        (name = "roles", description = "Roles / specialties and their assignment to items."),
        (name = "comments", description = "Comments on items."),
        (name = "dependencies", description = "Directed dependency edges between items."),
        (name = "attachments", description = "File attachments on items."),
        (name = "boards", description = "Saved board views and their grouped item layout."),
        (name = "custom-fields", description = "Per-project custom field definitions and values."),
        (name = "templates", description = "Reusable project templates."),
        (name = "import", description = "Import from JSON/YAML/CSV, GitHub Issues, and Linear."),
        (name = "export", description = "Project export to JSON / YAML / CSV."),
        (name = "search", description = "Full-text search within a project or globally."),
        (name = "backup", description = "Local and S3-compatible cloud backup / restore."),
        (name = "settings", description = "Runtime-editable server settings (cloud backup)."),
        (name = "orchestration", description = "Agent-Factory Control Center: control-plane registration, \
            per-project links, and the Fleet view aggregate. Every route is disabled — 404 — unless \
            TACK_ORCH_ENABLE is set."),
        (name = "execution-operator", description = "Harness-agnostic runner fleet (Part III): PM-side \
            execution-request/fleet/runner-enrollment/agent-profile/model-profile management. \
            Authenticated the same way as the rest of this API (operator session or API token); scopes \
            idempotency and audit actor to the server-derived `x-tack-principal`, which a client cannot \
            set (see `crate::middleware::inject_operator_principal`)."),
        (name = "runner-protocol-v1", description = "Harness-agnostic runner fleet (Part III): the pull \
            protocol a `tack-runner` process speaks at `/api/runner/v1` (enroll, claim, heartbeat, \
            report). Authenticated by a distinct, per-runner hashed bearer credential — never the \
            operator token, and never substitutable for it \
            (docs/contracts/runner-v1/protocol.json: `credentials_are_not_substitutable`). Every wire \
            shape is frozen by docs/contracts/runner-v1/, not independently re-specified here."),
    ),
    nest(
        (path = "/api/runner/v1", api = RunnerProtocolApiDoc),
        (path = "/api", api = ExecutionOperatorExtrasApiDoc),
    ),
)]
pub struct ApiDoc;

/// `GET /api/openapi.json` — serve the generated OpenAPI 3.1 document.
///
/// Public: reading the schema requires no auth (the token gate exempts this
/// path in `crate::middleware`).
pub async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}

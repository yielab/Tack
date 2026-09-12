//! Provisioning flow: the end-to-end path from "I want a new product" to a
//! Tack project wired to a live docket pod.
//!
//! `POST /api/templates/{id}/provision` — deliberately a **separate route**
//! from the plain `POST /api/projects/from-template/{id}` rather
//! than an extension of it, even though `router.rs`'s original placeholder
//!
//! Design notes: docs/dev-notes/tack-api/handlers/provisioning.md

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::{OrchBlueprint, Project, TemplateOrchestration};
use tack_db::repo;
use tack_db::repo::orch::UpsertOrchLink;
use tack_orch::adapters::registry::{self, RegistryError};
use tack_orch::{ControlPlane as OrchControlPlane, OrchError, ProvisionPodParams};

use crate::error::{ApiError, ApiResult};
use crate::handlers::orch::{self, StatusMap};
use crate::handlers::templates::{self, build_project_from_template};
use crate::router::AppState;

// ════════════════════════════════════════════════════════════════════════════
// Request / response DTOs — every wire assumption for this flow lives here
// ════════════════════════════════════════════════════════════════════════════

/// The pod-provisioning half of the request body. Every field mirrors
/// docket's real `POST /pods` body (see the module doc) except
/// `control_plane_id` (Tack's own reference, never sent to docket) and
/// `status_map`/`auto_dispatch`/`pipeline_file`, which configure the
/// `orch_links` row written *after* the pod exists, not the `POST /pods`
/// call itself. Any field left `None` falls back to the chosen template's
/// `orchestration` block, if it has one; if neither supplies a value,
/// docket's own blueprint default applies (for `blueprint`, `budget`,
/// `verify_cmd`) or the field is simply omitted from the link.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct ProvisionPodRequest {
    pub control_plane_id: Uuid,
    /// The docket-side pod identifier. Required — never derived from the
    /// Tack project name — so a retry after a partial failure can be typed
    /// back in verbatim instead of risking a second, differently-named pod
    /// for the same intent (see the module doc's rollback design).
    pub remote_project: String,
    #[serde(default)]
    pub blueprint: Option<OrchBlueprint>,
    /// Codebase path (`software`-kind blueprints) or shared work directory
    /// (`workdir`-kind blueprints — `research`/`content`/`ops`/
    /// `agentic-product`). Empty/omitted lets docket auto-provision a work
    /// directory for `workdir`-kind blueprints; meaningless to omit for
    /// `software`, which then gets an empty codebase.
    #[serde(default)]
    pub path: Option<String>,
    /// Mirrors docket's `pod` field. Only the literal string `"full"` is
    /// meaningful (docket's own `software`-only roster override); any
    /// other non-empty value is rejected before docket is ever called.
    #[serde(default)]
    pub pod_shape: Option<String>,
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub verify_cmd: Option<String>,
    #[serde(default)]
    pub auto_dispatch: Option<bool>,
    /// A pipeline docket already knows about by name/path — stored on the
    /// `orch_links` row (`pipeline_file`). Inline `pipeline_yaml` on a
    /// template has no delivery mechanism to docket yet (`POST /pods` has
    /// no pipeline field at all) — see the response's `warnings`.
    #[serde(default)]
    pub pipeline_file: Option<String>,
    #[serde(default)]
    pub status_map: Option<StatusMap>,
}

/// `POST /api/templates/{id}/provision` body.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateProjectWithPodRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub provision_pod: ProvisionPodRequest,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ProvisionedPodMemberResponse {
    pub id: String,
    pub role: String,
    pub model: String,
}

/// The outcome of the provisioning half of the request, once the Tack
/// project itself exists. See the module doc's rollback design for exactly
/// when each variant is produced — both are a `200`, never an error
/// response, because in both cases the project *and* the pod are real and
/// valid; `PodCreatedLinkFailed` just means one more step (linking) needs
/// finishing, manually, via the existing Settings → Orchestration UI.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProvisioningOutcome {
    /// The pod was provisioned and the project is linked to it — the
    /// reconciler will start picking it up on its next poll.
    Linked {
        control_plane_id: Uuid,
        remote_project: String,
        blueprint: String,
        members: Vec<ProvisionedPodMemberResponse>,
        /// Non-fatal notices — e.g. a template's inline `pipeline_yaml`
        /// that could not be delivered anywhere. Empty in the common case.
        warnings: Vec<String>,
    },
    /// The pod exists on docket (real, billable, cannot be undone through
    /// this API) but Tack failed to save the `orch_links` row. The project
    /// is real too — neither was rolled back. `warnings` always contains a
    /// concrete instruction naming the control plane + remote project the
    /// operator needs to link manually.
    PodCreatedLinkFailed {
        control_plane_id: Uuid,
        remote_project: String,
        blueprint: String,
        members: Vec<ProvisionedPodMemberResponse>,
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CreateProjectWithPodResponse {
    pub project: Project,
    pub provisioning: ProvisioningOutcome,
}

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

/// docket's exact wire strings for `OrchBlueprint` (`core/blueprints.py`,
/// the same source `OrchBlueprint`'s `rename_all = "kebab-case"` is
/// verified against). A plain `match`, not a
/// `serde_json` round-trip through the enum's own `Serialize` impl: a
/// non-exhaustive match here is a compile error the moment a variant is
/// added, which is a better safety net than trusting the derive stays in
/// sync by construction.
fn blueprint_wire_name(b: OrchBlueprint) -> &'static str {
    match b {
        OrchBlueprint::Software => "software",
        OrchBlueprint::Research => "research",
        OrchBlueprint::Content => "content",
        OrchBlueprint::Ops => "ops",
        OrchBlueprint::AgenticProduct => "agentic-product",
    }
}

/// Merge the request's `status_map` over the template's default — the
/// request wins whenever it names *anything* (even a single field), the
/// template's default is used only when the request's map is entirely
/// empty. Mirrors `handlers::templates::validate_template_orchestration`'s
/// own field-for-field conversion between `TemplateStatusMap` and
/// `orch::StatusMap` rather than a third copy of that mapping.
fn resolve_status_map(
    request: Option<StatusMap>,
    template: Option<&TemplateOrchestration>,
) -> StatusMap {
    fn is_empty(m: &StatusMap) -> bool {
        m.dispatch_from.is_empty()
            && m.on_running.is_none()
            && m.on_waiting_approval.is_none()
            && m.on_succeeded.is_none()
            && m.on_failed.is_none()
            && m.on_cancelled.is_none()
    }
    match request {
        Some(m) if !is_empty(&m) => m,
        _ => template
            .map(|o| StatusMap {
                dispatch_from: o.status_map.dispatch_from.clone(),
                on_running: o.status_map.on_running.clone(),
                on_waiting_approval: o.status_map.on_waiting_approval.clone(),
                on_succeeded: o.status_map.on_succeeded.clone(),
                on_failed: o.status_map.on_failed.clone(),
                on_cancelled: o.status_map.on_cancelled.clone(),
            })
            .unwrap_or_default(),
    }
}

/// Resolve `control_plane_id` into a live adapter, 404ing distinctly if
/// the id doesn't exist. Built on `adapters::registry::build`, the shared
/// point this function and its siblings —
/// `dispatcher::build_control_plane`, `handlers::orch::
/// build_control_plane_for_decision` — all resolve through, rather than
/// each duplicating adapter-construction logic. Each of the three keeps its
/// own request-scoped error mapping — see `registry::build`'s own doc
/// comment for why that duplication (not the adapter-construction logic
/// itself) is the part staying separate.
async fn resolve_control_plane(
    state: &AppState,
    control_plane_id: Uuid,
) -> ApiResult<std::sync::Arc<dyn OrchControlPlane>> {
    let row = match state.repo.get_control_plane(control_plane_id).await {
        Ok(row) => row,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "control plane {control_plane_id} not found"
            )));
        }
        Err(e) => return Err(e.into()),
    };
    let token = state.repo.get_control_plane_token(control_plane_id).await?;
    match registry::build(
        &row.kind,
        &row.base_url,
        token,
        &serde_json::json!({}),
        None,
    ) {
        Ok(adapter) => Ok(adapter),
        Err(RegistryError::Construction(e)) => Err(ApiError::Internal(anyhow::anyhow!(
            "failed to construct control-plane adapter: {e}"
        ))),
        Err(RegistryError::UnknownKind(kind)) => Err(ApiError::Internal(anyhow::anyhow!(
            "unsupported control-plane kind {kind:?}"
        ))),
    }
}

/// `handlers::templates::build_project_from_template` returns a bare
/// `StatusCode` — see that function's doc comment. Map it onto this
/// endpoint's richer error envelope.
fn status_to_api_error(status: StatusCode, context: &str) -> ApiError {
    match status {
        StatusCode::NOT_FOUND => ApiError::NotFound(format!("{context}: template not found")),
        StatusCode::UNPROCESSABLE_ENTITY => {
            ApiError::Unprocessable(format!("{context}: validation failed"))
        }
        other => ApiError::Internal(anyhow::anyhow!("{context} failed: {other}")),
    }
}

/// Every [`OrchError`] `provision_pod` can return, other than
/// [`OrchError::AlreadyExists`] (409, mapped to [`ApiError::Conflict`]),
/// means "nothing new was created on docket's side" (see the module doc's
/// citation of `core/pod_provisioning.py`'s rollback guarantee) — so this
/// deliberately does not try to distinguish docket's 400 (bad input) from
/// its rare 500 (an operational failure *after* docket's own rollback
/// already ran) any further than the message itself does; both leave the
/// same nothing-created state for Tack to react to.
fn map_provision_error(e: OrchError) -> ApiError {
    match e {
        OrchError::Auth => ApiError::Internal(anyhow::anyhow!(
            "control plane rejected Tack's own credentials while provisioning a pod"
        )),
        OrchError::AlreadyExists(message) => ApiError::Conflict(format!(
            "a pod already exists on this control plane under that remote project name: {message}"
        )),
        other => ApiError::BadRequest(format!("pod provisioning failed: {other}")),
    }
}

/// Delete the project created moments earlier for this failed attempt.
/// Returns a human-readable clause describing what happened to the
/// rollback itself — appended to the caller's error message so a rollback
/// failure is surfaced, never swallowed. Every outcome is also logged at
/// the appropriate level regardless of what the HTTP response ends up
/// saying.
async fn rollback_project(state: &AppState, project_id: Uuid) -> String {
    match state.repo.delete_project(project_id).await {
        Ok(true) => {
            tracing::warn!(
                project_id = %project_id,
                "rolled back a partially-provisioned project after a pre-pod failure"
            );
            "the partially-created project has been rolled back — nothing was left behind."
                .to_string()
        }
        Ok(false) => {
            tracing::error!(
                project_id = %project_id,
                "rollback found no project row to delete — it may have been deleted concurrently"
            );
            format!(
                "the partially-created project {project_id} could not be found to roll back \
                 (it may already be gone) — verify manually."
            )
        }
        Err(e) => {
            tracing::error!(
                project_id = %project_id,
                error = %e,
                "FAILED to roll back a partially-provisioned project — it must be deleted manually"
            );
            format!(
                "additionally, the partially-created project {project_id} could NOT be \
                 automatically deleted ({e}) — delete it manually."
            )
        }
    }
}

/// Append `note` to an [`ApiError`]'s message without changing its status
/// code. Variants with a non-caller-facing message (`Internal`/`Database`/
/// `Core`/`Dependency`) are returned unchanged — the rollback outcome was
/// already logged by [`rollback_project`] regardless of what reaches the
/// client in those cases.
fn with_note(err: ApiError, note: &str) -> ApiError {
    match err {
        ApiError::BadRequest(msg) => ApiError::BadRequest(format!("{msg} ({note})")),
        ApiError::NotFound(msg) => ApiError::NotFound(format!("{msg} ({note})")),
        ApiError::Conflict(msg) => ApiError::Conflict(format!("{msg} ({note})")),
        ApiError::Forbidden(msg) => ApiError::Forbidden(format!("{msg} ({note})")),
        ApiError::Unprocessable(msg) => ApiError::Unprocessable(format!("{msg} ({note})")),
        other => other,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// The handler
// ════════════════════════════════════════════════════════════════════════════

/// `POST /api/templates/{id}/provision` — create a Tack project from a
/// template, provision a docket pod for it, and link the two. See the
/// module doc for the full rollback design.
#[utoipa::path(
    post,
    path = "/api/templates/{id}/provision",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Template ID")),
    request_body = CreateProjectWithPodRequest,
    responses(
        (status = 200, description = "Project created; pod provisioned. `provisioning.status` distinguishes a fully-linked result from one where the pod exists but the link write failed (both are real, neither was rolled back)", body = CreateProjectWithPodResponse),
        (status = 400, description = "Validation error, or docket refused the provisioning request — the project (if one had been created for this attempt) was rolled back", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Template or control plane not found, or orchestration disabled (TACK_ORCH_ENABLE unset)", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "A pod already exists on this control plane under that remote project name — the project created for this attempt was rolled back", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "name/description validation failed", body = crate::openapi::ErrorEnvelope),
    ),
)]
#[instrument(skip(state, body))]
pub async fn create_project_with_pod(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
    Json(body): Json<CreateProjectWithPodRequest>,
) -> ApiResult<Json<CreateProjectWithPodResponse>> {
    let remote_project = body.provision_pod.remote_project.trim().to_string();
    if remote_project.is_empty() {
        return Err(ApiError::BadRequest(
            "provision_pod.remote_project must not be empty".into(),
        ));
    }
    if let Some(shape) = &body.provision_pod.pod_shape
        && !shape.trim().is_empty()
        && !shape.eq_ignore_ascii_case("full")
    {
        return Err(ApiError::BadRequest(format!(
            "provision_pod.pod_shape must be \"full\" if given (got {shape:?})"
        )));
    }

    // Resolve + sanity-check the control plane *before* anything is
    // created — a bad id should never cost the operator a throwaway
    // project.
    let control_plane = resolve_control_plane(&state, body.provision_pod.control_plane_id).await?;

    // Read the template's `orchestration` defaults up front.
    // `build_project_from_template` below re-reads the template itself —
    // a second, cheap lookup rather than restructuring that function to
    // hand its template back out to a caller in a different module.
    let template = repo::templates::get_template(state.pool(), template_id)
        .await
        .map_err(|_| ApiError::NotFound(format!("template {template_id} not found")))?;
    let template_orch = template.orchestration.as_ref();

    // ── 1. Create the Tack project — the existing project-creation path.
    //        Nothing to roll back if this itself fails: nothing exists yet.
    let create_data = templates::CreateProjectFromTemplate {
        name: body.name.clone(),
        description: body.description.clone(),
    };
    let project = build_project_from_template(&state, template_id, create_data)
        .await
        .map_err(|status| status_to_api_error(status, "project creation"))?;

    // From here on, a failure that happens *before* docket confirms the pod
    // was created rolls the fresh project back — see the module doc for
    // why that line moves exactly once, permanently, the moment `POST
    // /pods` succeeds.

    // ── 2. Resolve effective provisioning params (request overrides the
    //        template's `orchestration` defaults, which are optional).
    let mut warnings: Vec<String> = Vec::new();
    let blueprint = body
        .provision_pod
        .blueprint
        .or(template_orch.map(|o| o.blueprint))
        .unwrap_or_default();
    let path = body.provision_pod.path.clone().unwrap_or_default();
    let pod_field = body
        .provision_pod
        .pod_shape
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|_| "full".to_string());
    let budget_usd = body
        .provision_pod
        .budget_usd
        .or_else(|| template_orch.and_then(|o| o.budget_usd));
    let verify_cmd = body
        .provision_pod
        .verify_cmd
        .clone()
        .or_else(|| template_orch.and_then(|o| o.verify_cmd.clone()))
        .unwrap_or_default();
    let auto_dispatch = body
        .provision_pod
        .auto_dispatch
        .unwrap_or_else(|| template_orch.is_some_and(|o| o.auto_dispatch));
    let pipeline_file = body
        .provision_pod
        .pipeline_file
        .clone()
        .or_else(|| template_orch.and_then(|o| o.pipeline_file.clone()));
    if pipeline_file.is_none() && template_orch.is_some_and(|o| o.pipeline_yaml.is_some()) {
        warnings.push(
            "the template's inline orchestration.pipeline_yaml has no delivery mechanism to \
             docket yet (POST /pods has no pipeline field) — only pipeline_file, if set, was \
             applied to this project's link."
                .to_string(),
        );
    }
    let status_map = resolve_status_map(body.provision_pod.status_map.clone(), template_orch);

    // ── 3. Validate status_map against the *actual* new project's
    //        workflow — before spending anything on docket.
    if let Err(e) = orch::validate_status_map(&status_map, &project.workflow) {
        let rollback = rollback_project(&state, project.id).await;
        return Err(with_note(e, &rollback));
    }

    // ── 4. Provision the pod — the one irreversible step.
    let blueprint_wire = blueprint_wire_name(blueprint);
    let provisioned = match control_plane
        .provision_pod(ProvisionPodParams {
            project: remote_project.clone(),
            path,
            blueprint: blueprint_wire.to_string(),
            pod: pod_field,
            budget: budget_usd,
            verify_cmd: verify_cmd.clone(),
        })
        .await
    {
        Ok(p) => p,
        Err(e) => {
            let mapped = map_provision_error(e);
            let rollback = rollback_project(&state, project.id).await;
            return Err(with_note(mapped, &rollback));
        }
    };

    let members: Vec<ProvisionedPodMemberResponse> = provisioned
        .members
        .iter()
        .map(|m| ProvisionedPodMemberResponse {
            id: m.id.clone(),
            role: m.role.clone(),
            model: m.model.clone(),
        })
        .collect();

    // ── 5. Write orch_links — short, additive, no HTTP call inside (the
    //        only HTTP call in this whole flow already happened above).
    //        Never rolled back past this point — see the module doc for
    //        why deleting the project here would make things strictly
    //        worse, not better.
    let status_map_json =
        serde_json::to_value(&status_map).unwrap_or_else(|_| serde_json::json!({}));
    let link_result = state
        .repo
        .upsert_orch_link(
            project.id,
            UpsertOrchLink {
                control_plane_id: body.provision_pod.control_plane_id,
                remote_project: remote_project.clone(),
                pipeline_file: pipeline_file.clone(),
                blueprint: Some(blueprint_wire.to_string()),
                auto_dispatch,
                budget_usd,
                status_map: status_map_json,
            },
        )
        .await;

    let outcome = match link_result {
        Ok(_) => ProvisioningOutcome::Linked {
            control_plane_id: body.provision_pod.control_plane_id,
            remote_project,
            blueprint: blueprint_wire.to_string(),
            members,
            warnings,
        },
        Err(e) => {
            tracing::error!(
                error = %e,
                project_id = %project.id,
                remote_project = %remote_project,
                "pod was provisioned but writing orch_links failed — project and pod both \
                 exist, unlinked; operator must finish linking manually"
            );
            warnings.push(format!(
                "the pod was provisioned successfully but Tack could not save the link ({e}) \
                 — open this project's Settings → Orchestration panel and link it to control \
                 plane {} / remote project {remote_project:?} to finish (the pod already \
                 exists — do not provision another one).",
                body.provision_pod.control_plane_id
            ));
            ProvisioningOutcome::PodCreatedLinkFailed {
                control_plane_id: body.provision_pod.control_plane_id,
                remote_project,
                blueprint: blueprint_wire.to_string(),
                members,
                warnings,
            }
        }
    };

    Ok(Json(CreateProjectWithPodResponse {
        project,
        provisioning: outcome,
    }))
}

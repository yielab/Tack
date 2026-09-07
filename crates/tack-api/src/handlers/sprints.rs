use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::{CreateSprint, SprintStatus, UpdateSprint};
use validator::Validate;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/sprints",
    tag = "sprints",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    request_body = tack_core::models::CreateSprint,
    responses(
        (status = 200, description = "Sprint created", body = tack_core::models::Sprint),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_sprint(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateSprint>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let sprint = state.repo.create_sprint(project_id, input).await?;
    Ok(Json(serde_json::to_value(sprint).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/sprints",
    tag = "sprints",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Sprints for the project", body = Vec<tack_core::models::Sprint>),
    ),
)]
pub async fn list_sprints(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let sprints = state.repo.list_sprints(project_id).await?;
    Ok(Json(serde_json::to_value(sprints).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/sprints/{id}",
    tag = "sprints",
    params(
        ("id" = Uuid, Path, description = "Sprint ID"),
    ),
    responses(
        (status = 200, description = "The sprint", body = tack_core::models::Sprint),
        (status = 404, description = "Sprint not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_sprint(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let sprint = state
        .repo
        .get_sprint(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Sprint {id} not found")))?;
    Ok(Json(serde_json::to_value(sprint).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    patch,
    path = "/api/sprints/{id}",
    tag = "sprints",
    params(
        ("id" = Uuid, Path, description = "Sprint ID"),
    ),
    request_body = tack_core::models::UpdateSprint,
    responses(
        (status = 200, description = "The updated sprint", body = tack_core::models::Sprint),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Sprint not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_sprint(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateSprint>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let sprint = state
        .repo
        .update_sprint(id, input)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Sprint {id} not found")))?;

    if let Some(wh) = &state.webhook {
        wh.fire(
            "sprint.updated",
            serde_json::json!({
                "event": "sprint.updated",
                "timestamp": Utc::now().to_rfc3339(),
                "project_id": sprint.project_id,
                "sprint_id": id,
                "sprint_name": sprint.name,
            }),
        );
    }

    Ok(Json(serde_json::to_value(sprint).unwrap()))
}

#[instrument(skip(state))]
#[utoipa::path(
    patch,
    path = "/api/sprints/{id}/status",
    tag = "sprints",
    params(
        ("id" = Uuid, Path, description = "Sprint ID"),
    ),
    request_body = UpdateSprintStatus,
    responses(
        (status = 200, description = "Status updated", body = serde_json::Value),
        (status = 400, description = "Validation error", body = crate::openapi::ErrorEnvelope),
        (status = 404, description = "Sprint not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_sprint_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateSprintStatus>,
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // Fetch sprint first so we have project_id for the webhook payload
    let sprint = state
        .repo
        .get_sprint(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Sprint {id} not found")))?;

    let new_status = input.status;
    let updated = state
        .repo
        .update_sprint_status(id, new_status.clone())
        .await?;
    if !updated {
        return Err(ApiError::NotFound(format!("Sprint {id} not found")));
    }

    if let Some(wh) = &state.webhook {
        let event = match new_status {
            SprintStatus::Active => "sprint.started",
            SprintStatus::Closed => "sprint.completed",
            _ => "sprint.updated",
        };
        wh.fire(
            event,
            serde_json::json!({
                "event": event,
                "timestamp": Utc::now().to_rfc3339(),
                "project_id": sprint.project_id,
                "sprint_id": id,
                "sprint_name": sprint.name,
                "status": format!("{new_status:?}").to_lowercase(),
            }),
        );
    }

    Ok(Json(serde_json::json!({"updated": true})))
}

#[derive(Debug, serde::Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdateSprintStatus {
    pub status: SprintStatus,
}

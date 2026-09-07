use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::models::{CreateSprint, Sprint, SprintStatus, UpdateSprint};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_sprint(
        &self,
        project_id: Uuid,
        input: CreateSprint,
    ) -> Result<Sprint, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO sprints (id, project_id, name, goal, start_date, end_date, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'planning', ?, ?)"
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(&input.name)
        .bind(&input.goal)
        .bind(input.start_date.map(|d| d.to_rfc3339()))
        .bind(input.end_date.map(|d| d.to_rfc3339()))
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?;

        debug!(sprint_id = %id, name = %input.name, "Sprint created");

        Ok(Sprint {
            id,
            project_id,
            name: input.name,
            goal: input.goal,
            start_date: input.start_date,
            end_date: input.end_date,
            status: SprintStatus::Planning,
            created_at: now,
            updated_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_sprint(&self, id: Uuid) -> Result<Option<Sprint>, sqlx::Error> {
        let row = sqlx::query_as::<_, SprintRow>(
            "SELECT id, project_id, name, goal, start_date, end_date, status, created_at, updated_at
             FROM sprints WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_sprint()))
    }

    #[instrument(skip(self))]
    pub async fn list_sprints(&self, project_id: Uuid) -> Result<Vec<Sprint>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SprintRow>(
            "SELECT id, project_id, name, goal, start_date, end_date, status, created_at, updated_at
             FROM sprints WHERE project_id = ? ORDER BY created_at DESC"
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_sprint()).collect())
    }

    /// Replaces a sprint's editable fields and returns the stored row, or
    /// `None` when no sprint has that id. `status` is untouched here.
    #[instrument(skip(self))]
    pub async fn update_sprint(
        &self,
        id: Uuid,
        input: UpdateSprint,
    ) -> Result<Option<Sprint>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE sprints SET name = ?, goal = ?, start_date = ?, end_date = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&input.name)
        .bind(&input.goal)
        .bind(input.start_date.map(|d| d.to_rfc3339()))
        .bind(input.end_date.map(|d| d.to_rfc3339()))
        .bind(&now)
        .bind(id.to_string())
        .execute(self.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        debug!(sprint_id = %id, "Sprint updated");
        self.get_sprint(id).await
    }

    #[instrument(skip(self))]
    pub async fn update_sprint_status(
        &self,
        id: Uuid,
        status: SprintStatus,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query("UPDATE sprints SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status.to_string())
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct SprintRow {
    id: String,
    project_id: String,
    name: String,
    goal: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
}

impl SprintRow {
    fn into_sprint(self) -> Sprint {
        Sprint {
            id: Uuid::parse_str(&self.id).unwrap(),
            project_id: Uuid::parse_str(&self.project_id).unwrap(),
            name: self.name,
            goal: self.goal,
            start_date: self.start_date.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            end_date: self.end_date.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            status: parse_sprint_status(&self.status),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}

fn parse_sprint_status(s: &str) -> SprintStatus {
    match s {
        "planning" => SprintStatus::Planning,
        "active" => SprintStatus::Active,
        "review" => SprintStatus::Review,
        "closed" => SprintStatus::Closed,
        _ => SprintStatus::Planning,
    }
}

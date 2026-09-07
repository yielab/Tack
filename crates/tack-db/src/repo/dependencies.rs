use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::CoreError;
use tack_core::dependency::{DependencyEdge, DependencyGraph};
use tack_core::models::{CreateDependency, Dependency, DependencyType};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_dependency(
        &self,
        source_item_id: Uuid,
        input: CreateDependency,
    ) -> Result<Dependency, DependencyError> {
        // Load existing dependency graph for cycle detection
        let edges = self
            .load_dependency_edges(source_item_id)
            .await
            .map_err(DependencyError::Db)?;
        let graph = DependencyGraph::from_edges(&edges);

        // Validate no cycle
        graph
            .validate_new_edge(source_item_id, input.target_item_id)
            .map_err(DependencyError::Core)?;

        let id = Uuid::new_v4();
        let now = Utc::now();
        let dep_type_str = input.dependency_type.to_string();

        sqlx::query(
            "INSERT INTO dependencies (id, source_item_id, target_item_id, dependency_type, created_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(source_item_id.to_string())
        .bind(input.target_item_id.to_string())
        .bind(&dep_type_str)
        .bind(now.to_rfc3339())
        .execute(self.pool())
        .await
        .map_err(DependencyError::Db)?;

        debug!(
            dependency_id = %id,
            source = %source_item_id,
            target = %input.target_item_id,
            dep_type = %dep_type_str,
            "Dependency created"
        );

        Ok(Dependency {
            id,
            source_item_id,
            target_item_id: input.target_item_id,
            dependency_type: input.dependency_type,
            created_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn list_dependencies_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<Dependency>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DepRow>(
            "SELECT id, source_item_id, target_item_id, dependency_type, created_at
             FROM dependencies
             WHERE source_item_id = ? OR target_item_id = ?
             ORDER BY created_at",
        )
        .bind(item_id.to_string())
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_dependency()).collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_dependency(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM dependencies WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    pub async fn list_dependencies_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<Dependency>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DepRow>(
            "SELECT d.id, d.source_item_id, d.target_item_id, d.dependency_type, d.created_at
             FROM dependencies d
             JOIN items i ON d.source_item_id = i.id
             WHERE i.project_id = ?
             ORDER BY d.created_at",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_dependency()).collect())
    }

    /// Every dependency row that touches **any** of `item_ids`, as either
    /// endpoint — deliberately not scoped to one project. Sprint DAG
    /// dispatch uses this to discover a sprint item's blockers even when the
    /// blocker lives outside the sprint (an ordinary case — a sprint is a
    /// subset of a project's items — or, since nothing in this schema
    /// actually forecloses it, a different project entirely). Returns `[]`
    /// for an empty `item_ids` rather than issuing a query with an empty `IN
    /// ()`, which is invalid SQL.
    #[instrument(skip(self))]
    pub async fn list_dependencies_for_items(
        &self,
        item_ids: &[Uuid],
    ) -> Result<Vec<Dependency>, sqlx::Error> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, source_item_id, target_item_id, dependency_type, created_at
             FROM dependencies
             WHERE source_item_id IN ({placeholders}) OR target_item_id IN ({placeholders})
             ORDER BY created_at"
        );
        let mut q = sqlx::query_as::<_, DepRow>(sqlx::AssertSqlSafe(query));
        for id in item_ids {
            q = q.bind(id.to_string());
        }
        for id in item_ids {
            q = q.bind(id.to_string());
        }
        let rows = q.fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|r| r.into_dependency()).collect())
    }

    /// Load all dependency edges from the same project as the given item.
    async fn load_dependency_edges(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<DependencyEdge>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DepRow>(
            "SELECT d.id, d.source_item_id, d.target_item_id, d.dependency_type, d.created_at
             FROM dependencies d
             JOIN items s ON d.source_item_id = s.id
             JOIN items t ON d.target_item_id = t.id
             WHERE s.project_id = (SELECT project_id FROM items WHERE id = ?)",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .filter(|r| r.dependency_type == "blocks" || r.dependency_type == "is_blocked_by")
            .map(|r| DependencyEdge {
                source: Uuid::parse_str(&r.source_item_id).unwrap(),
                target: Uuid::parse_str(&r.target_item_id).unwrap(),
                dep_type: parse_dep_type(&r.dependency_type),
            })
            .collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Dependency validation error: {0}")]
    Core(#[from] CoreError),
}

#[derive(sqlx::FromRow)]
struct DepRow {
    id: String,
    source_item_id: String,
    target_item_id: String,
    dependency_type: String,
    created_at: String,
}

impl DepRow {
    fn into_dependency(self) -> Dependency {
        Dependency {
            id: Uuid::parse_str(&self.id).unwrap(),
            source_item_id: Uuid::parse_str(&self.source_item_id).unwrap(),
            target_item_id: Uuid::parse_str(&self.target_item_id).unwrap(),
            dependency_type: parse_dep_type(&self.dependency_type),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }
}

fn parse_dep_type(s: &str) -> DependencyType {
    match s {
        "blocks" => DependencyType::Blocks,
        "is_blocked_by" => DependencyType::IsBlockedBy,
        "relates_to" => DependencyType::RelatesTo,
        "duplicates" => DependencyType::Duplicates,
        _ => DependencyType::RelatesTo,
    }
}

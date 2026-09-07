# Adding Features

This chapter describes the three most common extension patterns: adding a new entity, adding a new workflow preset, and extending the workflow engine. Each section includes a concrete example and explains the reasoning behind the step order.

---

## Adding a New Entity

**Example: Milestone**

A Milestone is a date-anchored checkpoint associated with a project. It has a name, an optional description, a target date, and a status.

### Step 1: Define the model in `tack-core`

Open `crates/tack-core/src/models.rs` and add the struct and any associated DTOs:

```rust
// ─── Milestone ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMilestone {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMilestone {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub target_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

`tack-core` is the right home for these types because it is the layer all other crates share. The API crate uses `CreateMilestone` for deserialization; the DB crate uses it as the input to the repository function. Keeping them in one place avoids duplication.

### Step 2: Add the migration in `tack-db`

Open `crates/tack-db/src/migrations.rs`. Migration names are numbered sequentially and
never reused — find the highest number already in `all_migrations()` (`grep -o
'"[0-9]\{3\}_[a-zA-Z0-9_]*"' crates/tack-db/src/migrations.rs | sort -u | tail -1`) and
pick the next one. As of this writing that's `062`, so the new migration is `063`. Find
the `migrations` vec in `all_migrations()` and append:

```rust
("063_milestones", &MIGRATION_063[..]),
```

Then add the constant near the end of the file:

```rust
const MIGRATION_063: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS milestones (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        target_date TEXT,
        completed_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_milestones_project ON milestones(project_id)",
];
```

Migrations are append-only and run in order. Never edit an existing migration — if you need to change the schema, add a new one.

### Step 3: Create the repository module

Create `crates/tack-db/src/repo/milestones.rs`. Follow the same pattern as other repository files: functions take `&SqlitePool`, bind parameters, run the query, and return the struct.

```rust
use chrono::Utc;
use uuid::Uuid;
use sqlx::SqlitePool;
use tack_core::models::{CreateMilestone, Milestone, UpdateMilestone};

pub async fn create_milestone(
    pool: &SqlitePool,
    project_id: Uuid,
    input: CreateMilestone,
) -> Result<Milestone, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO milestones (id, project_id, name, description, target_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id.to_string())
    .bind(project_id.to_string())
    .bind(&input.name)
    .bind(&input.description)
    .bind(input.target_date.map(|d| d.to_rfc3339()))
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;

    Ok(Milestone {
        id,
        project_id,
        name: input.name,
        description: input.description,
        target_date: input.target_date,
        completed_at: None,
        created_at: now,
        updated_at: now,
    })
}

// add get_milestone, list_milestones, update_milestone, delete_milestone
```

### Step 4: Register in `repo/mod.rs`

Open `crates/tack-db/src/repo.rs` and add:

```rust
pub mod milestones;
```

Then add delegating methods to the `Repository` impl:

```rust
pub async fn create_milestone(
    &self,
    project_id: Uuid,
    data: CreateMilestone,
) -> Result<Milestone, sqlx::Error> {
    milestones::create_milestone(self.pool(), project_id, data).await
}

pub async fn list_milestones(&self, project_id: Uuid) -> Result<Vec<Milestone>, sqlx::Error> {
    milestones::list_milestones(self.pool(), project_id).await
}

// get, update, delete …
```

This pattern — thin delegating methods on `Repository` — keeps the struct as the single entry point callers interact with while keeping each entity's SQL isolated in its own file.

### Step 5: Create the handler

Create `crates/tack-api/src/handlers/milestones.rs`:

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateMilestone, Milestone, UpdateMilestone};

use crate::{error::{ApiError, ApiResult}, router::AppState};

pub async fn create_milestone(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateMilestone>,
) -> ApiResult<(StatusCode, Json<Milestone>)> {
    input.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    // Verify project exists
    state.repo.get_project(project_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;
    let milestone = state.repo.create_milestone(project_id, input).await?;
    Ok((StatusCode::CREATED, Json(milestone)))
}

pub async fn list_milestones(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Milestone>>> {
    let milestones = state.repo.list_milestones(project_id).await?;
    Ok(Json(milestones))
}

// get_milestone, update_milestone, delete_milestone …
```

### Step 6: Register routes in `router.rs`

Open `crates/tack-api/src/router.rs`. Add the import:

```rust
use crate::handlers::milestones;
```

Then add the routes in the `api` router builder:

```rust
// ─── Milestones ───────────────────────────────────────────────────────────
.route("/projects/{project_id}/milestones", post(milestones::create_milestone))
.route("/projects/{project_id}/milestones", get(milestones::list_milestones))
.route("/milestones/{id}", get(milestones::get_milestone))
.route("/milestones/{id}", patch(milestones::update_milestone))
.route("/milestones/{id}", delete(milestones::delete_milestone))
```

At this point the feature is complete. Run `cargo nextest run --workspace` to verify nothing is broken, then add repository tests in `crates/tack-db/tests/repository.rs` and handler tests in `crates/tack-api/tests/handlers.rs` — each test file is its own binary, so add to the file whose subject fits rather than creating a new one (see [Testing](testing.md)).

---

## Adding a New Workflow Preset

**Example: education_workflow for an online-course project type**

### Step 1: Add the preset function in `tack-core`

Open `crates/tack-core/src/workflow.rs` and add the function after the existing presets:

```rust
pub fn education_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Custom,
        statuses: vec![
            StatusDef {
                name: "Not Started".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Under Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 2,
            },
            StatusDef {
                name: "Completed".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 3,
            },
        ],
        transitions: None,
    }
}
```

### Step 2: Write unit tests

Add tests in the `#[cfg(test)]` module in the same file:

```rust
#[test]
fn education_initial_status_is_not_started() {
    assert_eq!(education_workflow().initial_status().unwrap(), "Not Started");
}

#[test]
fn education_allows_in_progress_to_completed() {
    let wf = education_workflow();
    assert!(wf.validate_transition("In Progress", "Completed").is_ok());
}
```

Unit testing presets here is valuable because these tests run without any database or runtime — they are essentially free to run and will catch any mistake in the status list.

### Step 3: Add the `ProjectType` variant

Open `crates/tack-core/src/models.rs` and add `Education` to the `ProjectType` enum. The
enum is exhaustive — every match on it must add an arm — so this is the full current
list, not an excerpt:

```rust
pub enum ProjectType {
    Software,
    Web,
    Mobile,
    Construction,
    Personal,
    Homework,
    Maintenance,
    Legal,
    Research,
    Event,
    Education,  // ← new
    Custom,
}
```

Also update the `Display` impl:

```rust
Self::Education => write!(f, "education"),
```

### Step 4: Update `workflow_for_type`

In `workflow.rs`, add the new arm to the `match` in `workflow_for_type`:

```rust
pub fn workflow_for_type(project_type: &ProjectType) -> WorkflowConfig {
    match project_type {
        ProjectType::Software | ProjectType::Web | ProjectType::Mobile => scrum_workflow(),
        ProjectType::Construction => construction_workflow(),
        ProjectType::Personal | ProjectType::Homework => simple_workflow(),
        ProjectType::Maintenance => kanban_workflow(),
        ProjectType::Legal => legal_workflow(),
        ProjectType::Research => research_workflow(),
        ProjectType::Event => event_workflow(),
        ProjectType::Education => education_workflow(),  // ← new
        ProjectType::Custom => simple_workflow(),
    }
}
```

### Step 5: Add a vocabulary preset

Open `crates/tack-core/src/vocabulary.rs` and add a case to `vocabulary_for_type`:

```rust
ProjectType::Education => HashMap::from([
    ("epic".into(), "Course".into()),
    ("feature".into(), "Module".into()),
    ("task".into(), "Lesson".into()),
    ("subtask".into(), "Exercise".into()),
    ("bug".into(), "Correction".into()),
    ("sprint".into(), "Week".into()),
    ("milestone".into(), "Exam".into()),
    // … other terms
]),
```

At this point `POST /api/projects` with `"project_type": "education"` will auto-select the new workflow and vocabulary.

---

## Extending the Workflow Engine

Sometimes you need new logic in the workflow engine itself — for example, a rule that prevents moving an item to `Done` if any of its dependencies are still incomplete.

### Step 1: Add the function to `tack-core`

Open `crates/tack-core/src/workflow.rs`. Add a pure function:

```rust
impl WorkflowConfig {
    /// Return an error if attempting to mark `item_id` done when it has
    /// unresolved blocking dependencies.
    pub fn check_dependencies_resolved(
        item_id: Uuid,
        blockers: &[(Uuid, DependencyType)],
        target_status: &str,
    ) -> Result<(), CoreError> {
        if self.is_done_status(target_status) && !blockers.is_empty() {
            return Err(CoreError::Validation(format!(
                "Item {item_id} has {} unresolved blocker(s)",
                blockers.len()
            )));
        }
        Ok(())
    }
}
```

Note that this function takes its inputs as parameters — it does not query the database. The caller (the handler) is responsible for loading the blocker list and passing it in.

### Step 2: Write unit tests

Add tests in the `#[cfg(test)]` block in the same file. Test both the passing case (no blockers, or target not Done) and the failing case (blockers present, target is Done).

Unit tests for workflow logic are intentionally cheap to write here because there is no I/O to mock — you just call the function with constructed data.

### Step 3: Update the handler

Open `crates/tack-api/src/handlers/items.rs`. In `update_item`, after loading the project and before calling `repo.update_item`, add the new check:

```rust
if let Some(new_status) = &input.status {
    let project = state.repo.get_project(item.project_id).await? ...;
    project.workflow.validate_transition(&item.status, new_status)?;
    project.workflow.check_wip_limit(new_status, current_count)?;

    // New check: load blockers and verify they are resolved
    let all_deps = state.repo.list_dependencies(item.id).await?;
    let graph = DependencyGraph::from_edges(&all_deps);
    let blockers = graph.blockers_of(item.id);
    project.workflow.check_dependencies_resolved(item.id, &blockers, new_status)?;
}
```

### What not to change

No migration is needed. The workflow engine is pure logic — it reads from a `WorkflowConfig` struct that is already stored as JSON. Adding a new method to `WorkflowConfig` does not require any database schema change.

---

## Anti-Patterns to Avoid

The crate layering is the project's load-bearing constraint. Most review feedback on new features comes down to one of these:

- **Don't put I/O in `tack-core`.** No file access, no HTTP calls, no `sqlx`, no `tokio` runtime needs. Core is pure, synchronous domain logic so it stays trivially testable. If a rule needs data, take it as a function parameter and let the handler load it (as in the dependency check above).
- **Don't scatter validation across handlers.** Transition rules, WIP limits, cycle checks, and field validation belong in `tack-core`, called from the handler. Duplicating a rule inline in a handler means the CLI, API, and MCP server can disagree about what's valid.
- **Don't reach past the repository layer.** Handlers call `Repository` methods; they never build SQL or touch the pool directly. New queries go in the matching `repo/<entity>.rs` module.
- **Don't let `tack-cli` import `tack-db`.** The CLI is an HTTP client — all data access goes through the API so workflow rules are enforced server-side. The same applies to the MCP server.
- **Don't edit an existing migration.** Migrations are append-only and idempotent. Changing a shipped migration corrupts databases that already applied it. Add a new numbered migration and wire it into `all_migrations()`.
- **Don't hardcode colors in the frontend.** Components consume `--color-*` design tokens via inline `style`, never raw hex, so the theme/palette system keeps working. See [Frontend & Design System](frontend.md).
- **Don't add an endpoint without a handler test.** Every new route gets at least a success-path and an error-path test in `crates/tack-api/tests/handlers.rs` (or the file matching the route's subject — see [Testing](testing.md)).

When a change feels like it needs to break one of these, that's usually a sign the logic belongs in a different layer — move it rather than bending the boundary.

If the new logic requires new *configuration* (for example, a per-project flag to enable or disable dependency-blocking), then you would add a field to `WorkflowConfig`, update the struct, and add a migration to handle existing rows that do not have that field (SQLite will use the column default).

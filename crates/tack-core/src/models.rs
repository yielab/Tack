use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::vocabulary::VocabularyMap;
use crate::workflow::{StatusCategory, WorkflowConfig};

// ─── Workspace ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub default_vocabulary: VocabularyMap,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Project ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: VocabularyMap,
    pub workflow: WorkflowConfig,
    pub default_model: Option<ProjectModelDefault>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived: bool,
}

/// A project's own opinion on which model its executions should use — the
/// `Project` tier in `tack-orch::model_policy`'s precedence walk, between the
/// agent-profile and fleet defaults. Absent (`None` on `Project`) means "no
/// opinion" and falls through to the fleet tier; `Auto` is a distinct, real
/// choice that stops the walk at this tier instead of falling through.
/// Because the API deserializes a request body directly into this enum, a
/// malformed shape is rejected before any handler or database write runs —
/// there is no second, tolerant parse of an opaque blob at read time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProjectModelDefault {
    Auto,
    Explicit { provider: String, model_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    Custom,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Software => write!(f, "software"),
            Self::Web => write!(f, "web"),
            Self::Mobile => write!(f, "mobile"),
            Self::Construction => write!(f, "construction"),
            Self::Personal => write!(f, "personal"),
            Self::Homework => write!(f, "homework"),
            Self::Maintenance => write!(f, "maintenance"),
            Self::Legal => write!(f, "legal"),
            Self::Research => write!(f, "research"),
            Self::Event => write!(f, "event"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ─── Item (universal work unit) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Item {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub item_type: ItemType,
    pub status: String,
    pub priority: Priority,
    pub estimate: Option<f64>,
    pub estimate_unit: EstimateUnit,
    pub tags: Vec<String>,
    pub sort_order: i32,
    pub sprint_id: Option<Uuid>,
    pub assignee: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Sticky provenance marker for the prompt-injection
    /// trust boundary: set once at creation time by whichever handler
    /// created the item, and never mutated afterward — `UpdateItem` has no
    /// `source` field, and the repository's `update_item` has no code path
    /// that writes this column, so an item's source can never change once
    /// set. `#[serde(default)]` matters here: any JSON that predates this
    /// field (an old export, or a hand-built import payload) deserializes
    /// to [`ItemSource::default()`] (`Unknown`), which [`ItemSource::is_trusted`]
    /// treats as untrusted — the same "unverifiable provenance resolves to
    /// untrusted" rule migration 029 applies to pre-existing database rows.
    #[serde(default)]
    pub source: ItemSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Where an item's title/description text came from. Backend for the
/// prompt-injection trust boundary: text imported from
/// GitHub Issues, Linear, or any bulk import is written by parties Tack
/// cannot vouch for, and becomes literal instructions to an autonomous
/// agent the moment the item is dispatched. [`is_trusted`](ItemSource::is_trusted)
/// is the single place that rule is encoded — nothing else in this codebase
/// should independently decide whether a `source` counts as trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ItemSource {
    /// Created directly through the normal create-item API (the UI, the
    /// CLI's `tack add`, or the MCP `create_item` tool) — the operator's own
    /// words, not third-party text.
    Manual,
    /// `POST /api/projects/{id}/import-github` — GitHub Issues, filed by
    /// anyone who can open an issue on the linked repo.
    Github,
    /// `POST /api/projects/{id}/import-linear` — Linear issues.
    Linear,
    /// `POST /api/projects/import` — a full project snapshot (JSON/YAML).
    /// Used both for legitimate backup/restore of a project this Tack
    /// instance already trusted (in which case the original item's own
    /// `source` rides through the export and is preserved — see
    /// `handlers::export::run_import`) and for an arbitrary externally
    /// supplied payload with no such provenance, which is why *this*
    /// variant itself (assigned when a payload item carries no `source` of
    /// its own) is untrusted, same as every non-`Manual` value.
    JsonImport,
    /// `POST /api/projects/{id}/import-csv` — a bare CSV has no concept of
    /// provenance at all, so every row lands here.
    CsvImport,
    /// The backfilled value for every item that existed before migration
    /// 029 added this column — including items imported from GitHub or
    /// Linear before this trust boundary existed (migration 018 shipped
    /// GitHub import, well before migration 029). We cannot recover which,
    /// so — per the "unsafe state is never the accidental default" rule —
    /// every pre-migration item resolves to untrusted rather than assuming
    /// the safe-looking but unverifiable `Manual`. No code path should ever
    /// write this value for a newly created item; it exists purely as the
    /// migration's backfill and the serde default for old payloads.
    #[default]
    Unknown,
}

impl ItemSource {
    /// The single source of truth for "does this item's text get to skip
    /// docket's `pre_input` untrusted-content policy". Only `Manual` is
    /// trusted; everything else — including `Unknown` — is not.
    pub fn is_trusted(&self) -> bool {
        matches!(self, ItemSource::Manual)
    }
}

impl std::fmt::Display for ItemSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::Github => write!(f, "github"),
            Self::Linear => write!(f, "linear"),
            Self::JsonImport => write!(f, "json_import"),
            Self::CsvImport => write!(f, "csv_import"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for ItemSource {
    type Err = std::convert::Infallible;

    /// Never fails — an unrecognised string (a future Tack version's new
    /// source value read by an older binary, or plain DB corruption)
    /// degrades to `Unknown`, i.e. untrusted, rather than a parse error
    /// that would take down an unrelated read. This mirrors
    /// `parse_priority`'s existing fallback pattern in
    /// `tack-db::repo::items`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "manual" => Self::Manual,
            "github" => Self::Github,
            "linear" => Self::Linear,
            "json_import" => Self::JsonImport,
            "csv_import" => Self::CsvImport,
            _ => Self::Unknown,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ItemType {
    Epic,
    Feature,
    Task,
    Subtask,
    Bug,
    Requirement,
    Custom(String),
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Epic => write!(f, "epic"),
            Self::Feature => write!(f, "feature"),
            Self::Task => write!(f, "task"),
            Self::Subtask => write!(f, "subtask"),
            Self::Bug => write!(f, "bug"),
            Self::Requirement => write!(f, "requirement"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Priority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
    None,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
            Self::None => write!(f, "none"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum EstimateUnit {
    #[default]
    StoryPoints,
    Hours,
    Days,
    Custom(String),
}

// ─── Dependency ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Dependency {
    pub id: Uuid,
    pub source_item_id: Uuid,
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum DependencyType {
    Blocks,
    IsBlockedBy,
    RelatesTo,
    Duplicates,
}

impl std::fmt::Display for DependencyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocks => write!(f, "blocks"),
            Self::IsBlockedBy => write!(f, "is_blocked_by"),
            Self::RelatesTo => write!(f, "relates_to"),
            Self::Duplicates => write!(f, "duplicates"),
        }
    }
}

// ─── Role / Specialty ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Role {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemRole {
    pub item_id: Uuid,
    pub role_id: Uuid,
}

// ─── Comment ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Comment {
    pub id: Uuid,
    pub item_id: Uuid,
    pub author: Option<String>,
    pub content: String,
    pub comment_type: CommentType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CommentType {
    Comment,
    StatusChange,
    Edit,
    System,
}

// ─── Attachment ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Attachment {
    pub id: Uuid,
    pub item_id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub storage_path: String,
    pub size_bytes: u64,
    pub uploaded_at: DateTime<Utc>,
}

// ─── Sprint ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Sprint {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub status: SprintStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SprintStatus {
    Planning,
    Active,
    Review,
    Closed,
}

impl std::fmt::Display for SprintStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Planning => write!(f, "planning"),
            Self::Active => write!(f, "active"),
            Self::Review => write!(f, "review"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

// ─── Board View Config ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BoardView {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub columns: Vec<BoardColumn>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BoardColumn {
    pub status: String,
    pub wip_limit: Option<usize>,
    pub collapsed: bool,
}

// ─── DTOs for creation/updates ───────────────────────────────

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub template: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateProject {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub default_model: Option<ProjectModelDefault>,
    pub archived: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateItem {
    #[validate(length(min = 1, max = 500, message = "title must be 1–500 characters"))]
    pub title: String,
    #[validate(length(max = 50_000, message = "description too long (max 50 000 chars)"))]
    pub description: Option<String>,
    pub item_type: Option<ItemType>,
    pub parent_id: Option<Uuid>,
    pub priority: Option<Priority>,
    #[validate(range(min = 0.0, message = "estimate must be non-negative"))]
    pub estimate: Option<f64>,
    pub estimate_unit: Option<EstimateUnit>,
    #[validate(length(max = 20, message = "too many tags (max 20)"))]
    pub tags: Option<Vec<String>>,
    pub due_date: Option<DateTime<Utc>>,
    pub sprint_id: Option<Uuid>,
    #[validate(length(max = 200, message = "assignee name too long (max 200 chars)"))]
    pub assignee: Option<String>,
}

#[derive(Debug, Default, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateItem {
    #[validate(length(min = 1, max = 500, message = "title must be 1–500 characters"))]
    pub title: Option<String>,
    #[validate(length(max = 50_000, message = "description too long (max 50 000 chars)"))]
    /// Omitted leaves the description untouched; JSON `null` clears it.
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub description: Option<Option<String>>,
    pub item_type: Option<ItemType>,
    #[validate(length(min = 1, max = 100, message = "status must be 1–100 characters"))]
    pub status: Option<String>,
    pub priority: Option<Priority>,
    #[validate(range(min = 0.0, message = "estimate must be non-negative"))]
    /// Omitted leaves the estimate untouched; JSON `null` clears it.
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub estimate: Option<Option<f64>>,
    // Double-`Option` fields: outer `None` = key absent (leave untouched),
    // `Some(None)` = JSON `null` (clear the column), `Some(Some(v))` = set to `v`.
    // This lets the frontend clear a sprint assignment / due date / estimate unit
    // by sending `null`, which a plain `Option<T>` cannot distinguish from "absent".
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub estimate_unit: Option<Option<EstimateUnit>>,
    #[validate(length(max = 20, message = "too many tags (max 20)"))]
    pub tags: Option<Vec<String>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub sprint_id: Option<Option<Uuid>>,
    pub sort_order: Option<i32>,
    #[validate(length(max = 200, message = "assignee name too long (max 200 chars)"))]
    /// Omitted leaves the assignee untouched; JSON `null` clears it.
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub assignee: Option<Option<String>>,
    /// Server-only: the target status's category, populated by the update handler
    /// when the status changes so the persistence layer can maintain
    /// `started_at` / `completed_at`. Never (de)serialized from client requests.
    #[serde(skip)]
    pub status_category: Option<StatusCategory>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateSprint {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 2_000, message = "goal too long (max 2 000 chars)"))]
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

/// A full replacement of a sprint's editable fields. Every field is written as
/// given, so an omitted `goal`, `start_date` or `end_date` clears the stored
/// value rather than leaving it alone — the edit form always sends all four,
/// and an empty box there means the user cleared it. `status` is not here: it
/// moves through its own route, which fires the lifecycle webhooks.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateSprint {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 2_000, message = "goal too long (max 2 000 chars)"))]
    pub goal: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateRole {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateComment {
    #[validate(length(min = 1, max = 10_000, message = "content must be 1–10 000 characters"))]
    pub content: String,
    #[validate(length(max = 200, message = "author name too long (max 200 chars)"))]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateDependency {
    pub target_item_id: Uuid,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
pub struct ItemFilter {
    pub status: Option<String>,
    pub item_type: Option<ItemType>,
    pub priority: Option<Priority>,
    pub sprint_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub assignee: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl ItemFilter {
    /// Default page size when the client does not specify one.
    pub const DEFAULT_PER_PAGE: u32 = 100;
    /// Hard cap on page size so a single request can never scan the whole table.
    pub const MAX_PER_PAGE: u32 = 500;

    /// The effective, clamped page size (1..=`MAX_PER_PAGE`). Shared by the
    /// list query, the pagination envelope, and any caller that needs to know
    /// how many rows a page can hold.
    pub fn effective_per_page(&self) -> u32 {
        self.per_page
            .unwrap_or(Self::DEFAULT_PER_PAGE)
            .clamp(1, Self::MAX_PER_PAGE)
    }

    /// The effective, 1-based page number (never below 1).
    pub fn effective_page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }
}

// ─── Project Template ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: VocabularyMap,
    pub workflow: WorkflowConfig,
    pub custom_fields: Vec<CustomFieldDefinition>,
    pub default_boards: Vec<BoardTemplate>,
    /// Optional agent-fleet defaults for a project created from this
    /// template. `#[serde(default)]` — absent means
    /// "this template does not touch orchestration," the same
    /// absent-means-nothing rule `Item::source` (migration 029) established
    /// for backward compatibility. `None` is the value every
    /// template had before this field existed and every built-in has today;
    /// nothing reads this field unless it is `Some`, so a template with no
    /// `orchestration` block behaves exactly as it did before this field
    /// existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<TemplateOrchestration>,
    pub is_builtin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BoardTemplate {
    pub name: String,
    pub description: Option<String>,
    pub columns: Vec<BoardColumn>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateProjectTemplate {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub project_type: ProjectType,
    pub vocabulary: Option<VocabularyMap>,
    pub workflow: Option<WorkflowConfig>,
    pub custom_fields: Option<Vec<CustomFieldDefinition>>,
    pub default_boards: Option<Vec<BoardTemplate>>,
    /// See [`ProjectTemplate::orchestration`]. Validated at save time by
    /// `tack-api`'s `handlers::templates::create_template` — this type is
    /// pure data (tack-core has zero I/O), so the validation itself lives
    /// one layer up, reusing `handlers::orch::validate_status_map` (the
    /// `status_map` validator) rather than duplicating it.
    #[serde(default)]
    pub orchestration: Option<TemplateOrchestration>,
}

/// Agent-fleet defaults captured on a template. Plain
/// `create_project_from_template` stores it and moves on — nothing in this
/// struct is applied automatically there. Turning it into a live
/// `orch_links` row needs a `control_plane_id` pointing at one specific,
/// already-registered docket instance, which that plain from-template path
/// never has; `handlers::provisioning::create_project_with_pod` is the
/// route that reads these fields as defaults (the request body overrides
/// them) when actually provisioning a pod, and that route is gated behind
/// `TACK_ORCH_ENABLE`. On this struct itself there is no route, no
/// reconciler, no dispatch — just a JSON blob riding along with the template.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateOrchestration {
    /// docket pod blueprint. Verified against `core/blueprints.py`:
    /// exactly these five values exist server-side today.
    /// Unlike the remote-state enums in `tack-orch` (`RunState` etc.),
    /// this is a value Tack *sends*, not one it decodes from docket's
    /// output, so no `Unknown` fallback here: an
    /// unrecognised blueprint name is a real authoring mistake worth
    /// rejecting, not a forward-compat case to shrug off.
    #[serde(default)]
    pub blueprint: OrchBlueprint,
    /// Inline docket pipeline YAML — the "pipeline library" entry (task
    /// 37.3). Stored as a template field rather than a new `pipelines`
    /// table: the roadmap names both as acceptable, and a template is
    /// already a named, reusable, save-time-validated bundle, so a second
    /// storage concept alongside it would just be a template under another
    /// name. See `handlers::templates::validate_template_orchestration`
    /// for what "validated" means here — deliberately narrower than
    /// docket's own schema; see that function's doc comment.
    #[serde(default)]
    pub pipeline_yaml: Option<String>,
    /// A pipeline docket already knows about by name/path, for a template
    /// that would rather point at one than ship inline YAML. Mirrors
    /// `orch_links.pipeline_file`. Not mutually exclusive with
    /// `pipeline_yaml`, but only this field currently reaches docket:
    /// `handlers::provisioning` has no `POST /pods` field for inline YAML
    /// yet, so a template with `pipeline_yaml` set and no `pipeline_file`
    /// surfaces a non-fatal warning instead of silently dropping it.
    #[serde(default)]
    pub pipeline_file: Option<String>,
    #[serde(default)]
    pub verify_cmd: Option<String>,
    /// Default budget *cap* for a project created from this template — an
    /// operator-set ceiling, not a derived spend figure, so it stays
    /// unsuffixed exactly like `orch_links.budget_usd`. The naming rule
    /// that requires an `_estimated` suffix governs *estimated spend*
    /// fields (`cost_usd_estimated`); a cap the operator chooses is a
    /// different thing and is out of that rule's scope.
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub status_map: TemplateStatusMap,
    #[serde(default)]
    pub auto_dispatch: bool,
    /// Mirrors docket's `POST /pods` `pod` field, which
    /// (`serve.py::_handle_post_pods`) accepts only `"full"` or absent.
    /// Stored permissively here (no enum, no validation) — enforcing that
    /// exact constraint happens at provisioning time, when the real
    /// `POST /pods` body is built; guessing at it here would just be a
    /// second, driftable copy of a one-value check.
    #[serde(default)]
    pub pod_shape: Option<String>,
}

/// docket pod blueprint names (`core/blueprints.py`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum OrchBlueprint {
    #[default]
    Software,
    Research,
    Content,
    Ops,
    AgenticProduct,
}

/// A template's default `status_map`. Field-for-field
/// identical to `tack_api::handlers::orch::StatusMap` by design — the two
/// are kept in lockstep deliberately (a template's map becomes a project's
/// `orch_links.status_map` verbatim once something applies it) — but they
/// stay two distinct Rust types because `tack-core` cannot depend on
/// `tack-api` (crate boundary in `crates/tack-orch/src/lib.rs`'s comment
/// applies here too: dependencies point inward, tack-core has zero I/O and
/// zero knowledge of the HTTP layer). Validation is not duplicated: the
/// handler converts this into an `orch::StatusMap` and calls
/// `orch::validate_status_map` directly — see
/// `handlers::templates::validate_template_orchestration`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TemplateStatusMap {
    #[serde(default)]
    pub dispatch_from: Vec<String>,
    #[serde(default)]
    pub on_running: Option<String>,
    #[serde(default)]
    pub on_waiting_approval: Option<String>,
    #[serde(default)]
    pub on_succeeded: Option<String>,
    #[serde(default)]
    pub on_failed: Option<String>,
    #[serde(default)]
    pub on_cancelled: Option<String>,
}

// ─── Custom Fields ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CustomFieldDefinition {
    pub id: Uuid,
    pub project_id: Option<Uuid>, // None for template-level fields
    pub name: String,
    pub field_type: CustomFieldType,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<Vec<String>>, // For select/multi-select types
    pub validation: Option<serde_json::Value>, // JSON schema or regex
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum CustomFieldType {
    Text,
    Number,
    Date,
    Boolean,
    Select,      // Single select from options
    MultiSelect, // Multiple select from options
    Url,
    Email,
    LongText, // Textarea
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CustomFieldValue {
    pub id: Uuid,
    pub item_id: Uuid,
    pub field_id: Uuid,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateCustomField {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: String,
    pub field_type: CustomFieldType,
    #[validate(length(max = 1_000, message = "description too long (max 1 000 chars)"))]
    pub description: Option<String>,
    pub required: Option<bool>,
    pub default_value: Option<serde_json::Value>,
    #[validate(length(max = 100, message = "too many options (max 100)"))]
    pub options: Option<Vec<String>>,
    pub validation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateCustomField {
    #[validate(length(min = 1, max = 100, message = "name must be 1–100 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 1_000, message = "description too long (max 1 000 chars)"))]
    pub description: Option<String>,
    pub required: Option<bool>,
    pub default_value: Option<serde_json::Value>,
    #[validate(length(max = 100, message = "too many options (max 100)"))]
    pub options: Option<Vec<String>>,
    pub validation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetCustomFieldValue {
    pub field_id: Uuid,
    pub value: serde_json::Value,
}

impl CustomFieldDefinition {
    /// Validate a JSON value against this field's type, options, and `validation` rules.
    /// Returns `Err(message)` if the value is invalid.
    pub fn validate_value(&self, value: &serde_json::Value) -> Result<(), String> {
        // ── Type + options check ─────────────────────────────────
        match &self.field_type {
            CustomFieldType::Text | CustomFieldType::LongText | CustomFieldType::Email => {
                if !value.is_string() {
                    return Err(format!("field '{}' expects a string value", self.name));
                }
            }
            CustomFieldType::Url => match value.as_str() {
                Some(s) if s.starts_with("http://") || s.starts_with("https://") => {}
                Some(_) => {
                    return Err(format!(
                        "field '{}' expects a URL starting with http:// or https://",
                        self.name
                    ));
                }
                None => return Err(format!("field '{}' expects a URL string", self.name)),
            },
            CustomFieldType::Number => {
                if !value.is_number() {
                    return Err(format!("field '{}' expects a numeric value", self.name));
                }
            }
            CustomFieldType::Boolean => {
                if !value.is_boolean() {
                    return Err(format!("field '{}' expects a boolean value", self.name));
                }
            }
            CustomFieldType::Date => match value.as_str() {
                Some(s) => {
                    let ok = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
                        || chrono::DateTime::parse_from_rfc3339(s).is_ok();
                    if !ok {
                        return Err(format!(
                            "field '{}' expects an ISO 8601 date (YYYY-MM-DD or RFC3339)",
                            self.name
                        ));
                    }
                }
                None => return Err(format!("field '{}' expects a date string", self.name)),
            },
            CustomFieldType::Select => match value.as_str() {
                Some(s) => {
                    if let Some(opts) = &self.options
                        && !opts.iter().any(|o| o == s)
                    {
                        return Err(format!(
                            "field '{}': '{}' is not a valid option",
                            self.name, s
                        ));
                    }
                }
                None => return Err(format!("field '{}' expects a string value", self.name)),
            },
            CustomFieldType::MultiSelect => match value.as_array() {
                Some(arr) => {
                    for v in arr {
                        match v.as_str() {
                            Some(s) => {
                                if let Some(opts) = &self.options
                                    && !opts.iter().any(|o| o == s)
                                {
                                    return Err(format!(
                                        "field '{}': '{}' is not a valid option",
                                        self.name, s
                                    ));
                                }
                            }
                            None => {
                                return Err(format!(
                                    "field '{}' expects an array of strings",
                                    self.name
                                ));
                            }
                        }
                    }
                }
                None => {
                    return Err(format!("field '{}' expects an array of strings", self.name));
                }
            },
        }

        // ── Extra validation rules from the `validation` JSON field ──
        if let Some(rules) = &self.validation {
            self.apply_validation_rules(value, rules)?;
        }

        Ok(())
    }

    /// Apply the extra validation rules stored in the `validation` JSON field.
    ///
    /// Supported rule keys:
    /// - `pattern`    (string)  — regex applied to Text/LongText/Email/Url
    /// - `min_length` (u64)    — minimum string length
    /// - `max_length` (u64)    — maximum string length
    /// - `min`        (f64)    — minimum numeric value
    /// - `max`        (f64)    — maximum numeric value
    /// - `max_items`  (u64)    — maximum array length for MultiSelect
    fn apply_validation_rules(
        &self,
        value: &serde_json::Value,
        rules: &serde_json::Value,
    ) -> Result<(), String> {
        // pattern — regex for string values
        if let Some(pattern) = rules.get("pattern").and_then(|v| v.as_str())
            && let Some(s) = value.as_str()
        {
            match Regex::new(pattern) {
                Ok(re) => {
                    if !re.is_match(s) {
                        return Err(format!(
                            "field '{}': value does not match required pattern",
                            self.name
                        ));
                    }
                }
                Err(_) => {
                    return Err(format!(
                        "field '{}': invalid regex pattern in field definition",
                        self.name
                    ));
                }
            }
        }

        // min_length / max_length — for string values
        if let Some(s) = value.as_str() {
            let len = s.chars().count() as u64;
            if let Some(min) = rules.get("min_length").and_then(|v| v.as_u64())
                && len < min
            {
                return Err(format!(
                    "field '{}': value is too short (min {} characters)",
                    self.name, min
                ));
            }
            if let Some(max) = rules.get("max_length").and_then(|v| v.as_u64())
                && len > max
            {
                return Err(format!(
                    "field '{}': value is too long (max {} characters)",
                    self.name, max
                ));
            }
        }

        // min / max — for numeric values
        if let Some(n) = value.as_f64() {
            if let Some(min) = rules.get("min").and_then(|v| v.as_f64())
                && n < min
            {
                return Err(format!(
                    "field '{}': value {} is below minimum {}",
                    self.name, n, min
                ));
            }
            if let Some(max) = rules.get("max").and_then(|v| v.as_f64())
                && n > max
            {
                return Err(format!(
                    "field '{}': value {} exceeds maximum {}",
                    self.name, n, max
                ));
            }
        }

        // max_items — for array values (MultiSelect)
        if let Some(arr) = value.as_array()
            && let Some(max_items) = rules.get("max_items").and_then(|v| v.as_u64())
            && arr.len() as u64 > max_items
        {
            return Err(format!(
                "field '{}': too many selections (max {})",
                self.name, max_items
            ));
        }

        Ok(())
    }
}

// ─── Board (Multiple Boards per Project) ────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Board {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>, // Filter criteria for this board
    pub grouping: Option<BoardGrouping>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum BoardGrouping {
    Status,            // Group by status (default Kanban)
    Priority,          // Group by priority
    ItemType,          // Group by item type
    Sprint,            // Group by sprint
    Assignee,          // Group by assignee (if we add that)
    CustomField(Uuid), // Group by custom field
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateBoard {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: String,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateBoard {
    #[validate(length(min = 1, max = 200, message = "name must be 1–200 characters"))]
    pub name: Option<String>,
    #[validate(length(max = 10_000, message = "description too long (max 10 000 chars)"))]
    pub description: Option<String>,
    pub filters: Option<serde_json::Value>,
    pub grouping: Option<BoardGrouping>,
    pub is_default: Option<bool>,
}

#[cfg(test)]
#[path = "models/tests.rs"]
mod tests;

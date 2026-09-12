use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::error::CoreError;
use crate::models::ProjectType;

/// Full workflow configuration for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WorkflowConfig {
    pub workflow_type: WorkflowType,
    pub statuses: Vec<StatusDef>,
    pub transitions: Option<Vec<Transition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum WorkflowType {
    Scrum,
    Kanban,
    Mixed,
    Simple,
    Construction,
    Legal,
    Research,
    Event,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StatusDef {
    pub name: String,
    pub category: StatusCategory,
    pub wip_limit: Option<usize>,
    pub order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum StatusCategory {
    Todo,
    InProgress,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Transition {
    pub from: String,
    pub to: String,
}

impl WorkflowConfig {
    /// Validate that a status transition is allowed.
    pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), CoreError> {
        // Verify both statuses exist
        let from_exists = self.statuses.iter().any(|s| s.name == from);
        let to_exists = self.statuses.iter().any(|s| s.name == to);

        if !from_exists {
            return Err(CoreError::InvalidTransition {
                from: from.to_string(),
                to: to.to_string(),
            });
        }
        if !to_exists {
            return Err(CoreError::InvalidTransition {
                from: from.to_string(),
                to: to.to_string(),
            });
        }

        // If explicit transitions defined, enforce them
        if let Some(ref transitions) = self.transitions {
            let allowed = transitions.iter().any(|t| t.from == from && t.to == to);
            if !allowed {
                debug!(from, to, "Transition not in allowed list");
                return Err(CoreError::InvalidTransition {
                    from: from.to_string(),
                    to: to.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Check WIP limits for a given status column.
    pub fn check_wip_limit(&self, status: &str, current_count: usize) -> Result<(), CoreError> {
        if let Some(status_def) = self.statuses.iter().find(|s| s.name == status)
            && let Some(limit) = status_def.wip_limit
            && current_count >= limit
        {
            warn!(status, limit, current_count, "WIP limit exceeded");
            return Err(CoreError::WipLimitExceeded {
                column: status.to_string(),
                limit,
                current: current_count,
            });
        }
        Ok(())
    }

    /// Get the initial (first) status for new items.
    pub fn initial_status(&self) -> Result<String, CoreError> {
        self.statuses
            .iter()
            .min_by_key(|s| s.order)
            .map(|s| s.name.clone())
            .ok_or(CoreError::EmptyWorkflow)
    }

    /// Get all status names in order.
    pub fn status_names(&self) -> Vec<String> {
        let mut statuses = self.statuses.clone();
        statuses.sort_by_key(|s| s.order);
        statuses.into_iter().map(|s| s.name).collect()
    }

    /// Return the name of the first Done-category status (by order), or None.
    pub fn find_first_done_status(&self) -> Option<&str> {
        self.statuses
            .iter()
            .filter(|s| s.category == StatusCategory::Done)
            .min_by_key(|s| s.order)
            .map(|s| s.name.as_str())
    }

    /// Return true if `status` maps to the Done category in this workflow.
    pub fn is_done_status(&self, status: &str) -> bool {
        self.statuses
            .iter()
            .any(|s| s.name == status && s.category == StatusCategory::Done)
    }

    /// Pure decision: should the parent item be marked complete?
    /// The caller is responsible for querying whether all siblings are done.
    pub fn should_complete_parent(all_siblings_done: bool) -> bool {
        all_siblings_done
    }

    /// Validate workflow shape: must have at least one status in each category.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.statuses.is_empty() {
            return Err(CoreError::InvalidWorkflow(
                "workflow must have at least one status".to_string(),
            ));
        }
        let has_todo = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::Todo);
        let has_in_progress = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::InProgress);
        let has_done = self
            .statuses
            .iter()
            .any(|s| s.category == StatusCategory::Done);

        if !has_todo {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'todo' status".to_string(),
            ));
        }
        if !has_in_progress {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'in_progress' status".to_string(),
            ));
        }
        if !has_done {
            return Err(CoreError::InvalidWorkflow(
                "workflow must include at least one 'done' status".to_string(),
            ));
        }

        // Duplicate status names are not allowed
        let mut seen = std::collections::HashSet::new();
        for s in &self.statuses {
            if !seen.insert(s.name.as_str()) {
                return Err(CoreError::InvalidWorkflow(format!(
                    "duplicate status name '{}'",
                    s.name
                )));
            }
        }

        Ok(())
    }
}

// ─── Preset Workflows ───────────────────────────────────────

pub fn workflow_for_type(project_type: &ProjectType) -> WorkflowConfig {
    match project_type {
        ProjectType::Software | ProjectType::Web | ProjectType::Mobile => scrum_workflow(),
        ProjectType::Construction => construction_workflow(),
        ProjectType::Personal | ProjectType::Homework => simple_workflow(),
        ProjectType::Maintenance => kanban_workflow(),
        ProjectType::Legal => legal_workflow(),
        ProjectType::Research => research_workflow(),
        ProjectType::Event => event_workflow(),
        ProjectType::Custom => simple_workflow(),
    }
}

pub fn scrum_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Scrum,
        statuses: vec![
            StatusDef {
                name: "Backlog".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "To Do".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(5),
                order: 2,
            },
            StatusDef {
                name: "In Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 3,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: None, // Allow all transitions by default
    }
}

pub fn kanban_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Kanban,
        statuses: vec![
            StatusDef {
                name: "Queue".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 1,
            },
            StatusDef {
                name: "Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(2),
                order: 2,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 3,
            },
        ],
        transitions: None,
    }
}

pub fn simple_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Simple,
        statuses: vec![
            StatusDef {
                name: "To Do".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Doing".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 2,
            },
        ],
        transitions: None,
    }
}

pub fn construction_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Construction,
        statuses: vec![
            StatusDef {
                name: "Permit".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Procurement".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Build".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 2,
            },
            StatusDef {
                name: "Inspect".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 3,
            },
            StatusDef {
                name: "Handover".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: Some(vec![
            Transition {
                from: "Permit".into(),
                to: "Procurement".into(),
            },
            Transition {
                from: "Procurement".into(),
                to: "Build".into(),
            },
            Transition {
                from: "Build".into(),
                to: "Inspect".into(),
            },
            Transition {
                from: "Inspect".into(),
                to: "Handover".into(),
            },
            Transition {
                from: "Inspect".into(),
                to: "Build".into(),
            }, // rework
        ]),
    }
}

/// Build a linear, phase-based workflow from an ordered list of
/// `(status_name, category)` stages. Consecutive stages get a forward
/// transition; the optional `rework` pair adds one backward transition
/// (typically `Inspect → <earlier build phase>`), mirroring the
/// `construction_workflow` Inspect→Build rework pattern.
///
/// Used by the construction sub-preset builders (wood-frame, steel-frame,
/// SIP-panel) which all reuse `WorkflowType::Construction`.
fn linear_phase_workflow(
    workflow_type: WorkflowType,
    stages: &[(&str, StatusCategory)],
    rework: Option<(&str, &str)>,
) -> WorkflowConfig {
    let statuses = stages
        .iter()
        .enumerate()
        .map(|(i, (name, category))| StatusDef {
            name: (*name).into(),
            category: category.clone(),
            wip_limit: None,
            order: i as i32,
        })
        .collect();

    let mut transitions: Vec<Transition> = stages
        .windows(2)
        .map(|w| Transition {
            from: w[0].0.into(),
            to: w[1].0.into(),
        })
        .collect();

    if let Some((from, to)) = rework {
        transitions.push(Transition {
            from: from.into(),
            to: to.into(),
        });
    }

    WorkflowConfig {
        workflow_type,
        statuses,
        transitions: Some(transitions),
    }
}

/// Wood-frame construction sub-preset. Linear stick-frame sequence with a
/// rework loop from Inspect back to Finish.
pub fn wood_frame_workflow() -> WorkflowConfig {
    use StatusCategory::*;
    linear_phase_workflow(
        WorkflowType::Construction,
        &[
            ("Permit", Todo),
            ("Foundation", InProgress),
            ("Framing", InProgress),
            ("Rough-In (MEP)", InProgress),
            ("Insulation & Drywall", InProgress),
            ("Finish", InProgress),
            ("Inspect", InProgress),
            ("Handover", Done),
        ],
        Some(("Inspect", "Finish")),
    )
}

/// Steel-frame construction sub-preset. Linear structural-steel sequence with
/// a rework loop from Inspect back to Fireproofing.
pub fn steel_frame_workflow() -> WorkflowConfig {
    use StatusCategory::*;
    linear_phase_workflow(
        WorkflowType::Construction,
        &[
            ("Permit", Todo),
            ("Engineering", Todo),
            ("Fabrication", InProgress),
            ("Erection", InProgress),
            ("Decking/MEP", InProgress),
            ("Fireproofing", InProgress),
            ("Inspect", InProgress),
            ("Handover", Done),
        ],
        Some(("Inspect", "Fireproofing")),
    )
}

/// SIP (structural insulated panel) construction sub-preset. Panelized build
/// sequence with a rework loop from Inspect back to MEP Chases.
pub fn sip_panel_workflow() -> WorkflowConfig {
    use StatusCategory::*;
    linear_phase_workflow(
        WorkflowType::Construction,
        &[
            ("Design/Shop Drawings", Todo),
            ("Panel Fabrication", Todo),
            ("Delivery", InProgress),
            ("Foundation", InProgress),
            ("Panel Set", InProgress),
            ("Seal & Penetrations", InProgress),
            ("MEP Chases", InProgress),
            ("Inspect", InProgress),
            ("Handover", Done),
        ],
        Some(("Inspect", "MEP Chases")),
    )
}

/// Legal case management: a staged matter lifecycle from intake to close.
/// Linear with a review→drafting rework loop, mirroring the construction model.
pub fn legal_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Legal,
        statuses: vec![
            StatusDef {
                name: "Intake".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Discovery".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Drafting".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 2,
            },
            StatusDef {
                name: "Review".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 3,
            },
            StatusDef {
                name: "Closed".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: Some(vec![
            Transition {
                from: "Intake".into(),
                to: "Discovery".into(),
            },
            Transition {
                from: "Discovery".into(),
                to: "Drafting".into(),
            },
            Transition {
                from: "Drafting".into(),
                to: "Review".into(),
            },
            Transition {
                from: "Review".into(),
                to: "Closed".into(),
            },
            Transition {
                from: "Review".into(),
                to: "Drafting".into(),
            }, // revise after review
        ]),
    }
}

/// Research / lab study: hypothesis → design → experiment → analysis → published.
/// A Kanban-style flow with WIP caps on the active stages.
pub fn research_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Research,
        statuses: vec![
            StatusDef {
                name: "Hypothesis".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Design".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "Experiment".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(4),
                order: 2,
            },
            StatusDef {
                name: "Analysis".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(3),
                order: 3,
            },
            StatusDef {
                name: "Published".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: None,
    }
}

/// Event planning: ideas → booked → in progress → confirmed → done.
pub fn event_workflow() -> WorkflowConfig {
    WorkflowConfig {
        workflow_type: WorkflowType::Event,
        statuses: vec![
            StatusDef {
                name: "Ideas".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 0,
            },
            StatusDef {
                name: "Booked".into(),
                category: StatusCategory::Todo,
                wip_limit: None,
                order: 1,
            },
            StatusDef {
                name: "In Progress".into(),
                category: StatusCategory::InProgress,
                wip_limit: Some(5),
                order: 2,
            },
            StatusDef {
                name: "Confirmed".into(),
                category: StatusCategory::InProgress,
                wip_limit: None,
                order: 3,
            },
            StatusDef {
                name: "Done".into(),
                category: StatusCategory::Done,
                wip_limit: None,
                order: 4,
            },
        ],
        transitions: None,
    }
}

#[cfg(test)]
#[path = "workflow/tests.rs"]
mod tests;

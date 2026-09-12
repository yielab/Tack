use super::*;

// ── initial_status ───────────────────────────────────────

#[test]
fn initial_status_scrum_is_backlog() {
    assert_eq!(scrum_workflow().initial_status().unwrap(), "Backlog");
}

#[test]
fn initial_status_simple_is_todo() {
    assert_eq!(simple_workflow().initial_status().unwrap(), "To Do");
}

#[test]
fn initial_status_construction_is_permit() {
    assert_eq!(construction_workflow().initial_status().unwrap(), "Permit");
}

#[test]
fn initial_status_kanban_is_queue() {
    assert_eq!(kanban_workflow().initial_status().unwrap(), "Queue");
}

#[test]
fn initial_status_legal_is_intake() {
    assert_eq!(legal_workflow().initial_status().unwrap(), "Intake");
}

#[test]
fn initial_status_research_is_hypothesis() {
    assert_eq!(research_workflow().initial_status().unwrap(), "Hypothesis");
}

#[test]
fn initial_status_event_is_ideas() {
    assert_eq!(event_workflow().initial_status().unwrap(), "Ideas");
}

// ── construction sub-presets ─────────────────────────────

#[test]
fn construction_subpresets_are_construction_type_and_start_correctly() {
    assert_eq!(
        wood_frame_workflow().workflow_type,
        WorkflowType::Construction
    );
    assert_eq!(
        steel_frame_workflow().workflow_type,
        WorkflowType::Construction
    );
    assert_eq!(
        sip_panel_workflow().workflow_type,
        WorkflowType::Construction
    );

    assert_eq!(wood_frame_workflow().initial_status().unwrap(), "Permit");
    assert_eq!(steel_frame_workflow().initial_status().unwrap(), "Permit");
    assert_eq!(
        sip_panel_workflow().initial_status().unwrap(),
        "Design/Shop Drawings"
    );
}

#[test]
fn wood_frame_linear_transitions_and_rework() {
    let wf = wood_frame_workflow();
    // Legal linear step
    assert!(wf.validate_transition("Framing", "Rough-In (MEP)").is_ok());
    // Rework loop back from Inspect
    assert!(wf.validate_transition("Inspect", "Finish").is_ok());
    // Forward from Inspect to Handover
    assert!(wf.validate_transition("Inspect", "Handover").is_ok());
    // Illegal skip (Permit → Handover) is rejected
    assert!(wf.validate_transition("Permit", "Handover").is_err());
}

#[test]
fn steel_frame_linear_transitions_and_rework() {
    let wf = steel_frame_workflow();
    assert!(wf.validate_transition("Erection", "Decking/MEP").is_ok());
    assert!(wf.validate_transition("Inspect", "Fireproofing").is_ok());
    // Illegal backward jump not defined as rework
    assert!(wf.validate_transition("Fireproofing", "Permit").is_err());
}

#[test]
fn sip_panel_linear_transitions_and_rework() {
    let wf = sip_panel_workflow();
    assert!(
        wf.validate_transition("Panel Set", "Seal & Penetrations")
            .is_ok()
    );
    assert!(wf.validate_transition("Inspect", "MEP Chases").is_ok());
    // Illegal skip is rejected
    assert!(
        wf.validate_transition("Design/Shop Drawings", "Handover")
            .is_err()
    );
}

#[test]
fn workflow_for_type_maps_new_domains() {
    use crate::models::ProjectType;
    assert_eq!(
        workflow_for_type(&ProjectType::Legal).workflow_type,
        WorkflowType::Legal
    );
    assert_eq!(
        workflow_for_type(&ProjectType::Research).workflow_type,
        WorkflowType::Research
    );
    assert_eq!(
        workflow_for_type(&ProjectType::Event).workflow_type,
        WorkflowType::Event
    );
}

#[test]
fn legal_review_can_revise_or_close() {
    let wf = legal_workflow();
    assert!(wf.validate_transition("Review", "Closed").is_ok());
    assert!(wf.validate_transition("Review", "Drafting").is_ok());
    // Skipping a stage is not allowed under the linear matter lifecycle.
    assert!(wf.validate_transition("Intake", "Closed").is_err());
}

#[test]
fn initial_status_empty_workflow_returns_err() {
    let wf = WorkflowConfig {
        workflow_type: WorkflowType::Custom,
        statuses: vec![],
        transitions: None,
    };
    assert!(wf.initial_status().is_err());
}

// ── status_names ─────────────────────────────────────────

#[test]
fn status_names_scrum_ordered() {
    let names = scrum_workflow().status_names();
    assert_eq!(
        names,
        vec!["Backlog", "To Do", "In Progress", "In Review", "Done"]
    );
}

// ── validate_transition (scrum — open, any → any if both exist) ──

#[test]
fn scrum_allows_any_valid_status_pair() {
    let wf = scrum_workflow();
    assert!(wf.validate_transition("Backlog", "In Progress").is_ok());
    assert!(wf.validate_transition("In Progress", "Done").is_ok());
    assert!(wf.validate_transition("Done", "Backlog").is_ok()); // reopen
    assert!(wf.validate_transition("In Review", "Backlog").is_ok()); // send back
}

#[test]
fn scrum_rejects_unknown_from_status() {
    let wf = scrum_workflow();
    assert!(wf.validate_transition("Nonexistent", "Done").is_err());
}

#[test]
fn scrum_rejects_unknown_to_status() {
    let wf = scrum_workflow();
    assert!(wf.validate_transition("Backlog", "Shipped").is_err());
}

// ── validate_transition (construction — strict linear) ──────────

#[test]
fn construction_allows_each_forward_step() {
    let wf = construction_workflow();
    assert!(wf.validate_transition("Permit", "Procurement").is_ok());
    assert!(wf.validate_transition("Procurement", "Build").is_ok());
    assert!(wf.validate_transition("Build", "Inspect").is_ok());
    assert!(wf.validate_transition("Inspect", "Handover").is_ok());
}

#[test]
fn construction_allows_rework_inspect_to_build() {
    let wf = construction_workflow();
    assert!(wf.validate_transition("Inspect", "Build").is_ok());
}

#[test]
fn construction_rejects_skipping_stages() {
    let wf = construction_workflow();
    assert!(wf.validate_transition("Permit", "Handover").is_err());
    assert!(wf.validate_transition("Permit", "Build").is_err());
    assert!(wf.validate_transition("Procurement", "Handover").is_err());
    assert!(wf.validate_transition("Build", "Permit").is_err()); // can't go back to start
}

// ── check_wip_limit ──────────────────────────────────────

#[test]
fn wip_under_limit_ok() {
    let wf = scrum_workflow(); // In Progress limit = 5
    assert!(wf.check_wip_limit("In Progress", 4).is_ok());
}

#[test]
fn wip_exactly_at_limit_fails() {
    let wf = scrum_workflow(); // In Progress limit = 5
    assert!(wf.check_wip_limit("In Progress", 5).is_err());
}

#[test]
fn wip_over_limit_fails() {
    let wf = scrum_workflow();
    assert!(wf.check_wip_limit("In Progress", 99).is_err());
}

#[test]
fn wip_no_limit_set_always_ok() {
    let wf = scrum_workflow(); // Backlog has no WIP limit
    assert!(wf.check_wip_limit("Backlog", 10_000).is_ok());
}

#[test]
fn wip_unknown_status_ok() {
    // Unknown status has no limit entry — should not error
    let wf = scrum_workflow();
    assert!(wf.check_wip_limit("Nonexistent", 99).is_ok());
}

#[test]
fn kanban_in_progress_limit_three() {
    let wf = kanban_workflow(); // In Progress limit = 3
    assert!(wf.check_wip_limit("In Progress", 2).is_ok());
    assert!(wf.check_wip_limit("In Progress", 3).is_err());
}

// ── find_first_done_status ────────────────────────────────

#[test]
fn done_status_scrum_is_done() {
    assert_eq!(scrum_workflow().find_first_done_status(), Some("Done"));
}

#[test]
fn done_status_simple_is_done() {
    assert_eq!(simple_workflow().find_first_done_status(), Some("Done"));
}

#[test]
fn done_status_kanban_is_done() {
    assert_eq!(kanban_workflow().find_first_done_status(), Some("Done"));
}

#[test]
fn done_status_construction_is_handover() {
    assert_eq!(
        construction_workflow().find_first_done_status(),
        Some("Handover")
    );
}

#[test]
fn done_status_none_when_no_done_category() {
    let wf = WorkflowConfig {
        workflow_type: WorkflowType::Custom,
        statuses: vec![StatusDef {
            name: "Open".into(),
            category: StatusCategory::Todo,
            wip_limit: None,
            order: 0,
        }],
        transitions: None,
    };
    assert_eq!(wf.find_first_done_status(), None);
}

// ── is_done_status ────────────────────────────────────────

#[test]
fn is_done_status_true_for_done() {
    assert!(scrum_workflow().is_done_status("Done"));
}

#[test]
fn is_done_status_false_for_in_progress() {
    assert!(!scrum_workflow().is_done_status("In Progress"));
}

#[test]
fn is_done_status_false_for_unknown() {
    assert!(!scrum_workflow().is_done_status("Nonexistent"));
}

#[test]
fn construction_handover_is_done() {
    assert!(construction_workflow().is_done_status("Handover"));
    assert!(!construction_workflow().is_done_status("Build"));
}

// ── should_complete_parent ───────────────────────────────

#[test]
fn should_complete_parent_true_when_all_siblings_done() {
    assert!(WorkflowConfig::should_complete_parent(true));
}

#[test]
fn should_complete_parent_false_when_siblings_incomplete() {
    assert!(!WorkflowConfig::should_complete_parent(false));
}

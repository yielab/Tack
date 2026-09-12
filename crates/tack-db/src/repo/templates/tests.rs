use super::*;

async fn test_pool() -> SqlitePool {
    let pool = crate::init_pool("sqlite::memory:")
        .await
        .expect("in-memory pool");
    crate::migrations::run_all(&pool).await.expect("migrations");
    pool
}

#[tokio::test]
async fn seeds_three_construction_verticals_with_fields_and_workflows() {
    let pool = test_pool().await;
    seed_builtin_templates(&pool).await.expect("seed");
    // Re-running is idempotent (per-name dedup).
    seed_builtin_templates(&pool).await.expect("re-seed");

    let construction = list_templates(&pool, Some(ProjectType::Construction))
        .await
        .expect("list");

    // Base + three verticals, all ProjectType::Construction.
    for name in [
        "Construction Project",
        "Wood Frame Build",
        "Steel Frame Build",
        "SIP Panel Build",
    ] {
        let t = construction
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("missing template {name}"));
        assert_eq!(t.project_type, ProjectType::Construction);
        assert!(t.is_builtin);
    }

    // Exactly one of each name (idempotent seeding, no duplicates).
    assert_eq!(
        construction
            .iter()
            .filter(|t| t.name == "SIP Panel Build")
            .count(),
        1
    );

    let wood = construction
        .iter()
        .find(|t| t.name == "Wood Frame Build")
        .unwrap();
    // Construction vocabulary base is preserved.
    assert_eq!(
        wood.vocabulary.get("task").map(String::as_str),
        Some("Work Order")
    );
    assert_eq!(
        wood.vocabulary.get("sprint").map(String::as_str),
        Some("Phase")
    );
    // Build-system-specific custom fields present, incl. the select options.
    assert_eq!(wood.custom_fields.len(), 4);
    let stud = wood
        .custom_fields
        .iter()
        .find(|f| f.name == "Stud Spacing")
        .expect("stud spacing field");
    assert_eq!(stud.field_type, CustomFieldType::Select);
    assert_eq!(
        stud.options.as_deref(),
        Some(["16\" o.c.".to_string(), "24\" o.c.".to_string()].as_slice())
    );

    // SIP panel count is a Number field.
    let sip = construction
        .iter()
        .find(|t| t.name == "SIP Panel Build")
        .unwrap();
    let panel_count = sip
        .custom_fields
        .iter()
        .find(|f| f.name == "Panel Count")
        .expect("panel count field");
    assert_eq!(panel_count.field_type, CustomFieldType::Number);
}

#[tokio::test]
async fn seeded_vertical_workflows_enforce_transitions() {
    let pool = test_pool().await;
    seed_builtin_templates(&pool).await.expect("seed");

    let construction = list_templates(&pool, Some(ProjectType::Construction))
        .await
        .expect("list");

    let steel = construction
        .iter()
        .find(|t| t.name == "Steel Frame Build")
        .unwrap();

    // Linear step is allowed.
    assert!(
        steel
            .workflow
            .validate_transition("Erection", "Decking/MEP")
            .is_ok()
    );
    // Rework loop back from Inspect is allowed.
    assert!(
        steel
            .workflow
            .validate_transition("Inspect", "Fireproofing")
            .is_ok()
    );
    // Illegal skip is rejected.
    assert!(
        steel
            .workflow
            .validate_transition("Permit", "Handover")
            .is_err()
    );
}

// ─── Orchestration block ─────────────────────

#[tokio::test]
async fn builtin_templates_have_no_orchestration_block() {
    // Backward compatibility: every built-in predates this field and
    // seeds through a code path that never sets it — the column stays
    // NULL, and NULL must deserialize to `None`, not a default-valued
    // `Some(TemplateOrchestration::default())`.
    let pool = test_pool().await;
    seed_builtin_templates(&pool).await.expect("seed");

    let all = list_templates(&pool, None).await.expect("list");
    assert!(!all.is_empty());
    for t in &all {
        assert!(
            t.orchestration.is_none(),
            "built-in template {:?} should have no orchestration block",
            t.name
        );
    }
}

#[tokio::test]
async fn create_template_without_orchestration_round_trips_to_none() {
    let pool = test_pool().await;
    let created = create_template(
        &pool,
        CreateProjectTemplate {
            name: "Plain Template".to_string(),
            description: None,
            project_type: ProjectType::Software,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            default_boards: None,
            orchestration: None,
        },
    )
    .await
    .expect("create");
    assert!(created.orchestration.is_none());

    // Re-fetch by id — exercises the SELECT/parse path, not just the
    // value handed back from the INSERT's own read-after-write.
    let fetched = get_template(&pool, created.id).await.expect("get");
    assert!(fetched.orchestration.is_none());
}

#[tokio::test]
async fn create_template_with_orchestration_round_trips_through_get_and_list() {
    let pool = test_pool().await;
    let orch = TemplateOrchestration {
        blueprint: OrchBlueprint::AgenticProduct,
        pipeline_yaml: Some("name: demo\nsteps:\n  - id: lead\n".to_string()),
        pipeline_file: None,
        verify_cmd: Some("cargo test --workspace".to_string()),
        budget_usd: Some(25.0),
        status_map: TemplateStatusMap {
            dispatch_from: vec!["To Do".to_string()],
            on_running: Some("In Progress".to_string()),
            on_waiting_approval: None,
            on_succeeded: Some("Done".to_string()),
            on_failed: None,
            on_cancelled: None,
        },
        auto_dispatch: true,
        pod_shape: Some("full".to_string()),
    };

    let created = create_template(
        &pool,
        CreateProjectTemplate {
            name: "Agentic Product Template".to_string(),
            description: None,
            project_type: ProjectType::Software,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            default_boards: None,
            orchestration: Some(orch.clone()),
        },
    )
    .await
    .expect("create");

    assert_eq!(created.orchestration.as_ref(), Some(&orch));

    let fetched = get_template(&pool, created.id).await.expect("get");
    assert_eq!(fetched.orchestration.as_ref(), Some(&orch));

    let listed = list_templates(&pool, Some(ProjectType::Software))
        .await
        .expect("list");
    let found = listed
        .iter()
        .find(|t| t.id == created.id)
        .expect("template present in list");
    assert_eq!(found.orchestration.as_ref(), Some(&orch));
}

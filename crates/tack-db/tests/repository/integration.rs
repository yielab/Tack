use crate::common::{self, create_test_workspace, setup_test_db};
use tack_core::models::*;
use tack_core::vocabulary;
use tack_core::workflow;

/// A minimal text field, for tests that only need *a* custom field to exist.
fn text_field(name: &str) -> CreateCustomField {
    CreateCustomField {
        name: name.into(),
        field_type: CustomFieldType::Text,
        description: None,
        required: Some(false),
        default_value: None,
        options: None,
        validation: None,
    }
}

// ─── GitHub link Tests ────────────────────────────

#[tokio::test]
async fn github_link_round_trip_and_upsert() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "P".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();
    let status = project.workflow.initial_status().unwrap().to_string();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Issue item".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    // No link initially.
    assert_eq!(repo.get_github_link(item.id).await.unwrap(), None);

    // Set, then read back.
    repo.set_github_link(item.id, "acme/widgets", 42)
        .await
        .unwrap();
    assert_eq!(
        repo.get_github_link(item.id).await.unwrap(),
        Some(("acme/widgets".to_string(), 42))
    );

    // Upsert replaces the existing link (no duplicate-key error).
    repo.set_github_link(item.id, "acme/widgets", 99)
        .await
        .unwrap();
    assert_eq!(
        repo.get_github_link(item.id).await.unwrap(),
        Some(("acme/widgets".to_string(), 99))
    );
}

// ─── Project Tests ───────────────────────────────────────────

#[tokio::test]
async fn create_and_get_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Test Project".into(),
                description: Some("A test project".into()),
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(project.name, "Test Project");
    assert_eq!(project.project_type, ProjectType::Software);

    let fetched = repo.get_project(project.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Test Project");
}

#[tokio::test]
async fn list_projects() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    for i in 0..3 {
        repo.create_project(
            ws_id,
            CreateProject {
                name: format!("Project {i}"),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();
    }

    let projects = repo.list_projects(ws_id).await.unwrap();
    assert_eq!(projects.len(), 3);
}

#[tokio::test]
async fn update_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Old Name".into(),
                description: None,
                project_type: ProjectType::Personal,
                template: None,
            },
        )
        .await
        .unwrap();

    let updated = repo
        .update_project(
            project.id,
            UpdateProject {
                name: Some("New Name".into()),
                description: None,
                vocabulary: None,
                workflow: None,
                default_model: None,
                archived: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.name, "New Name");
}

#[tokio::test]
async fn delete_project() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "To Delete".into(),
                description: None,
                project_type: ProjectType::Custom,
                template: None,
            },
        )
        .await
        .unwrap();

    assert!(repo.delete_project(project.id).await.unwrap());
    assert!(repo.get_project(project.id).await.unwrap().is_none());
}

// ─── Item Tests ──────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_items() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let initial_status = project.workflow.initial_status().unwrap();

    // Create parent epic
    let epic = repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: "Epic One".into(),
                description: Some("An epic".into()),
                item_type: Some(ItemType::Epic),
                parent_id: None,
                priority: Some(Priority::High),
                estimate: Some(13.0),
                estimate_unit: None,
                tags: Some(vec!["backend".into()]),
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(epic.title, "Epic One");
    assert_eq!(epic.item_type, ItemType::Epic);
    assert_eq!(epic.priority, Priority::High);

    // Create child task under epic
    let task = repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: "Task under epic".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: Some(epic.id),
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(task.parent_id, Some(epic.id));

    // List items
    let items = repo
        .list_items(project.id, &ItemFilter::default())
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn update_item_status() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Status Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    let item = repo
        .create_item(
            project.id,
            "Backlog",
            CreateItem {
                title: "Move me".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(item.status, "Backlog");

    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                status: Some("In Progress".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.status, "In Progress");
}

#[tokio::test]
async fn item_tree() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Tree Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    let epic = repo
        .create_item(
            project.id,
            "Backlog",
            CreateItem {
                title: "Root Epic".into(),
                description: None,
                item_type: Some(ItemType::Epic),
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    repo.create_item(
        project.id,
        "Backlog",
        CreateItem {
            title: "Child Task".into(),
            description: None,
            item_type: Some(ItemType::Task),
            parent_id: Some(epic.id),
            priority: None,
            estimate: None,
            estimate_unit: None,
            tags: None,
            due_date: None,
            sprint_id: None,
            assignee: None,
        },
    )
    .await
    .unwrap();

    let tree = repo.get_item_tree(project.id).await.unwrap();
    assert_eq!(tree.len(), 2);
    // Root items come first (parent_id is NULL)
    assert!(tree[0].parent_id.is_none());
}

// ─── Sprint Tests ────────────────────────────────────────────

#[tokio::test]
async fn sprint_lifecycle() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Sprint Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    let sprint = repo
        .create_sprint(
            project.id,
            CreateSprint {
                name: "Sprint 1".into(),
                goal: Some("Ship MVP".into()),
                start_date: None,
                end_date: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(sprint.status, SprintStatus::Planning);

    repo.update_sprint_status(sprint.id, SprintStatus::Active)
        .await
        .unwrap();

    let fetched = repo.get_sprint(sprint.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, SprintStatus::Active);
}

// ─── Role Tests ──────────────────────────────────────────────

#[tokio::test]
async fn roles_and_assignment() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Role Test".into(),
                description: None,
                project_type: ProjectType::Construction,
                template: None,
            },
        )
        .await
        .unwrap();

    let role = repo
        .create_role(
            project.id,
            CreateRole {
                name: "Electrician".into(),
                color: Some("#f59e0b".into()),
                icon: None,
            },
        )
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: "Wire the kitchen".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    // Assign role
    repo.assign_role_to_item(item.id, role.id).await.unwrap();

    let roles = repo.get_roles_for_item(item.id).await.unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].name, "Electrician");

    // Remove role
    repo.remove_role_from_item(item.id, role.id).await.unwrap();
    let roles = repo.get_roles_for_item(item.id).await.unwrap();
    assert_eq!(roles.len(), 0);
}

// ─── Comment Tests ───────────────────────────────────────────

#[tokio::test]
async fn comments() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Comment Test".into(),
                description: None,
                project_type: ProjectType::Personal,
                template: None,
            },
        )
        .await
        .unwrap();

    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: "Commentable".into(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .unwrap();

    repo.create_comment(
        item.id,
        CreateComment {
            content: "First comment".into(),
            author: Some("Alice".into()),
        },
    )
    .await
    .unwrap();

    repo.create_comment(
        item.id,
        CreateComment {
            content: "Second comment".into(),
            author: None,
        },
    )
    .await
    .unwrap();

    let comments = repo.list_comments(item.id).await.unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].content, "First comment");
}

// ─── Vocabulary Tests ────────────────────────────────────────

#[tokio::test]
async fn project_vocabulary_by_type() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let construction = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Building Project".into(),
                description: None,
                project_type: ProjectType::Construction,
                template: None,
            },
        )
        .await
        .unwrap();

    // Construction vocabulary should use domain-specific terms
    assert_eq!(
        vocabulary::resolve(&construction.vocabulary, "task"),
        "Work Order"
    );
    assert_eq!(
        vocabulary::resolve(&construction.vocabulary, "sprint"),
        "Phase"
    );

    let homework = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Math Homework".into(),
                description: None,
                project_type: ProjectType::Homework,
                template: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        vocabulary::resolve(&homework.vocabulary, "task"),
        "Assignment"
    );
}

// ─── Workflow Tests ──────────────────────────────────────────

#[test]
fn workflow_transition_validation() {
    let wf = workflow::construction_workflow();

    // Valid: Permit -> Procurement
    assert!(wf.validate_transition("Permit", "Procurement").is_ok());

    // Invalid: Permit -> Handover (skipping steps)
    assert!(wf.validate_transition("Permit", "Handover").is_err());

    // Valid: Inspect -> Build (rework loop)
    assert!(wf.validate_transition("Inspect", "Build").is_ok());
}

#[test]
fn wip_limits() {
    let wf = workflow::scrum_workflow();

    // In Progress has WIP limit of 5
    assert!(wf.check_wip_limit("In Progress", 4).is_ok());
    assert!(wf.check_wip_limit("In Progress", 5).is_err());

    // Backlog has no limit
    assert!(wf.check_wip_limit("Backlog", 9999).is_ok());
}

// ─── Template Tests (v1.2) ───────────────────────────────────

#[tokio::test]
async fn create_and_get_template() {
    let repo = setup_test_db().await;

    use tack_core::models::CreateProjectTemplate;
    use tack_db::repo::templates;

    let template_data = CreateProjectTemplate {
        name: "My Software Template".into(),
        description: Some("A custom scrum template".into()),
        project_type: ProjectType::Software,
        vocabulary: None,
        workflow: None,
        custom_fields: None,
        default_boards: None,
        orchestration: None,
    };

    let template = templates::create_template(repo.pool(), template_data)
        .await
        .unwrap();

    assert_eq!(template.name, "My Software Template");
    assert_eq!(template.project_type, ProjectType::Software);
    assert!(!template.is_builtin);

    let fetched = templates::get_template(repo.pool(), template.id)
        .await
        .unwrap();
    assert_eq!(fetched.name, "My Software Template");
}

#[tokio::test]
async fn list_templates_with_filter() {
    let repo = setup_test_db().await;

    use tack_core::models::CreateProjectTemplate;
    use tack_db::repo::templates;

    // Create templates of different types
    templates::create_template(
        repo.pool(),
        CreateProjectTemplate {
            name: "Software Template".into(),
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
    .unwrap();

    templates::create_template(
        repo.pool(),
        CreateProjectTemplate {
            name: "Construction Template".into(),
            description: None,
            project_type: ProjectType::Construction,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            default_boards: None,
            orchestration: None,
        },
    )
    .await
    .unwrap();

    // List all templates
    let all = templates::list_templates(repo.pool(), None).await.unwrap();
    assert_eq!(all.len(), 2);

    // Filter by type
    let software_only = templates::list_templates(repo.pool(), Some(ProjectType::Software))
        .await
        .unwrap();
    assert_eq!(software_only.len(), 1);
    assert_eq!(software_only[0].name, "Software Template");
}

#[tokio::test]
async fn delete_template_not_builtin() {
    let repo = setup_test_db().await;

    use tack_core::models::CreateProjectTemplate;
    use tack_db::repo::templates;

    let user_template = templates::create_template(
        repo.pool(),
        CreateProjectTemplate {
            name: "User Template".into(),
            description: None,
            project_type: ProjectType::Personal,
            vocabulary: None,
            workflow: None,
            custom_fields: None,
            default_boards: None,
            orchestration: None,
        },
    )
    .await
    .unwrap();

    // Should be able to delete user template
    templates::delete_template(repo.pool(), user_template.id)
        .await
        .unwrap();

    // Verify deleted
    let result = templates::get_template(repo.pool(), user_template.id).await;
    assert!(result.is_err());
}

// ─── Custom Fields Tests (v1.2) ──────────────────────────────

#[tokio::test]
async fn create_and_list_custom_fields() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;

    use tack_db::repo::custom_fields;

    // Create a text field
    let field1 = custom_fields::create_field(repo.pool(), project.id, text_field("Customer"))
        .await
        .unwrap();

    assert_eq!(field1.name, "Customer");
    assert_eq!(field1.field_type, CustomFieldType::Text);
    assert!(!field1.required);

    // Create a select field with options
    let field2 = custom_fields::create_field(
        repo.pool(),
        project.id,
        CreateCustomField {
            name: "Priority Level".into(),
            field_type: CustomFieldType::Select,
            description: None,
            required: Some(true),
            default_value: None,
            options: Some(vec!["Low".into(), "Medium".into(), "High".into()]),
            validation: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(field2.field_type, CustomFieldType::Select);
    assert!(field2.required);
    assert_eq!(field2.options.as_ref().unwrap().len(), 3);

    // List fields
    let fields = custom_fields::list_fields_for_project(repo.pool(), project.id)
        .await
        .unwrap();
    assert_eq!(fields.len(), 2);
}

#[tokio::test]
async fn custom_field_value_upsert() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    use tack_db::repo::custom_fields;

    let field = custom_fields::create_field(repo.pool(), project.id, text_field("Customer"))
        .await
        .unwrap();

    // Set value
    custom_fields::set_field_value(
        repo.pool(),
        item.id,
        field.id,
        serde_json::json!("Acme Corp"),
    )
    .await
    .unwrap();

    // Get value
    let value = custom_fields::get_field_value(repo.pool(), item.id, field.id)
        .await
        .unwrap();
    assert_eq!(value.value, serde_json::json!("Acme Corp"));

    // Update value (upsert)
    custom_fields::set_field_value(
        repo.pool(),
        item.id,
        field.id,
        serde_json::json!("Updated Corp"),
    )
    .await
    .unwrap();

    let updated_value = custom_fields::get_field_value(repo.pool(), item.id, field.id)
        .await
        .unwrap();
    assert_eq!(updated_value.value, serde_json::json!("Updated Corp"));
}

#[tokio::test]
async fn custom_field_cascade_delete() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    use tack_db::repo::custom_fields;

    let field = custom_fields::create_field(repo.pool(), project.id, text_field("Test Field"))
        .await
        .unwrap();

    // Set value
    custom_fields::set_field_value(
        repo.pool(),
        item.id,
        field.id,
        serde_json::json!("Test Value"),
    )
    .await
    .unwrap();

    // Delete field - should cascade delete values
    custom_fields::delete_field(repo.pool(), field.id)
        .await
        .unwrap();

    // Value should be gone
    let result = custom_fields::get_field_value(repo.pool(), item.id, field.id).await;
    assert!(result.is_err());
}

// ─── Multiple Boards Tests (v1.2) ────────────────────────────

#[tokio::test]
async fn create_and_list_boards() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Board Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    use tack_core::models::{BoardGrouping, CreateBoard};
    use tack_db::repo::boards;

    // Create main board
    let board1 = boards::create_board(
        repo.pool(),
        project.id,
        CreateBoard {
            name: "Main Board".into(),
            description: Some("Default status board".into()),
            grouping: Some(BoardGrouping::Status),
            filters: None,
            is_default: Some(true),
        },
    )
    .await
    .unwrap();

    assert_eq!(board1.name, "Main Board");
    assert!(board1.is_default);

    // Create priority board
    let board2 = boards::create_board(
        repo.pool(),
        project.id,
        CreateBoard {
            name: "Priority View".into(),
            description: None,
            grouping: Some(BoardGrouping::Priority),
            filters: None,
            is_default: Some(false),
        },
    )
    .await
    .unwrap();

    assert!(!board2.is_default);

    // List boards
    let all_boards = boards::list_boards(repo.pool(), project.id).await.unwrap();
    assert_eq!(all_boards.len(), 2);
}

#[tokio::test]
async fn default_board_management() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Default Board Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    use tack_core::models::{BoardGrouping, CreateBoard};
    use tack_db::repo::boards;

    // Create first board as default
    let board1 = boards::create_board(
        repo.pool(),
        project.id,
        CreateBoard {
            name: "Board 1".into(),
            description: None,
            grouping: Some(BoardGrouping::Status),
            filters: None,
            is_default: Some(true),
        },
    )
    .await
    .unwrap();

    // Create second board and make it default
    let board2 = boards::create_board(
        repo.pool(),
        project.id,
        CreateBoard {
            name: "Board 2".into(),
            description: None,
            grouping: Some(BoardGrouping::Priority),
            filters: None,
            is_default: Some(true),
        },
    )
    .await
    .unwrap();

    // Board 1 should no longer be default
    let board1_updated = boards::get_board(repo.pool(), board1.id).await.unwrap();
    assert!(!board1_updated.is_default);

    // Board 2 should be default
    assert!(board2.is_default);

    // Get default board
    let default = boards::get_default_board(repo.pool(), project.id)
        .await
        .unwrap();
    assert_eq!(default.unwrap().id, board2.id);
}

#[tokio::test]
async fn board_grouping_types() {
    let repo = setup_test_db().await;
    let ws_id = create_test_workspace(&repo).await;

    let project = repo
        .create_project(
            ws_id,
            CreateProject {
                name: "Grouping Test".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .unwrap();

    use tack_core::models::{BoardGrouping, CreateBoard};
    use tack_db::repo::boards;

    // Test all grouping types
    let groupings = vec![
        ("Status Board", BoardGrouping::Status),
        ("Priority Board", BoardGrouping::Priority),
        ("Type Board", BoardGrouping::ItemType),
        ("Sprint Board", BoardGrouping::Sprint),
    ];

    for (name, grouping) in groupings {
        let board = boards::create_board(
            repo.pool(),
            project.id,
            CreateBoard {
                name: name.into(),
                description: None,
                grouping: Some(grouping),
                filters: None,
                is_default: Some(false),
            },
        )
        .await
        .unwrap();

        assert_eq!(board.name, name);
    }

    let all_boards = boards::list_boards(repo.pool(), project.id).await.unwrap();
    assert_eq!(all_boards.len(), 4);
}

// ─── Correctness hotfix regressions ─────────────────

/// `update_item` must persist a sprint assignment and later clear it when
/// the PATCH sends `null` (double-`Option` `Some(None)`), while an absent field
/// (outer `None`) leaves the value untouched.
#[tokio::test]
async fn update_item_persists_and_clears_sprint_id() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.sprint_id, None);

    let sprint = repo
        .create_sprint(
            project.id,
            CreateSprint {
                name: "Sprint 1".into(),
                goal: None,
                start_date: None,
                end_date: None,
            },
        )
        .await
        .unwrap();

    // Assign the sprint, then re-fetch: it persists.
    repo.update_item(
        item.id,
        UpdateItem {
            sprint_id: Some(Some(sprint.id)),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        repo.get_item(item.id).await.unwrap().unwrap().sprint_id,
        Some(sprint.id),
        "sprint assignment must survive a re-fetch"
    );

    // An unrelated update (absent sprint_id) must NOT disturb the assignment.
    repo.update_item(
        item.id,
        UpdateItem {
            title: Some("renamed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        repo.get_item(item.id).await.unwrap().unwrap().sprint_id,
        Some(sprint.id),
        "absent sprint_id must leave the value untouched"
    );

    // A `null` clears it.
    repo.update_item(
        item.id,
        UpdateItem {
            sprint_id: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        repo.get_item(item.id).await.unwrap().unwrap().sprint_id,
        None,
        "sprint_id: null must clear the assignment"
    );
}

/// Same absent/set/clear contract for `due_date`.
#[tokio::test]
async fn update_item_persists_and_clears_due_date() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.due_date, None);

    let due = chrono::Utc::now();
    repo.update_item(
        item.id,
        UpdateItem {
            due_date: Some(Some(due)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(
        repo.get_item(item.id)
            .await
            .unwrap()
            .unwrap()
            .due_date
            .is_some(),
        "due_date must persist across a re-fetch"
    );

    repo.update_item(
        item.id,
        UpdateItem {
            due_date: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        repo.get_item(item.id).await.unwrap().unwrap().due_date,
        None,
        "due_date: null must clear it"
    );
}

/// Moving an item across status categories maintains started_at /
/// completed_at: enter in-progress → stamp started_at; enter done → stamp
/// completed_at; leave done → clear completed_at (keeping started_at).
#[tokio::test]
async fn status_transition_timestamps() {
    use tack_core::workflow::StatusCategory;

    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert!(item.started_at.is_none());
    assert!(item.completed_at.is_none());

    // → In Progress: started_at stamped, completed_at still empty.
    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                status: Some("In Progress".into()),
                status_category: Some(StatusCategory::InProgress),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        updated.started_at.is_some(),
        "started_at must be set on in-progress"
    );
    assert!(updated.completed_at.is_none());
    let started = updated.started_at;

    // → Done: completed_at stamped, started_at preserved.
    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                status: Some("Done".into()),
                status_category: Some(StatusCategory::Done),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        updated.completed_at.is_some(),
        "completed_at must be set on done"
    );
    assert_eq!(updated.started_at, started, "started_at must be preserved");

    // → back to a Todo column: completed_at cleared, started_at preserved.
    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                status: Some("To Do".into()),
                status_category: Some(StatusCategory::Todo),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        updated.completed_at.is_none(),
        "completed_at must clear when leaving done"
    );
    assert_eq!(
        updated.started_at, started,
        "started_at must still be preserved"
    );
}

/// Foreign keys are enforced on every pooled connection, so an insert
/// that references a non-existent project is rejected instead of orphaning.
#[tokio::test]
async fn foreign_key_rejects_orphan_item() {
    let repo = setup_test_db().await;
    let bogus_project = uuid::Uuid::new_v4();

    let result = repo
        .create_item(
            bogus_project,
            "Backlog",
            CreateItem {
                title: "orphan".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await;

    assert!(
        result.is_err(),
        "inserting an item with a dangling project_id must be rejected by the FK constraint"
    );
}

use super::*;
use serde_json::json;

fn make_field(field_type: CustomFieldType, options: Option<Vec<String>>) -> CustomFieldDefinition {
    CustomFieldDefinition {
        id: Uuid::new_v4(),
        project_id: None,
        name: "test_field".to_string(),
        field_type,
        description: None,
        required: false,
        default_value: None,
        options,
        validation: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn validate_text_accepts_string() {
    let f = make_field(CustomFieldType::Text, None);
    assert!(f.validate_value(&json!("hello")).is_ok());
}

#[test]
fn validate_text_rejects_number() {
    let f = make_field(CustomFieldType::Text, None);
    assert!(f.validate_value(&json!(42)).is_err());
}

#[test]
fn validate_number_accepts_int_and_float() {
    let f = make_field(CustomFieldType::Number, None);
    assert!(f.validate_value(&json!(42)).is_ok());
    assert!(f.validate_value(&json!(2.75)).is_ok());
}

#[test]
fn validate_number_rejects_string() {
    let f = make_field(CustomFieldType::Number, None);
    assert!(f.validate_value(&json!("not a number")).is_err());
}

#[test]
fn validate_boolean_accepts_true_and_false() {
    let f = make_field(CustomFieldType::Boolean, None);
    assert!(f.validate_value(&json!(true)).is_ok());
    assert!(f.validate_value(&json!(false)).is_ok());
}

#[test]
fn validate_boolean_rejects_string() {
    let f = make_field(CustomFieldType::Boolean, None);
    assert!(f.validate_value(&json!("true")).is_err());
}

#[test]
fn validate_date_accepts_ymd() {
    let f = make_field(CustomFieldType::Date, None);
    assert!(f.validate_value(&json!("2025-06-15")).is_ok());
}

#[test]
fn validate_date_accepts_rfc3339() {
    let f = make_field(CustomFieldType::Date, None);
    assert!(f.validate_value(&json!("2025-06-15T12:00:00Z")).is_ok());
}

#[test]
fn validate_date_rejects_freeform() {
    let f = make_field(CustomFieldType::Date, None);
    assert!(f.validate_value(&json!("June 15 2025")).is_err());
}

#[test]
fn validate_url_accepts_http_and_https() {
    let f = make_field(CustomFieldType::Url, None);
    assert!(f.validate_value(&json!("https://example.com")).is_ok());
    assert!(f.validate_value(&json!("http://example.com")).is_ok());
}

#[test]
fn validate_url_rejects_bare_domain() {
    let f = make_field(CustomFieldType::Url, None);
    assert!(f.validate_value(&json!("example.com")).is_err());
}

#[test]
fn validate_select_accepts_valid_option() {
    let f = make_field(
        CustomFieldType::Select,
        Some(vec!["Red".into(), "Blue".into()]),
    );
    assert!(f.validate_value(&json!("Red")).is_ok());
}

#[test]
fn validate_select_rejects_unlisted_option() {
    let f = make_field(
        CustomFieldType::Select,
        Some(vec!["Red".into(), "Blue".into()]),
    );
    assert!(f.validate_value(&json!("Green")).is_err());
}

#[test]
fn validate_multiselect_accepts_valid_array() {
    let f = make_field(
        CustomFieldType::MultiSelect,
        Some(vec!["A".into(), "B".into(), "C".into()]),
    );
    assert!(f.validate_value(&json!(["A", "C"])).is_ok());
}

#[test]
fn validate_multiselect_rejects_unlisted_element() {
    let f = make_field(
        CustomFieldType::MultiSelect,
        Some(vec!["A".into(), "B".into()]),
    );
    assert!(f.validate_value(&json!(["A", "Z"])).is_err());
}

#[test]
fn validate_multiselect_rejects_non_array() {
    let f = make_field(CustomFieldType::MultiSelect, None);
    assert!(f.validate_value(&json!("A")).is_err());
}

fn field_with_validation(
    field_type: CustomFieldType,
    options: Option<Vec<String>>,
    validation: serde_json::Value,
) -> CustomFieldDefinition {
    let mut f = make_field(field_type, options);
    f.validation = Some(validation);
    f
}

#[test]
fn validation_pattern_accepts_matching_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"pattern": r"^\d{4}$"}));
    assert!(f.validate_value(&json!("1234")).is_ok());
}

#[test]
fn validation_pattern_rejects_non_matching_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"pattern": r"^\d{4}$"}));
    assert!(f.validate_value(&json!("abc")).is_err());
}

#[test]
fn validation_min_length_accepts_long_enough_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"min_length": 3}));
    assert!(f.validate_value(&json!("hello")).is_ok());
}

#[test]
fn validation_min_length_rejects_short_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"min_length": 3}));
    assert!(f.validate_value(&json!("hi")).is_err());
}

#[test]
fn validation_max_length_accepts_short_enough_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"max_length": 5}));
    assert!(f.validate_value(&json!("hello")).is_ok());
}

#[test]
fn validation_max_length_rejects_too_long_string() {
    let f = field_with_validation(CustomFieldType::Text, None, json!({"max_length": 5}));
    assert!(f.validate_value(&json!("toolong")).is_err());
}

#[test]
fn validation_min_accepts_value_at_boundary() {
    let f = field_with_validation(CustomFieldType::Number, None, json!({"min": 0}));
    assert!(f.validate_value(&json!(0)).is_ok());
}

#[test]
fn validation_min_rejects_value_below_boundary() {
    let f = field_with_validation(CustomFieldType::Number, None, json!({"min": 0}));
    assert!(f.validate_value(&json!(-1)).is_err());
}

#[test]
fn validation_max_accepts_value_at_boundary() {
    let f = field_with_validation(CustomFieldType::Number, None, json!({"max": 100}));
    assert!(f.validate_value(&json!(100)).is_ok());
}

#[test]
fn validation_max_rejects_value_above_boundary() {
    let f = field_with_validation(CustomFieldType::Number, None, json!({"max": 100}));
    assert!(f.validate_value(&json!(101)).is_err());
}

#[test]
fn validation_max_items_accepts_array_within_limit() {
    let f = field_with_validation(
        CustomFieldType::MultiSelect,
        Some(vec!["A".into(), "B".into(), "C".into()]),
        json!({"max_items": 2}),
    );
    assert!(f.validate_value(&json!(["A", "B"])).is_ok());
}

#[test]
fn validation_max_items_rejects_array_exceeding_limit() {
    let f = field_with_validation(
        CustomFieldType::MultiSelect,
        Some(vec!["A".into(), "B".into(), "C".into()]),
        json!({"max_items": 2}),
    );
    assert!(f.validate_value(&json!(["A", "B", "C"])).is_err());
}

// 26.1 — the double-`Option` PATCH fields must distinguish "absent" (leave
// untouched) from "null" (clear) from an explicit value.
#[test]
fn update_item_double_option_distinguishes_absent_null_and_value() {
    // Absent → outer None.
    let absent: UpdateItem = serde_json::from_value(json!({})).unwrap();
    assert_eq!(absent.sprint_id, None);
    assert_eq!(absent.due_date, None);
    assert_eq!(absent.estimate_unit, None);

    // Explicit null → Some(None) (clear).
    let nulled: UpdateItem =
        serde_json::from_value(json!({"sprint_id": null, "due_date": null})).unwrap();
    assert_eq!(nulled.sprint_id, Some(None));
    assert_eq!(nulled.due_date, Some(None));

    // Concrete value → Some(Some(_)).
    let id = Uuid::new_v4();
    let set: UpdateItem = serde_json::from_value(json!({"sprint_id": id.to_string()})).unwrap();
    assert_eq!(set.sprint_id, Some(Some(id)));

    // status_category is server-only: never populated from client JSON.
    let with_cat: UpdateItem = serde_json::from_value(json!({"status_category": "done"})).unwrap();
    assert_eq!(with_cat.status_category, None);
}

// ─── ItemSource — the prompt-injection trust boundary ─────────────────

#[test]
fn item_source_only_manual_is_trusted() {
    assert!(ItemSource::Manual.is_trusted());
    assert!(!ItemSource::Github.is_trusted());
    assert!(!ItemSource::Linear.is_trusted());
    assert!(!ItemSource::JsonImport.is_trusted());
    assert!(!ItemSource::CsvImport.is_trusted());
    assert!(!ItemSource::Unknown.is_trusted());
}

#[test]
fn item_source_default_is_unknown_and_untrusted() {
    // The Rust-level default matters wherever `#[serde(default)]` or
    // `Default::default()` is used to backfill a missing value (an old
    // export payload, a `FromStr` fallback) — it must be the *safe*
    // value, not `Manual`.
    assert_eq!(ItemSource::default(), ItemSource::Unknown);
    assert!(!ItemSource::default().is_trusted());
}

#[test]
fn item_source_display_and_fromstr_round_trip() {
    use std::str::FromStr;
    for source in [
        ItemSource::Manual,
        ItemSource::Github,
        ItemSource::Linear,
        ItemSource::JsonImport,
        ItemSource::CsvImport,
        ItemSource::Unknown,
    ] {
        let s = source.to_string();
        assert_eq!(ItemSource::from_str(&s).unwrap(), source);
    }
}

#[test]
fn item_source_fromstr_never_fails_and_unrecognised_text_is_untrusted() {
    use std::str::FromStr;
    // A future Tack version's new source value, read by this binary, or
    // outright corruption — either way this must degrade to Unknown
    // (untrusted), never a parse error and never a variant that
    // silently ends up trusted.
    let parsed = ItemSource::from_str("some_future_source_this_binary_has_never_heard_of")
        .expect("FromStr for ItemSource is infallible");
    assert_eq!(parsed, ItemSource::Unknown);
    assert!(!parsed.is_trusted());
}

#[test]
fn item_source_serde_rename_all_snake_case() {
    assert_eq!(serde_json::to_value(ItemSource::Manual).unwrap(), "manual");
    assert_eq!(serde_json::to_value(ItemSource::Github).unwrap(), "github");
    assert_eq!(
        serde_json::to_value(ItemSource::JsonImport).unwrap(),
        "json_import"
    );
}

/// The exact scenario `handlers::export::run_import` relies on to make
/// the trust marker survive an export → import round-trip: an `Item`
/// deserialized from JSON that predates this field (or from a
/// hand-built payload that never had it) must resolve to `Unknown`
/// (untrusted), never silently to `Manual`.
#[test]
fn item_deserialization_defaults_missing_source_to_unknown() {
    let value = json!({
        "id": Uuid::new_v4(),
        "project_id": Uuid::new_v4(),
        "parent_id": null,
        "title": "Pre-existing export without a source field",
        "description": null,
        "item_type": "task",
        "status": "To Do",
        "priority": "medium",
        "estimate": null,
        "estimate_unit": "story_points",
        "tags": [],
        "sort_order": 1,
        "sprint_id": null,
        "assignee": null,
        "due_date": null,
        "started_at": null,
        "completed_at": null,
        // "source" deliberately omitted.
        "created_at": chrono::Utc::now().to_rfc3339(),
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });
    let item: Item = serde_json::from_value(value).expect("deserialize legacy item JSON");
    assert_eq!(item.source, ItemSource::Unknown);
    assert!(!item.source.is_trusted());
}

// ─── Template `orchestration` block ────────────────────────────────────

/// Backward compatibility: a
/// `ProjectTemplate` payload that predates this field (every built-in
/// seed, every template saved before this field existed) has no `orchestration`
/// key at all — it must deserialize to `None`, not fail or default to
/// `Some(TemplateOrchestration::default())`.
#[test]
fn project_template_deserialization_defaults_missing_orchestration_to_none() {
    let value = json!({
        "id": Uuid::new_v4(),
        "name": "Pre-existing template without an orchestration field",
        "description": null,
        "project_type": "software",
        "vocabulary": {},
        "workflow": { "statuses": [], "workflow_type": "kanban", "transitions": null },
        "custom_fields": [],
        "default_boards": [],
        // "orchestration" deliberately omitted.
        "is_builtin": false,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });
    let template: ProjectTemplate =
        serde_json::from_value(value).expect("deserialize legacy template JSON");
    assert!(template.orchestration.is_none());
}

/// Same rule for `CreateProjectTemplate` — a hand-built or pre-cycle
/// create payload with no `orchestration` key must not fail validation
/// or deserialization.
#[test]
fn create_project_template_deserialization_defaults_missing_orchestration_to_none() {
    let value = json!({
        "name": "New template, old client",
        "description": null,
        "project_type": "software",
    });
    let create: CreateProjectTemplate =
        serde_json::from_value(value).expect("deserialize legacy create payload");
    assert!(create.orchestration.is_none());
}

/// A `TemplateOrchestration` with every field left at its default must
/// itself round-trip — this is the "explicit-but-empty" state a client
/// gets from `TemplateOrchestration::default()`, distinct from the
/// `None` case above at the `Option` layer.
#[test]
fn template_orchestration_default_round_trips() {
    let orch = TemplateOrchestration::default();
    assert_eq!(orch.blueprint, OrchBlueprint::Software);
    assert!(orch.pipeline_yaml.is_none());
    assert!(!orch.auto_dispatch);
    assert!(orch.status_map.dispatch_from.is_empty());

    let json = serde_json::to_value(&orch).unwrap();
    let round_tripped: TemplateOrchestration = serde_json::from_value(json).unwrap();
    assert_eq!(orch, round_tripped);
}

/// docket's five real blueprint names (`core/blueprints.py`),
/// including the one with a hyphen — `serde`'s
/// `rename_all = "kebab-case"` must actually produce `"agentic-product"`,
/// not `"agentic_product"` or `"AgenticProduct"`.
#[test]
fn orch_blueprint_serializes_to_dockets_exact_names() {
    let cases = [
        (OrchBlueprint::Software, "software"),
        (OrchBlueprint::Research, "research"),
        (OrchBlueprint::Content, "content"),
        (OrchBlueprint::Ops, "ops"),
        (OrchBlueprint::AgenticProduct, "agentic-product"),
    ];
    for (variant, expected) in cases {
        assert_eq!(serde_json::to_value(variant).unwrap(), json!(expected));
        let parsed: OrchBlueprint = serde_json::from_value(json!(expected)).unwrap();
        assert_eq!(parsed, variant);
    }
}

/// An orchestration block with a populated `status_map` round-trips
/// exactly — this is the shape `handlers::templates::create_template`
/// converts into `orch::StatusMap` before calling
/// `orch::validate_status_map`; a field getting dropped or renamed here
/// would silently break that conversion without a type error (both
/// structs use plain field names, not a shared type).
#[test]
fn template_status_map_round_trips_every_field() {
    let sm = TemplateStatusMap {
        dispatch_from: vec!["To Do".to_string(), "Ready".to_string()],
        on_running: Some("In Progress".to_string()),
        on_waiting_approval: Some("Blocked".to_string()),
        on_succeeded: Some("Done".to_string()),
        on_failed: Some("Blocked".to_string()),
        on_cancelled: Some("Ready".to_string()),
    };
    let json = serde_json::to_value(&sm).unwrap();
    let round_tripped: TemplateStatusMap = serde_json::from_value(json).unwrap();
    assert_eq!(sm, round_tripped);
}

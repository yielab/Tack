use super::*;

/// One "unrecognised value round-trips" test per enum — deserializes
/// to `Unknown(original)` instead of
/// erroring, and reserializes to exactly the same string.
fn assert_unknown_round_trips<T>(wire_value: &str)
where
    T: Serialize + for<'de> Deserialize<'de> + fmt::Debug + PartialEq + From<String>,
{
    let json = format!("\"{wire_value}\"");
    let decoded: T = serde_json::from_str(&json).expect("unknown value must not error");
    assert_eq!(decoded, T::from(wire_value.to_string()));
    let re_encoded = serde_json::to_string(&decoded).expect("must reserialize");
    assert_eq!(
        re_encoded, json,
        "must round-trip to the exact original string"
    );
}

#[test]
fn run_state_unknown_round_trips() {
    assert_unknown_round_trips::<RunState>("paused");
}

#[test]
fn run_state_known_values_round_trip() {
    for wire in ["queued", "running", "succeeded", "failed", "cancelled"] {
        let json = format!("\"{wire}\"");
        let decoded: RunState = serde_json::from_str(&json).unwrap();
        assert!(!matches!(decoded, RunState::Unknown(_)));
        assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
    }
}

#[test]
fn run_source_unknown_round_trips() {
    assert_unknown_round_trips::<RunSource>("telegram-bot");
}

#[test]
fn task_status_unknown_round_trips() {
    assert_unknown_round_trips::<TaskStatus>("archived");
}

#[test]
fn approval_state_unknown_round_trips() {
    assert_unknown_round_trips::<ApprovalState>("expired");
}

#[test]
fn health_deserializes_from_docket_shape() {
    let h: Health = serde_json::from_str(r#"{"status":"ok","gateway":1}"#).unwrap();
    assert_eq!(h.status, "ok");
    assert_eq!(h.gateway, 1);
}

#[test]
fn fleet_status_deserializes_from_docket_shape() {
    let raw = r#"{
        "apiVersion": "2",
        "timestamp": "2026-08-04T00:00:00Z",
        "gateway": "active",
        "channels": ["telegram"],
        "agents": [{
            "id": "proj-1",
            "name": "proj-1",
            "kind": "project",
            "scope": "project",
            "model": "claude-sonnet-5",
            "registered": true,
            "bindings": [{"channel": "telegram", "peerId": "12345"}],
            "lastActivity": "never",
            "costUsd": 1.5,
            "budgetUsd": null
        }],
        "totalCostUsd": 1.5
    }"#;
    let status: FleetStatus = serde_json::from_str(raw).unwrap();
    assert_eq!(status.api_version, "2");
    assert_eq!(status.total_cost_usd_estimated, 1.5);
    assert_eq!(status.agents.len(), 1);
    assert_eq!(status.agents[0].cost_usd_estimated, 1.5);
    assert_eq!(status.agents[0].budget_usd, None);
    assert_eq!(status.agents[0].bindings[0].peer_id, "12345");
}

#[test]
fn remote_run_deserializes_from_docket_shape() {
    let raw = r#"{
        "id": "run-abc123",
        "source": "webhook",
        "project": "demo",
        "state": "running",
        "taskIds": [],
        "error": "",
        "created": "2026-08-04T00:00:00+00:00",
        "startedAt": "2026-08-04T00:00:01+00:00",
        "finishedAt": null,
        "pids": [],
        "variables": {}
    }"#;
    let run: RemoteRun = serde_json::from_str(raw).unwrap();
    assert_eq!(run.state, RunState::Running);
    assert_eq!(run.source, RunSource::Webhook);
    assert_eq!(run.finished_at, None);
}

#[test]
fn remote_approval_deserializes_from_docket_shape() {
    let raw = r#"{
        "token": "apr-xyz",
        "project": "demo",
        "role": "lead",
        "action": "pod dispatch — task enqueue for 'demo': do the thing",
        "state": "pending",
        "created": "2026-08-04T00:00:00Z",
        "context": {"taskId": "task-1", "pipelineIndex": 0}
    }"#;
    let approval: RemoteApproval = serde_json::from_str(raw).unwrap();
    assert_eq!(approval.state, ApprovalState::Pending);
    assert_eq!(approval.context["taskId"], "task-1");
}

#[test]
fn remote_event_deserializes_from_snake_case_trace_shape() {
    let raw = r#"{
        "ts": "2026-08-04T00:00:00Z",
        "project": "demo",
        "session_id": "agent:demo:task-1",
        "agent_role": "lead",
        "event_type": "some_future_event_type",
        "payload": {"text": "hi"},
        "cost_usd": 0.01,
        "duration_ms": 1200
    }"#;
    let event: RemoteEvent = serde_json::from_str(raw).unwrap();
    assert_eq!(event.event_type, "some_future_event_type");
    assert_eq!(event.cost_usd_estimated, Some(0.01));
}

#[test]
fn new_remote_task_omits_absent_priority() {
    let task = NewRemoteTask {
        description: "do the thing".to_string(),
        priority: None,
        trusted: false,
    };
    let json = serde_json::to_string(&task).unwrap();
    assert!(!json.contains("priority"));
    assert!(json.contains("\"trusted\":false"));
}

#[test]
fn orch_error_variants_have_messages() {
    // Cheap smoke test that every variant constructs and displays without
    // panicking — mostly here to keep the match arms honest if a variant
    // is ever added without a message.
    let errors = [
        OrchError::Http("boom".into()),
        OrchError::Auth,
        OrchError::Decode("bad json".into()),
        OrchError::NotFound("run-1".into()),
        OrchError::Unavailable("connection refused".into()),
        OrchError::Disabled,
        OrchError::PolicyBlocked {
            policy_id: "prompt-injection".into(),
            message: "untrusted input matched a deny rule".into(),
        },
        OrchError::AlreadyDecided("Already granted: apr-1".into()),
    ];
    for e in errors {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn already_decided_display_names_the_docket_message() {
    let e = OrchError::AlreadyDecided("Already granted: apr-1".into());
    assert!(e.to_string().contains("Already granted: apr-1"));
}

#[test]
fn policy_blocked_display_names_the_policy_id() {
    let e = OrchError::PolicyBlocked {
        policy_id: "prompt-injection".into(),
        message: "untrusted input matched a deny rule".into(),
    };
    let text = e.to_string();
    assert!(text.contains("prompt-injection"), "{text}");
    assert!(
        text.contains("untrusted input matched a deny rule"),
        "{text}"
    );
}

#[test]
fn traces_page_round_trips_events_and_the_opaque_cursor() {
    // TracesPage's own `events` field is already-decoded `RemoteEvent`s —
    // the double-encoded-JSON-strings wire quirk is `adapters::docket`'s
    // problem to unwrap before it ever builds a `TracesPage`, not this
    // struct's. This test only exercises what this crate owns: the page
    // envelope and the opaque cursor field.
    let event = RemoteEvent {
        ts: "2026-08-05T00:00:00Z".to_string(),
        project: "demo".to_string(),
        session_id: "agent:demo:task-1".to_string(),
        agent_role: "lead".to_string(),
        event_type: "tool_call".to_string(),
        payload: serde_json::json!({}),
        cost_usd_estimated: None,
        duration_ms: None,
    };
    let page = TracesPage {
        events: vec![event],
        next: Some("2026-08-05T00:00:00Z:1".to_string()),
    };
    let json = serde_json::to_string(&page).unwrap();
    let decoded: TracesPage = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, page);
}

#[test]
fn traces_page_next_defaults_to_none_when_absent() {
    let page: TracesPage = serde_json::from_str(r#"{"events":[]}"#).unwrap();
    assert_eq!(page.next, None);
    assert!(page.events.is_empty());
}

// ── Capabilities ──────────────────────────────────────────

#[test]
fn support_serializes_to_the_wire_strings_the_openapi_contract_promises() {
    // `docs/plans/agnostic-control-plane.md`'s acceptance check reads
    // this back with `jq '.capabilities.pause'` and expects exactly
    // `"unsupported"` — pin the wire form here, not just at the API
    // layer, since a `#[serde(rename_all)]` typo would otherwise only
    // surface as a much harder-to-place openapi-contract diff.
    assert_eq!(
        serde_json::to_string(&Support::Unsupported).unwrap(),
        "\"unsupported\""
    );
    assert_eq!(
        serde_json::to_string(&Support::Advisory).unwrap(),
        "\"advisory\""
    );
    assert_eq!(
        serde_json::to_string(&Support::Supported).unwrap(),
        "\"supported\""
    );
}

#[test]
fn rated_serializes_level_and_reason_together() {
    // `Rated` is `Serialize`-only (see its own doc comment: `reason:
    // &'static str` cannot support `Deserialize` in general) — assert
    // the wire shape directly instead of a round trip.
    let r = Rated::new(EventScope::Project, "scoped per project");
    let json = serde_json::to_value(r).unwrap();
    assert_eq!(json["level"], "project");
    assert_eq!(json["reason"], "scoped per project");
}

#[test]
fn docket_capabilities_match_the_verified_facts() {
    // Field-by-field against the verified facts —
    // a docket instance with no token configured is enough, since
    // `capabilities()` does no I/O and never reads the stored
    // credential.
    let adapter = crate::adapters::docket::DocketAdapter::new("http://127.0.0.1:7331", None)
        .expect("adapter must construct");
    let caps = adapter.capabilities();

    assert!(
        caps.dispatch,
        "docket accepts new work via enqueue_task (POST /tasks/{{project}})"
    );
    assert!(!caps.cancel, "docket exposes no cancel route over HTTP");
    assert_eq!(
        caps.pause.level,
        Support::Unsupported,
        "pause's level must be Unsupported (docket exposes no HTTP pause route), got: {:?}",
        caps.pause.level
    );
    assert!(
        caps.pause.reason.contains("docket profile"),
        "pause's reason must name the docket CLI remedy, got: {:?}",
        caps.pause.reason
    );
    assert_eq!(
        caps.resume.level,
        Support::Unsupported,
        "resume's level must be Unsupported (docket exposes no HTTP resume route), got: {:?}",
        caps.resume.level
    );
    assert!(
        caps.resume.reason.contains("docket profile"),
        "resume's reason must name the docket CLI remedy, got: {:?}",
        caps.resume.reason
    );
    assert_eq!(
        caps.event_scope.level,
        EventScope::Project,
        "event_scope's level must be Project (docket's /status.json scopes events per \
         project, not per run), got: {:?}",
        caps.event_scope.level
    );
    assert!(
        !caps.artifacts,
        "docket exposes no artifact-retrieval route"
    );
    assert_eq!(
        caps.decisions.level,
        DecisionSupport::Poll,
        "decisions' level must be Poll (docket's approvals are discovered by polling \
         GET /approvals, not pushed), got: {:?}",
        caps.decisions.level
    );
    assert_eq!(
        caps.usage.level,
        UsageSupport::FromProvider,
        "usage's level must be FromProvider (docket's own driver reports its usage \
         estimate, not a separate metering gateway), got: {:?}",
        caps.usage.level
    );
    assert_eq!(
        caps.model_selection.level,
        ModelSelection::Unsupported,
        "model_selection's level must be Unsupported (docket owns its own model routing \
         and may ignore an externally supplied model), got: {:?}",
        caps.model_selection.level
    );
    assert!(caps.runtimes, "docket's /status.json agents[] is a roster");
    assert!(caps.plane_metrics, "docket exposes GET /metrics");
    assert!(caps.provisioning, "docket exposes POST /pods");
}

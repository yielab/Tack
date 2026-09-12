use super::*;

#[allow(clippy::too_many_arguments)]
fn item(
    project_type: &str,
    item_type: &str,
    attempt_count: i64,
    first_dispatched_at: Option<DateTime<Utc>>,
    last_dispatched_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: Option<f64>,
) -> ItemEconomicsRow {
    ItemEconomicsRow {
        item_id: Uuid::new_v4(),
        project_id: Uuid::new_v4(),
        project_type: project_type.to_string(),
        item_type: item_type.to_string(),
        title: "t".to_string(),
        status: "Done".to_string(),
        started_at,
        completed_at,
        attempt_count,
        first_dispatched_at,
        last_dispatched_at,
        tokens_in,
        tokens_out,
        cost_usd_estimated,
    }
}

fn hours_ago(h: i64) -> DateTime<Utc> {
    Utc::now() - Duration::hours(h)
}

#[test]
fn agent_and_human_populations_split_correctly() {
    let agent = item(
        "software",
        "task",
        1,
        Some(hours_ago(10)),
        Some(hours_ago(10)),
        None,
        Some(hours_ago(2)),
        100,
        200,
        Some(1.5),
    );
    let human = item(
        "software",
        "task",
        0,
        None,
        None,
        Some(hours_ago(20)),
        Some(hours_ago(4)),
        0,
        0,
        None,
    );
    let rows = vec![agent, human];
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);

    assert_eq!(summary.overall.completed_item_count, 2);
    assert_eq!(summary.overall.agent_completed_count, 1);
    assert_eq!(summary.overall.human_completed_count, 1);
    assert_eq!(summary.overall.tokens_in, 100);
    assert_eq!(summary.overall.tokens_out, 200);
    assert_eq!(summary.overall.cost_usd_estimated, Some(1.5));
    // Below MIN_SAMPLE_SIZE (1 sample each) — raw hours, not an average.
    assert!(summary.overall.agent_lead_time.below_min_sample);
    assert!(summary.overall.agent_lead_time.avg_hours.is_none());
    assert_eq!(
        summary
            .overall
            .agent_lead_time
            .raw_hours
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert!(summary.overall.human_lead_time.below_min_sample);
}

#[test]
fn zero_agent_items_yields_none_cost_not_zero() {
    let human = item(
        "software",
        "task",
        0,
        None,
        None,
        Some(hours_ago(5)),
        Some(hours_ago(1)),
        0,
        0,
        None,
    );
    let rows = vec![human];
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);
    assert_eq!(summary.overall.agent_completed_count, 0);
    assert_eq!(summary.overall.cost_usd_estimated, None);
    assert_eq!(summary.overall.cost_usd_estimated_per_item, None);
}

#[test]
fn lead_time_reports_average_at_or_above_min_sample() {
    let mut rows = Vec::new();
    for h in [10, 20, 30, 40, 50] {
        rows.push(item(
            "software",
            "task",
            1,
            Some(hours_ago(h + 5)),
            Some(hours_ago(h + 5)),
            None,
            Some(hours_ago(5)),
            10,
            10,
            Some(0.1),
        ));
    }
    assert_eq!(rows.len(), MIN_SAMPLE_SIZE as usize);
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);
    assert!(!summary.overall.agent_lead_time.below_min_sample);
    assert!(summary.overall.agent_lead_time.avg_hours.is_some());
    assert!(summary.overall.agent_lead_time.raw_hours.is_none());
}

#[test]
fn cost_per_item_withheld_below_min_sample_shown_at_or_above() {
    let mut rows = Vec::new();
    for _ in 0..4 {
        rows.push(item(
            "software",
            "task",
            1,
            Some(hours_ago(5)),
            Some(hours_ago(5)),
            None,
            Some(hours_ago(1)),
            10,
            10,
            Some(2.0),
        ));
    }
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);
    // 4 agent items — below MIN_SAMPLE_SIZE (5).
    assert_eq!(summary.overall.agent_completed_count, 4);
    assert!(summary.overall.cost_usd_estimated_per_item.is_none());
    assert_eq!(summary.overall.cost_usd_estimated, Some(8.0));

    rows.push(item(
        "software",
        "task",
        1,
        Some(hours_ago(5)),
        Some(hours_ago(5)),
        None,
        Some(hours_ago(1)),
        10,
        10,
        Some(2.0),
    ));
    let summary2 = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);
    assert_eq!(summary2.overall.agent_completed_count, 5);
    assert_eq!(summary2.overall.cost_usd_estimated_per_item, Some(2.0));
}

#[test]
fn rework_rate_excludes_stale_attempts_from_the_denominator() {
    let cutoff = Utc::now() - Duration::days(30);
    // 5 fresh items (within retention), 2 with a rework signal.
    let mut rows = Vec::new();
    let mut signal_ids = std::collections::HashSet::new();
    for i in 0..5 {
        let row = item(
            "software",
            "task",
            1,
            Some(hours_ago(10)),
            Some(hours_ago(10)),
            None,
            Some(hours_ago(1)),
            10,
            10,
            Some(1.0),
        );
        if i < 2 {
            signal_ids.insert(row.item_id);
        }
        rows.push(row);
    }
    // 3 stale items (last_dispatched_at before the cutoff) — must be excluded
    // from the denominator entirely, not counted as "no rework".
    for _ in 0..3 {
        rows.push(item(
            "software",
            "task",
            1,
            Some(Utc::now() - Duration::days(120)),
            Some(Utc::now() - Duration::days(120)),
            None,
            Some(hours_ago(1)),
            10,
            10,
            Some(1.0),
        ));
    }

    let summary = build_summary(&rows, &signal_ids, cutoff, 30);
    let rework = &summary.overall.rework;
    assert_eq!(rework.attempts_total, 8);
    assert_eq!(rework.attempts_excluded_stale, 3);
    assert_eq!(rework.attempts_with_rework_signal, 2);
    // Eligible = 5, which meets MIN_SAMPLE_SIZE, so a rate is shown.
    assert_eq!(rework.rate, Some(2.0 / 5.0));
}

#[test]
fn rework_rate_is_none_below_min_sample_even_with_signal_data() {
    let cutoff = Utc::now() - Duration::days(30);
    let mut rows = Vec::new();
    let mut signal_ids = std::collections::HashSet::new();
    for _ in 0..3 {
        let row = item(
            "software",
            "task",
            1,
            Some(hours_ago(10)),
            Some(hours_ago(10)),
            None,
            Some(hours_ago(1)),
            10,
            10,
            Some(1.0),
        );
        signal_ids.insert(row.item_id);
        rows.push(row);
    }
    let summary = build_summary(&rows, &signal_ids, cutoff, 30);
    assert!(summary.overall.rework.below_min_sample);
    assert_eq!(summary.overall.rework.rate, None);
}

#[test]
fn negative_duration_is_excluded_not_shown_as_negative() {
    // first_dispatched_at AFTER completed_at — a data anomaly (e.g. a
    // redispatch recorded post-completion).
    let row = item(
        "software",
        "task",
        1,
        Some(hours_ago(1)),
        Some(hours_ago(1)),
        None,
        Some(hours_ago(10)),
        10,
        10,
        Some(1.0),
    );
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&[row], &signal, Utc::now() - Duration::days(90), 90);
    assert_eq!(summary.overall.agent_lead_time.sample_count, 0);
}

#[test]
fn slices_by_project_type_and_item_type_are_disjoint_and_sum_to_overall() {
    let rows = vec![
        item(
            "software",
            "bug",
            1,
            Some(hours_ago(5)),
            Some(hours_ago(5)),
            None,
            Some(hours_ago(1)),
            5,
            5,
            Some(0.5),
        ),
        item(
            "construction",
            "task",
            0,
            None,
            None,
            Some(hours_ago(5)),
            Some(hours_ago(1)),
            0,
            0,
            None,
        ),
    ];
    let signal = std::collections::HashSet::new();
    let summary = build_summary(&rows, &signal, Utc::now() - Duration::days(90), 90);
    assert_eq!(summary.by_project_type.len(), 2);
    assert_eq!(summary.by_item_type.len(), 2);
    let total_from_slices: i64 = summary
        .by_project_type
        .iter()
        .map(|s| s.completed_item_count)
        .sum();
    assert_eq!(total_from_slices, summary.overall.completed_item_count);
}

#[test]
fn item_response_marks_rework_not_applicable_for_human_items() {
    let human = item(
        "software",
        "task",
        0,
        None,
        None,
        Some(hours_ago(5)),
        Some(hours_ago(1)),
        0,
        0,
        None,
    );
    let signal = std::collections::HashSet::new();
    let resp = to_item_response(&human, &signal, Utc::now() - Duration::days(90));
    assert_eq!(resp.population, EconomicsPopulation::Human);
    assert!(!resp.rework_applicable);
    assert!(!resp.rework_data_reliable);
    assert!(!resp.rework_signal);
}

#[test]
fn constants_are_asserted_verbatim_so_the_number_and_the_words_cannot_drift() {
    assert!(REWORK_RATE_DEFINITION.contains("rework_started"));
    assert!(REWORK_RATE_DEFINITION.contains("verification_failed"));
    assert!(REWORK_RATE_DEFINITION.contains("tester_verdict_failed"));
    assert!(LEAD_TIME_SELECTION_BIAS_NOTE.contains("not a random sample"));
    assert!(REWORK_TRUNCATION_NOTE.contains("retention window"));
}

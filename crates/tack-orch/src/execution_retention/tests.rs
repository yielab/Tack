use super::*;
use std::sync::Mutex;

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new(now: DateTime<Utc>) -> Self {
        Self(Mutex::new(now))
    }
}

impl RetentionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[derive(Default)]
struct FakeStore {
    replay_calls: Mutex<Vec<(DateTime<Utc>, i64)>>,
    event_calls: Mutex<Vec<(DateTime<Utc>, i64)>>,
    fail_replays: Mutex<bool>,
}

#[async_trait]
impl ExecutionRetentionStore for FakeStore {
    async fn purge_stale_replays(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError> {
        self.replay_calls.lock().unwrap().push((cutoff, batch_size));
        if *self.fail_replays.lock().unwrap() {
            return Err(OrchError::Unavailable("simulated failure".into()));
        }
        Ok(PurgeOutcome::default())
    }

    async fn purge_stale_terminal_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError> {
        self.event_calls.lock().unwrap().push((cutoff, batch_size));
        Ok(PurgeOutcome::default())
    }
}

/// Bounded, deterministic wait for a background task's first tick to
/// land — not a business-logic assertion via sleep (CLAUDE.md's "no
/// blocking sleeps" targets that), just yielding to the real tokio
/// scheduler so the real spawned task can run. Panics (fails the test)
/// rather than hanging if the condition is never met.
async fn wait_for(mut condition: impl FnMut() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
    panic!("condition not met within the 2s test timeout");
}

#[tokio::test]
async fn disabled_sweep_spawns_nothing_and_never_touches_the_store() {
    let store = Arc::new(FakeStore::default());
    let clock: Arc<dyn RetentionClock> = Arc::new(FakeClock::new(Utc::now()));
    let (_tx, rx) = watch::channel(false);

    let handle = spawn_execution_retention_sweep(
        false,
        store.clone(),
        clock,
        ExecutionRetentionConfig::default(),
        rx,
    );
    assert!(handle.is_none());

    // Give a hypothetical (bugged) task a chance to run before asserting
    // silence — this is the "never even queries" proof, mirroring
    // reconciler's own `disabled_orchestration_spawns_no_tasks...` test.
    tokio::time::sleep(StdDuration::from_millis(30)).await;
    assert!(store.replay_calls.lock().unwrap().is_empty());
    assert!(store.event_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn enabled_sweep_calls_both_purges_with_the_configured_cutoff_and_batch_size() {
    let now = DateTime::parse_from_rfc3339("2026-08-12T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let store = Arc::new(FakeStore::default());
    let clock: Arc<dyn RetentionClock> = Arc::new(FakeClock::new(now));
    let (tx, rx) = watch::channel(false);
    let config = ExecutionRetentionConfig {
        retention_days: 90,
        batch_size: 250,
        sweep_interval_secs: 3600,
    };

    let handle =
        spawn_execution_retention_sweep(true, store.clone(), clock, config, rx).expect("spawned");

    wait_for(|| !store.replay_calls.lock().unwrap().is_empty()).await;

    let expected_cutoff = now - Duration::days(90);
    {
        let calls = store.replay_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (expected_cutoff, 250));
    }
    {
        let calls = store.event_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (expected_cutoff, 250));
    }

    let _ = tx.send(true);
    handle.await.expect("task joins cleanly");
}

#[tokio::test]
async fn shutdown_joins_the_task_and_no_further_purge_happens_after() {
    // A 1s interval means a second tick would fire if the stop signal
    // were not observed — this is what makes "no further writes after
    // shutdown" a real assertion instead of a vacuous one (the sweep
    // never got a second chance to write regardless).
    let store = Arc::new(FakeStore::default());
    let clock: Arc<dyn RetentionClock> = Arc::new(FakeClock::new(Utc::now()));
    let (tx, rx) = watch::channel(false);
    let config = ExecutionRetentionConfig {
        retention_days: 90,
        batch_size: 500,
        sweep_interval_secs: 1,
    };

    let handle =
        spawn_execution_retention_sweep(true, store.clone(), clock, config, rx).expect("spawned");
    wait_for(|| !store.replay_calls.lock().unwrap().is_empty()).await;
    let calls_before_stop = store.replay_calls.lock().unwrap().len();

    let _ = tx.send(true);
    // Load-bearing: this is a real `.await` on the `JoinHandle`, not
    // just a signal-and-return. If the task never actually observed the
    // stop signal (e.g. the `tokio::select!` were wired wrong), this
    // line would hang until the test harness times out rather than
    // silently "passing."
    handle.await.expect("task joins cleanly after stop signal");

    // Wait out more than one full interval past shutdown and confirm
    // no new call landed — proves the task is truly gone, not merely
    // slow to notice.
    tokio::time::sleep(StdDuration::from_millis(1_300)).await;
    let calls_after_wait = store.replay_calls.lock().unwrap().len();
    assert_eq!(
        calls_before_stop, calls_after_wait,
        "no purge call should happen after the join handle completed"
    );
}

#[tokio::test]
async fn a_failing_purge_is_logged_and_retried_next_cycle_not_panicked() {
    let store = Arc::new(FakeStore::default());
    *store.fail_replays.lock().unwrap() = true;
    let clock: Arc<dyn RetentionClock> = Arc::new(FakeClock::new(Utc::now()));
    let (tx, rx) = watch::channel(false);
    let config = ExecutionRetentionConfig {
        retention_days: 90,
        batch_size: 500,
        sweep_interval_secs: 1,
    };

    let handle =
        spawn_execution_retention_sweep(true, store.clone(), clock, config, rx).expect("spawned");
    wait_for(|| !store.replay_calls.lock().unwrap().is_empty()).await;
    // The event purge still runs even though the replay purge failed —
    // one failing sub-step must not abort the tick.
    wait_for(|| !store.event_calls.lock().unwrap().is_empty()).await;

    let _ = tx.send(true);
    handle
        .await
        .expect("a failing purge must not panic the task — it logs and retries next cycle");
}

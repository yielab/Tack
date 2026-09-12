use super::*;

#[test]
fn embedded_runner_refuses_non_loopback_bind() {
    let config = tack_api::config::AppConfig {
        host: "0.0.0.0".to_owned(),
        ..tack_api::config::AppConfig::default()
    };

    let error = ensure_loopback(&config).expect_err("non-loopback bind must be refused");
    assert!(error.to_string().contains("loopback"));
}

#[test]
fn embedded_runner_accepts_the_default_loopback_bind() {
    assert!(ensure_loopback(&tack_api::config::AppConfig::default()).is_ok());
}

#[test]
fn with_runner_enabled_reads_the_environment_gate() {
    // SAFETY: this test owns the variable for its own duration and restores
    // it, but `std::env::set_var` is unsound to run concurrently with other
    // threads reading the same variable — `cargo test` runs this crate's
    // tests in one process, so serialize via the variable's own uniqueness
    // (nothing else in this crate reads `TACK_LOCAL_RUNNER_ENABLE`).
    let previous = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
    assert!(!with_runner_enabled(false));

    unsafe {
        std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
    }
    assert!(with_runner_enabled(false));
    assert!(with_runner_enabled(true));

    unsafe {
        match &previous {
            Some(value) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", value),
            None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
        }
    }
}

fn unique_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A `database_url` that fails at parse time, before any filesystem or
/// network I/O — so a test built against it proves whether
/// `tack_db::init_pool` (and therefore self-provisioning) was ever
/// reached, without needing a real database.
fn unreachable_database_url() -> tack_api::config::AppConfig {
    tack_api::config::AppConfig {
        database_url: "not-a-real-database-url".to_owned(),
        ..tack_api::config::AppConfig::default()
    }
}

#[tokio::test]
async fn ensure_runner_credential_leaves_a_manual_credential_untouched() {
    let state_dir_guard = unique_temp_dir("manual");
    let state_dir = state_dir_guard.path();
    let mut runner_config = RunnerConfig {
        enrollment_credential: Some(EnrollmentCredential::new("manually-configured-token")),
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_ok(),
        "a manual credential must never require touching the database: {result:?}"
    );
    assert_eq!(
        runner_config
            .enrollment_credential
            .expect("credential must remain set")
            .expose(),
        "manually-configured-token",
        "a manual credential must win over both the stored-session placeholder and self-provisioning"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

#[tokio::test]
async fn ensure_runner_credential_reuses_a_stored_session_without_self_provisioning() {
    let state_dir_guard = unique_temp_dir("stored-session");
    let state_dir = state_dir_guard.path();
    std::fs::write(state_dir.join("session.json"), "{}").expect("write session.json");
    let mut runner_config = RunnerConfig {
        enrollment_credential: None,
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    // Deliberately unreachable: if this path wrongly called
    // `local_enrollment::self_provision`, opening the pool would fail
    // and this assertion below would see an `Err`, not an `Ok`.
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_ok(),
        "a stored session must satisfy the credential requirement without opening the database: {result:?}"
    );
    assert!(
        runner_config.enrollment_credential.is_some(),
        "build_runtime's precondition still needs a credential to be present"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

#[tokio::test]
async fn ensure_runner_credential_attempts_self_provisioning_when_nothing_else_is_available() {
    let state_dir_guard = unique_temp_dir("no-session");
    let state_dir = state_dir_guard.path();
    let mut runner_config = RunnerConfig {
        enrollment_credential: None,
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_err(),
        "with no manual credential and no stored session, self-provisioning must actually be \
         attempted — proved here by its failure against a deliberately unreachable database, \
         the same database_url the previous test proves does NOT get touched when a session exists"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

fn control_with_state_dir(state_dir: &Path) -> EmbeddedRunnerControl {
    let mut runner_config = RunnerConfig::defaults();
    runner_config.state_dir = state_dir.to_path_buf();
    EmbeddedRunnerControl {
        server_config: unreachable_database_url(),
        state: Mutex::new(State {
            bound_addr: None,
            runner_config,
            running: None,
            secret_meta: HashMap::new(),
            vercel_ai_gateway_secret_auto_enabled: false,
        }),
    }
}

#[tokio::test]
async fn a_fresh_control_reports_stopped_with_no_since() {
    let dir = unique_temp_dir("status-fresh");
    let control = control_with_state_dir(dir.path());

    let status = control.status().await;

    assert_eq!(status.state, RuntimeState::Stopped);
    assert!(status.since.is_none());
}

#[tokio::test]
async fn start_fails_typed_before_the_bound_address_is_known() {
    let dir = unique_temp_dir("start-no-addr");
    let control = control_with_state_dir(dir.path());

    let result = control.start().await;

    assert!(matches!(
        result,
        Err(LocalRunnerControlError::StartFailed(_))
    ));
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
}

#[tokio::test]
async fn a_disabled_provider_reports_not_configured_with_no_network_call() {
    let dir = unique_temp_dir("catalog-disabled");
    let control = control_with_state_dir(dir.path());

    // `RunnerConfig::defaults()` seeds the Vercel provider disabled, and
    // a disabled entry is skipped before any network I/O — which is what
    // makes this assertion safe to run with no network access.
    let catalog = control.catalog().await;

    assert!(matches!(catalog, CatalogSnapshot::NotConfigured));
}

/// A stand-in for a spawned runner task: it does exactly what the real one
/// does with its `Shutdown` — waits for the request, then exits — and
/// records that it observed the request, so a test can tell "joined
/// after being told to stop" apart from "still running" without a real
/// runner behind it.
fn fake_running(stopped: Arc<std::sync::atomic::AtomicBool>) -> Running {
    let (mut shutdown, shutdown_handle) = Shutdown::channel();
    let runner_task = tokio::spawn(async move {
        shutdown.requested().await;
        stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    Running {
        shutdown_handle,
        runner_task,
        since: Utc::now(),
    }
}

/// The running task is joined before anything else happens, and the
/// provider it was spawned without is on in the configuration the next
/// task will read. The restart's own `start` then fails typed here —
/// this control's database is deliberately unreachable, so credential
/// resolution stops it before any task is spawned — which is what lets
/// the test also pin that a failed restart leaves the control honestly
/// `Stopped` rather than pretending the old task still serves.
#[tokio::test]
async fn setting_a_provider_secret_while_running_stops_the_old_task_before_anything_else() {
    let dir = unique_temp_dir("restart-on-provider-secret");
    let control = control_with_state_dir(dir.path());
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut state = control.state.lock().await;
        state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
        state.running = Some(fake_running(Arc::clone(&stopped)));
    }
    assert_eq!(control.status().await.state, RuntimeState::Running);

    let result = control
        .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "key")
        .await;

    assert!(
        stopped.load(std::sync::atomic::Ordering::SeqCst),
        "the task spawned with the old provider table must be stopped and joined"
    );
    assert!(
        matches!(result, Err(LocalRunnerControlError::StartFailed(_))),
        "the restart's own start must fail typed against an unreachable database"
    );
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
    let state = control.state.lock().await;
    assert!(
        state
            .runner_config
            .providers
            .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            .is_some_and(|provider| provider.enabled),
        "the configuration the next task reads must carry the provider on"
    );
}

/// A secret no configured provider resolves is read live by whatever
/// requests reference it, so the running task keeps serving untouched.
#[tokio::test]
async fn setting_an_unrelated_secret_while_running_leaves_the_task_alone() {
    let dir = unique_temp_dir("no-restart-on-unrelated-secret");
    let control = control_with_state_dir(dir.path());
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut state = control.state.lock().await;
        state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
        state.running = Some(fake_running(Arc::clone(&stopped)));
    }

    control
        .set_secret("unrelated/entry", "value")
        .await
        .expect("an unrelated secret stores without touching the runner");

    assert!(!stopped.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(control.status().await.state, RuntimeState::Running);
}

/// Removing the credential a running provider resolves is the same
/// event in the other direction: that task was spawned with the
/// provider on, and nothing it holds can learn otherwise.
#[tokio::test]
async fn removing_a_provider_secret_while_running_stops_the_old_task() {
    let dir = unique_temp_dir("restart-on-provider-secret-removal");
    let control = control_with_state_dir(dir.path());
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut state = control.state.lock().await;
        state.bound_addr = Some("127.0.0.1:1".parse().expect("socket address"));
        state.running = Some(fake_running(Arc::clone(&stopped)));
    }

    let _ = control
        .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
        .await;

    assert!(stopped.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
}

#[tokio::test]
async fn set_then_list_then_remove_a_secret_round_trips_with_a_recorded_set_at() {
    let dir = unique_temp_dir("secret-round-trip");
    // Forces the file backend (`SecretStore::open`'s own fallback path)
    // so this test never touches a real keychain.
    // SAFETY: serialized by this variable's own uniqueness within this
    // process, same justification as `with_runner_enabled_reads_the_
    // environment_gate` above.
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());

    control
        .set_secret("vi-b3-test-secret", "shh")
        .await
        .expect("set_secret");
    let listed = control.list_secrets().await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "vi-b3-test-secret");
    assert!(
        listed[0].set_at.is_some(),
        "a secret set by this process must record when"
    );

    control
        .remove_secret("vi-b3-test-secret")
        .await
        .expect("remove_secret");
    assert!(control.list_secrets().await.is_empty());
}

#[tokio::test]
async fn setting_the_default_vercel_secret_enables_that_provider() {
    let dir = unique_temp_dir("secret-enables-provider");
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());
    assert!(matches!(
        control.catalog().await,
        CatalogSnapshot::NotConfigured
    ));

    control
        .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
        .await
        .expect("set_secret");

    let enabled = control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled;
    assert!(
        enabled,
        "the default provider must be enabled once its secret is set"
    );
}

#[tokio::test]
async fn removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled() {
    let dir = unique_temp_dir("secret-removal-disables");
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());

    control
        .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
        .await
        .expect("set_secret");
    control
        .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
        .await
        .expect("remove_secret");

    let enabled = control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled;
    assert!(
        !enabled,
        "removing the only secret that enabled this provider must leave it exactly \
         as it was before the set: disabled"
    );

    // A capability claim, not just an internal flag — assert the
    // observable absence directly: a disabled provider reports
    // `not_configured`, never `secret_unresolved` (which is what an
    // enabled-with-no-credential provider reports forever).
    assert!(matches!(
        control.catalog().await,
        CatalogSnapshot::NotConfigured
    ));
}

#[tokio::test]
async fn removing_the_default_vercel_secret_leaves_an_operator_enabled_provider_on() {
    let dir = unique_temp_dir("secret-removal-preserves-operator-enable");
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());
    // Simulates an operator's own explicit configuration (TOML, or
    // `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED`) turning the
    // provider on before any key was ever pasted through this control —
    // never something `set_secret` itself did.
    control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled = true;

    control
        .set_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET, "shh")
        .await
        .expect("set_secret");
    control
        .remove_secret(tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET)
        .await
        .expect("remove_secret");

    let enabled = control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled;
    assert!(
        enabled,
        "a provider the operator enabled directly must never be turned off by a \
         later key removal through this route"
    );
}

#[tokio::test]
async fn removing_a_non_default_secret_never_touches_the_providers_enabled_flag() {
    let dir = unique_temp_dir("secret-removal-narrow-name");
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());
    control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled = true;

    control
        .remove_secret("some-other-secret-name")
        .await
        .expect("remove_secret");

    let enabled = control
        .state
        .lock()
        .await
        .runner_config
        .providers
        .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("seeded provider entry")
        .enabled;
    assert!(
        enabled,
        "removing a secret under any name other than the default must never touch \
         this provider's enabled flag — mirrors set_secret's own narrow scope"
    );
}

#[tokio::test]
async fn start_then_stop_round_trips_the_runtime_state() {
    let dir = unique_temp_dir("start-stop");
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
    let control = control_with_state_dir(dir.path());
    // `unreachable_database_url` makes self-provisioning fail deliberately
    // (no session on disk, no manual credential) — this test only needs
    // to prove `start()` reaches the point of attempting it, and that a
    // failure there is reported rather than silently leaving `running`
    // set. A live start-then-claim proof belongs to an integration test
    // with a real server, not this unit.
    control.set_bound_addr("127.0.0.1:1".parse().unwrap()).await;

    let result = control.start().await;

    assert!(
        matches!(result, Err(LocalRunnerControlError::StartFailed(_))),
        "self-provisioning against an unreachable database must fail typed: {result:?}"
    );
    assert_eq!(
        control.status().await.state,
        RuntimeState::Stopped,
        "a failed start must not leave the control reporting running"
    );
}

#[test]
fn embedded_default_state_dir_nests_under_storage_dir() {
    assert_eq!(
        embedded_default_state_dir("./storage"),
        PathBuf::from("./storage/runner")
    );
    assert_eq!(
        embedded_default_state_dir("/srv/tack-b/storage"),
        PathBuf::from("/srv/tack-b/storage/runner")
    );
}

/// Changes the process's current directory for the duration of a
/// closure, restoring it afterward even if the closure panics — needed
/// by every test below that exercises [`migrate_legacy_state_dir`],
/// since its legacy side is the crate's bare, cwd-relative default.
/// Sound under this crate's own rule for mutating process-global state
/// in tests: nextest gives each test its own process, so no sibling
/// test's thread observes this change.
fn with_cwd<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(dir).expect("set cwd");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::env::set_current_dir(original).expect("restore cwd");
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn migrate_legacy_state_dir_moves_an_existing_legacy_directory_once() {
    let cwd_guard = unique_temp_dir("migrate-cwd");
    let target_root = unique_temp_dir("migrate-target");
    let new_dir = target_root.path().join("storage").join("runner");

    with_cwd(cwd_guard.path(), || {
        let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
        std::fs::create_dir_all(legacy).expect("create legacy dir");
        std::fs::write(legacy.join("session.json"), "legacy-session")
            .expect("write legacy session");

        migrate_legacy_state_dir(&new_dir);

        assert!(
            new_dir.join("session.json").is_file(),
            "legacy state must be moved to the new, scoped directory"
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
            "legacy-session"
        );
        assert!(
            !legacy.exists(),
            "the legacy directory must not be left behind after a successful migration"
        );
    });
}

#[test]
fn migrate_legacy_state_dir_never_touches_an_already_provisioned_new_directory() {
    let cwd_guard = unique_temp_dir("migrate-cwd-noop");
    let target_root = unique_temp_dir("migrate-target-noop");
    let new_dir = target_root.path().join("runner");
    std::fs::create_dir_all(&new_dir).expect("pre-create the new directory");
    std::fs::write(new_dir.join("session.json"), "already-provisioned").expect("write new session");

    with_cwd(cwd_guard.path(), || {
        let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
        std::fs::create_dir_all(legacy).expect("create legacy dir");
        std::fs::write(legacy.join("session.json"), "stale-legacy-session")
            .expect("write legacy session");

        migrate_legacy_state_dir(&new_dir);

        assert_eq!(
            std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
            "already-provisioned",
            "an existing new-style directory must never be overwritten by a stale legacy one"
        );
        assert!(
            legacy.join("session.json").is_file(),
            "the legacy directory is left untouched once the new one already exists"
        );
    });
}

#[test]
fn migrate_legacy_state_dir_is_a_no_op_when_neither_directory_exists() {
    let cwd_guard = unique_temp_dir("migrate-cwd-fresh");
    let target_root = unique_temp_dir("migrate-target-fresh");
    let new_dir = target_root.path().join("storage").join("runner");

    with_cwd(cwd_guard.path(), || {
        migrate_legacy_state_dir(&new_dir);

        assert!(
            !new_dir.exists(),
            "a fresh install has nothing to migrate; the new directory is created \
             later, on first write, not by the migration step itself"
        );
    });
}

// The acceptance proof for two servers each seeing only their own
// runner's enrollment lives in `tests/embedded_runner_state_scoping.rs`
// as a real two-subprocess test, not here: `tack_api::server::serve_inner`
// installs a process-global `tracing` subscriber via `try_init`, so only
// the first call in a process actually configures it — a second call is
// a silent no-op rather than a crash, but a second embedded server in
// that same process would still run under the first one's logging
// config instead of its own. So this crate's own unit tests — which all
// share one test binary process per test function, not per server —
// still test at most one real embedded server each. Two genuinely
// separate `tack` processes have no such conflict.

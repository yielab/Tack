use super::*;

/// A store file inside a directory that removes itself when the returned
/// guard drops. The caller holds the guard: the file must not outlive it.
fn temp_store() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("secrets.json");
    (dir, path)
}

#[test]
fn file_backend_round_trips_a_secret_and_reports_itself() {
    let (_dir, path) = temp_store();
    let store = SecretStore::file(path.clone());

    assert_eq!(store.backend(), SecretBackendKind::File);
    store.set("demo", "topsecret-value").expect("set");
    assert_eq!(store.get("demo").expect("get").expose(), "topsecret-value");
    assert_eq!(store.list().expect("list"), vec!["demo".to_string()]);

    store.remove("demo").expect("remove");
    assert!(matches!(store.get("demo"), Err(SecretError::NotFound(name)) if name == "demo"));

    let _ = fs::remove_file(&path);
}

#[test]
fn file_backend_missing_name_is_not_found_not_a_crash() {
    let (_dir, path) = temp_store();
    let store = SecretStore::file(path);
    let error = store.get("absent").expect_err("nothing was ever set");
    assert!(matches!(error, SecretError::NotFound(name) if name == "absent"));
}

#[test]
fn file_backend_writes_the_secrets_file_owner_only() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, path) = temp_store();
        let store = SecretStore::file(path.clone());
        store.set("demo", "value").expect("set");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "secrets file must be owner-only (600), was {mode:o}"
        );

        let _ = fs::remove_file(&path);
    }
}

#[test]
fn resolve_defaults_to_the_store_scheme_for_a_bare_name() {
    let (_dir, path) = temp_store();
    let store = SecretStore::file(path);
    store.set("demo", "bare-name-value").expect("set");

    assert_eq!(
        store.resolve("demo").expect("resolve").expose(),
        "bare-name-value"
    );
    assert_eq!(
        store.resolve("store:demo").expect("resolve").expose(),
        "bare-name-value"
    );
}

#[test]
fn resolve_env_scheme_reads_the_process_environment() {
    // SAFETY: single-threaded within this test's own env var name; no
    // other test in this crate reads or writes this name.
    unsafe {
        std::env::set_var("TACK_SECRETS_TEST_ENV_VALUE", "env-value");
    }
    let (_dir, path) = temp_store();
    let store = SecretStore::file(path);

    assert_eq!(
        store
            .resolve("env:TACK_SECRETS_TEST_ENV_VALUE")
            .expect("resolve")
            .expose(),
        "env-value"
    );

    unsafe {
        std::env::remove_var("TACK_SECRETS_TEST_ENV_VALUE");
    }
    assert!(matches!(
        store.resolve("env:TACK_SECRETS_TEST_ENV_VALUE"),
        Err(SecretError::EnvVarNotSet(name)) if name == "TACK_SECRETS_TEST_ENV_VALUE"
    ));
}

#[test]
fn resolve_rejects_an_empty_name() {
    let (_dir, path) = temp_store();
    let store = SecretStore::file(path);
    assert!(matches!(
        store.resolve("store:"),
        Err(SecretError::InvalidReference(reference)) if reference == "store:"
    ));
}

#[test]
fn debug_and_display_never_print_the_secret_value() {
    let secret = SecretValue::new("must-never-be-printed");
    assert!(!format!("{secret:?}").contains("must-never-be-printed"));
    assert!(!format!("{secret}").contains("must-never-be-printed"));
}

// ---------------------------------------------------------------
// Keychain code path, exercised against `keyring_core`'s in-crate
// mock store (ships unconditionally in 1.0.0's `mock` module) —
// never a real Secret Service, so this runs the same in CI.
// ---------------------------------------------------------------

#[test]
fn keychain_backend_round_trips_against_the_mock_store() {
    let mock = keyring_core::mock::Store::new().expect("mock store");
    let store = SecretStore::with_store(mock);

    assert_eq!(store.backend(), SecretBackendKind::Keychain);
    store.set("demo", "mock-secret-value").expect("set");
    assert_eq!(
        store.get("demo").expect("get").expose(),
        "mock-secret-value"
    );
    assert_eq!(store.list().expect("list"), vec!["demo".to_string()]);

    store.remove("demo").expect("remove");
    assert!(matches!(store.get("demo"), Err(SecretError::NotFound(_))));
}

#[test]
fn keychain_backend_missing_name_is_not_found_not_a_crash() {
    let mock = keyring_core::mock::Store::new().expect("mock store");
    let store = SecretStore::with_store(mock);

    let error = store.get("absent").expect_err("nothing was ever set");
    assert!(matches!(error, SecretError::NotFound(name) if name == "absent"));
}

// ---------------------------------------------------------------
// The bound on a hung platform-store probe. `bounded` is the whole
// mechanism `open` relies on to avoid stalling on a real Secret
// Service, so it is tested directly against a fake probe that never
// returns — the same shape a D-Bus activation stall has — rather than
// against the real, target-gated `platform_store_unbounded`.
// ---------------------------------------------------------------

#[test]
fn bounded_times_out_promptly_when_the_work_never_answers() {
    let bound = Duration::from_millis(50);
    let started = std::time::Instant::now();

    let result: Result<(), String> = SecretStore::bounded(bound, || {
        std::thread::sleep(Duration::from_secs(5));
        Ok(())
    });

    let elapsed = started.elapsed();
    assert!(
        result.is_err(),
        "a probe that never answers must not be treated as success"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "bounded() took {elapsed:?} to return against a {bound:?} bound; \
         the abandoned probe's 5s sleep must not be on the caller's critical path"
    );
}

#[test]
fn bounded_returns_the_work_s_own_result_when_it_answers_in_time() {
    let ok: Result<i32, String> = SecretStore::bounded(Duration::from_secs(1), || Ok(42));
    assert_eq!(ok, Ok(42));

    let err: Result<i32, String> =
        SecretStore::bounded(
            Duration::from_secs(1),
            || Err("backend said no".to_string()),
        );
    assert_eq!(err, Err("backend said no".to_string()));
}

//! HTTP-level proof for `handlers::local_runner` (ADR 0061 decisions 2 and
//! 6): the routes exist only on a loopback bind with a control actually
//! wired in, they are a genuine 404 otherwise (never present-and-refusing),
//! the on/off preference persists in `app_meta` and nowhere else, and a
//! secret value is never echoed back. A fake [`LocalRunnerControl`] stands
//! in for the real embedded-runner composition
//! (`crates/tack-cli/src/local_runner.rs`), unreachable from this crate —
//! this proves the HTTP contract those routes must uphold regardless of
//! which control answers them.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tack_api::{
    CatalogSnapshot, LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
    SecretMeta, config::AppConfig,
};
use tower::ServiceExt;

use crate::common::test_app_with_local_runner;

/// Records every call it receives so a test can assert on which methods
/// actually ran, without needing a real runner process.
#[derive(Default)]
struct FakeControl {
    running: AtomicBool,
    start_calls: std::sync::atomic::AtomicUsize,
    set_secret_calls: std::sync::Mutex<Vec<(String, String)>>,
    secrets: std::sync::Mutex<Vec<SecretMeta>>,
}

#[async_trait::async_trait]
impl LocalRunnerControl for FakeControl {
    async fn status(&self) -> RuntimeStatus {
        if self.running.load(Ordering::SeqCst) {
            RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(Utc::now()),
            }
        } else {
            RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
            }
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        self.start_calls.fetch_add(1, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    async fn list_secrets(&self) -> Vec<SecretMeta> {
        self.secrets.lock().unwrap().clone()
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), LocalRunnerControlError> {
        self.set_secret_calls
            .lock()
            .unwrap()
            .push((name.to_owned(), value.to_owned()));
        self.secrets.lock().unwrap().push(SecretMeta {
            name: name.to_owned(),
            set_at: Some(Utc::now()),
        });
        Ok(())
    }

    async fn remove_secret(&self, name: &str) -> Result<(), LocalRunnerControlError> {
        self.secrets.lock().unwrap().retain(|s| s.name != name);
        Ok(())
    }

    async fn catalog(&self) -> CatalogSnapshot {
        CatalogSnapshot::NotConfigured
    }
}

fn loopback_config() -> AppConfig {
    AppConfig {
        host: "127.0.0.1".to_owned(),
        ..AppConfig::default()
    }
}

fn non_loopback_config() -> AppConfig {
    AppConfig {
        host: "0.0.0.0".to_owned(),
        allow_unauthenticated_nonloopback: true,
        ..AppConfig::default()
    }
}

type MaybeControl = Option<Arc<dyn LocalRunnerControl>>;

#[tokio::test]
async fn routes_are_absent_without_both_loopback_and_a_control() {
    let control: Arc<dyn LocalRunnerControl> = Arc::new(FakeControl::default());
    let cases: Vec<(&str, AppConfig, MaybeControl)> = vec![
        (
            "non-loopback bind, control wired in",
            non_loopback_config(),
            Some(control.clone()),
        ),
        (
            "loopback bind, no control wired in",
            loopback_config(),
            None,
        ),
    ];
    for (name, config, control) in cases {
        let (app, _workspace_id) = test_app_with_local_runner(config, control).await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/local-runner")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // A genuine 404 — not a 409/403 "disabled" envelope. `build_router`
        // never merges `local_runner_routes` at all unless both hold, so
        // this is axum's own fallback for an unmatched path, proving the
        // routes are absent rather than present-and-refusing.
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{name}");
    }
}

#[tokio::test]
async fn loopback_bind_with_control_mounts_routes_and_starts_it() {
    let control = Arc::new(FakeControl::default());
    let control_trait_object: Arc<dyn LocalRunnerControl> = control.clone();
    let (app, _workspace_id) =
        test_app_with_local_runner(loopback_config(), Some(control_trait_object)).await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/local-runner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["enabled"], false);
    assert_eq!(body["state"], "stopped");
    assert!(body["since"].is_null());

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/local-runner")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"enabled": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        control.start_calls.load(Ordering::SeqCst),
        1,
        "PUT {{enabled:true}} must call the exact same start() the auto-start check would"
    );

    let get_after = app
        .oneshot(
            Request::builder()
                .uri("/api/local-runner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(get_after.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["state"], "running");
}

#[tokio::test]
async fn secret_write_never_touches_the_enable_preference_row() {
    let control: Arc<dyn LocalRunnerControl> = Arc::new(FakeControl::default());
    let (app, _workspace_id) = test_app_with_local_runner(loopback_config(), Some(control)).await;

    let put_secret = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/local-runner/secrets/vercel-ai-gateway%2Fdefault")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"value": "positive-control-marker-value"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_secret.status(), StatusCode::NO_CONTENT);

    // A secret write must never touch `app_meta` — the on/off preference
    // is the only key this module ever writes there, and this request
    // never wrote it. Asserted from inside the handler's own crate isn't
    // possible from here (this is a black-box HTTP test), so this proves
    // the same claim through the route contract instead: `GET
    // /api/local-runner/secrets` reflects the write, and a `GET
    // /api/local-runner` still reports the untouched `enabled: false`
    // env-default — the two are independent, exactly as
    // `handlers::local_runner`'s module doc says the secret half must be.
    let list_secrets = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/local-runner/secrets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(list_secrets.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let names: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["vercel-ai-gateway/default"]);
    // The response never carries the value under any key.
    assert!(!body.to_string().contains("positive-control-marker-value"));

    let status = app
        .oneshot(
            Request::builder()
                .uri("/api/local-runner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(status.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        body["enabled"], false,
        "the secret write above must not have touched the persisted enable preference"
    );
}

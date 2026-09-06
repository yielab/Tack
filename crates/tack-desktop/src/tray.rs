//! System tray icon and menu (ADR 0062 decision 3): *Open Tack*, the agent
//! execution status, *Launch at login*, and *Quit*. Tauri's tray icon does
//! not emit click events on Linux, so every action lives in the menu —
//! nothing here depends on clicking the icon itself.

use std::time::Duration;

use tauri::AppHandle;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_autostart::ManagerExt;

use crate::lifecycle;
use crate::paths::DataPaths;
use crate::supervisor::DEFAULT_PORT;

const MENU_ID_OPEN: &str = "open";
const MENU_ID_AGENT_EXECUTION: &str = "agent_execution";
const MENU_ID_LAUNCH_AT_LOGIN: &str = "launch_at_login";
const MENU_ID_QUIT: &str = "quit";

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// `GET /api/local-runner`'s body, narrowed to the two fields the tray
/// renders. `enabled` is the persisted preference; `state` is what the
/// embedded runner is actually doing right now, so the two can disagree for
/// a moment while a toggle takes effect.
#[derive(Debug, serde::Deserialize)]
struct LocalRunnerBody {
    enabled: bool,
    state: String,
}

/// Every label the menu entry can show. A failure is always one of the two
/// typed variants below, never folded into `Off` — a server that hasn't
/// answered yet says nothing about whether the runner is on.
enum AgentExecutionStatus {
    On,
    Off,
    TurningOn,
    TurningOff,
    ServerNotAnswering,
    RequestFailed,
}

impl AgentExecutionStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::On => "Agent execution: on",
            Self::Off => "Agent execution: off",
            Self::TurningOn => "Agent execution: turning on…",
            Self::TurningOff => "Agent execution: turning off…",
            Self::ServerNotAnswering => "Agent execution: waiting for the server…",
            Self::RequestFailed => "Agent execution: status unavailable (request failed)",
        }
    }

    fn from_body(body: &LocalRunnerBody) -> Self {
        match (body.enabled, body.state.as_str()) {
            (true, "running") => Self::On,
            (false, "stopped") => Self::Off,
            (true, _) => Self::TurningOn,
            (false, _) => Self::TurningOff,
        }
    }
}

/// The server's base URL, built from the port this app's own settings.json
/// names — the same file [`crate::first_run`] writes — falling back to the
/// default port when the file is missing or unreadable (nothing has run yet,
/// or a transient read error), exactly like [`crate::first_run::Settings`]'s
/// own default.
fn resolve_base_url() -> String {
    let port = DataPaths::resolve()
        .ok()
        .and_then(|paths| std::fs::read(&paths.settings_file).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|settings| settings.get("port").and_then(serde_json::Value::as_u64))
        .and_then(|port| u16::try_from(port).ok())
        .unwrap_or(DEFAULT_PORT);
    format!("http://127.0.0.1:{port}")
}

/// One poll of `GET /api/local-runner`. A connection error or timeout means
/// the server has not come up (or has gone away) — distinct from a reachable
/// server answering with something this app cannot parse.
async fn poll_once(client: &reqwest::Client, base_url: &str) -> AgentExecutionStatus {
    let response = match client
        .get(format!("{base_url}/api/local-runner"))
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) if err.is_connect() || err.is_timeout() => {
            return AgentExecutionStatus::ServerNotAnswering;
        }
        Err(_) => return AgentExecutionStatus::RequestFailed,
    };
    if !response.status().is_success() {
        return AgentExecutionStatus::RequestFailed;
    }
    match response.json::<LocalRunnerBody>().await {
        Ok(body) => AgentExecutionStatus::from_body(&body),
        Err(_) => AgentExecutionStatus::RequestFailed,
    }
}

/// Builds the tray icon and attaches its menu. Call once from `setup`, after
/// [`ensure_launch_at_login_default_on_first_run`] so the checkbox's initial
/// state matches whatever that just decided.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, MENU_ID_OPEN, "Open Tack", true, None::<&str>)?;
    let agent_execution_item = MenuItem::with_id(
        app,
        MENU_ID_AGENT_EXECUTION,
        AgentExecutionStatus::ServerNotAnswering.label(),
        false,
        None::<&str>,
    )?;
    let launch_at_login_checked = app.autolaunch().is_enabled().unwrap_or(false);
    let launch_at_login_item = CheckMenuItem::with_id(
        app,
        MENU_ID_LAUNCH_AT_LOGIN,
        "Launch at login",
        true,
        launch_at_login_checked,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, MENU_ID_QUIT, "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &agent_execution_item,
            &PredefinedMenuItem::separator(app)?,
            &launch_at_login_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let launch_at_login_for_handler = launch_at_login_item.clone();
    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            MENU_ID_OPEN => lifecycle::show_and_focus(app),
            MENU_ID_LAUNCH_AT_LOGIN => {
                lifecycle::toggle_launch_at_login(app, &launch_at_login_for_handler)
            }
            MENU_ID_QUIT => lifecycle::quit(app.clone()),
            _ => {}
        })
        .build(app)?;

    let poll_target = agent_execution_item;
    tauri::async_runtime::spawn(async move {
        let base_url = resolve_base_url();
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        loop {
            let status = poll_once(&client, &base_url).await;
            if let Err(err) = poll_target.set_text(status.label()) {
                tracing::error!(error = %err, "failed to update the agent-execution tray label");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });

    Ok(())
}

/// Applies the "on by default" half of decision 3 exactly once: the first
/// time this app ever runs, launch-at-login is turned on; every later run
/// leaves whatever the user has it set to alone. The marker
/// (`.autostart-initialized`, an empty flag file) lives directly under the
/// app's real per-user data root (`DataPaths::root`), next to settings.json
/// and the server's own storage/runner/log folders.
pub fn ensure_launch_at_login_default_on_first_run(app: &AppHandle, data_root: &std::path::Path) {
    let marker = data_root.join(".autostart-initialized");
    if marker.exists() {
        return;
    }
    match app.autolaunch().enable() {
        Ok(()) => {
            if let Err(err) = std::fs::write(&marker, b"") {
                tracing::error!(error = %err, "failed to write the autostart first-run marker");
            }
            tracing::info!("launch at login enabled by default on first run");
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to enable launch at login by default");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(enabled: bool, state: &str) -> LocalRunnerBody {
        LocalRunnerBody {
            enabled,
            state: state.to_string(),
        }
    }

    #[test]
    fn running_and_enabled_reads_as_on() {
        assert_eq!(
            AgentExecutionStatus::from_body(&body(true, "running")).label(),
            "Agent execution: on"
        );
    }

    #[test]
    fn stopped_and_disabled_reads_as_off() {
        assert_eq!(
            AgentExecutionStatus::from_body(&body(false, "stopped")).label(),
            "Agent execution: off"
        );
    }

    #[test]
    fn enabled_but_not_yet_running_reads_as_turning_on() {
        assert_eq!(
            AgentExecutionStatus::from_body(&body(true, "stopped")).label(),
            "Agent execution: turning on…"
        );
    }

    #[test]
    fn disabled_but_still_running_reads_as_turning_off() {
        assert_eq!(
            AgentExecutionStatus::from_body(&body(false, "running")).label(),
            "Agent execution: turning off…"
        );
    }

    #[test]
    fn resolve_base_url_falls_back_to_the_default_port_with_no_settings_file() {
        // `DataPaths::resolve()` only fails when the OS cannot name a data
        // directory at all; on any real host this reads a settings file
        // that (in this test process) was never written, so the fallback
        // path is what actually runs.
        assert!(resolve_base_url().starts_with("http://127.0.0.1:"));
    }
}

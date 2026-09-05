// The app that supervises `tack`: no window of its own content, no API of its
// own — it spawns (or attaches to) the real server and loads its web UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod first_run;
mod lifecycle;
mod paths;
mod supervisor;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;

use paths::DataPaths;
use supervisor::{
    Outcome, SidecarHandle, SidecarLauncher, SupervisorError, attach_or_start, shutdown,
};

/// [`SidecarHandle`] backed by the real Tauri sidecar child.
struct TauriSidecarHandle(CommandChild);

impl SidecarHandle for TauriSidecarHandle {
    fn pid(&self) -> u32 {
        self.0.pid()
    }

    fn kill(self) -> std::io::Result<()> {
        self.0
            .kill()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// [`SidecarLauncher`] that spawns the bundled `tack` binary through
/// `tauri-plugin-shell`'s sidecar mechanism (ADR 0062 decision 2).
struct TauriLauncher {
    app: tauri::AppHandle,
    port: u16,
}

impl SidecarLauncher for TauriLauncher {
    type Process = TauriSidecarHandle;

    fn spawn(&self, env: &[(String, String)]) -> Result<Self::Process, SupervisorError> {
        let mut command = self
            .app
            .shell()
            .sidecar("tack")
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?
            .args(["serve", "--with-runner"])
            .env("TACK_HOST", "127.0.0.1")
            .env("TACK_PORT", self.port.to_string());
        for (key, value) in env {
            command = command.env(key, value);
        }
        let (_events, child) = command
            .spawn()
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))?;
        Ok(TauriSidecarHandle(child))
    }
}

/// Holds whatever the supervisor decided so the shutdown path (on app exit)
/// knows whether there is a child to stop. `None` until the async setup task
/// resolves; `Attached` is never touched on exit (rule: never stop a server
/// this app did not start).
enum ServerMode {
    Attached,
    Started(TauriSidecarHandle),
}

struct DesktopState(Mutex<Option<ServerMode>>);

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        // Must be registered before any other plugin (tauri-plugin-single-instance's
        // own requirement): a second launch focuses the existing window instead of
        // opening a new one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            lifecycle::show_and_focus(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState(Mutex::new(None)))
        .setup(|app| {
            let handle = app.handle().clone();

            // Only creates/locates the data directory — never touches
            // settings.json or shows a dialog. That has to wait for the
            // async task below: `setup()` runs on the main thread before
            // `.run()` starts the event loop, and `ensure_settings`'s
            // first-run dialog is a `blocking_show()`, which
            // tauri-plugin-dialog documents as unsafe to call on that
            // thread — it marshals the dialog itself onto the main
            // thread's event loop via `run_on_main_thread` and waits for
            // the answer there, so calling it from the thread whose loop
            // hasn't started yet deadlocks forever: no window, no log
            // line, no spawned sidecar, indistinguishable from the app
            // doing nothing at all.
            let paths = DataPaths::resolve().map_err(|e| e.to_string())?;

            tray::ensure_launch_at_login_default_on_first_run(&handle, &paths.root);
            tray::build(&handle).map_err(|e| e.to_string())?;

            tauri::async_runtime::spawn(async move {
                // Runs on a runtime worker thread, not the one `setup()`
                // ran on — safe for `blocking_show()` to call into.
                let settings = tauri::async_runtime::spawn_blocking({
                    let handle = handle.clone();
                    let paths = paths.clone();
                    move || first_run::ensure_settings(&handle, &paths)
                })
                .await
                .expect("ensure_settings must not panic");
                let port = settings.port;
                let base_url = format!("http://127.0.0.1:{port}");
                let folders = paths.server_folders(settings.database_path.as_deref());

                let client = reqwest::Client::new();
                let launcher = TauriLauncher {
                    app: handle.clone(),
                    port,
                };

                match attach_or_start(
                    &client,
                    &base_url,
                    port,
                    &launcher,
                    &folders,
                    env!("CARGO_PKG_VERSION"),
                )
                .await
                {
                    Ok(outcome) => {
                        let mode = match outcome {
                            Outcome::Attached { health } => {
                                tracing::info!(version = %health.version, status = %health.status, "attached to an existing Tack server");
                                ServerMode::Attached
                            }
                            Outcome::Started { health, process } => {
                                tracing::info!(version = %health.version, status = %health.status, pid = process.pid(), "started the Tack server");
                                ServerMode::Started(process)
                            }
                        };
                        if let Some(state) = handle.try_state::<DesktopState>() {
                            *state.0.lock().unwrap() = Some(mode);
                        }

                        let url = base_url.parse().expect("base_url is a valid URL");
                        if let Err(err) =
                            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::External(url))
                                .title("Tack")
                                .inner_size(1200.0, 800.0)
                                .build()
                        {
                            tracing::error!(error = %err, "failed to open the main window");
                            handle.exit(1);
                        }
                    }
                    Err(SupervisorError::PortOccupiedByOther(port)) => {
                        tracing::error!(port, "port is occupied by something that is not Tack");
                        handle
                            .dialog()
                            .message(format!(
                                "Port {port} is already in use by something other than Tack. \
                                 Close whatever is using it and reopen Tack."
                            ))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
                        handle.exit(1);
                    }
                    Err(SupervisorError::OutdatedServer {
                        server_version,
                        bundled_version,
                    }) => {
                        tracing::error!(
                            server_version = %server_version,
                            bundled_version = %bundled_version,
                            "attached server is older than the bundled version"
                        );
                        handle
                            .dialog()
                            .message(format!(
                                "The Tack server already running is version {server_version}, \
                                 older than the {bundled_version} this app bundles. Update the \
                                 server, then reopen Tack."
                            ))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
                        handle.exit(1);
                    }
                    Err(err) => {
                        // Covers `HealthTimeout` and `SpawnFailed` — the two
                        // variants above already have their own tailored
                        // dialogs. `{err}` carries the specific reason
                        // (`thiserror`'s `Display`), including the bounded
                        // wait's own length for a timeout — never a generic
                        // apology, and never a longer sleep in its place.
                        tracing::error!(error = %err, "could not attach to or start a Tack server");
                        handle
                            .dialog()
                            .message(format!("Tack could not start: {err}"))
                            .title("Tack")
                            .kind(MessageDialogKind::Error)
                            .blocking_show();
                        handle.exit(1);
                    }
                }
            });

            Ok(())
        })
        .on_window_event(lifecycle::handle_window_event)
        .build(tauri::generate_context!())
        .expect("error while building the tauri application")
        .run(|app_handle, event| {
            // The window hides rather than closes, so this fires only from
            // an explicit Quit or an external signal — never from a window
            // close. Either way, a spawned child must never outlive the app.
            if let RunEvent::ExitRequested { .. } = event
                && let Some(state) = app_handle.try_state::<DesktopState>()
            {
                let mode = state.0.lock().unwrap().take();
                if let Some(ServerMode::Started(process)) = mode
                    && let Err(err) = shutdown(process)
                {
                    tracing::error!(error = %err, "failed to stop the spawned Tack server");
                }
            }
        });
}

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures::{SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::{error::ApiError, router::AppState};

/// Messages sent over WebSocket for board updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardEvent {
    /// Item was created
    ItemCreated {
        project_id: Uuid,
        item_id: Uuid,
        status: String,
    },
    /// Item was updated (moved, edited, etc)
    ItemUpdated {
        project_id: Uuid,
        item_id: Uuid,
        old_status: Option<String>,
        new_status: String,
    },
    /// Item was deleted
    ItemDeleted { project_id: Uuid, item_id: Uuid },
    /// Board configuration changed (columns, WIP limits)
    BoardConfigUpdated { project_id: Uuid },
    /// Sprint changed
    SprintUpdated { project_id: Uuid, sprint_id: Uuid },
    /// An orchestrated agent run mirrored from a control plane (docket) changed
    /// state, or was newly attributed to this item. Emitted from
    /// `orch_store::RepoControlPlaneStore::upsert_runs` — never from the
    /// reconciler itself, which has no WebSocket dependency.
    AgentRunUpdated {
        project_id: Uuid,
        item_id: Uuid,
        run_id: String,
        /// Raw `RunState` string (`queued` / `running` / `succeeded` / `failed` /
        /// `cancelled`, or an unrecognised value) — stored and forwarded as-is,
        /// same convention as `orch_runs.state`.
        state: String,
    },
    /// A mirrored approval became (newly) pending on an item this project can
    /// see. Deliberately narrower than a generic "approval updated" event: it
    /// only fires on the transition into `pending`, not on every re-poll or on
    /// a grant/deny decision. Emitted from
    /// `orch_store::RepoControlPlaneStore::upsert_approvals`.
    ApprovalPending {
        project_id: Uuid,
        item_id: Uuid,
        token: String,
        action: Option<String>,
    },
    /// Keepalive ping
    Ping,
}

/// The one subprotocol this handshake ever selects — it names the message
/// format this module actually sends (the tagged `BoardEvent` JSON above),
/// nothing more. The frontend (`shared/realtime/boardSocket.ts`) always
/// offers it. A client that offers a subprotocol and gets a handshake
/// response with no `Sec-WebSocket-Protocol` header at all must fail the
/// connection per the WebSocket spec (RFC 6455 §4.1); a server that never
/// selects a protocol here was silently unreachable from any browser that
/// offered one, even though the TCP-level handshake itself still completes.
const BOARD_SUBPROTOCOL: &str = "tack.v1";

/// WebSocket handler for live board updates
#[instrument(skip(ws, state))]
pub async fn board_live(
    ws: WebSocketUpgrade,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    // Verify project exists
    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", project_id)))?;

    let ws = ws.protocols([BOARD_SUBPROTOCOL]);
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, project_id, state)))
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, project_id: Uuid, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast channel
    let mut rx = state.broadcast_tx.subscribe();

    // Spawn task to receive broadcast messages and send to client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Only send events for this project
            if event_matches_project(&event, project_id) {
                let msg = match serde_json::to_string(&event) {
                    Ok(json) => Message::Text(json.into()),
                    Err(e) => {
                        tracing::error!("Failed to serialize event: {}", e);
                        continue;
                    }
                };

                if sender.send(msg).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn task to receive messages from client (mostly pings)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Received WebSocket message: {}", text);
                    // Could handle client commands here if needed
                }
                Message::Close(_) => {
                    break;
                }
                Message::Ping(data) => {
                    // Axum handles ping/pong automatically, but we log it
                    tracing::trace!("Received ping: {:?}", data);
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }
}

/// Check if event is relevant for this project
fn event_matches_project(event: &BoardEvent, project_id: Uuid) -> bool {
    match event {
        BoardEvent::ItemCreated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::ItemUpdated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::ItemDeleted {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::BoardConfigUpdated { project_id: pid } => *pid == project_id,
        BoardEvent::SprintUpdated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::AgentRunUpdated {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::ApprovalPending {
            project_id: pid, ..
        } => *pid == project_id,
        BoardEvent::Ping => true,
    }
}

/// Helper function to broadcast events (called from other handlers)
pub fn broadcast_event(state: &AppState, event: BoardEvent) {
    // Ignore errors - no subscribers is fine
    let _ = state.broadcast_tx.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_run_updated_serializes_with_snake_case_tag() {
        let event = BoardEvent::AgentRunUpdated {
            project_id: Uuid::nil(),
            item_id: Uuid::nil(),
            run_id: "run-1".into(),
            state: "running".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "agent_run_updated");
        assert_eq!(json["run_id"], "run-1");
        assert_eq!(json["state"], "running");
    }

    #[test]
    fn approval_pending_serializes_with_snake_case_tag_and_optional_action() {
        let event = BoardEvent::ApprovalPending {
            project_id: Uuid::nil(),
            item_id: Uuid::nil(),
            token: "tok-1".into(),
            action: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "approval_pending");
        assert_eq!(json["token"], "tok-1");
        assert!(json["action"].is_null());
    }

    #[test]
    fn agent_run_updated_is_filtered_by_project_id_like_every_other_event() {
        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let event = BoardEvent::AgentRunUpdated {
            project_id: project_a,
            item_id: Uuid::new_v4(),
            run_id: "run-1".into(),
            state: "succeeded".into(),
        };
        assert!(event_matches_project(&event, project_a));
        assert!(!event_matches_project(&event, project_b));
    }

    #[test]
    fn approval_pending_is_filtered_by_project_id_like_every_other_event() {
        let project_a = Uuid::new_v4();
        let project_b = Uuid::new_v4();
        let event = BoardEvent::ApprovalPending {
            project_id: project_a,
            item_id: Uuid::new_v4(),
            token: "tok-1".into(),
            action: Some("merge".into()),
        };
        assert!(event_matches_project(&event, project_a));
        assert!(!event_matches_project(&event, project_b));
    }

    /// The client-side requirement: an unknown event
    /// variant on the wire must be ignored, not thrown. On the Rust side the
    /// analogous guarantee is that decoding never panics on a *future* tag —
    /// `#[serde(tag = "type", rename_all = "snake_case")]` on a closed enum
    /// means an unrecognised `type` fails deserialization with an `Err`
    /// rather than a panic, so callers (like the frontend, and any future
    /// Rust WebSocket client) always get a recoverable `Result`.
    #[test]
    fn an_unrecognised_event_type_fails_to_deserialize_without_panicking() {
        let raw = serde_json::json!({"type": "some_future_event", "project_id": Uuid::nil()});
        let result: Result<BoardEvent, _> = serde_json::from_value(raw);
        assert!(result.is_err());
    }
}

//! `DocketAdapter` — the [`ControlPlane`] implementation for docket.
//!
//! # Constructor
//!
//! ```ignore
//! let adapter = DocketAdapter::new("http://127.0.0.1:7331", Some(token))?;
//!
//! Design notes: docs/dev-notes/tack-orch/adapters/docket.md

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::adapters::prometheus;
use crate::{
    ApprovalState, Capabilities, ControlPlane, DecisionSupport, EventScope, FleetStatus, Health,
    MetricSample, ModelSelection, NewRemoteTask, OrchError, ProvisionPodParams, ProvisionedPod,
    Rated, RemoteApproval, RemoteEvent, RemoteRun, RemoteTask, Support, TracesPage, UsageSupport,
};

/// The fixed `channel` docket records against every approval decision made
/// through Tack's UI (`approval.APPROVAL_CHANNELS` already lists `"tack"`
/// alongside `cli`/`http`/`mcp`/`telegram`/`timeout` — verified against
/// `~/Sites/rack-cli/src/docket/core/approval.py`). See
/// [`ControlPlane::decide_approval`]'s doc comment for why this isn't a
/// parameter.
const APPROVAL_CHANNEL: &str = "tack";

/// Every request this adapter makes gets this timeout — docket runs on
/// loopback in every real deployment, so 5s is generous for a
/// live plane and still fails fast against a hung/unreachable one rather
/// than blocking a reconciler poll tick indefinitely.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of a non-2xx response body to fold into an [`OrchError`]
/// message — enough to be useful in a log line, not enough to dump an
/// arbitrarily large or (in principle) sensitive body into `tracing` output.
const ERROR_BODY_SNIPPET_LEN: usize = 500;

/// Extracts the policy id docket names in a `pre_input` **block** response's
/// `error` text (`"task rejected by guardrail policy '<id>' at enqueue:
/// <message>"`) and builds the typed
/// [`OrchError::PolicyBlocked`]. Falls back to `policy_id: "unknown"` rather
/// than panicking or discarding the message if docket's wording ever drifts
/// — this must degrade the same way the `Unknown(String)` remote-enum
/// variants do (module doc, "Unknown enum values never fail a poll"): a
/// reworded message still surfaces as a block, just without a parsed id.
fn parse_policy_block(message: String) -> OrchError {
    let policy_id = message
        .split_once("guardrail policy '")
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(id, _)| id.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    OrchError::PolicyBlocked { policy_id, message }
}

/// [`ControlPlane`] for a single docket instance. See the module doc for
/// the constructor, the auth split, and why the write methods and
/// `list_tasks`/`traces` behave the way they do.
pub struct DocketAdapter {
    client: Client,
    /// Always ends with exactly one trailing `/` (normalized in [`Self::new`])
    /// so [`Url::join`] resolves every route as a same-origin relative path.
    base_url: String,
    token: Option<String>,
}

impl DocketAdapter {
    /// Build an adapter for the docket instance at `base_url`
    /// (e.g. `"http://127.0.0.1:7331"` — a trailing slash is fine either
    /// way). `token` is docket's `/approvals` + `/runs` + `/dispatch` Bearer
    /// token (`DOCKET_SERVE_TOKEN`, or the value `docket serve` prints /
    /// writes to `--token-file` at startup) — `None` disables every
    /// authenticated route (see the module doc).
    ///
    /// Returns `Err` only if `reqwest::Client::builder().build()` itself
    /// fails (e.g. no usable TLS backend at runtime) — building a plain
    /// HTTP(S) client with just a timeout and a User-Agent essentially never
    /// fails in practice, but the constructor propagates it rather than
    /// panicking so a misconfigured host can never crash the process that
    /// registers a control plane.
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, OrchError> {
        let mut base_url = base_url.into();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(format!("tack-orch/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| OrchError::Http(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            client,
            base_url,
            token,
        })
    }

    /// Resolve `path` (relative, no leading `/`) against `self.base_url`.
    fn url(&self, path: &str) -> Result<Url, OrchError> {
        let base = Url::parse(&self.base_url)
            .map_err(|e| OrchError::Http(format!("invalid control-plane base URL: {e}")))?;
        base.join(path)
            .map_err(|e| OrchError::Http(format!("invalid route {path:?}: {e}")))
    }

    /// GET an unauthenticated route (`/health`, `/status.json`, `/metrics`).
    /// Never attaches the Bearer token — see the module doc's "Auth split".
    async fn get_unauthed(&self, path: &str) -> Result<reqwest::Response, OrchError> {
        let url = self.url(path)?;
        self.send(self.client.get(url)).await
    }

    /// GET an authenticated route, attaching `Authorization: Bearer <token>`
    /// when one is configured. With no token configured, the request still
    /// goes out (without the header) so docket's own 401 — mapped to
    /// [`OrchError::Auth`] by [`Self::send`] — is what the caller sees,
    /// rather than a client-side short-circuit that could drift from
    /// docket's actual auth behavior.
    async fn get_authed(&self, path: &str) -> Result<reqwest::Response, OrchError> {
        let url = self.url(path)?;
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        self.send(req).await
    }

    /// Send a request and classify the response: network failure →
    /// [`OrchError::Http`]; 401/403 → [`OrchError::Auth`]; 404 →
    /// [`OrchError::NotFound`] (message extracted from a `{"error": "..."}`
    /// JSON body when present, else the raw response text — docket's
    /// generic "route doesn't exist" 404 is plain text, not JSON, see the
    /// module doc); any other non-2xx → [`OrchError::Http`] with a
    /// truncated body snippet. A 2xx response is returned as-is for the
    /// caller to decode.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, OrchError> {
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::NotFound(message));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }
        Ok(resp)
    }

    /// Read the full response body and decode it as JSON, mapping both a
    /// body-read failure and a JSON decode failure to [`OrchError::Decode`]
    /// — from the caller's perspective both mean "docket sent something
    /// this adapter can't turn into the DTO it asked for".
    async fn decode_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, OrchError> {
        let text = resp
            .text()
            .await
            .map_err(|e| OrchError::Decode(format!("failed to read response body: {e}")))?;
        serde_json::from_str(&text).map_err(|e| {
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            OrchError::Decode(format!("{e} (body: {snippet})"))
        })
    }
}

/// docket's generic JSON error body: `{"ok": false, "error": "..."}`
/// (`serve.py`'s `_send_json_error`). `ok` is intentionally not modeled —
/// only `error` is ever read.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
}

/// `GET /runs` wraps the list in `{"runs": [...]}` (`serve.py`'s `do_GET`).
#[derive(Debug, Deserialize)]
struct RunsResponse {
    runs: Vec<RemoteRun>,
}

/// `GET /approvals` wraps the list in `{"pending": [...]}`.
#[derive(Debug, Deserialize)]
struct ApprovalsResponse {
    pending: Vec<RemoteApproval>,
}

/// `GET /tasks/{project}` — real wire shape, confirmed by a live HTTP
/// capture (see the module doc's "Verified live" section). Wrapper key
/// really is `{"tasks": [...]}`, matching `/runs`/`/approvals`'s own
/// wrapping convention — `tests/fixtures/tasks_list.json` is a genuine
/// capture, not a derived projection.
/// `POST /tasks/{project}`'s success response — real wire shape, confirmed
/// by a live HTTP capture (see the module doc's
/// "Write methods" section). Only `task` is modeled: `ok`/`project` are
/// never read, and `status`/`approvalToken` — real fields, but this
/// method's frozen return type has nowhere to carry them — are recovered by
/// the caller via a follow-up [`ControlPlane::list_tasks`] instead (see the
/// module doc). An unknown/unmodeled JSON key costs nothing; `serde_json`
/// ignores it.
#[derive(Debug, Deserialize)]
struct EnqueueTaskResponse {
    task: String,
}

/// `POST /dispatch/{project}`'s success response —
/// `{"ok": true, "run": "<id>", "project": "...", "status": "dispatched"}`
/// (`serve.py`'s `do_POST`, the `/dispatch/` branch). Only `run` is
/// modeled — `ok`/`project`/`status` are never read, same "unmodeled keys
/// cost nothing" discipline as [`EnqueueTaskResponse`]. The id lands under
/// `"run"`, not `"task"`: docket's own vocabulary split between a pod
/// *task* (`EnqueueTaskResponse`) and a pipeline *run*.
#[derive(Debug, Deserialize)]
struct DispatchResponse {
    run: String,
}

/// `POST /approvals/{token}` request body — `serve.py`'s `do_POST` reads
/// exactly these two keys. `channel` is optional on the wire (docket
/// defaults to `"http"` if absent) but this adapter always sends it — see
/// [`APPROVAL_CHANNEL`].
#[derive(Debug, Serialize)]
struct DecideApprovalRequest<'a> {
    action: &'a str,
    channel: &'a str,
}

/// `POST /approvals/{token}` success response: `{"ok": true, "token": "...",
/// "state": "granted"|"denied"}` (live-verified for the grant
/// case). `ok`/`token` are never read — only `state` is modeled, same
/// "unmodeled keys cost nothing" discipline as [`EnqueueTaskResponse`].
#[derive(Debug, Deserialize)]
struct DecideApprovalResponse {
    state: String,
}

#[derive(Debug, Deserialize)]
struct TasksResponse {
    tasks: Vec<RemoteTask>,
}

/// `GET /traces/{project}` — real wire shape, verified against `serve.py`
/// (see the module doc's "list_tasks / traces" section for the full
/// wire-format trap this struct exists to route around). `events` is
/// **not** `Vec<RemoteEvent>`: each element is itself a raw JSON string
/// that must be decoded a second time — see [`DocketAdapter::traces`].
/// `next` (docket's own minted resume cursor) is read and passed through
/// verbatim as [`TracesPage::next`] — this adapter never inspects its
/// contents, only forwards it.
#[derive(Debug, Deserialize)]
struct TracesResponse {
    events: Vec<String>,
    #[serde(default)]
    next: Option<String>,
}

#[async_trait]
impl ControlPlane for DocketAdapter {
    fn kind(&self) -> &'static str {
        "docket"
    }

    /// The verified truth, not optimism — every field below is justified
    /// against this adapter's own implementation or `serve.py`'s real route
    /// table, not against what a docket-shaped provider *could* plausibly
    /// do. See `docs/book/src/developer/orchestration.md`.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // "Can this plane accept new work at all?" docket answers yes
            // two ways: `enqueue_task` (`POST /tasks/{project}`, queues a
            // task against an existing pod, and what `dispatcher.rs` calls
            // today) and the trait's own `dispatch` method
            // (`POST /dispatch/{project}`, triggers a full pipeline run).
            // Both routes are live and this adapter implements both.
            dispatch: true,
            // No cancel route exists anywhere in docket's HTTP surface —
            // `serve.py`'s full route table (this module's "Verified live"
            // section, and `docs/book/src/developer/orchestration.md`'s
            // route inventory) has nothing under `/runs/{id}/cancel` or
            // equivalent. A queued/running task can only be abandoned by
            // the pod itself, never revoked over HTTP.
            cancel: false,
            // Checked line by line against `serve.py` (see
            // `docs/book/src/developer/orchestration.md`'s "What's
            // genuinely missing" section): neither `/status.json` nor
            // `/metrics` ever emits a `paused`/`pausedReason` field, and no
            // HTTP route accepts a pause or resume request in either
            // direction. The only real remedy is the docket CLI, which is
            // why the reason names it directly rather than describing the
            // absence in the abstract.
            pause: Rated::new(
                Support::Unsupported,
                "docket exposes no pause endpoint over HTTP in either direction; from the \
                 docket CLI, run `docket profile <pod-id> --resume` to clear a \
                 budget-triggered pause",
            ),
            resume: Rated::new(
                Support::Unsupported,
                "docket exposes no resume endpoint over HTTP in either direction; from the \
                 docket CLI, run `docket profile <pod-id> --resume`",
            ),
            // `GET /traces/{project}` is scoped by project, and `RemoteEvent`
            // carries no run id to narrow further — see `persist_events`'s
            // own doc comment in `reconciler.rs` ("docket's trace payload
            // carries no run_id, only session_id... left unset rather than
            // guessing").
            event_scope: Rated::new(
                EventScope::Project,
                "docket's trace stream (GET /traces/{project}) is scoped per project; \
                 individual events carry no run id to narrow further",
            ),
            // No artifact-retrieval route exists on docket's HTTP surface.
            artifacts: false,
            // `GET /approvals` is read on the reconciler's regular poll
            // cadence — docket has no webhook or push mechanism for a
            // pending approval.
            decisions: Rated::new(
                DecisionSupport::Poll,
                "pending approvals are read via GET /approvals on the reconciler's poll \
                 cadence; docket has no push/webhook path for a new approval",
            ),
            // docket's own driver estimates cost/token figures itself (see
            // the crate doc's "Money is always an estimate" note) and
            // reports them via /status.json, /metrics, and trace events —
            // there is no separate metering gateway in front of it.
            usage: Rated::new(
                UsageSupport::FromProvider,
                "docket estimates cost/token usage itself and reports it via /status.json, \
                 /metrics, and trace events; there is no metering gateway in front of it",
            ),
            // docket owns its own model routing per role/blueprint
            // (`core/dispatch.py`) and has no documented HTTP input that
            // lets a caller override it per task.
            model_selection: Rated::new(
                ModelSelection::Unsupported,
                "docket owns its own model routing per role/blueprint and has no HTTP input \
                 to override it per task; a caller-supplied model would be silently ignored",
            ),
            // `GET /status.json`'s `agents[]` is exactly this roster — see
            // `FleetStatus`/`FleetAgent`.
            runtimes: true,
            // `GET /metrics`, Prometheus text exposition (`adapters::prometheus`).
            plane_metrics: true,
            // `POST /pods`, live-verified — see this module's
            // "Verified live" section.
            provisioning: true,
        }
    }

    async fn health(&self) -> Result<Health, OrchError> {
        let resp = self.get_unauthed("health").await?;
        Self::decode_json(resp).await
    }

    async fn status(&self) -> Result<FleetStatus, OrchError> {
        let resp = self.get_unauthed("status.json").await?;
        Self::decode_json(resp).await
    }

    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError> {
        let resp = self.get_unauthed("metrics").await?;
        // Text, not JSON — a malformed/truncated body degrades to whatever
        // the parser could salvage (never an error and never a panic; see
        // `adapters::prometheus`'s module doc), so only a transport-level
        // failure reaches this point as `Err`.
        let text = resp
            .text()
            .await
            .map_err(|e| OrchError::Decode(format!("failed to read metrics body: {e}")))?;
        Ok(prometheus::parse(&text))
    }

    async fn list_runs(&self, project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        let mut url = self.url("runs")?;
        if let Some(project) = project {
            url.query_pairs_mut().append_pair("project", project);
        }
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = self.send(req).await?;
        let wrapper: RunsResponse = Self::decode_json(resp).await?;
        Ok(wrapper.runs)
    }

    async fn get_run(&self, run_id: &str) -> Result<RemoteRun, OrchError> {
        let path = format!("runs/{run_id}");
        let resp = self.get_authed(&path).await?;
        Self::decode_json(resp).await
    }

    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        let resp = self.get_authed("approvals").await?;
        let wrapper: ApprovalsResponse = Self::decode_json(resp).await?;
        Ok(wrapper.pending)
    }

    async fn list_tasks(&self, project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        // Route confirmed live — see the module doc's
        // "list_tasks / traces" section. A 404 still surfaces as
        // `OrchError::NotFound` via `Self::send`, same as any other 404 (a
        // real docket build old enough to lack this route, not a routing
        // bug on our side).
        let path = format!("tasks/{project}");
        let resp = self.get_authed(&path).await?;
        let wrapper: TasksResponse = Self::decode_json(resp).await?;
        Ok(wrapper.tasks)
    }

    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError> {
        let mut url = self.url(&format!("traces/{project}"))?;
        if let Some(since) = since {
            url.query_pairs_mut().append_pair("since", since);
        }
        let mut req = self.client.get(url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = self.send(req).await?;
        let wrapper: TracesResponse = Self::decode_json(resp).await?;

        // Each element of `wrapper.events` is a raw JSON string (see
        // TracesResponse's doc comment) — decode it a second time. A single
        // corrupt/unparseable line is dropped with a `warn!`, not treated as
        // a whole-page failure: docket's own trace store is append-only
        // JSONL written by many independent processes, so one bad line
        // should never take down every other event in the same poll.
        let events = wrapper
            .events
            .iter()
            .filter_map(|raw| match serde_json::from_str::<RemoteEvent>(raw) {
                Ok(event) => Some(event),
                Err(e) => {
                    warn!(
                        error = %e,
                        "dropping an unparseable trace event line from docket"
                    );
                    None
                }
            })
            .collect();
        // `wrapper.next` is forwarded exactly as received — opaque, never
        // parsed or recomputed here (see the module doc and TracesPage's
        // own doc comment).
        Ok(TracesPage {
            events,
            next: wrapper.next,
        })
    }

    async fn enqueue_task(&self, project: &str, task: NewRemoteTask) -> Result<String, OrchError> {
        // POST /tasks/{project}, Bearer-authed. Built by hand
        // rather than through `get_authed`/`send` — those only ever GET, and
        // the `pre_input` policy **block** (HTTP 400) needs distinct
        // handling from `send`'s generic non-2xx branch (see the module doc
        // and `parse_policy_block`).
        let url = self.url(&format!("tasks/{project}"))?;
        let mut req = self.client.post(url).json(&task);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::BAD_REQUEST {
            // A `pre_input` policy block — never a transport failure. Extract
            // docket's own `error` text (which names the policy id) the same
            // way `send`'s 404 branch does, and parse the id out of it into a
            // typed `OrchError::PolicyBlocked`.
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(parse_policy_block(message));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }

        let parsed: EnqueueTaskResponse = Self::decode_json(resp).await?;
        Ok(parsed.task)
    }

    async fn dispatch(&self, project: &str, vars: serde_json::Value) -> Result<String, OrchError> {
        // POST /dispatch/{project}, Bearer-authed. Built by hand rather than
        // through `get_authed`/`send`, the same reason `enqueue_task` does:
        // a 400 here needs distinct handling before `send`'s generic
        // non-2xx branch would collapse it.
        //
        // `vars` is sent as the request body verbatim — docket reads it as
        // a plain `{name: value}` object and resolves it against the
        // project's pipeline variable namespace; this adapter never
        // inspects or reshapes it.
        let url = self.url(&format!("dispatch/{project}"))?;
        let mut req = self.client.post(url).json(&vars);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::NotFound(message));
        }
        if status == StatusCode::BAD_REQUEST {
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            // Reuses `enqueue_task`'s policy-block parser for the one shape
            // that would mean the same thing here — but every 400
            // `serve.py`'s `/dispatch/` branch can actually raise today
            // (malformed JSON, a non-object body, an unresolved pipeline
            // variable) predates any guardrail check: the `pre_input` gate
            // runs later, inside the async dispatch this response doesn't
            // wait on (see the module doc). A message that doesn't name a
            // guardrail policy is a plain request error, not a block.
            if message.contains("guardrail policy") {
                return Err(parse_policy_block(message));
            }
            return Err(OrchError::Http(format!("unexpected status 400: {message}")));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }

        let parsed: DispatchResponse = Self::decode_json(resp).await?;
        Ok(parsed.run)
    }

    async fn decide_approval(&self, token: &str, grant: bool) -> Result<ApprovalState, OrchError> {
        // POST /approvals/{token}, Bearer-authed. Built by hand rather than
        // through `get_authed`/`send` — those only ever GET, and this route
        // needs 409 classified distinctly from `send`'s generic non-2xx
        // branch, the same reason `enqueue_task` builds its own request
        // (see the module doc's "Verified live" section — grant, deny, the
        // 409 `ApprovalNoop` replay and the unknown-token 404 are all
        // captured against a real server there).
        let url = self.url(&format!("approvals/{token}"))?;
        let body = DecideApprovalRequest {
            action: if grant { "grant" } else { "deny" },
            channel: APPROVAL_CHANNEL,
        };
        let mut req = self.client.post(url).json(&body);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::CONFLICT {
            // `approval.ApprovalNoop` — already granted/denied/expired. Not a
            // transport failure; see `OrchError::AlreadyDecided`'s doc comment.
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::AlreadyDecided(message));
        }
        if status == StatusCode::NOT_FOUND {
            // Covers both a genuinely unknown token and docket's
            // `approval.ApprovalError` for an illegal state transition on a
            // known token (e.g. denying an already-granted one) — `serve.py`
            // maps both to 404 with different `error` text; this adapter
            // surfaces docket's own message rather than collapsing the
            // distinction further (see the module doc).
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::NotFound(message));
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let snippet: String = text.chars().take(ERROR_BODY_SNIPPET_LEN).collect();
            return Err(OrchError::Http(format!(
                "unexpected status {status}: {snippet}"
            )));
        }

        let parsed: DecideApprovalResponse = Self::decode_json(resp).await?;
        Ok(ApprovalState::from(parsed.state))
    }

    async fn provision_pod(&self, params: ProvisionPodParams) -> Result<ProvisionedPod, OrchError> {
        // POST /pods, Bearer-authed. Built by hand
        // rather than through `get_authed`/`send` — those only ever GET, and
        // this route needs 409 (`PodAlreadyExistsError`) classified
        // distinctly from `send`'s generic non-2xx branch, same reason
        // `enqueue_task`/`decide_approval` each build their own request.
        let url = self.url("pods")?;
        let mut req = self.client.post(url).json(&params);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| OrchError::Http(format!("request failed: {e}")))?;
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(OrchError::Auth);
        }
        if status == StatusCode::CONFLICT {
            // PodAlreadyExistsError — raised before anything is touched (see
            // ControlPlane::provision_pod's doc comment). Not a transport
            // failure.
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::AlreadyExists(message));
        }
        if !status.is_success() {
            // Covers docket's 400 (bad blueprint / bad verify_cmd / bad
            // `pod` field / missing `project` — `BlueprintError`/
            // `VerifyCmdError`, request-shaped) and its 500 (`PodProvisionError`
            // — an operational failure *after* docket's own rollback already
            // ran, per the module docstring on `core/pod_provisioning.py`).
            // Either way nothing was created; this adapter doesn't need to
            // distinguish "your input was bad" from "docket had trouble
            // provisioning" any further than the message itself does — the
            // caller (`tack-api::handlers::provisioning`) treats every
            // non-409, non-Auth error identically: nothing to roll back on
            // docket's side, only on Tack's own.
            let text = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| (!b.error.is_empty()).then_some(b.error))
                .unwrap_or_else(|| text.trim().to_string());
            return Err(OrchError::Http(format!(
                "pod provisioning failed ({status}): {message}"
            )));
        }

        Self::decode_json(resp).await
    }
}

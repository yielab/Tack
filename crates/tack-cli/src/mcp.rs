//! Minimal Model Context Protocol (MCP) server over stdio.
//!
//! `tack mcp` speaks JSON-RPC 2.0 over stdin/stdout (newline-delimited, per the
//! MCP stdio transport) and proxies tool calls to a running Tack server through
//! the same blocking HTTP client every other CLI command uses. Writes therefore
//! go through the REST API — workflow validation, WIP limits, and
//! parent-auto-completion all still apply, and the live board updates over the
//! existing WebSocket.
//!
//! See `docs/MCP.md` for the transport decision and the Claude Code config.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::client::TackClient;
use crate::execution;

/// Protocol version we advertise when the client doesn't specify one.
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the stdio MCP loop until stdin closes.
///
/// `stdout` carries the protocol — nothing else may be written there.
pub fn run(client: &TackClient) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(client, &line) {
            writeln!(out, "{response}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Handle a single JSON-RPC message. Returns `Some(serialized response)`, or
/// `None` for notifications (which take no response).
fn handle_line(client: &TackClient, line: &str) -> Option<String> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            ));
        }
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(success(id, initialize_result(&params))),
        // Notifications carry no id and get no reply.
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, json!({ "tools": tool_specs() }))),
        "tools/call" => Some(handle_tool_call(client, id, &params)),
        other => {
            // Unknown notification → silence; unknown request → method-not-found.
            id.map(|id| error_response(id, -32601, &format!("method not found: {other}")))
        }
    }
}

fn initialize_result(params: &Value) -> Value {
    // Echo the client's requested protocol version when present for compatibility.
    let protocol = params
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "tack",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

// ── Tool dispatch ───────────────────────────────────────────────────────────

fn handle_tool_call(client: &TackClient, id: Option<Value>, params: &Value) -> String {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match dispatch_tool(client, name, &args) {
        Ok(value) => success(id, tool_text(&value, false)),
        Err(msg) => success(id, tool_text(&Value::String(msg), true)),
    }
}

/// Run a tool by name. Required-argument validation happens before any network
/// call so bad requests fail fast (and are unit-testable without a server).
fn dispatch_tool(client: &TackClient, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => {
            let projects = call(client.get("/projects"))?;
            Ok(project_list(&projects))
        }
        "list_items" => {
            let project_id = require_str(args, "project_id")?;
            let mut qs = Vec::new();
            for key in ["status", "item_type", "assignee"] {
                if let Some(v) = opt_str(args, key) {
                    qs.push(format!("{key}={}", urlencode(&v)));
                }
            }
            let suffix = if qs.is_empty() {
                String::new()
            } else {
                format!("?{}", qs.join("&"))
            };
            let items = call(client.get(&format!("/projects/{project_id}/items{suffix}")))?;
            Ok(item_list(&items))
        }
        "get_item" => {
            let id = require_str(args, "id")?;
            call(client.get(&format!("/items/{id}")))
        }
        "search_items" => {
            let query = require_str(args, "query")?;
            let path = match opt_str(args, "project_id") {
                Some(pid) => format!("/projects/{pid}/search?q={}", urlencode(&query)),
                None => format!("/search?q={}", urlencode(&query)),
            };
            let items = call(client.get(&path))?;
            Ok(item_list(&items))
        }
        "create_item" => {
            let project_id = require_str(args, "project_id")?;
            let title = require_str(args, "title")?;
            let mut body = json!({
                "title": title,
                "item_type": opt_str(args, "item_type").unwrap_or_else(|| "task".into()),
                "priority": opt_str(args, "priority").unwrap_or_else(|| "medium".into()),
            });
            for (arg, field) in [("parent_id", "parent_id"), ("assignee", "assignee")] {
                if let Some(v) = opt_str(args, arg) {
                    body[field] = json!(v);
                }
            }
            call(client.post(&format!("/projects/{project_id}/items"), &body))
        }
        "update_item" => {
            let id = require_str(args, "id")?;
            let mut body = json!({});
            let mut touched = false;
            for field in [
                "title",
                "description",
                "priority",
                "assignee",
                "status",
                "due_date",
            ] {
                if let Some(v) = args.get(field).filter(|v| !v.is_null()) {
                    body[field] = v.clone();
                    touched = true;
                }
            }
            if !touched {
                return Err("update_item: provide at least one field to change".into());
            }
            write_item(client, &id, &body)
        }
        "move_item" => {
            let id = require_str(args, "id")?;
            let status = require_str(args, "status")?;
            write_item(client, &id, &json!({ "status": status }))
        }
        "add_comment" => {
            let item_id = require_str(args, "item_id")?;
            let content = require_str(args, "content")?;
            let body = json!({ "content": content, "author": opt_str(args, "author") });
            call(client.post(&format!("/items/{item_id}/comments"), &body))
        }

        // ── Execution/fleet/profile tools ─────────────────────────
        //
        // Deliberately a *subset* of what the CLI exposes: read tools cover
        // discovery (an agent needs valid fleet/profile ids before it can
        // create an execution) plus the core create/list/get/cancel
        // lifecycle. `runner enroll`/`revoke`, `fleet create`,
        // `agent-profile create`/`model-profile create`, and
        // `execution reconcile` are CLI-only, matching this server's
        // existing precedent of keeping admin-ish actions (backup/restore,
        // template/role/field management) off the agent-facing tool
        // surface (see docs/MCP.md). `reconcile` specifically is an
        // operator's explicit, audited recovery decision after reviewing an
        // ambiguous `needs_operator` state — not something an
        // agent should be able to trigger on its own say-so. `runner
        // enroll` additionally returns a one-time secret; keeping it out of
        // the MCP surface means that secret can never end up in an agent's
        // tool-call transcript.
        "list_fleets" => {
            let fleets = call(client.get("/runner-fleets"))?;
            Ok(data_list("fleets", &fleets))
        }
        "list_agent_profiles" => {
            let profiles = call(client.get("/agent-profiles"))?;
            Ok(data_list("agent_profiles", &profiles))
        }
        "list_model_profiles" => {
            let profiles = call(client.get("/model-profiles"))?;
            Ok(data_list("model_profiles", &profiles))
        }
        "list_executions" => {
            let executions = call(client.get("/executions"))?;
            Ok(data_list("executions", &executions))
        }
        "get_execution" => {
            let id = require_str(args, "request_id")?;
            call(client.get(&format!("/executions/{id}")))
        }
        "cancel_execution" => {
            let id = require_str(args, "request_id")?;
            call(client.post(&format!("/executions/{id}/cancel"), &json!({})))
        }
        "create_execution" => {
            let item_id = require_str(args, "item_id")?;
            let selector = execution::selector_from_flags(
                opt_str(args, "runner_id").as_deref(),
                opt_str(args, "fleet_id").as_deref(),
            )?;
            let agent_profile_id = require_str(args, "agent_profile_id")?;
            let harness = require_str(args, "harness")?;
            let agent_profile_snapshot = require_object(args, "agent_profile_snapshot")?;
            let repository_snapshot = require_object(args, "repository")?;
            let permission_policy = require_object(args, "permission_policy")?;
            let timeout_seconds = require_u64(args, "timeout_seconds")?;
            let idempotency_key = opt_str(args, "idempotency_key");
            let model_provider = opt_str(args, "model_provider");
            let model_id = opt_str(args, "model_id");
            let status_map_policy_id = opt_str(args, "status_map_policy_id");

            let values = execution::CreateExecutionValues {
                item_id: &item_id,
                idempotency_key: idempotency_key.as_deref(),
                agent_profile_id: &agent_profile_id,
                requested_harness_kind: &harness,
                requested_model_provider: model_provider.as_deref(),
                requested_model_id: model_id.as_deref(),
                agent_profile_snapshot,
                repository_snapshot,
                permission_policy,
                budgets: opt_object_or_empty(args, "budgets"),
                environment: opt_object_or_empty(args, "environment"),
                metadata: opt_object_or_empty(args, "metadata"),
                timeout_seconds,
                status_map_policy_id: status_map_policy_id.as_deref(),
            };
            let body = execution::create_execution_body(&selector, values);
            call(client.post("/executions", &body))
        }

        other => Err(format!("unknown tool: {other}")),
    }
}

/// Map an HTTP-client result into a tool result, flattening the error to text.
/// Generic (not `Value`-only) so `get_with_etag`'s `(Value, Option<String>)`
/// pair goes through the same `?`-friendly path as every other client call.
fn call<T>(result: anyhow::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

/// Read-modify-write an item with an `If-Match` precondition, so the two MCP
/// tools that mutate an existing item (`update_item`, `move_item`) can never
/// clobber a change made by a human in the UI or another agent between the
/// read and the write — the exact agent-versus-human race that
/// `version`/`ETag`/`If-Match` exists to catch, on exactly the write path
/// that would otherwise have no way to send the header at all. When the server sends
/// no `ETag` (no concurrency support on this route yet, or an older server),
/// `if_match` is `None` and `patch_if_match` sends no header — identical to
/// today's unconditional write, never a new failure mode.
///
/// A `412` surfaces through `patch_if_match`'s own error text, which already
/// names the precondition failure and tells the caller to re-read — this
/// function does not need to special-case it further; the point is only
/// that the agent sees that message instead of a generic one, and it does
/// because nothing here swallows or rewrites the error.
fn write_item(client: &TackClient, id: &str, body: &Value) -> Result<Value, String> {
    let (_, etag) = call(client.get_with_etag(&format!("/items/{id}")))?;
    call(client.patch_if_match(&format!("/items/{id}"), body, etag.as_deref()))
}

// ── Argument helpers ─────────────────────────────────────────────────────────

fn require_str(args: &Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("missing required argument: {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// A required JSON-object argument (`agent_profile_snapshot`, `repository`,
/// `permission_policy` on `create_execution`) — these map onto typed nested
/// snapshot structs server-side with their own required fields (see
/// `execution.rs`'s module doc), so unlike `opt_object_or_empty` there is no
/// safe default to fall back to.
fn require_object(args: &Value, key: &str) -> Result<Value, String> {
    match args.get(key) {
        Some(v) if v.is_object() => Ok(v.clone()),
        Some(_) => Err(format!("{key} must be a JSON object")),
        None => Err(format!("missing required argument: {key}")),
    }
}

/// An optional JSON-object argument that defaults to `{}` when omitted
/// (`budgets`/`environment`/`metadata` on `create_execution` — all
/// genuinely untyped `Value` fields server-side).
fn opt_object_or_empty(args: &Value, key: &str) -> Value {
    args.get(key)
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}))
}

fn require_u64(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key).and_then(|v| v.as_u64()).ok_or_else(|| {
        format!("missing or invalid required argument: {key} (expected a non-negative integer)")
    })
}

// ── Compact projections (keep agent context small) ───────────────────────────

fn project_list(value: &Value) -> Value {
    let arr = value.as_array().cloned().unwrap_or_default();
    let projects: Vec<Value> = arr
        .iter()
        .map(|p| {
            json!({
                "id": p.get("id"),
                "name": p.get("name"),
                "project_type": p.get("project_type"),
            })
        })
        .collect();
    json!({ "projects": projects, "count": projects.len() })
}

/// Unwraps the `{"protocol_version":1,"data":[...]}` envelope every
/// execution/fleet/runner/profile list route returns (`executions.rs`,
/// `runner_admin.rs`) into `{"<key>": [...], "count": n}`, matching
/// `project_list`/`item_list`'s wrapper convention. No further projection —
/// unlike items, these rows are already small and have no verbose fields to
/// drop.
fn data_list(key: &str, value: &Value) -> Value {
    let arr = value
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    let count = arr.len();
    json!({ key: arr, "count": count })
}

fn item_list(value: &Value) -> Value {
    // The list endpoint returns a `{ data, total, page, per_page }` envelope;
    // fall back to a bare array for other item-producing shapes.
    let arr = value
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default();
    let items: Vec<Value> = arr.iter().map(compact_item).collect();
    json!({ "items": items, "count": items.len() })
}

fn compact_item(item: &Value) -> Value {
    json!({
        "id": item.get("id"),
        "title": item.get("title"),
        "item_type": item.get("item_type"),
        "status": item.get("status"),
        "priority": item.get("priority"),
        "assignee": item.get("assignee"),
        "project_id": item.get("project_id"),
    })
}

// ── JSON-RPC envelope helpers ────────────────────────────────────────────────

fn success(id: Option<Value>, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
    .to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

/// Wrap a value as an MCP tool result (a single text content block).
fn tool_text(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": is_error,
    })
}

/// Percent-encode a query-string value (RFC 3986 unreserved set passes through).
fn urlencode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}

// ── Tool schemas (advertised via tools/list) ─────────────────────────────────

fn tool_specs() -> Value {
    json!([
        tool(
            "list_projects",
            "List all projects.",
            json!({
                "type": "object", "properties": {}
            })
        ),
        tool(
            "list_items",
            "List items in a project, optionally filtered.",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Project UUID" },
                    "status": { "type": "string", "description": "Filter by status column name" },
                    "item_type": { "type": "string", "description": "Filter by item type" },
                    "assignee": { "type": "string", "description": "Filter by assignee" }
                },
                "required": ["project_id"]
            })
        ),
        tool(
            "get_item",
            "Get one item with full detail, roles, and dependencies.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Item UUID" } },
                "required": ["id"]
            })
        ),
        tool(
            "search_items",
            "Full-text search items, globally or within a project.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "project_id": { "type": "string", "description": "Optional; omit to search all projects" }
                },
                "required": ["query"]
            })
        ),
        tool(
            "create_item",
            "Create an item in a project.",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "title": { "type": "string" },
                    "item_type": { "type": "string", "description": "task (default), epic, feature, bug, subtask, requirement" },
                    "priority": { "type": "string", "description": "critical, high, medium (default), low" },
                    "parent_id": { "type": "string" },
                    "assignee": { "type": "string" }
                },
                "required": ["project_id", "title"]
            })
        ),
        tool(
            "update_item",
            "Update fields on an item. Status changes are validated against the workflow.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": { "type": "string" },
                    "assignee": { "type": "string" },
                    "status": { "type": "string" },
                    "due_date": { "type": "string", "description": "RFC3339 timestamp" }
                },
                "required": ["id"]
            })
        ),
        tool(
            "move_item",
            "Move an item to a new status (validated transition + WIP limits).",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string", "description": "Target status column name (case-sensitive)" }
                },
                "required": ["id", "status"]
            })
        ),
        tool(
            "add_comment",
            "Add a comment to an item.",
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "content": { "type": "string" },
                    "author": { "type": "string" }
                },
                "required": ["item_id", "content"]
            })
        ),
        tool(
            "list_fleets",
            "List runner fleets.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "list_agent_profiles",
            "List agent profiles (instructions, tool policy, limits).",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "list_model_profiles",
            "List model profiles (provider + model id combinations).",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "list_executions",
            "List execution requests (newest first).",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "get_execution",
            "Get one execution request's current lifecycle state. A state of \
             needs_operator or lost is an ambiguous outcome, not just another \
             in-progress value — surface it distinctly to the user rather than \
             treating it like queued/running.",
            json!({
                "type": "object",
                "properties": { "request_id": { "type": "string" } },
                "required": ["request_id"]
            })
        ),
        tool(
            "cancel_execution",
            "Request cancellation of an execution. Recorded as a request only \
             — the execution is not made falsely terminal; call get_execution \
             afterward to see the actual outcome once the runner reports it.",
            json!({
                "type": "object",
                "properties": { "request_id": { "type": "string" } },
                "required": ["request_id"]
            })
        ),
        tool(
            "create_execution",
            "Create (or idempotently replay, if idempotency_key repeats) an \
             execution request that assigns a Tack item to a coding-harness \
             runner or fleet. Exactly one of runner_id/fleet_id is required. \
             Use list_fleets/list_agent_profiles/list_model_profiles first to \
             discover valid ids.",
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "runner_id": { "type": "string", "description": "Exact runner id (mutually exclusive with fleet_id)" },
                    "fleet_id": { "type": "string", "description": "Fleet id (mutually exclusive with runner_id)" },
                    "agent_profile_id": { "type": "string" },
                    "harness": { "type": "string", "description": "Requested harness kind, e.g. codex, claude_code" },
                    "agent_profile_snapshot": {
                        "type": "object",
                        "description": "Resolved agent profile: {name, instructions, tool_policy, timeout_seconds, budgets} — all required, no safe empty default"
                    },
                    "repository": {
                        "type": "object",
                        "description": "{kind, remote, base_revision, subdirectory?} — kind/remote/base_revision required"
                    },
                    "permission_policy": {
                        "type": "object",
                        "description": "{network: bool, tools?: [string]} — network required, no safe empty default"
                    },
                    "timeout_seconds": { "type": "integer" },
                    "model_provider": { "type": "string", "description": "Omit to allow auto-selection" },
                    "model_id": { "type": "string", "description": "Omit to allow auto-selection" },
                    "budgets": { "type": "object", "description": "Default: {}" },
                    "environment": { "type": "object", "description": "Default: {}" },
                    "metadata": { "type": "object", "description": "Default: {}" },
                    "status_map_policy_id": { "type": "string" },
                    "idempotency_key": { "type": "string", "description": "Defaults to a fresh key each call; pass a stable value to safely retry the exact same request" }
                },
                "required": [
                    "item_id",
                    "agent_profile_id",
                    "harness",
                    "agent_profile_snapshot",
                    "repository",
                    "permission_policy",
                    "timeout_seconds"
                ]
            })
        ),
    ])
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

#[cfg(test)]
#[path = "mcp/tests.rs"]
mod tests;

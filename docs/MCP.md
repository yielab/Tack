# Tack MCP Server

`tack mcp` exposes a Tack instance to AI agents (Claude Code, Codex, etc.) via the
[Model Context Protocol](https://modelcontextprotocol.io). An agent can list
projects, search and read items, and create/update/move items and add comments —
all through the same HTTP API the CLI uses, so **workflow validation, WIP limits,
and parent-auto-completion still apply** and the live board updates over WebSocket.

## Transport decision

Two options were considered:

| Option | How | Verdict |
| --- | --- | --- |
| **(a) stdio sidecar** — `tack mcp` | A subcommand spawned per-agent; speaks JSON-RPC 2.0 over stdin/stdout; reaches `tack serve` over HTTP using the existing CLI client. | **Chosen for v1.** |
| (b) HTTP/SSE endpoint in `tack serve` | Mount an MCP transport inside the server. | Deferred — adds an auth surface and SSE plumbing to the server for no v1 benefit. |

**Decision:** ship the **stdio sidecar**. It is the simplest thing to wire into
Claude Code's MCP config, adds no new server-side attack surface, and reuses the
blocking `reqwest` client already in `tack-cli`. The protocol layer is a thin,
hand-rolled JSON-RPC 2.0 loop (newline-delimited messages per the MCP stdio
transport) — no heavyweight async MCP SDK is pulled into the otherwise-blocking
CLI, keeping the single binary small.

If a remote/multi-agent HTTP transport is needed later, option (b) can be added
without changing the tool definitions.

## Usage

`tack mcp` reads JSON-RPC from stdin and writes responses to stdout. It needs a
running Tack server; point it at one with the same flags/env as any CLI command:

```sh
# Defaults to http://127.0.0.1:3210, no token
tack mcp

# Explicit server + token
TACK_API_URL=http://127.0.0.1:3210 TACK_API_TOKEN=secret tack mcp
```

> **stdout is the protocol channel** — the MCP server prints only JSON-RPC. Do not
> pipe anything else into its stdin or expect human-readable output.

## Wiring into Claude Code

Add Tack to your project's `.mcp.json` (or the global Claude Code MCP config):

```json
{
  "mcpServers": {
    "tack": {
      "command": "tack",
      "args": ["mcp"],
      "env": {
        "TACK_API_URL": "http://127.0.0.1:3210",
        "TACK_API_TOKEN": "your-token-if-set"
      }
    }
  }
}
```

Then start the Tack server (`tack serve`) and the agent can call the tools below.

## Tools

| Tool | Kind | Arguments | Maps to |
| --- | --- | --- | --- |
| `list_projects` | read | — | `GET /api/projects` |
| `list_items` | read | `project_id`*, `status`, `item_type`, `assignee` | `GET /api/projects/{id}/items` |
| `get_item` | read | `id`* | `GET /api/items/{id}` |
| `search_items` | read | `query`*, `project_id` | `GET /api/search` or `/projects/{id}/search` |
| `create_item` | write | `project_id`*, `title`*, `item_type`, `priority`, `parent_id`, `assignee` | `POST /api/projects/{id}/items` |
| `update_item` | write | `id`*, `title`, `description`, `priority`, `assignee`, `status`, `due_date` | `PATCH /api/items/{id}` |
| `move_item` | write | `id`*, `status`* | `PATCH /api/items/{id}` |
| `add_comment` | write | `item_id`*, `content`*, `author` | `POST /api/items/{id}/comments` |

`*` = required. Read tools return a compact projection (id, title, type, status,
priority, assignee) to keep agent context small; `get_item` returns full detail.

### Execution/fleet/profile tools (Part III, card E5)

Runs a Tack item through the agent-fleet execution surface — the same
`/api/executions`, `/api/runner-fleets`, `/api/agent-profiles`, and
`/api/model-profiles` operator routes the `tack execution|fleet|agent-profile|
model-profile` CLI commands use, via the exact same request-body builders (see
`crates/tack-cli/src/execution.rs`), so an agent-issued `create_execution` call
can never diverge in shape from what the CLI (or, once it ships, the web UI)
would send for the same operation.

| Tool | Kind | Arguments | Maps to |
| --- | --- | --- | --- |
| `list_fleets` | read | — | `GET /api/runner-fleets` |
| `list_agent_profiles` | read | — | `GET /api/agent-profiles` |
| `list_model_profiles` | read | — | `GET /api/model-profiles` |
| `list_executions` | read | — | `GET /api/executions` |
| `get_execution` | read | `request_id`* | `GET /api/executions/{id}` |
| `cancel_execution` | write | `request_id`* | `POST /api/executions/{id}/cancel` |
| `create_execution` | write | `item_id`*, one of `runner_id`/`fleet_id`*, `agent_profile_id`*, `harness`*, `agent_profile_snapshot`* (object), `repository`* (object), `permission_policy`* (object), `timeout_seconds`*, `model_provider`, `model_id`, `budgets`, `environment`, `metadata`, `status_map_policy_id`, `idempotency_key` | `POST /api/executions` |

Use the three `list_*` tools first to discover valid `fleet_id`/`agent_profile_id`/
model values before calling `create_execution`. `get_execution`'s `state` can be
`needs_operator` or `lost` — an ambiguous outcome, not just another in-progress
value; the tool description says so explicitly so an agent surfaces it to the
human rather than treating it like `queued`/`running`.

**Deliberately CLI-only, not exposed as MCP tools:** `tack runner enroll`/`revoke`
(enrollment returns a one-time secret — keeping it off the MCP surface means that
secret can never land in an agent's tool-call transcript; see
`crates/tack-cli/src/secure_fs.rs`), `tack fleet create`, `tack agent-profile
create`, `tack model-profile create` (admin-ish setup, in the same spirit as
`backup`/`restore`/`template`/`role`/`field` never having been exposed here), and
`tack execution reconcile` (an operator's explicit, audited recovery decision
after reviewing an ambiguous `needs_operator` state — not something an agent
should be able to trigger on its own say-so).

## Security

The MCP server inherits the CLI's `TACK_API_TOKEN`. It has the **same access as
any API client** — there is no per-tool scoping in v1. Run it against a server you
control, and treat the token as a secret in the MCP config. Writes are validated
server-side, so an agent cannot bypass workflow rules, but it **can** create and
modify items within the projects the token can reach. The same holds for the
execution/fleet/profile tools above: an agent with this token can create
executions and cancel them but cannot enroll or revoke runners, create fleets or
profiles, or reconcile a `needs_operator` request — those remain CLI-only.

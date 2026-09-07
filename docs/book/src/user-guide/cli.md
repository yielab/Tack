# CLI Reference

`tack` is a single binary that is both the **server** and the **CLI client**.
Run `tack` with no arguments (or `tack serve`) to start the server + web UI;
run `tack <command>` to use the CLI.

The CLI is an alternative to the web UI — reach for it when you want to script
Tack, wire it into automation or CI, or work without leaving the terminal. It is
also how you create a git branch straight from an item (`tack branch`) and run the
[MCP server](../../../MCP.md) for AI agents.

The CLI commands below talk to a running server over HTTP, so start the server
first (`tack serve`) — all client commands require it to be reachable.

## Configuration

```sh
# Set URL and token (saved to ~/.config/tack/config.toml)
tack config --url http://127.0.0.1:3210 --token your-token

# Print current config
tack config --show

# Or use environment variables
export TACK_API_URL=http://127.0.0.1:3210
export TACK_API_TOKEN=your-token
```

**Shell completions:**

```sh
tack completions bash  >> ~/.bashrc
tack completions zsh   >> ~/.zshrc
tack completions fish  > ~/.config/fish/completions/tack.fish
```

---

## Projects

```sh
# Create a project
tack init "Kitchen Reno" --type construction

# Types: software · web · mobile · construction · personal · homework · maintenance · legal · research · event · custom
```

---

## Items

```sh
# List items in a project
tack list --project <id>

# Add an item
tack add "Design login page" \
  --project <id> \
  --type task \
  --priority high

# Priorities: high · medium · low
# Types: task · epic · story · bug · feature · subtask · milestone (or vocabulary-mapped)

# Move an item to a different status column
tack move <item-id> "In Progress"
# The status name must exactly match the column name (case-sensitive)

# Derive a git branch name from an item
tack branch <item-id>
# → prints: git checkout -b feat/<short-id>-<title-slug>

# Create and switch to the branch in one step
tack branch <item-id> --checkout

# Override the type-derived prefix (default maps feature→feat, bug→fix, …)
tack branch <item-id> --prefix hotfix
```

`tack branch` reads the item over the API and builds a conventional branch name
of the form `<prefix>/<short-id>-<title-slug>`. Without `--checkout` it prints
the `git checkout -b …` command (handy to `eval` or copy-paste); with
`--checkout` it runs it. Add `--json` for `{ branch, item_id, checked_out }`.

---

## Sprints

```sh
tack sprint create --project <id> --name "Sprint 1"
tack sprint start  <sprint-id>    # Planning → Active
tack sprint close  <sprint-id>    # Active → Closed
```

Only one sprint can be Active per project at a time.

---

## Templates

```sh
# List available templates (built-in + user-created)
tack template list
tack template list --type construction   # filter by project type

# Show a template's full details
tack template show <template-id>

# Create a new project from a template
tack template create-from <template-id> "My New Project"
tack template create-from <template-id> "My New Project" --description "Optional description"
```

---

## Roles

Roles represent specialties or disciplines (Designer, Engineer, Reviewer, …) that can be
assigned to items for tracking who is responsible.

```sh
# List roles in a project
tack role list --project <project-id>

# Create a role
tack role create "Designer" --project <project-id>
tack role create "Engineer" --project <project-id> --color "#4A90D9"

# Assign / unassign a role on an item
tack role assign  <item-id> <role-id>
tack role unassign <item-id> <role-id>

# Delete a role (removes all its item assignments)
tack role delete <role-id>
```

---

## Comments

```sh
# List comments on an item
tack comment list <item-id>

# Add a comment
tack comment add <item-id> "Looks good to merge"
tack comment add <item-id> "Blocked on client sign-off" --author "Alice"
```

---

## Custom Fields

Custom fields extend items with project-specific metadata.

```sh
# List field definitions for a project
tack field list --project <project-id>

# Create a field definition
tack field create "Client Name"   --project <project-id> --type text
tack field create "Story Points"  --project <project-id> --type number --required
tack field create "Phase"         --project <project-id> --type select \
    --options "Design,Development,QA,Done"

# Types: text · long_text · number · date · boolean · select · multi_select · url · email

# List all custom field values set on an item
tack field values <item-id>

# Set a value (parsed as JSON if valid, otherwise treated as a string)
tack field set <item-id> <field-id> "Acme Corp"
tack field set <item-id> <field-id> 8          # number
tack field set <item-id> <field-id> true        # boolean
tack field set <item-id> <field-id> '"Design"'  # string that looks like JSON — quote it

# Remove a value
tack field unset <item-id> <field-id>

# Delete a field definition (also removes all item values for that field)
tack field delete <field-id>
```

---

## Backup and Restore

```sh
# Download backup to current directory (timestamped filename)
tack backup

# Download to a specific path
tack backup --path /safe/place/tack.db

# Stage a restore (applied on next server startup)
tack restore /safe/place/tack.db
```

---

## Execution

Create, inspect, cancel and recover agent-fleet execution requests — the CLI form of
`POST /api/executions` and friends. See [Running an item with an
agent](agent-runners.md#running-an-item-with-an-agent) for a full worked example
reaching a completed attempt, and [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider) for what belongs in
`--model-provider`/`--model-id` (they may be omitted; see that section for what fills
them in when you do).

```text
$ tack execution --help
Create/list/cancel/reconcile agent-fleet execution requests

Commands:
  create     Create (or idempotently replay) an execution request
  list       List execution requests (newest first)
  get        Get one execution request's current lifecycle state
  cancel     Request cancellation of an execution (recorded as a request only — the runner observes and reports the actual outcome)
  reconcile  Requeue a needs_operator execution after an audited recovery decision
```

```sh
tack execution list
```

```text
ID        ITEM                              STATE         CREATED         
────────  ────────────  ────────────  ────────────
exec_9b3  691e721e                          queued        2026-09-03      
exec_1ec  1bb00bc7                          succeeded (…  2026-09-03      
exec_0fe  e1d5e03d                          succeeded (…  2026-09-03      
exec_14a  e4e3e686                          succeeded (…  2026-09-03      
```

```sh
tack execution get exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```text
Execution request exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
  item:    e1d5e03d-4610-45c2-b5f1-835e69f148a7
  state:   succeeded (done)
  created: 2026-09-03T19:00:16.697646760+00:00
```

`tack execution cancel <ID>` only records the request — the runner observes it and
reports the actual outcome (cancellation is `advisory`, never `supported`; see the
[capability matrix](agent-runners.md#capability-matrix)). `tack execution reconcile <ID>
--recovery-key <KEY> --reason "..."` requeues a `needs_operator` request after an
audited decision; see the [Recovery Runbook](recovery-runbook.md).

---

## Runner

Enroll, revoke, and — for the runner side of the fence — actually run the runner role
or check what this machine can do. `tack runner enroll` / `revoke` / `revoke-token` are
the operator surface, covered in full in [Enrolling a
runner](agent-runners.md#enrolling-a-runner); this section covers the other
subcommands, which act on the local machine rather than the operator's server.

`tack runner doctor` needs no server and enrolls nothing — it runs the same
discovery/capability probe as `runner start`, read-only, so you can check what a
machine can do before enrolling it:

```sh
tack runner doctor
```

```text
Tack runner doctor — harness discovery for this machine

codex
  status:      present
  version:     0.149.1
  credentials: Codex authenticates itself (its own CLI login flow or an API key it reads from its own environment/config — see `codex --help`). Tack never reads, stores, or forwards it. This adapter forwards no ambient host environment into an actual run: only entries explicitly set on the execution request's own `environment` field ever reach the codex process.
  model_combinations: (none reported)
  model_passthrough: supported — the adapter forwards requested_model_id verbatim via --model and rejects specs without an explicit model pre-spawn; model validity is established by the Codex CLI at run time, so operator-specified opaque models are accepted without the probe claiming any model list

claude-code
  status:      present
  version:     2.1.252
  credentials: Claude Code authenticates itself: typically an OAuth session under $HOME/.claude established by its own login flow, or an API key it reads from its own environment. Tack never reads, stores, or forwards it. This adapter forwards only HOME and PATH from the runner process's own environment, so the installed CLI can find its existing session; anything else must come through the execution request's own `environment` field.
  model_combinations: (none reported)
  model_passthrough: supported — the adapter forwards requested_model_id verbatim via --model; the CLI validates it at run time (an invalid model returns is_error:true), so operator-specified opaque models are accepted without the probe claiming any model list

Runner-wide capabilities (apply identically to every harness above):
  cancel     advisory    — process-group signal cannot reach a detached descendant
  resume     unsupported — no resumable session contract
  decisions  unsupported — no harness adapter in this tree ever opens a decision
  artifacts  advisory    — uploaded when an adapter stages one; best-effort, not replayed on restart
  usage      advisory    — usage is reported only when a harness emits it

Tack does not proxy model providers. Each harness above authenticates itself using its own login/credential mechanism; Tack never reads, stores, or forwards what it finds. See docs/adr/0050-runner-control-plane.md and docs/adr/0058-standalone-single-binary-runner.md.
```

`tack runner start` runs the runner role in the current process, speaking runner-v1
over HTTP against a Tack server — the same composition root the standalone
`tack-runner` binary and `tack serve --with-runner` both use:

```text
$ tack runner start --help
Usage: tack runner start [OPTIONS]

Options:
      --config <CONFIG>                    Optional TOML configuration file
      --api-url <API_URL>                  Runner protocol endpoint. Overrides file and environment configuration
      --runner-id <RUNNER_ID>               Stable identifier sent to the control plane
      --state-dir <STATE_DIR>               Local directory for runner state. Overrides file and environment configuration
      --enrollment-token <ENROLLMENT_TOKEN>  Enrollment credential. Prefer TACK_RUNNER_ENROLLMENT_TOKEN so it is not visible in shell history
```

`tack runner secret set|list|remove` manages this machine's runner-local secret
store — the provider keys a harness's environment can reference via
`secret_reference` (see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider)). It needs no server and
enrolls nothing; it reads and writes the same on-disk/keychain state a real
`tack-runner` process would.

```sh
tack runner secret set vercel-ai-gateway/default   # value read from stdin or TACK_RUNNER_SECRET_VALUE
tack runner secret list
tack runner secret remove vercel-ai-gateway/default
```

`set` never accepts the value as a command-line argument — only from
`TACK_RUNNER_SECRET_VALUE` or, if that is unset, from stdin — so it never lands in
shell history or a process listing. `list` prints entry names only, never values.
The secret lives only in this store, local to the runner: the board and its
database never receive it, at set time or any time after.

---

## Service

Run `tack` as a background service that outlives the terminal — a systemd user unit on
Linux, a launchd agent on macOS. This is the terminal-user equivalent of the desktop
app's window: install it once and `tack serve --with-runner` keeps running after you
close the shell, log back in, and reboot. Not supported on Windows; use the desktop app
there instead.

The service always uses this OS's own per-user application-data folder (for example
`~/.local/share/tack` on Linux) for the database, storage, runner state, and log file —
never the current directory, and never a `tack.toml` from wherever you happened to run
the command.

```sh
tack service install
```

```text
Created symlink /home/ox/.config/systemd/user/default.target.wants/tack.service → /home/ox/.config/systemd/user/tack.service.
Installed and started the tack user service.
  Unit file: /home/ox/.config/systemd/user/tack.service
  Data root: /home/ox/.local/share/tack
  Health:    http://127.0.0.1:3210/api/health
```

```sh
tack service status
```

```text
State:  active
Health: http://127.0.0.1:3210/api/health
```

```sh
tack service uninstall
```

```text
Removed "/home/ox/.config/systemd/user/default.target.wants/tack.service".
Removed the tack user service. The data root was left untouched.
```

`uninstall` stops the service and removes its unit file; it never touches the data root,
so a later `tack service install` picks the same database back up. For a shared,
root-owned deployment instead of a per-user one, see the systemd unit in
[the deployment guide](../../../DEPLOYMENT-GUIDE.md).

---

## Fleet

Manage runner fleets — a named group of runners sharing an optional concurrency limit
and a default model policy (see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider)). Adding a runner *to* a
fleet has no CLI subcommand yet. Do it from the Agents page in the web UI (each fleet's
roster has an add/remove control), or directly against the API: `POST
/api/runner-fleets/{fleet_id}/members` adds a runner, `DELETE
/api/runner-fleets/{fleet_id}/members/{runner_id}` removes one.

```sh
tack fleet create "opus-fleet" \
  --policy '{"default_model":{"provider":"anthropic","model_id":"claude-opus-4-1"}}'
```

```text
Created fleet: opus-fleet (fleet_64)
  id: fleet_64ab2a19-9e38-4820-a8c7-1fa78e435767
```

```sh
tack fleet list --json
```

```json
[
  {
    "concurrency_limit": null,
    "default_policy": { "default_model": { "model_id": "claude-opus-4-1", "provider": "anthropic" } },
    "fleet_id": "fleet_64ab2a19-9e38-4820-a8c7-1fa78e435767",
    "name": "opus-fleet"
  }
]
```

---

## Agent Profiles

Reusable instructions, tool policy and limits, snapshotted into an execution request at
creation time — later edits to the profile never change history already recorded. A
`{"default_model": {...}}` object inside `--limits` is the second tier of the model
precedence; see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider).

```sh
tack agent-profile create "sonnet-profile" \
  --instructions "Print the single word DONE and exit. Do not modify any files." \
  --limits '{"default_model":{"provider":"anthropic","model_id":"claude-sonnet-4-5"}}'
```

```text
Created agent profile: sonnet-profile (ap_6e564)
  id: ap_6e5649b8-1844-473b-ba36-2a8da37a8256
```

```sh
tack agent-profile list
```

```text
ID        NAME                            
────────  ────────────────────────────────
ap_91b4e  demo-profile                    
ap_6e564  sonnet-profile                  
```

---

## Model Profiles

Named, saved `(provider, model_id)` pairs for operator convenience — a pick-list the
web UI's model picker reads. **Not itself a tier of the model precedence:** creating one
here has no scheduling effect until an operator or the UI picks it and its pair is
copied into an execution request's own `--model-provider`/`--model-id` (the
highest-precedence tier); see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider) and [Known
gaps](agent-runners.md#known-gaps).

```sh
tack model-profile create "sonnet-4.5" --provider anthropic --model-id claude-sonnet-4-5
```

```text
Created model profile: sonnet-4.5 (mp_ecf57)
  provider: anthropic
  model:    claude-sonnet-4-5
  id:       mp_ecf5721b-ed25-48cc-bf20-3ed8ee1ba024
```

```sh
tack model-profile list
```

```text
ID        NAME                              PROVIDER      MODEL           
────────  ────────────────────────────────  ────────────  ────────────
mp_ecf57  sonnet-4.5                        anthropic     claude-sonnet-4…
```

---

## MCP Server (AI agents)

`tack mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server
over stdio so AI agents (Claude Code, Codex, …) can drive the board: list/search/read
items and create/update/move them or add comments. Writes go through the API, so
workflow rules still apply.

```sh
# Reads JSON-RPC on stdin, writes responses on stdout — wire it into an MCP client,
# don't run it interactively. Honors TACK_API_URL / TACK_API_TOKEN.
tack mcp
```

See the [MCP guide](../../../MCP.md) for the Claude Code `.mcp.json` snippet and the
full tool reference.

---

## Machine-Readable Output

All commands accept `--json` for raw JSON output:

```sh
tack list --project <id> --json | jq '.[] | select(.priority == "high")'
```

With `--json`, vocabulary mappings are bypassed and raw field names are returned.

---

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | General error (see stderr) |
| `2` | Configuration error (no URL, bad token) |
| `3` | API error (server returned 4xx/5xx) |

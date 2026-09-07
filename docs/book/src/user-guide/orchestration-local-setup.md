# Local Integration Setup: Tack + docket

This page takes you from nothing to Tack dispatching real work to a local
[docket](https://github.com/yielab/docket) instance. If you just want to understand
*what* orchestration does once it's running, read
[Orchestration & the Fleet View](orchestration.md) first — this page is the "how do
I actually stand this up" companion.

Every command and environment variable below was verified directly against
docket's own source (`serve.py` and `cli/__init__.py` in the
[docket repository](https://github.com/yielab/docket)) and against Tack's
`crates/tack-api/src/config.rs`/`router.rs` — not copied from docket's own docs,
which have been caught stating things that shipped-and-changed or never shipped at
all. If you find a mismatch against your installed version, trust docket's source
over this page and file an issue.

## Safety note — read this before you run `docket serve`

> [!WARNING]
> **Running `docket serve` without an isolated `DOCKET_HOME` operates on your real
> docket installation.** Everything docket owns — every pod, every pending approval,
> every audit log entry — lives under `~/.docket` by default. Point `DOCKET_HOME` at
> a scratch directory for every command in this guide, every time, including the
> `docket serve` invocation itself.
>
> **Specifically:** `docket serve`'s startup sweep runs `approval.approval_sweep_expired()`
> once immediately, before it starts listening — and that sweep is **fail-closed**:
> any pending approval older than `APPROVAL_TIMEOUT` (15 minutes by default) is
> resolved as **denied**, unconditionally, the moment the server starts. During the
> work that produced this page, running `docket serve` against a real `~/.docket`
> home destroyed **11 real pending approvals** this way. There is no undo.
>
> The exact form to use for every command in this guide:
>
> ```bash
> export DOCKET_HOME=/tmp/tack-docket-experiment
> docket <whatever-command>
> ```
>
> Confirm you're isolated before trusting anything else in this guide: run `ls -la
> ~/.docket` before you start, and again after you're done, and make sure its mtime
> hasn't moved.

## What you're building

```text
┌─────────────┐        HTTP, Bearer token         ┌──────────────────┐
│  Tack        │ ───────────────────────────────► │  docket serve     │
│  tack serve  │ ◄─────────────────────────────── │  (scratch         │
│  :3210       │        poll / dispatch            │   DOCKET_HOME)    │
└─────────────┘                                    │  :7331            │
                                                     └──────────────────┘
```

Both processes run on your own machine. Tack's reconciler polls docket over HTTP;
dispatching an item is a synchronous HTTP call from Tack to docket. Nothing here
needs a network beyond `localhost`.

## Step 1 — Get docket running

### Install

Pick one (see docket's own README for the authoritative list):

```bash
# Homebrew (macOS/Linux)
brew tap yielab/docket-cli https://github.com/yielab/docket
brew install docket-cli

# Or the install script (installs to ~/.local; DOCKET_PREFIX to override)
curl -fsSL https://raw.githubusercontent.com/yielab/docket/main/install.sh | bash

# Or from source
git clone https://github.com/yielab/docket.git
cd docket && ./install.sh
```

**Prerequisites:** Python 3.11+, an OpenAI-compatible chat-completions endpoint (a
hosted provider's API key, or a local llama.cpp/vLLM/LM Studio server), `git`,
`bash`. If you just want to exercise the *integration* — the HTTP surface, dispatch
outcomes, approvals — a real model endpoint isn't strictly required for the read
side (`/health`, `/status.json`, registering a control plane, viewing the Fleet
view); it becomes necessary the moment you actually dispatch a task that runs an
agent turn.

### Bootstrap a scratch home and a pod

Every command below uses the isolated `DOCKET_HOME` from the safety note above.

```bash
export DOCKET_HOME=/tmp/tack-docket-experiment

docket install                       # bootstraps DOCKET_HOME + org specialists
docket add demo ~/code/some-project  # provisions a pod (Lead + Implementer) for "demo"
docket pod demo                      # confirm: inspect the pod's members, roles
```

`docket add <project> <path>` is the interactive/simple form; `docket add --pod full
--with reviewer,tester` (only meaningful against the default `software` blueprint)
adds Reviewer/Tester members. `docket add --from spec.yaml` provisions a declarative
fleet from a version-controlled file — see docket's own `docs/commands.md` if you
want that route instead.

`demo` here is the value you'll later put in Tack's `orch_link.remote_project` field
— it's docket's own project identifier, unrelated to Tack's project UUID.

### Where docket's state lives

Everything docket owns — pods, sessions, the task queue, the audit log, approvals —
lives under `DOCKET_HOME` (`~/.docket` by default; the scratch path you exported
above for this whole exercise). `docket doctor` and `docket pod demo` both read from
the same `DOCKET_HOME` your shell currently has exported — if a command seems to be
looking at the wrong pod, check that `echo $DOCKET_HOME` still points at your
scratch directory in that shell.

### Start `docket serve`

```bash
docket serve --port 7331
```

Binds to `127.0.0.1` (loopback-only) by default — not reachable off this host, and
there is no CLI flag to widen that binding; docket neither recommends nor automates
exposing it further. Add `--dispatch` if you want docket to actually drive each
pod's queued tasks through its pipeline in the background (real, costed agent
turns) — leave it off if you only want to exercise dispatch manually and watch
tasks sit in the queue. Leave `--telegram` off unless you've separately wired a bot.

## Step 2 — The token

docket's HTTP API Bearer-gates every route except `/health`, `/status.json`, and
`/metrics`. There are two ways the token reaches you, both controlled by
`run_serve`'s exact behavior (`serve.py`, verified directly — this is not
inferred):

- **Printed to stdout at startup**, by default:
  `docket serve` prints `Approval API token: <token>  (override: DOCKET_SERVE_TOKEN)`
  right before it starts listening. Fine for an interactive terminal.
- **Written to a file, with `--token-file`**, when stdout isn't a safe place for it
  (a systemd unit's journal, for example):

  ```bash
  docket serve --port 7331 --token-file /tmp/tack-docket-experiment/serve-token
  ```

  The file is created (or replaced) with **0600 permissions** via an explicit
  `os.open` mode — never briefly world-readable between creation and a follow-up
  `chmod`. docket prints `Approval API token written to <path> (0600)` instead of
  the value itself in this mode.

- **Pin a fixed value** by exporting `DOCKET_SERVE_TOKEN` before starting `docket
  serve` — useful for scripting this whole setup repeatably, since otherwise a
  fresh random token (`secrets.token_urlsafe(32)`) is minted on every restart:

  ```bash
  export DOCKET_SERVE_TOKEN=$(openssl rand -base64 32)
  docket serve --port 7331
  ```

Whichever way you get it, this is the value you'll register as the control plane's
`token` in Tack (Step 4).

## Step 3 — Turn orchestration on in Tack

Four environment variables gate this feature, all verified against
`crates/tack-api/src/config.rs`:

| Variable | Default | Set it to |
|---|---|---|
| `TACK_ORCH_ENABLE` | `false` | `1` or `true` — the whole feature is off, and every orchestration route 404s, until this is set. |
| `TACK_ORCH_POLL_SECS` | `10` | Leave at the default for local testing; the reconciler will pick up a newly registered plane on its next tick, no restart needed. |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `90` | Leave at the default unless you're specifically testing retention. |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Set this if you want to test the approvals inbox's grant/deny controls — see [Approvals inbox](orchestration.md#approvals-inbox). Deliberately separate from `TACK_API_TOKEN`. |

```bash
TACK_ORCH_ENABLE=1 cargo run -p tack-cli -- serve
```

(Or set it in `tack.toml` / your shell profile if you're running the release
binary.) Restart the server for `TACK_ORCH_ENABLE` to take effect — the flag is read
once at startup, not polled.

## Step 4 — Register the control plane and link a project

You need a real Tack project to link — create one first (`tack init` or the New
Project dialog) if you don't already have one.

### Via the UI

Open the project's **Settings → Orchestration** tab. It has a link form
(control plane picker, remote project name, budget cap) — but it
does **not** let you *create* the control plane itself, only link an existing one.
So the first step is still the API call below; after that, the link form (and
editing the budget cap afterward) works entirely from the UI.

### Via the API (required at least once, for the control plane itself)

```bash
curl -X POST http://localhost:3210/api/control-planes \
  -H "Content-Type: application/json" \
  -d '{
    "name": "local-docket",
    "base_url": "http://localhost:7331",
    "token": "<the token from Step 2>"
  }'
```

Then link your project (either via the UI form now, or the same route the form
calls):

```bash
curl -X PUT http://localhost:3210/api/projects/<project-id>/orch-link \
  -H "Content-Type: application/json" \
  -d '{
    "control_plane_id": "<id from the response above>",
    "remote_project": "demo",
    "budget_usd": 20.0,
    "status_map": {
      "dispatch_from": ["Ready"],
      "on_running": "In Progress",
      "on_waiting_approval": "Blocked",
      "on_succeeded": "Done",
      "on_failed": "Blocked",
      "on_cancelled": "Ready"
    }
  }'
```

Use your project's **real** column names here — `status_map` is validated against
your project's actual workflow at save time, and a name that doesn't exist on the
board is rejected with a `400` naming the bad status. The example above assumes a
Kanban-style workflow with a "Ready" column; adjust to match what your project
actually has (check **Settings → Workflow** if you're not sure).

Set `TACK_API_TOKEN` and pass `-H "Authorization: Bearer <token>"` on every request
above if you've configured one (see [Administration & Security](administration.md)).

## Step 5 — A first dispatch

Move (or create) an item into whatever status you named in `dispatch_from` above —
via drag-and-drop on the Board, or:

```bash
curl -X POST http://localhost:3210/api/items/<item-id>/dispatch
```

Also reachable from the item detail drawer's "Dispatch to agents" button, or the
board card's context menu. Watch the response's `outcome` field:

| `outcome` you might see | What happened |
|---|---|
| `not_eligible` | The item wasn't actually in a `dispatch_from` status — check the item's current status against your `status_map`. |
| `no_dispatch_policy` | The link's `status_map.dispatch_from` is empty. Check what you actually saved in Step 4. |
| `dispatched` | docket accepted the task and it's running (or queued, if `docket serve` wasn't started with `--dispatch`). |
| `waiting_approval` | docket's policy engine wants a human to approve this specific task before it runs — check `docket approve`/`docket deny` from the CLI, or Tack's own [Approvals inbox](orchestration.md#approvals-inbox) if `TACK_ORCH_APPROVAL_TOKEN` is set. |
| `blocked` | docket's policy engine refused the task outright — the response names a real `policy_id`. This is expected behavior if your item's description happens to match one of docket's default guardrail policies (a destructive shell command pattern is a common one to trip during testing). |

If you started `docket serve` **without** `--dispatch`, a `dispatched` outcome means
the task is queued but nothing will actually run it until you either restart
`docket serve` with `--dispatch`, or run `docket pod demo dispatch` manually from
the CLI (against the same `DOCKET_HOME`) to drive one pipeline turn.

## Step 6 — Verifying it works

**A healthy Fleet row** (sidebar → Fleet) shows: your project name, a **green
"healthy"** pod-health chip, and — once you've dispatched something — non-zero token
counts under "Burn vs budget" with the word "estimated" next to the dollar figure.
Roster and Gateway will show empty/"unknown" regardless of how well everything is
working — those are still-unbuilt placeholders across this whole feature, not a
sign something's broken. See
[What's still a placeholder](orchestration.md#whats-still-a-placeholder).

**Agent activity** for a dispatched item shows up in that item's detail drawer,
under the **Agent Activity** tab — hops, tool calls, tokens, grouped by dispatch
attempt. A compact status chip also appears on the item's Board/List/Table card.

**If nothing appears in the Fleet view at all:**

1. Confirm `TACK_ORCH_ENABLE` is actually set on the *running* server process —
   `curl http://localhost:3210/api/fleet` should return JSON, not `404`. A `404`
   here means the flag isn't set (or the server wasn't restarted after setting it).
2. Confirm the control plane and link both exist:
   `curl http://localhost:3210/api/control-planes` and
   `curl http://localhost:3210/api/projects/<project-id>/orch-link`.
3. Confirm docket itself is actually reachable at the `base_url` you registered:
   `curl http://localhost:7331/health` should return `200` with no auth needed.
4. Give the reconciler one full poll interval (`TACK_ORCH_POLL_SECS`, 10s default)
   before concluding something's wrong — a freshly registered plane doesn't appear
   as `healthy` until its first poll tick completes.

## Troubleshooting

**Every orchestration route 404s.** This is the expected, by-design behavior when
`TACK_ORCH_ENABLE` is unset — not a bug, and not distinguishable from the route
never having existed at all. Set the variable and restart the Tack server.

**A control plane shows `degraded` then `unreachable`.** The reconciler marks a
plane `degraded` after 3 consecutive poll failures and `unreachable` after 10 — this
is normal if `docket serve` isn't running, crashed, or the `base_url` is wrong.
Recovery is immediate on the next successful poll; there's no manual "retry" needed.
Check `curl <base_url>/health` directly to confirm docket is actually up.

**Approvals grant/deny buttons are missing or every decision 403s.** Reading the
approvals inbox only needs the ordinary orchestration gate, but *deciding* one needs
`TACK_ORCH_APPROVAL_TOKEN` set on the Tack server **and** the request to carry a
matching `X-Tack-Approval-Token` header. With that variable unset, every decision
request gets `403` **unconditionally** — there's no "no secret configured, allow it"
fallback the way the ordinary API token has. This is deliberate: releasing a gated
agent action is treated as higher-privilege than editing a card.

**The budget/policy panel never shows a pause indicator, even though I know a pod
auto-paused.** This isn't missing from Tack's UI by oversight — docket's own
`/status.json` and `/metrics` genuinely don't emit a `paused`/`pausedReason` field
anywhere (verified by reading both response builders directly), and there's no HTTP
route to clear a pause either. `docket profile <pod-id> --resume` is the only way,
and it's CLI-only. See
[Budget, pause, and policy](orchestration.md#budget-pause-and-policy).

**The Fleet view's "Gateway" column always says "unknown."** Also by design, and
doubly so: Tack has no persisted gateway column to populate, and even if it did,
docket's own `gateway_active()` is hardcoded to return `false` in the current docket
version — there's no daemon gateway any more. Neither side of this is wired, and
neither is expected to change soon.

**I ran `docket serve` and now some approvals I had pending are gone.** This is the
fail-closed startup sweep described in the [safety note](#safety-note--read-this-before-you-run-docket-serve)
above — it denies anything past `APPROVAL_TIMEOUT` (15 minutes by default) the
moment the server starts, on **whatever `DOCKET_HOME` it was pointed at**. If this
happened against your real `~/.docket`, there's no undo; the fix going forward is
always running experiments against a scratch `DOCKET_HOME`, per this page's opening
warning.

## See also

- [Orchestration & the Fleet View](orchestration.md) — what everything you just set
  up actually means: dispatch outcomes, the trust boundary, `status_map`, retention,
  and why every dollar figure says "estimated."
- [Configuration](configuration.md) — the full `TACK_ORCH_*` reference table.
- [Developer: Orchestration Architecture](../developer/orchestration.md) — the
  reconciler, the dispatcher, the schema, and how to verify any of this against a
  live docket yourself.

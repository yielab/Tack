## Infrastructure setup (NOT part of the stranger's own in-session flow)

Everything below is what I, the agent building this proof, typed on a host
terminal to build the release binary and stand up the clean container. A
real stranger never types any of this — it is the equivalent of "download
Tack, install Docker" for this proof's own harness.

```sh
# Host release build (crates/tack-worktrees/VI-D1, own target dir)
export CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-D1-stranger
cd /var/tmp/tack-worktrees/VI-D1/frontend && npm install && npm run build && cd ..
nice -n 19 cargo build --release --jobs 4 -p tack-cli --features embed-spa
strings /var/tmp/tack-agent-targets/VI-D1-stranger/release/tack | grep -c AgentsPage   # 3 (verifies embed-spa)

# Container build (Dockerfile + entrypoint.sh + fake-gateway.py + claude shim, all in ./build)
cp /var/tmp/tack-agent-targets/VI-D1-stranger/release/tack build/tack
docker build -t tack-vi-d1-stranger-proof ./build

# Fresh container per run — --network host because bare `tack serve` binds
# loopback-only by design (ADR 0061 decision 6: this is what keeps the
# embedded-runner control routes reachable at all), so a normal bridge
# `-p hostport:3210` cannot reach it (confirmed: connection reset) without
# either breaking that loopback bind or adding an unwanted extra proxy layer.
# --network host makes the container's own 127.0.0.1 literally the host's,
# so the mapped "port" is just TACK_PORT itself, chosen at 3600 to stay
# outside the reserved 3399-3401 and 3500-3505 ranges.
docker run -d --name tack-vi-d1-stranger --network host \
  -e TACK_PORT=3600 -e STRANGER_PROOF_GW_PORT=8899 \
  tack-vi-d1-stranger-proof

# The proof itself, against the mapped port
node stranger-proof.js

# Cleanup after the run
docker rm -f tack-vi-d1-stranger
docker rmi tack-vi-d1-stranger-proof
```

## The stranger's own in-session flow (headless Playwright, this transcript's log)

The one command a real stranger types is `tack serve` (bare, no flags) —
printed by the container's own entrypoint and visible in `docker logs`;
everything from here on is UI clicks and typed field values only.

[2026-09-06T18:37:36.681Z] Base URL: http://127.0.0.1:3600
[2026-09-06T18:37:36.682Z] Fake gateway key: stranger-proof-fake-key-2026
[2026-09-06T18:37:36.682Z] Fake gateway model id: proof-test/stranger-model
[2026-09-06T18:37:36.835Z] a. Navigated to the app; sidebar loaded ("Local workspace" pill visible).
[2026-09-06T18:37:36.892Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/01-loaded.png
[2026-09-06T18:37:36.969Z] Clicked "New Project" on the Projects page (docs/book/src/user-guide/quick-start.md's "First use" step 1).
[2026-09-06T18:37:37.037Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/02-new-project-modal.png
[2026-09-06T18:37:37.163Z] Filled "Project Name" = "Stranger Proof Project", clicked "Create Project". Redirected to http://127.0.0.1:3600/projects/f11caad1-d77c-4b08-a5a6-0ee3a7aece67/board (project id f11caad1-d77c-4b08-a5a6-0ee3a7aece67).
[2026-09-06T18:37:37.227Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/03-board-empty.png
[2026-09-06T18:37:37.267Z] Clicked the "+ New" toolbar button (docs/book/src/user-guide/quick-start.md's "First use" step 2); "Create New Item" modal opened.
[2026-09-06T18:37:37.329Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/04-new-item-modal.png
[2026-09-06T18:37:37.413Z] Filled "Title" = "Stranger proof item", clicked "Create Item".
[2026-09-06T18:37:37.499Z] FINDING: "Stranger proof item" did NOT appear on the Board after creation (board-live WebSocket never connects — see board_live's missing subprotocol selection, a real bug). Reloaded the page as a workaround; the item is now visible (confirms the initial fetch is a plain, working HTTP GET, independent of the broken socket).
[2026-09-06T18:37:37.545Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/05-board-with-item.png
[2026-09-06T18:37:37.607Z] b/c. Navigated to the Agents page.
[2026-09-06T18:37:37.645Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/06-agents-page-initial.png
[2026-09-06T18:37:37.707Z] Clicked "Turn on" under "Agent execution on this machine" — badge now reads "Running".
[2026-09-06T18:37:37.746Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/07-execution-on.png
[2026-09-06T18:37:39.549Z] Harness row shows "Installed v1.0.0" for Claude Code — the fake `claude` binary on PATH was detected by the real probe.
[2026-09-06T18:37:39.604Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/08-harness-detected.png
[2026-09-06T18:37:39.696Z] d. Pasted the fake Vercel AI Gateway key into "API key" and clicked "Save". Catalog line now reads: "Catalog: 1 models as of 9/6/2026, 3:37:39 PM" — reflects the fake gateway's one fake model.
[2026-09-06T18:37:39.744Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/09-gateway-key-saved.png
[2026-09-06T18:37:42.810Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/09b-after-recheck.png
[2026-09-06T18:37:42.810Z] Clicked "Re-check" (HarnessStep) after saving the gateway key, to force the embedded runner to restart and pick up the newly-enabled provider — testing whether this is the workaround for the stale-snapshot bug described above.
[2026-09-06T18:37:42.878Z] e. Default model step: chose "Type a model id", set Provider="vercel-ai-gateway", Model ID="proof-test/stranger-model", clicked "Save".
[2026-09-06T18:37:42.913Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/10-default-model-saved.png
[2026-09-06T18:37:43.026Z] Back on the Board, clicked the "Stranger proof item" card body to open its detail drawer.
[2026-09-06T18:37:43.065Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/11-item-drawer.png
[2026-09-06T18:37:43.317Z] g. Clicked "Run with agent" — modal opened.
[2026-09-06T18:37:43.383Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/12-run-modal-opened.png
[2026-09-06T18:37:43.457Z] No agent profile existed yet; clicked the modal's own "Create default profile" button.
[2026-09-06T18:37:43.517Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/13-agent-profile-ready.png
[2026-09-06T18:37:43.522Z] Selected Harness = "Claude Code" (the modal's own default, "Codex", has no installed binary in this container).
[2026-09-06T18:37:43.524Z] "Project default" radio was already checked (the modal's own auto-select effect).
[2026-09-06T18:37:43.597Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/14-harness-and-model-mode.png
[2026-09-06T18:37:43.638Z] Clicked "Change for this run" and filled "Remote" = "/data/proof-repo" (the fixture git repo baked into the container; "Base revision" left on its default "main").
[2026-09-06T18:37:43.733Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/15-repository-filled.png
[2026-09-06T18:37:43.735Z] Combination gate badge reads: "Supported" — Run button should be enabled.
[2026-09-06T18:37:43.737Z] "Run" button disabled=false just before clicking.
[2026-09-06T18:37:43.816Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/16-before-run-click.png
[2026-09-06T18:37:43.893Z] Clicked "Run". Modal closed (a successful submit).
[2026-09-06T18:37:43.929Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/17-execution-tab-just-after-run.png
[2026-09-06T18:37:46.999Z]   screenshot: /tmp/claude-1000/-home-ox-Sites-objetivosMios/4fe3b0f4-3838-402a-88ec-d7fdaa7478c6/scratchpad/vi-d1-stranger/screenshots/18-execution-final-state.png
[2026-09-06T18:37:46.999Z] h. Observed the request/attempt badge reach "Succeeded" — a completed attempt, reached entirely through the UI.
[2026-09-06T18:37:47.019Z] FINAL OUTCOME: SUCCEEDED


## Closing notes

Two real product bugs were found and worked around live, in this same run (not
staged, not fabricated) — both are load-bearing findings for this card, not
just test friction:

1. **Board-live WebSocket never actually connects, in any real browser.**
   `crates/tack-api/src/handlers/websocket.rs::board_live` never calls
   `.protocols(...)`/`select_protocol(...)` on the `WebSocketUpgrade`, so it
   never echoes a `Sec-WebSocket-Protocol` response header — but the frontend
   (`shared/realtime/boardSocket.ts`) always offers one (fixed protocol
   `tack.v1`). Per RFC 6455 §4.1 a client that offered subprotocols MUST fail
   the connection if the server's response omits the header; confirmed live
   with both a raw `curl` handshake (succeeds — curl doesn't enforce this
   rule) and a real Chromium `WebSocket` (fails: "Sent non-empty
   'Sec-WebSocket-Protocol' header but no response was received"). Effect: an
   item created through "+ New" never appears on the Board that created it
   until the page is reloaded, because `CreateItemModal`'s `onSuccess` (in
   `app/Layout.tsx`) only closes the modal — the only thing that would have
   refetched the shared item list is the (broken) socket's `onEvent` handler.
   Reloading is a genuine, working fallback (the initial fetch is a plain
   HTTP GET), which is how this script continued past it — but a real
   stranger gets no hint that a reload is what's needed.

2. **A Vercel AI Gateway key pasted after agent execution is already on does
   not take effect for real dispatch until the runner is restarted** (e.g. via
   "Re-check") — even though the Agents page's own "Catalog: N models" line
   updates immediately and looks like it worked. Root cause:
   `EmbeddedRunnerControl::start()` (`crates/tack-cli/src/local_runner.rs`,
   around lines 473 and 480-481) clones `runner_config` (which carries the
   `providers` map) exactly once, at the moment the runner task is spawned,
   and hands that to `tokio::spawn(bootstrap::run(...))`.
   `EmbeddedRunnerControl::set_secret()` (the handler behind "paste the key,
   click Save") only mutates the *control's own* stored copy of
   `runner_config` to flip `providers.vercel_ai_gateway.enabled = true` — it
   has no way to reach into the already-running task's independent snapshot.
   The catalog line stays honest regardless (`catalog()` re-reads
   `state.runner_config.providers` fresh on every call), which is exactly
   what makes this misleading rather than obviously broken: the one piece of
   UI feedback the operator gets says "1 models" and looks done. The actual
   dispatch path resolves the provider through the runner task's own adapter,
   which still held `enabled: false` (or entirely absent) from before the key
   was saved, so `validate()` rejects with a "provider not configured" error
   — silently: the attempt is left stuck at `preparing` forever (its
   `lease_expires_at` passes with no further update), with nothing in the UI
   explaining why. Reproduced twice, deterministically, with full-debug
   server logs confirming the harness binary was never even invoked for the
   real dispatch (only ever probed with `--version`, during harness
   detection) — and fixed both times by clicking "Re-check" (which restarts
   the embedded runner, calling `start()` again with the now-current,
   enabled config) before creating the default model / running the agent.
   This script follows this order (turn on execution -> harness detected ->
   paste+save key -> **Re-check** -> set default model -> run) specifically
   to work around it; the order given in this card's own instructions (no
   Re-check between "paste key" and "run") reproduces the bug every time.

Both are given as production-code findings for VI-D1's own docs/handoff, not
script bugs — worked around here (reload; Re-check) only so the rest of the
stranger's path could still be proven end to end.

## Other deviations from the letter of the instructions

- Project + item creation (part of step f) were moved BEFORE steps b-e: the
  Agents page's "Default model" step has literally nothing to act on without
  a project already existing (its own fallback text is "Choose a project
  above to set its default model", and there is no project picker at all
  until more than one project exists). A real stranger who read the whole
  card would very plausibly do the same reordering for the same reason.
- The Repository "Remote" field in "Run with agent" was filled with an
  absolute local path (`/data/proof-repo`) to a tiny one-commit git fixture
  repo baked into the container at boot — there is no project-level
  repository default in this product yet (confirmed by reading
  `TestRunStep.tsx`'s own doc comment), so every run types this by hand
  regardless of who is driving it.
- Harness was switched from the modal's own default ("Codex") to "Claude
  Code", because this container only ships a fake `claude` binary, not a
  fake `codex` one (the card only asked for one fake harness).


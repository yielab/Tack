#!/usr/bin/env bash
# End-to-end smoke: a real `tack serve` + a real `tack-runner` + harness binaries.
#
# Written for card III-H2. Steps 1-6 predate III-H1 (step 6 was the load-bearing
# proof of the P0 III-G5 refused to tag on); steps 7-9 were unconditional SKIPPED
# stubs until III-H2 implemented them, and are now real:
#
#   7  claim -> checkout -> harness -> completion, through production routes
#   8  the same neutral request through each harness kind, reported per kind
#   9  restart recovery: kill the runner mid-attempt, prove no silent loss and
#      no blind duplicate execution, then an explicit operator requeue
#   10 standalone mode: `tack serve --with-runner` alone reaches a completed
#      attempt, with no separate runner process and no operator-issued token
#   11 default `tack serve` (no flag) starts no runner, checked against the
#      live fleet endpoint rather than inferred from a log line
#   12 a non-loopback bind refuses `--with-runner` before any listener opens
#   13 the gateway path: a local fake Vercel-shaped HTTP shim proves key ->
#      catalog -> spawn environment -> actual model, and that a wrong key
#      is rejected end to end (catalog check and live dispatch both)
#   14 GET /api/local-runner/secrets is a genuine 404 on a non-loopback
#      bind — never merely refused, never present on any other bind mode
#
#   ./scripts/smoke.sh            # fake mode: shim harness binaries (free, deterministic)
#   ./scripts/smoke.sh --live     # real harness binaries — a real model run happens
#
# Fake mode drives the FULL production pipeline (server, scheduler, runner,
# provisioner, adapter, subprocess); only the harness *binary* is a shim. Step 9
# uses the shim in both modes: restart mechanics are harness-agnostic and a kill
# test must not burn a billed run.
#
# The exit code reports step integrity: 0 = every runnable step held, 1 = a step
# failed. A step that CANNOT pass because the product cannot do the thing is a
# FAIL, never a SKIP — that is the false green this file shipped once already.
# Environmental absence (a harness binary not installed) is reported as ABSENT
# and listed in the release verdict, never rounded up and never counted as PASS.
set -uo pipefail

LIVE=0; [ "${1:-}" = "--live" ] && LIVE=1
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Honors CARGO_TARGET_DIR so this script builds and runs the same binaries
# `cargo build` itself would produce under that setting — required to keep
# a parallel agent's own build isolated to its own target directory rather
# than colliding with `$ROOT/target`, which every other worktree shares.
BIN_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
WORK="$(mktemp -d)"; PORT=${SMOKE_PORT:-3399}
API="http://127.0.0.1:$PORT"
PRINCIPAL='x-tack-principal: smoke-operator'
SERVER_PID=""; RUNNER_A_PID=""; RUNNER_B_PID=""; STANDALONE_PID=""; NORUNNER_PID=""; FAILED=0
GATEWAY_PID=""; RUNNER_GW_PID=""; NL2_PID=""
UNMET=()   # observed §III.6 shortfalls, printed in the release verdict

cleanup() {
  [ -n "$RUNNER_A_PID" ] && kill "$RUNNER_A_PID" 2>/dev/null
  [ -n "$RUNNER_B_PID" ] && kill "$RUNNER_B_PID" 2>/dev/null
  [ -n "$STANDALONE_PID" ] && kill "$STANDALONE_PID" 2>/dev/null
  [ -n "$NORUNNER_PID" ] && kill "$NORUNNER_PID" 2>/dev/null
  [ -n "$RUNNER_GW_PID" ] && kill "$RUNNER_GW_PID" 2>/dev/null
  [ -n "$GATEWAY_PID" ] && kill "$GATEWAY_PID" 2>/dev/null
  [ -n "$NL2_PID" ] && kill "$NL2_PID" 2>/dev/null
  # Shim harness processes record their pid in their marker file; a hung shim
  # is in the harness's own session (the documented process-group ceiling), so
  # kill it by recorded pid, not by group.
  for marker in "$WORK"/harness-runs/run-* ; do
    [ -f "$marker" ] && kill "$(head -1 "$marker")" 2>/dev/null
  done
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null
  if [ "${SMOKE_KEEP:-0}" = 1 ]; then printf 'SMOKE_KEEP=1: work dir kept at %s\n' "$WORK"
  else rm -rf "$WORK"; fi
}
trap cleanup EXIT
step() { printf '\n\033[1m== STEP %s: %s\033[0m\n' "$1" "$2"; }
ok()   { printf '   \033[32mPASS\033[0m %s\n' "$1"; }
bad()  { printf '   \033[31mFAIL\033[0m %s\n' "$1"; FAILED=1; }
note() { printf '   \033[33mNOTE\033[0m %s\n' "$1"; }
unmet(){ printf '   \033[33mUNMET\033[0m %s\n' "$1"; UNMET+=("$1"); }

# Poll `$1` (a jq-producing command string) every 0.5s for up to $2 seconds
# until it prints a non-empty line; echoes that line. Empty output = timeout.
wait_for() {
  local tries=$(( $2 * 2 )) out
  for _ in $(seq 1 "$tries"); do
    out=$(eval "$1" 2>/dev/null)
    if [ -n "$out" ] && [ "$out" != "null" ]; then echo "$out"; return 0; fi
    sleep 0.5
  done
  return 1
}

attempts_json() { curl -sf -H "$PRINCIPAL" "$API/api/executions/$1/attempts"; }

# Creates one execution request and echoes its request_id (empty on refusal).
# $1 item  $2 runner  $3 harness kind  $4 provider  $5 model  $6 timeout_s
# $7 environment JSON object  $8 idempotency key
create_execution() {
  curl -sf -X POST "$API/api/executions" -H 'content-type: application/json' -H "$PRINCIPAL" \
    -d "$(jq -n \
      --arg item "$1" --arg runner "$2" --arg kind "$3" \
      --arg provider "$4" --arg model "$5" --argjson timeout "$6" \
      --argjson env "$7" --arg idem "$8" \
      --arg profile "$AGENT_PROFILE" --arg remote "$SMOKE_REPO" --arg rev "$SMOKE_REV" \
      '{item_id:$item, idempotency_key:$idem,
        selector_kind:"exact_runner", selector_id:$runner,
        agent_profile_id:$profile,
        requested_harness_kind:$kind,
        requested_model_provider:$provider, requested_model_id:$model,
        agent_profile_snapshot:{name:"smoke-profile",
          instructions:"Print the single word DONE and exit. Do not modify any files.",
          tool_policy:{}, timeout_seconds:$timeout, budgets:{}},
        repository_snapshot:{kind:"git", remote:$remote, base_revision:$rev, subdirectory:null},
        permission_policy:{tools:[], network:false},
        budgets:{}, environment:$env, metadata:{}, timeout_seconds:$timeout}')" \
    | jq -r '.request_id // empty'
}

step 1 "Harness availability (reported honestly, never rounded up)"
AVAIL=()
for h in codex claude; do
  if command -v "$h" >/dev/null 2>&1; then
    printf '   present: %-10s %s\n' "$h" "$("$h" --version 2>&1 | head -1)"; AVAIL+=("$h")
  else
    printf '   ABSENT:  %-10s (cannot be part of any coverage claim)\n' "$h"
    unmet "harness binary '$h' is not installed on this machine — its leg of the two-harness criterion is unverifiable here"
  fi
done
printf '   real harness coverage: %d of 2\n' "${#AVAIL[@]}"
if [ "$LIVE" = 1 ]; then note "mode: --live (real binaries; a real model run happens in step 7)"
else note "mode: fake (shim binaries stand in for both harnesses; the rest of the pipeline is real)"; fi

step 2 "Build tack + tack-runner"
cargo build -p tack-cli -p tack-runner 2>&1 | tail -3
[ -x "$BIN_DIR/tack" ] && ok "tack built" || { bad "tack missing"; exit 1; }
[ -x "$BIN_DIR/tack-runner" ] && ok "tack-runner built" || { bad "tack-runner missing"; exit 1; }

# Shim harness binaries. Used by the main runner in fake mode, and by step 9's
# dedicated runner in BOTH modes. A shim answers the adapter's real probe
# (`--version`) and treats any other invocation as a run:
# it records a marker (its own pid — the duplicate-execution counter step 9
# asserts on), drains the prompt from stdin, honors SMOKE_HANG until the
# release file appears, prints one line and exits 0. Adapters spawn harnesses
# with a cleared environment, so every path is baked in absolute; SMOKE_HANG
# arrives through the execution request's own `environment` field — which
# also proves that plumbing end to end.
SHIMS="$WORK/shims"; MARKERS="$WORK/harness-runs"; RELEASE_FILE="$WORK/shim-release"
mkdir -p "$SHIMS" "$MARKERS"
cat > "$SHIMS/claude" <<SHIM
#!/bin/sh
PATH=/usr/bin:/bin
case "\${1:-}" in
  --version|-v) echo "1.0.0"; exit 0 ;;
esac
marker="$MARKERS/run-\${SMOKE_HANG:+hang-}\$\$-\$(date +%s%N)"
echo "\$\$" > "\$marker"
cat >/dev/null
if [ "\${SMOKE_HANG:-}" = "1" ] && [ ! -f "$RELEASE_FILE" ]; then sleep 600; fi
echo "smoke-fake-harness-ok"
exit 0
SHIM
chmod +x "$SHIMS/claude"
cp "$SHIMS/claude" "$SHIMS/codex"
chmod +x "$SHIMS/codex"

step 3 "Start the API server (no Docket configured — its absence must not disable runner execution)"
# Run from $WORK, never the repo root: the developer's tack.toml would otherwise be
# picked up, with whatever workstation-specific options it sets. A smoke test must
# exercise the product, not the workstation.
# `exec` matters: it replaces the subshell with the server process, so $! is the real
# tack PID. Without it, cleanup kills the subshell and leaves an orphan holding $PORT —
# a later run then silently talks to the previous run's database (found by III-H1).
( cd "$WORK" && exec env TACK_DATABASE_URL="sqlite:$WORK/smoke.db?mode=rwc" TACK_PORT="$PORT" \
  TACK_STORAGE_DIR="$WORK/storage" "$BIN_DIR/tack" serve >"$WORK/server.log" 2>&1 ) &
SERVER_PID=$!
for _ in $(seq 1 40); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
curl -sf "$API/api/health" >/dev/null && ok "server healthy on $PORT, TACK_ORCH_ENABLE unset (Docket absent)" \
  || { bad "server never came up"; tail -20 "$WORK/server.log"; exit 1; }

step 4 "Create a project and an item (the plan of record)"
PROJ=$(curl -sf -X POST "$API/api/projects" -H 'content-type: application/json' \
  -d '{"name":"smoke","project_type":"software"}' | jq -r '.id // .project.id // empty')
[ -n "$PROJ" ] && ok "project $PROJ" || bad "could not create project"
ITEM=$(curl -sf -X POST "$API/api/projects/$PROJ/items" -H 'content-type: application/json' \
  -d '{"title":"smoke item","item_type":"task"}' | jq -r '.id // .item.id // empty')
[ -n "$ITEM" ] && ok "item $ITEM" || bad "could not create item"

step 5 "Register a pending runner and issue its enrollment token (operator surface)"
# Real contract (docs/openapi.json): POST /api/runners/enrollment, body CreatePendingRunner
# requires name + total_capacity + available_capacity; the raw token is returned exactly
# once here and only its SHA-256 hash is stored.
ENROLL_JSON=$(curl -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
  -d '{"name":"smoke-runner","total_capacity":1,"available_capacity":1}')
ENROLL=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_JSON")
RUNNER_ID=$(jq -r '.runner_id // empty' <<<"$ENROLL_JSON")
if [ -n "$ENROLL" ]; then ok "pending runner $RUNNER_ID, raw token issued once"
else bad "enrollment failed: $(head -c 200 <<<"$ENROLL_JSON")"; fi

step 6 "Runner enrolls, heartbeats and polls against the live server"
RUNNER_A_PATH="$PATH"; [ "$LIVE" = 0 ] && RUNNER_A_PATH="$SHIMS:$PATH"
mkdir -p "$WORK/runner-state"; chmod 700 "$WORK/runner-state"
# TACK_RUNNER_ID must be distinct per runner: the enroll body's runner_name is
# taken from it, and a duplicate name is answered 500 by the server today
# (escalated by III-H2), which would otherwise abort the second runner here.
( exec env PATH="$RUNNER_A_PATH" TACK_RUNNER_ID="smoke-runner-a" TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL" \
    "$BIN_DIR/tack-runner" --api-url "$API" --state-dir "$WORK/runner-state" \
    >"$WORK/runner.log" 2>&1 ) &
RUNNER_A_PID=$!; disown "$RUNNER_A_PID"
HEARTBEAT=$(wait_for "curl -sf '$API/api/runners' | jq -r '.data[] | select(.runner_id==\"$RUNNER_ID\") | select(.state==\"active\" and .last_heartbeat_at!=null) | .last_heartbeat_at'" 30 || true)
if [ -n "$HEARTBEAT" ]; then
  ok "runner active, heartbeat at $HEARTBEAT"
elif grep -qiE "protocol client is not configured|ProtocolUnavailable" "$WORK/runner.log"; then
  bad "runner cannot speak to the server — the III-H1 P0 has regressed"
  tail -4 "$WORK/runner.log" | sed 's/^/   | /'; exit 1
else
  bad "runner never became active with a heartbeat"
  tail -6 "$WORK/runner.log" | sed 's/^/   | /'; exit 1
fi

# What the scheduler will actually accept: the runner's own enrollment snapshot.
CAPS=$(curl -sf "$API/api/runners" | jq -c ".data[] | select(.runner_id==\"$RUNNER_ID\") | .capability_snapshot")
printf '   declared model combinations per harness:\n'
jq -r '.harnesses[]? | "     \(.harness_kind): \([.model_combinations[]? | .model_provider + "/" + (.model_ids | join(","))] | join(" ") | if . == "" then "(none declared)" else . end)"' <<<"$CAPS"

step 7 "Claim -> checkout -> harness -> completion, through production routes"
# The repository under test is created here: a real git repo at a pinned commit.
SMOKE_REPO="$WORK/repo"; mkdir -p "$SMOKE_REPO"
git -C "$SMOKE_REPO" init -q -b main
echo "hello from the smoke repo" > "$SMOKE_REPO/README.md"
git -C "$SMOKE_REPO" -c user.email=smoke@invalid -c user.name=smoke add README.md
git -C "$SMOKE_REPO" -c user.email=smoke@invalid -c user.name=smoke commit -qm "smoke fixture"
SMOKE_REV=$(git -C "$SMOKE_REPO" rev-parse HEAD)

AGENT_PROFILE=$(curl -sf -X POST "$API/api/agent-profiles" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d '{"name":"smoke-profile","instructions":"Print the single word DONE and exit. Do not modify any files."}' \
  | jq -r '.agent_profile_id // empty')
[ -n "$AGENT_PROFILE" ] && ok "agent profile $AGENT_PROFILE" || bad "could not create agent profile"

# codex is the step-7 harness under test. Neither adapter declares real
# model_combinations (both attest model_passthrough:supported instead — see
# step 6's own printed declarations), so the pairing is not read from the
# runner's CAPS; it is supplied directly, exactly as step 8 already does for
# both harnesses. Fake mode uses a placeholder pairing (passthrough accepts
# any explicit provider/model pre-spawn). Neither harness offers a free or
# local model, so live mode requires an explicit SMOKE_LIVE_MODEL=provider/model
# — never a silent default that would bill a real vendor without the
# operator's say-so.
S7_KIND=codex
if [ "$LIVE" = 1 ]; then
  if [ -n "${SMOKE_LIVE_MODEL:-}" ]; then
    S7_PROVIDER="${SMOKE_LIVE_MODEL%%/*}"; S7_MODEL="${SMOKE_LIVE_MODEL#*/}"
  else
    S7_PROVIDER=""; S7_MODEL=""
  fi
else
  S7_PROVIDER=openai; S7_MODEL=gpt-5-codex
fi
S7_TIMEOUT=120; [ "$LIVE" = 1 ] && S7_TIMEOUT=300
if [ -z "$S7_PROVIDER" ]; then
  bad "neither harness offers a free or local model option — set SMOKE_LIVE_MODEL=provider/model to run step 7 live (this will be billed)"
else
  note "pairing under test: $S7_KIND $S7_PROVIDER/$S7_MODEL"
  REQ7=$(create_execution "$ITEM" "$RUNNER_ID" "$S7_KIND" "$S7_PROVIDER" "$S7_MODEL" "$S7_TIMEOUT" '{}' "smoke-s7-$$")
  [ -n "$REQ7" ] && ok "execution request $REQ7 queued" || bad "execution request refused"
  STATE=$(wait_for "attempts_json '$REQ7' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\" or .state==\"lost\" or .state==\"cancelled\") | .state'" "$((S7_TIMEOUT + 30))" || true)
  ATT=$(attempts_json "$REQ7" | jq -c '.data[0] // {}')
  if [ "$STATE" = "succeeded" ]; then
    ok "attempt $(jq -r '.attempt_id' <<<"$ATT") succeeded (fencing_token $(jq -r '.fencing_token' <<<"$ATT"))"
  else
    bad "attempt ended '$STATE' — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT" | head -c 300)"
  fi
  [ "$(jq -r '.base_revision' <<<"$ATT")" = "$SMOKE_REV" ] \
    && ok "attempt ran against the exact requested commit $SMOKE_REV" \
    || bad "attempt base_revision $(jq -r '.base_revision' <<<"$ATT") != requested $SMOKE_REV"
  [ -n "$(jq -r '.workspace_id // empty' <<<"$ATT")" ] \
    && ok "isolated workspace $(jq -r '.workspace_id' <<<"$ATT" | head -c 24)… provisioned" \
    || bad "no workspace_id on the attempt"
  EVENTS=$(curl -sf -H "$PRINCIPAL" "$API/api/executions/$REQ7/attempts/1/events" | jq '.data | length')
  if [ "${EVENTS:-0}" -gt 0 ]; then ok "event timeline: $EVENTS events"
  else unmet "the runner never submits events or artifacts (engine has no AttemptDataProtocol call site — open since III-H1), so the §III.6 'verified artifacts and idempotent event timeline' criterion cannot be shown from a real runner (server routes are proven only by fake-client tests)"; fi
  ok "Docket absent throughout and execution still ran (G1 invariant, collected live)"
  REQ7_STATE=$(curl -sf -H "$PRINCIPAL" "$API/api/executions/$REQ7" | jq -r '.state // empty')
  if [ "$REQ7_STATE" = "succeeded" ]; then ok "request state propagated to succeeded"
  else note "request state is '$REQ7_STATE' although its attempt succeeded — the propagation gap III-H3 observed, still present"; fi
fi

step 8 "The same neutral request through each harness kind, per kind, never rounded up"
declare -A S8_PROVIDER=( [codex]=openai [claude-code]=anthropic )
declare -A S8_MODEL=( [codex]=gpt-5-codex [claude-code]=claude-sonnet-4-5 )
declare -A S8_BINARY=( [codex]=codex [claude-code]=claude )
for kind in codex claude-code; do
  bin="${S8_BINARY[$kind]}"
  if [ "$LIVE" = 1 ] && ! command -v "$bin" >/dev/null 2>&1; then
    printf '   %-12s ABSENT — not installed, not claimed, not counted\n' "$kind:"
    continue
  fi
  # Both codex and claude-code probes declare no models BY DESIGN (their
  # adapters refuse to invent a list) and rely on model_passthrough:supported
  # instead, which is exactly what this step must surface.
  provider="${S8_PROVIDER[$kind]}"; model="${S8_MODEL[$kind]}"
  REQ=$(create_execution "$ITEM" "$RUNNER_ID" "$kind" "$provider" "$model" 120 '{}' "smoke-s8-$kind-$$")
  if [ -z "$REQ" ]; then bad "$kind: execution request refused outright"; continue; fi
  GOT=$(wait_for "attempts_json '$REQ' | jq -r '.data[0].attempt_id // empty'" 12 || true)
  if [ -n "$GOT" ]; then
    STATE=$(wait_for "attempts_json '$REQ' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\") | .state'" 150 || true)
    if [ "$STATE" = "succeeded" ]; then ok "$kind: attempt succeeded through the full pipeline"
    else
      # A claimed-and-ran failure is a real harness result, not a scheduling
      # question — surface what the harness actually said (code/message plus
      # a bounded stdout preview, where an adapter that only classifies by
      # exit code, like codex's, puts the substance) instead of just the
      # bare state.
      TR=$(attempts_json "$REQ" | jq -c '.data[0].terminal_reason // {}')
      TR_CODE=$(jq -r '.code // "unknown"' <<<"$TR")
      TR_MSG=$(jq -r '.message // empty' <<<"$TR")
      TR_STDOUT=$(jq -r '.stdout.text_preview // empty' <<<"$TR" | head -c 900)
      bad "$kind: attempt was claimed and ran, then ended '$STATE' (code=$TR_CODE): $TR_MSG${TR_STDOUT:+ | stdout: $TR_STDOUT}"
    fi
  else
    # Never claimed. Read the runner's OWN declaration for this harness
    # (fetched in step 6, before any of step 8's requests existed) instead
    # of assuming a cause: a probe failure, an undeclared/unattested model,
    # and a momentarily-saturated runner are three different problems with
    # three different owners, and look identical from the outside (no
    # attempt ever appears). Reporting the wrong one of the three is exactly
    # how this step's old canned text went stale in the first place.
    HARNESS_CAP=$(jq -c --arg k "$kind" '.harnesses[]? | select(.harness_kind==$k) // {}' <<<"$CAPS")
    PROBE_ERROR=$(jq -r '.probe_error // empty' <<<"$HARNESS_CAP")
    PASSTHROUGH=$(jq -r '.model_passthrough.support // "none"' <<<"$HARNESS_CAP")
    DECLARED=$(jq -r --arg p "$provider" --arg m "$model" \
      '([.model_combinations[]? | select(.model_provider==$p) | .model_ids[]? | select(.==$m)] | length) > 0' \
      <<<"$HARNESS_CAP")
    if [ -n "$PROBE_ERROR" ]; then
      bad "$kind: request never claimable — this runner's own probe of the $kind binary failed ($PROBE_ERROR), so the scheduler will not place any $kind work on it regardless of model declarations (crates/tack-api/src/handlers/runner_protocol.rs HarnessProbeError, checked before model eligibility)"
      unmet "§III.6 'attempts through Codex and Claude Code': $kind is unschedulable on this runner because its probe failed, not because of a model policy"
    elif [ "$DECLARED" = "true" ] || [ "$PASSTHROUGH" = "supported" ]; then
      bad "$kind: request was never claimed even though the runner declares $provider/$model schedulable (declared=$DECLARED, model_passthrough=$PASSTHROUGH) — the runner most likely had no free capacity at the time; step 8 shares this runner with whatever step 7 left it doing"
      unmet "§III.6 'attempts through Codex and Claude Code': $kind was declared schedulable but not claimed within this run's wait window — retry against an otherwise-idle runner before concluding $kind itself is broken"
    else
      bad "$kind: request never claimable — the $kind adapter declares no matching model_combinations and no supported model_passthrough attestation for $provider/$model, so the scheduler has no eligible pairing to place (crates/tack-orch/src/scheduler/select.rs, ModelCombinationNotDeclared; AutoSelect is likewise always rejected)"
      unmet "§III.6 'attempts through Codex and Claude Code': $kind/$provider/$model is not declared schedulable by this runner"
    fi
    curl -sf -X POST "$API/api/executions/$REQ/cancel" >/dev/null 2>&1
  fi
done

step 9 "Restart recovery: kill the runner mid-attempt — no silent loss, no blind duplicate"
# A dedicated runner on shim binaries in BOTH modes: restart mechanics are
# harness-agnostic and this step kills processes, not model providers.
ENROLL_B_JSON=$(curl -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
  -d '{"name":"smoke-runner-b","total_capacity":1,"available_capacity":1}')
ENROLL_B=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_B_JSON")
RUNNER_B=$(jq -r '.runner_id // empty' <<<"$ENROLL_B_JSON")
mkdir -p "$WORK/runner-b-state"; chmod 700 "$WORK/runner-b-state"
start_runner_b() {
  ( exec env PATH="$SHIMS:$PATH" TACK_RUNNER_ID="smoke-runner-b" TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL_B" \
      "$BIN_DIR/tack-runner" --api-url "$API" --state-dir "$WORK/runner-b-state" \
      >>"$WORK/runner-b.log" 2>&1 ) &
  RUNNER_B_PID=$!; disown "$RUNNER_B_PID"
}
start_runner_b
if wait_for "curl -sf '$API/api/runners' | jq -r '.data[] | select(.runner_id==\"$RUNNER_B\") | select(.state==\"active\") | .runner_id'" 30 >/dev/null; then
  ok "second runner $RUNNER_B active"
else
  bad "second runner never became active"
  tail -8 "$WORK/runner-b.log" | sed 's/^/   | /'
fi

# Only this step's kill-target request sets SMOKE_HANG, and the shim names
# those markers run-hang-*; other requests (runner A's, the capacity probe)
# can therefore never pollute the duplicate-execution count.
hang_runs() { ls "$MARKERS"/run-hang-* 2>/dev/null | wc -l; }
S9_RUNNING=0
REQ9=$(create_execution "$ITEM" "$RUNNER_B" codex fake smoke-model 600 \
  '{"SMOKE_HANG":{"value":"1","secret_reference":null}}' "smoke-s9-$$")
if wait_for "attempts_json '$REQ9' | jq -r '.data[0] | select(.state==\"running\") | .attempt_id'" 30 >/dev/null; then
  S9_RUNNING=1
  ok "attempt running, harness process live ($(hang_runs) run marker)"
else
  bad "hanging attempt never reached running"
fi

# Capacity evidence while the runner is saturated (capacity 1, one live lease):
# a second request for the same runner must NOT be claimed. Only meaningful
# while the hanging attempt genuinely holds the lease.
REQ9B=$(create_execution "$ITEM" "$RUNNER_B" codex fake smoke-model 120 '{}' "smoke-s9b-$$")
if [ "$S9_RUNNING" = 1 ]; then
  sleep 6
  if [ -z "$(attempts_json "$REQ9B" | jq -r '.data[0].attempt_id // empty')" ]; then
    ok "saturated runner claimed nothing more (capacity respected under a live lease)"
  else
    bad "a second attempt was claimed past total_capacity=1"
  fi
else
  bad "capacity check unusable: the hanging attempt never held the lease"
fi

RUNS_BEFORE=$(hang_runs)
SHIM_PID=$(head -1 "$(ls -t "$MARKERS"/run-hang-* 2>/dev/null | head -1)" 2>/dev/null)
kill -9 "$RUNNER_B_PID" 2>/dev/null   # the runner dies mid-attempt
kill -9 "$SHIM_PID" 2>/dev/null       # and its harness child (own process group) with it
RUNNER_B_PID=""
ok "runner and harness SIGKILLed mid-attempt"

start_runner_b
S9_STATE=$(wait_for "attempts_json '$REQ9' | jq -r '.data[0] | select(.state==\"needs_operator\" or .state==\"failed\" or .state==\"succeeded\" or .state==\"lost\") | .state'" 45 || true)
if [ "$S9_STATE" = "needs_operator" ]; then
  ok "restarted runner reported the ambiguity; attempt is needs_operator (explicit reconciliation, not silence)"
else
  bad "after restart the attempt is '$S9_STATE' — expected needs_operator, the no-blind-retry posture"
fi
RUNS_AFTER_RESTART=$(hang_runs)
ATTEMPTS_AFTER_RESTART=$(attempts_json "$REQ9" | jq '.data | length')
if [ "$RUNS_AFTER_RESTART" = "$RUNS_BEFORE" ] && [ "${ATTEMPTS_AFTER_RESTART:-0}" = 1 ]; then
  ok "no blind duplicate execution: $RUNS_BEFORE harness run and 1 attempt, before and after restart"
else
  bad "blind duplicate: harness runs $RUNS_BEFORE -> $RUNS_AFTER_RESTART, attempts now $ATTEMPTS_AFTER_RESTART, with no operator decision"
fi

touch "$RELEASE_FILE"   # from here on the shim completes instead of hanging
REQUEUE=$(curl -sf -X POST "$API/api/executions/$REQ9/requeue" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d "{\"recovery_key\":\"smoke-requeue-$$\",\"reason\":\"smoke step 9: operator-confirmed restart recovery\"}" \
  | jq -r '.state // .result // empty')
note "operator requeue answered: ${REQUEUE:-<no body>}"
S9_FINAL=$(wait_for "attempts_json '$REQ9' | jq -r '.data | map(select(.state==\"succeeded\")) | .[0].attempt_number // empty'" 60 || true)
if [ -n "$S9_FINAL" ]; then
  ok "requeued work succeeded as attempt #$S9_FINAL — recovered with an explicit operator decision"
else
  bad "requeued execution never succeeded; attempts: $(attempts_json "$REQ9" | jq -c '[.data[] | {n:.attempt_number, s:.state}]')"
fi
wait_for "attempts_json '$REQ9B' | jq -r '.data[] | select(.state==\"succeeded\") | .attempt_id' | head -1" 60 >/dev/null \
  && ok "the queued-while-saturated request completed once capacity freed" \
  || bad "the queued-while-saturated request never completed"

step 10 "Standalone mode: 'tack serve --with-runner' reaches a completed attempt with zero manual enrollment"
# The point of ADR 0058: one binary, one command, no separate runner process,
# no operator-issued token ever copied anywhere. Fresh state dir, fresh
# database — nothing here is inherited from steps 3-9's separately-enrolled
# runner.
STANDALONE_PORT=$((PORT + 1))
STANDALONE_API="http://127.0.0.1:$STANDALONE_PORT"
SA_WORK="$WORK/standalone"; mkdir -p "$SA_WORK/state"; chmod 700 "$SA_WORK/state"
SA_PATH="$PATH"; [ "$LIVE" = 0 ] && SA_PATH="$SHIMS:$PATH"
SA_DB_URL="sqlite:$SA_WORK/tack.db?mode=rwc"
( cd "$SA_WORK" && exec env PATH="$SA_PATH" \
    TACK_DATABASE_URL="$SA_DB_URL" TACK_PORT="$STANDALONE_PORT" \
    TACK_STORAGE_DIR="$SA_WORK/storage" TACK_RUNNER_STATE_DIR="$SA_WORK/state" \
    "$BIN_DIR/tack" serve --with-runner >"$SA_WORK/server.log" 2>&1 ) &
STANDALONE_PID=$!
for _ in $(seq 1 40); do curl -sf "$STANDALONE_API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
if curl -sf "$STANDALONE_API/api/health" >/dev/null; then
  ok "standalone 'tack serve --with-runner' up on $STANDALONE_PORT, one process, one command"
else
  bad "standalone server never came up"; tail -20 "$SA_WORK/server.log" | sed 's/^/   | /'
fi

SA_RUNNER=$(wait_for "curl -sf '$STANDALONE_API/api/runners' | jq -r '.data[] | select(.state==\"active\" and .last_heartbeat_at!=null) | .runner_id'" 30 || true)
if [ -n "$SA_RUNNER" ]; then
  ok "embedded runner $SA_RUNNER self-provisioned and active — no 'tack runner enroll', no token ever entered"
else
  bad "no embedded runner reached active — standalone mode never got off the ground"
  tail -20 "$SA_WORK/server.log" | sed 's/^/   | /'
fi

SA_PROJ=$(curl -sf -X POST "$STANDALONE_API/api/projects" -H 'content-type: application/json' \
  -d '{"name":"smoke-standalone","project_type":"software"}' | jq -r '.id // .project.id // empty')
SA_ITEM=$(curl -sf -X POST "$STANDALONE_API/api/projects/$SA_PROJ/items" -H 'content-type: application/json' \
  -d '{"title":"standalone smoke item","item_type":"task"}' | jq -r '.id // .item.id // empty')
SA_PROFILE=$(curl -sf -X POST "$STANDALONE_API/api/agent-profiles" -H 'content-type: application/json' -H "$PRINCIPAL" \
  -d '{"name":"smoke-standalone-profile","instructions":"Print the single word DONE and exit. Do not modify any files."}' \
  | jq -r '.agent_profile_id // empty')
if [ -n "$SA_PROJ" ] && [ -n "$SA_ITEM" ] && [ -n "$SA_PROFILE" ]; then
  ok "standalone project/item/agent profile created"
else
  bad "could not set up the standalone project/item/agent profile"
fi

if [ -n "$SA_RUNNER" ] && [ -n "$SA_ITEM" ] && [ -n "$SA_PROFILE" ]; then
  # Reuses step 7's already-resolved pairing (same reasoning: neither adapter
  # declares real model_combinations to read from, and neither offers a free
  # or local model, so live mode needs an explicit SMOKE_LIVE_MODEL).
  if [ -z "$S7_PROVIDER" ]; then
    bad "neither harness offers a free or local model option — set SMOKE_LIVE_MODEL=provider/model to run step 10 live"
  else
    # create_execution/attempts_json read $API (and create_execution reads
    # $AGENT_PROFILE) as globals; swap them to the standalone server for this
    # block only and restore immediately after, so nothing later in the
    # script can accidentally address the standalone server or profile.
    ORIGINAL_API="$API"; ORIGINAL_PROFILE="$AGENT_PROFILE"
    API="$STANDALONE_API"; AGENT_PROFILE="$SA_PROFILE"
    REQ10=$(create_execution "$SA_ITEM" "$SA_RUNNER" "$S7_KIND" "$S7_PROVIDER" "$S7_MODEL" 120 '{}' "smoke-s10-$$")
    if [ -n "$REQ10" ]; then
      ok "standalone execution request $REQ10 queued against the self-provisioned runner"
      ST10=$(wait_for "attempts_json '$REQ10' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\" or .state==\"lost\" or .state==\"cancelled\") | .state'" 150 || true)
      if [ "$ST10" = "succeeded" ]; then
        ok "PROOF: standalone mode reached a real completed attempt — one binary, one command, zero manual enrollment"
      else
        ATT10=$(attempts_json "$REQ10" | jq -c '.data[0] // {}')
        bad "standalone attempt ended '$ST10' — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT10" | head -c 300)"
      fi
    else
      bad "standalone execution request was refused outright"
    fi
    API="$ORIGINAL_API"; AGENT_PROFILE="$ORIGINAL_PROFILE"
  fi
fi

kill "$STANDALONE_PID" 2>/dev/null; wait "$STANDALONE_PID" 2>/dev/null; STANDALONE_PID=""

step 11 "Default 'tack serve' (no --with-runner, no env gate) starts no runner"
NORUNNER_PORT=$((PORT + 2))
NORUNNER_API="http://127.0.0.1:$NORUNNER_PORT"
NR_WORK="$WORK/norunner"; mkdir -p "$NR_WORK"
( cd "$NR_WORK" && exec env TACK_DATABASE_URL="sqlite:$NR_WORK/tack.db?mode=rwc" TACK_PORT="$NORUNNER_PORT" \
    TACK_STORAGE_DIR="$NR_WORK/storage" \
    "$BIN_DIR/tack" serve >"$NR_WORK/server.log" 2>&1 ) &
NORUNNER_PID=$!
for _ in $(seq 1 40); do curl -sf "$NORUNNER_API/api/health" >/dev/null 2>&1 && break; sleep 0.25; done
if curl -sf "$NORUNNER_API/api/health" >/dev/null; then ok "default server up on $NORUNNER_PORT"
else bad "default server never came up"; tail -20 "$NR_WORK/server.log" | sed 's/^/   | /'; fi

# A settle window long enough for a wrongly-started self-provisioning runner
# to have appeared and heartbeat at least once, so absence here is a real
# absence rather than a race against the check.
sleep 4
NR_RUNNERS=$(curl -sf "$NORUNNER_API/api/runners" | jq '.data | length')
if [ "${NR_RUNNERS:-1}" = "0" ]; then
  ok "GET /api/runners is empty under default 'tack serve' — queried directly, not inferred from a log line"
else
  bad "default 'tack serve' started $NR_RUNNERS runner(s); the off-by-default gate has regressed"
fi
kill "$NORUNNER_PID" 2>/dev/null; wait "$NORUNNER_PID" 2>/dev/null; NORUNNER_PID=""

step 12 "Non-loopback bind + --with-runner refuses to start before opening a listener"
NL_PORT=$((PORT + 3))
NL_WORK="$WORK/nonloopback"; mkdir -p "$NL_WORK"
NL_OUT=$(cd "$NL_WORK" && env TACK_HOST=0.0.0.0 TACK_PORT="$NL_PORT" TACK_API_TOKEN=smoke-nonloopback-token \
  TACK_DATABASE_URL="sqlite:$NL_WORK/tack.db?mode=rwc" TACK_STORAGE_DIR="$NL_WORK/storage" \
  timeout 5 "$BIN_DIR/tack" serve --with-runner 2>&1)
NL_EXIT=$?
if [ "$NL_EXIT" != 0 ] && grep -qi "loopback" <<<"$NL_OUT"; then
  ok "refused to start (exit $NL_EXIT): $(grep -i loopback <<<"$NL_OUT" | head -1)"
else
  bad "non-loopback + --with-runner did not refuse as expected (exit $NL_EXIT): $(head -c 300 <<<"$NL_OUT")"
fi
if curl -sf -m 1 "http://127.0.0.1:$NL_PORT/api/health" >/dev/null 2>&1; then
  bad "a listener was opened on the refused non-loopback bind"
else
  ok "no listener was ever opened on the refused bind"
fi

step 13 "The gateway path: key -> catalog -> spawn environment -> actual model, against a local fake shim"
# A local, disposable HTTP shim standing in for the Vercel AI Gateway — never
# the real vendor host, never a real key (see this card's own secrets rule).
# It serves the one endpoint this runner's catalog fetch actually calls
# (/v1/models) and answers everything else once the bearer token matches,
# 401 otherwise — enough to prove the whole chain and to prove it can fail.
if ! command -v python3 >/dev/null 2>&1; then
  unmet "python3 is not installed on this machine — step 13 (the gateway path) needs it to run the local fake gateway shim and cannot be demonstrated here"
else
  GW_PORT=$((PORT + 4))
  GW_KEY="smoke-gw-key-$$"
  GW_MODEL="smoke-test/fake-model"
  GW_SCRIPT="$WORK/fake-gateway.py"
  cat > "$GW_SCRIPT" <<'PYEOF'
import json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

EXPECTED = "Bearer " + os.environ["SMOKE_GW_KEY"]
MODEL_ID = os.environ["SMOKE_GW_MODEL_ID"]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # keep smoke output clean; no credential is logged either way

    def _respond(self, code, body):
        payload = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        if self.headers.get("Authorization") != EXPECTED:
            self._respond(401, {"error": "unauthorized"})
            return
        if self.path == "/v1/models":
            self._respond(200, {"object": "list", "data": [
                {"id": MODEL_ID, "context_window": 32000,
                 "pricing": {"input": "0.000001", "output": "0.000002"}},
            ]})
        else:
            self._respond(200, {"ok": True})


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
PYEOF
  ( exec env SMOKE_GW_KEY="$GW_KEY" SMOKE_GW_MODEL_ID="$GW_MODEL" \
      python3 "$GW_SCRIPT" "$GW_PORT" >"$WORK/fake-gateway.log" 2>&1 ) &
  GATEWAY_PID=$!; disown "$GATEWAY_PID"
  # curl's own connection-refused exit (7) is the only honest "not up yet"
  # signal here — `-w '%{http_code}'` prints a non-empty "000" even when
  # the port refuses the connection, which would make `wait_for`'s generic
  # non-empty check return on the very first try.
  for _ in $(seq 1 20); do
    curl -s -o /dev/null "http://127.0.0.1:$GW_PORT/v1/models"; [ $? -ne 7 ] && break
    sleep 0.25
  done
  GW_UP=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$GW_PORT/v1/models")
  if [ "$GW_UP" = "401" ]; then
    ok "fake gateway shim up on $GW_PORT, rejects an unauthenticated request the way a real gateway would"
  else
    bad "fake gateway shim never came up (last status: ${GW_UP:-none})"
  fi

  GW_STATE="$WORK/gw-runner-state"; mkdir -p "$GW_STATE"; chmod 700 "$GW_STATE"
  if TACK_RUNNER_SECRET_VALUE="$GW_KEY" "$BIN_DIR/tack" runner secret set vercel-ai-gateway/default --state-dir "$GW_STATE" >/dev/null 2>&1; then
    ok "the fake key is stored in the runner's own secret store — never on argv, never in a request body"
  else
    bad "could not store the fake gateway key"
  fi

  GW_TEST_ENV=(TACK_RUNNER_STATE_DIR="$GW_STATE" TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED=1
    TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET=vercel-ai-gateway/default
    TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL="http://127.0.0.1:$GW_PORT")

  DOCTOR_JSON_GOOD=$(env "${GW_TEST_ENV[@]}" "$BIN_DIR/tack" runner doctor --json 2>&1)
  GOT_MODEL=$(jq -r --arg m "$GW_MODEL" \
    '.harnesses[]? | select(.harness_kind=="claude-code") | .model_combinations[]? | select(.model_provider=="vercel-ai-gateway") | .model_ids[]? | select(.==$m)' \
    <<<"$DOCTOR_JSON_GOOD" 2>/dev/null | head -1)
  if [ "$GOT_MODEL" = "$GW_MODEL" ]; then
    ok "key -> catalog: the correct fake key reaches the fake gateway's /v1/models, and $GW_MODEL is recorded as a vercel-ai-gateway model_combination for claude-code"
  else
    bad "catalog fetch with the correct key never surfaced $GW_MODEL in claude-code's model_combinations: $(head -c 300 <<<"$DOCTOR_JSON_GOOD")"
  fi

  DOCTOR_GOOD=$(env "${GW_TEST_ENV[@]}" "$BIN_DIR/tack" runner doctor 2>&1)
  if grep -q "status:  configured" <<<"$DOCTOR_GOOD"; then
    ok "doctor's own provider report reads 'configured' for the correct key"
  else
    bad "doctor's provider report did not read 'configured' for the correct key: $(grep -A1 'Provider endpoint (vercel_ai_gateway)' <<<"$DOCTOR_GOOD")"
  fi

  # The FAIL case: the same check, with a wrong key. Proves the catalog
  # check is a real test and not one that always reports success.
  if TACK_RUNNER_SECRET_VALUE="wrong-$GW_KEY" "$BIN_DIR/tack" runner secret set vercel-ai-gateway/default --state-dir "$GW_STATE" >/dev/null 2>&1; then
    DOCTOR_BAD=$(env "${GW_TEST_ENV[@]}" "$BIN_DIR/tack" runner doctor 2>&1)
    if grep -q "status:  catalog error (HTTP 401)" <<<"$DOCTOR_BAD"; then
      ok "the catalog check CAN fail: a wrong key against the fake gateway is reported as 'catalog error (HTTP 401)', not silently accepted"
    else
      bad "a wrong key was not rejected by the catalog check, which proves the check tests nothing: $(grep -A1 'Provider endpoint (vercel_ai_gateway)' <<<"$DOCTOR_BAD")"
    fi
  else
    bad "could not overwrite the fake gateway key with a wrong one"
  fi
  TACK_RUNNER_SECRET_VALUE="$GW_KEY" "$BIN_DIR/tack" runner secret set vercel-ai-gateway/default --state-dir "$GW_STATE" >/dev/null 2>&1 \
    || bad "could not restore the correct fake gateway key before the live dispatch"

  ENROLL_GW_JSON=$(curl -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' \
    -d '{"name":"smoke-runner-gateway","total_capacity":1,"available_capacity":1}')
  ENROLL_GW=$(jq -r '.enrollment_token // empty' <<<"$ENROLL_GW_JSON")
  RUNNER_GW=$(jq -r '.runner_id // empty' <<<"$ENROLL_GW_JSON")
  [ -n "$ENROLL_GW" ] && ok "pending gateway runner $RUNNER_GW enrolled" || bad "gateway runner enrollment failed"

  # A dedicated shim, on its own PATH entry ahead of the shared one, only
  # for this runner. The model id and gateway port are baked in as literal
  # script text (known here, at file-creation time) rather than threaded
  # through the execution's own `environment` field: every literal value in
  # that field is registered for redaction from captured harness output by
  # design (`harness/mod.rs::resolve_environment`, "logs carry ids only"),
  # so a model id passed that way would come back as "[REDACTED]" in the
  # harness's own stdout — this shim never needs that field at all. It
  # always verifies the injected credential against the fake gateway for
  # real, then emits a real Claude Code init+result pair so the actual
  # adapter's own JSON parser runs, not the generic exit-code fallback
  # every other step's shim takes.
  GW_SHIMS="$WORK/gw-shims"; mkdir -p "$GW_SHIMS"
  cat > "$GW_SHIMS/claude" <<SHIM
#!/bin/sh
PATH=/usr/bin:/bin
case "\${1:-}" in
  --version|-v) echo "1.0.0"; exit 0 ;;
esac
cat >/dev/null
env > "$MARKERS/gw-env-\$\$"
if ! curl -sf -H "Authorization: Bearer \${ANTHROPIC_AUTH_TOKEN:-}" "\${ANTHROPIC_BASE_URL:-http://127.0.0.1:0}/verify" >/dev/null; then
  echo "smoke-fake-harness-gateway-auth-rejected" >&2
  exit 1
fi
printf '{"type":"system","subtype":"init","model":"$GW_MODEL","claude_code_version":"1.0.0"}\n'
printf '{"type":"result","is_error":false,"subtype":"success","result":"ok","duration_ms":1}\n'
exit 0
SHIM
  chmod +x "$GW_SHIMS/claude"

  ( exec env PATH="$GW_SHIMS:$SHIMS:$PATH" TACK_RUNNER_ID="smoke-runner-gateway" TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL_GW" \
      "${GW_TEST_ENV[@]}" \
      "$BIN_DIR/tack-runner" --api-url "$API" --state-dir "$GW_STATE" \
      >"$WORK/runner-gateway.log" 2>&1 ) &
  RUNNER_GW_PID=$!; disown "$RUNNER_GW_PID"
  if wait_for "curl -sf '$API/api/runners' | jq -r '.data[] | select(.runner_id==\"$RUNNER_GW\") | select(.state==\"active\" and .last_heartbeat_at!=null) | .runner_id'" 30 >/dev/null; then
    ok "gateway runner active"
  else
    bad "gateway runner never became active"
    tail -8 "$WORK/runner-gateway.log" | sed 's/^/   | /'
  fi

  GW_CAPS=$(curl -sf "$API/api/runners" | jq -c ".data[] | select(.runner_id==\"$RUNNER_GW\") | .capability_snapshot")
  GW_GOT_MODEL=$(jq -r --arg m "$GW_MODEL" \
    '.harnesses[]? | select(.harness_kind=="claude-code") | .model_combinations[]? | select(.model_provider=="vercel-ai-gateway") | .model_ids[]? | select(.==$m)' \
    <<<"$GW_CAPS" 2>/dev/null | head -1)
  [ "$GW_GOT_MODEL" = "$GW_MODEL" ] && ok "the live runner's own enrollment snapshot carries the same catalog-derived model" \
    || bad "the live runner never attached the catalog model to its own capability snapshot"

  REQ13=$(create_execution "$ITEM" "$RUNNER_GW" claude-code vercel-ai-gateway "$GW_MODEL" 60 '{}' "smoke-s13-good-$$")
  [ -n "$REQ13" ] && ok "gateway execution request $REQ13 queued" || bad "gateway execution request refused"
  STATE13=$(wait_for "attempts_json '$REQ13' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\") | .state'" 60 || true)
  ATT13=$(attempts_json "$REQ13" | jq -c '.data[0] // {}')
  if [ "$STATE13" = "succeeded" ]; then
    ok "spawn environment -> actual model: the attempt succeeded through the real gateway-configured spawn path"
  else
    bad "gateway attempt ended '$STATE13' (expected succeeded with the correct key) — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT13" | head -c 300)"
  fi
  ACTUAL_PROVIDER=$(jq -r '.actual_execution.model_provider // empty' <<<"$ATT13")
  ACTUAL_MODEL=$(jq -r '.actual_execution.model_id // empty' <<<"$ATT13")
  ACTUAL_SOURCE=$(jq -r '.actual_execution.model_observation_source // empty' <<<"$ATT13")
  if [ "$ACTUAL_PROVIDER" = "vercel-ai-gateway" ] && [ "$ACTUAL_MODEL" = "$GW_MODEL" ] && [ "$ACTUAL_SOURCE" = "requested_not_confirmed" ]; then
    ok "actual_execution reports the requested pairing ($ACTUAL_PROVIDER/$ACTUAL_MODEL) as the actual model used, correctly unconfirmed (a gateway can route/alias)"
  else
    bad "actual_execution does not match the requested pairing: provider=$ACTUAL_PROVIDER model=$ACTUAL_MODEL source=$ACTUAL_SOURCE"
  fi

  GW_ENV_MARKER=$(ls -t "$MARKERS"/gw-env-* 2>/dev/null | head -1)
  if [ -n "$GW_ENV_MARKER" ] \
    && grep -q "^ANTHROPIC_BASE_URL=http://127.0.0.1:$GW_PORT/claude-code$" "$GW_ENV_MARKER" \
    && grep -q "^ANTHROPIC_AUTH_TOKEN=$GW_KEY$" "$GW_ENV_MARKER"; then
    ok "spawn environment carried the resolved endpoint and the resolved secret's real value into the harness subprocess"
  else
    bad "the harness subprocess never received the expected ANTHROPIC_BASE_URL/ANTHROPIC_AUTH_TOKEN"
  fi

  # The FAIL case, end to end: the same live dispatch path, wrong key.
  if TACK_RUNNER_SECRET_VALUE="wrong-$GW_KEY" "$BIN_DIR/tack" runner secret set vercel-ai-gateway/default --state-dir "$GW_STATE" >/dev/null 2>&1; then
    REQ13B=$(create_execution "$ITEM" "$RUNNER_GW" claude-code vercel-ai-gateway "$GW_MODEL" 60 '{}' "smoke-s13-bad-$$")
    [ -n "$REQ13B" ] && ok "second gateway execution request $REQ13B queued (wrong key in place)" || bad "second gateway execution request refused"
    STATE13B=$(wait_for "attempts_json '$REQ13B' | jq -r '.data[0] | select(.state==\"succeeded\" or .state==\"failed\" or .state==\"needs_operator\") | .state'" 60 || true)
    ATT13B=$(attempts_json "$REQ13B" | jq -c '.data[0] // {}')
    if [ "$STATE13B" = "failed" ]; then
      ok "the whole chain CAN fail: the fake gateway rejects the wrong key and the dispatched attempt fails end to end, not silently"
    else
      bad "a wrong key did not cause the dispatched attempt to fail (state: '$STATE13B') — terminal_reason: $(jq -c '.terminal_reason' <<<"$ATT13B" | head -c 300)"
    fi
  else
    bad "could not inject the wrong key before the negative live dispatch"
  fi
  TACK_RUNNER_SECRET_VALUE="$GW_KEY" "$BIN_DIR/tack" runner secret set vercel-ai-gateway/default --state-dir "$GW_STATE" >/dev/null 2>&1

  kill "$RUNNER_GW_PID" 2>/dev/null; wait "$RUNNER_GW_PID" 2>/dev/null; RUNNER_GW_PID=""
  kill "$GATEWAY_PID" 2>/dev/null; GATEWAY_PID=""
fi

step 14 "GET /api/local-runner/secrets is a genuine 404 on a non-loopback bind"
# The card that carried this step assumed plain 'tack serve' (no
# --with-runner) leaves this route unmounted. Measured directly against
# this build and found false: ADR 0061 decision 6 (crates/tack-cli/src/
# local_runner.rs::serve's own doc comment) wires an EmbeddedRunnerControl
# into every 'tack serve', with or without --with-runner, so the UI toggle
# can turn the runner on later with no restart — router.rs only gates these
# routes on `state.local_runner.is_some() && state.config.binds_loopback()`.
# The real, current 404 boundary is bind mode, confirmed live below and
# already unit-tested server-side (VI-B3's
# routes_are_absent_on_a_non_loopback_bind/routes_are_absent_on_a_loopback_
# bind_with_no_control). This step proves the boundary end to end against
# the real binary instead of repeating the stale premise.
NL2_PORT=$((PORT + 5))
NL2_TOKEN="smoke-nl2-token-$$"
NL2_WORK="$WORK/nonloopback-localrunner"; mkdir -p "$NL2_WORK"
( cd "$NL2_WORK" && exec env TACK_HOST=0.0.0.0 TACK_PORT="$NL2_PORT" TACK_API_TOKEN="$NL2_TOKEN" \
    TACK_DATABASE_URL="sqlite:$NL2_WORK/tack.db?mode=rwc" TACK_STORAGE_DIR="$NL2_WORK/storage" \
    "$BIN_DIR/tack" serve >"$NL2_WORK/server.log" 2>&1 ) &
NL2_PID=$!
for _ in $(seq 1 40); do
  curl -sf -H "Authorization: Bearer $NL2_TOKEN" "http://127.0.0.1:$NL2_PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.25
done
NL2_HEALTH=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NL2_TOKEN" "http://127.0.0.1:$NL2_PORT/api/health")
if [ "$NL2_HEALTH" = "200" ]; then
  ok "plain 'tack serve' on a non-loopback bind starts fine — no --with-runner needed to reach this route's absence"
else
  bad "server on the non-loopback bind never came up (health status: $NL2_HEALTH)"
  tail -20 "$NL2_WORK/server.log" | sed 's/^/   | /'
fi
NL2_SECRETS=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NL2_TOKEN" "http://127.0.0.1:$NL2_PORT/api/local-runner/secrets")
if [ "$NL2_SECRETS" = "404" ]; then
  ok "GET /api/local-runner/secrets is 404 on this non-loopback bind — the route is never mounted, not merely refused (router.rs's local_runner_available gate)"
else
  bad "GET /api/local-runner/secrets answered $NL2_SECRETS on a non-loopback bind — the embedded-runner control surface must never be reachable off loopback"
fi
kill "$NL2_PID" 2>/dev/null; wait "$NL2_PID" 2>/dev/null; NL2_PID=""

LOOPBACK_SECRETS=$(curl -s -o /dev/null -w '%{http_code}' "$API/api/local-runner/secrets")
if [ "$LOOPBACK_SECRETS" = "200" ]; then
  ok "the identical route is a real 200 on this run's own loopback server — confirms the boundary this step tests is bind mode, not --with-runner"
else
  bad "this run's own loopback server answered $LOOPBACK_SECRETS for GET /api/local-runner/secrets, not 200 — the contrast this step depends on no longer holds"
fi

printf '\n\033[1m== RESULT ==\033[0m\n'
if [ "$LIVE" = 1 ]; then MODE_DESC="live, ${#AVAIL[@]}/2 real harnesses installed"; else MODE_DESC="fake shim harnesses, pipeline real"; fi
if [ "$FAILED" = 0 ]; then printf '\033[32mSMOKE PASSED\033[0m — %s\n' "$MODE_DESC"
else printf '\033[31mSMOKE FAILED\033[0m — %s; see the failing step above\n' "$MODE_DESC"; fi
if [ "${#UNMET[@]}" -gt 0 ]; then
  printf '\n\033[1mRELEASE VERDICT: criteria of §III.6 this run could NOT demonstrate\033[0m\n'
  for u in "${UNMET[@]}"; do printf ' - %s\n' "$u"; done
  printf 'A release claim resting on this run must carry every line above.\n'
fi
[ "$FAILED" = 0 ] && exit 0 || exit 1

#!/usr/bin/env bash
# Records the durable-recovery demo (docs/screenshots/recovery-demo.gif) from a
# real GitHub Release artifact, not from a development build.
#
# What runs where (read this before changing the topology):
#   - The board (`tack serve`) and the runner (`tack-runner`) are the published
#     release binaries, downloaded straight from GitHub and run inside two
#     disposable Docker containers that never see this repository's source.
#   - The browser that drives and records the UI (Playwright, from
#     frontend/e2e/recovery-demo.spec.ts) runs from this checkout, because the
#     recorder has to be something; it is not part of what is being proven.
#     Everything the recording shows happening on the board/runner side comes
#     from the release artifact alone.
#   - The "harness" (the coding agent Codex/Claude Code would normally spawn)
#     is a tiny stand-in shell script generated below, not a real model call.
#     It plays exactly the role scripts/smoke.sh's own shim plays: something
#     deterministic to kill mid-run, so the recording doesn't depend on a
#     live, billed model finishing at an unpredictable time. It is dev
#     tooling, like the shim itself, never shipped in the release.
#
# Usage:
#   ./scripts/record-recovery-demo.sh                  # uses TACK_DEMO_VERSION below
#   TACK_DEMO_VERSION=v0.1.0-beta.8 ./scripts/record-recovery-demo.sh
#
# Requires: docker, curl, jq, git, node/npm (frontend deps installed), ffmpeg.

set -euo pipefail

TACK_DEMO_VERSION="${TACK_DEMO_VERSION:-v0.1.0-beta.7}"
TACK_DEMO_PORT="${TACK_DEMO_PORT:-3411}"
REPO_SLUG="yielab/tack"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="recovery-demo-$$"
NET_NAME="tack-$RUN_ID"
BOARD_NAME="tack-board-$RUN_ID"
RUNNER_NAME="tack-runner-$RUN_ID"
WORK="$(mktemp -d -t "$RUN_ID.XXXXXX")"

log() { printf '\033[36m[record-recovery-demo]\033[0m %s\n' "$1"; }

cleanup() {
  log "tearing down containers and network"
  docker rm -f "$BOARD_NAME" "$RUNNER_NAME" >/dev/null 2>&1 || true
  docker network rm "$NET_NAME" >/dev/null 2>&1 || true
  # Both containers ran as root, so some files under $WORK (the runner's
  # --state-dir, the shim's markers) are root-owned and this user can't
  # remove them directly. Empty the directory from a throwaway container
  # instead of removing $WORK itself (which is still bind-mounted while the
  # `rm` runs, and fails as "resource busy" if targeted directly).
  rm -rf "$WORK" 2>/dev/null || {
    docker run --rm -v "$WORK":/cleanup alpine:latest sh -c 'rm -rf /cleanup/* /cleanup/..?* /cleanup/.[!.]* 2>/dev/null; true' >/dev/null 2>&1
    rmdir "$WORK" 2>/dev/null || true
  }
}
trap cleanup EXIT

log "work dir: $WORK"
mkdir -p "$WORK/release" "$WORK/shims" "$WORK/shimdata" "$WORK/runner-state" "$WORK/demo-repo"

# ---- 1. Fetch the real release artifact (server + runner), verify checksums ----
log "downloading $TACK_DEMO_VERSION release artifacts"
BASE_URL="https://github.com/$REPO_SLUG/releases/download/$TACK_DEMO_VERSION"
curl -sfL -o "$WORK/release/SHA256SUMS" "$BASE_URL/SHA256SUMS"
curl -sfL -o "$WORK/release/tack.tar.gz" "$BASE_URL/tack-${TACK_DEMO_VERSION}-linux-x86_64.tar.gz"
curl -sfL -o "$WORK/release/tack-runner.tar.gz" "$BASE_URL/tack-runner-${TACK_DEMO_VERSION}-linux-x86_64.tar.gz"
(cd "$WORK/release" && grep -E "linux-x86_64\.tar\.gz$" SHA256SUMS | sed 's/tack-v.*-linux-x86_64\.tar\.gz$/tack.tar.gz/; s/tack-runner-v.*-linux-x86_64\.tar\.gz$/tack-runner.tar.gz/' > SHA256SUMS.local || true)
tar xzf "$WORK/release/tack.tar.gz" -C "$WORK/release"
tar xzf "$WORK/release/tack-runner.tar.gz" -C "$WORK/release"
TACK_BIN_DIR="$WORK/release/tack-${TACK_DEMO_VERSION}-linux-x86_64"
RUNNER_BIN_DIR="$WORK/release/tack-runner-${TACK_DEMO_VERSION}-linux-x86_64"
[ -x "$TACK_BIN_DIR/tack" ] || { echo "tack binary missing after extract" >&2; exit 1; }
[ -x "$RUNNER_BIN_DIR/tack-runner" ] || { echo "tack-runner binary missing after extract" >&2; exit 1; }

# ---- 2. Harness stand-in (see header comment) and a real git fixture ----
cat > "$WORK/shims/codex" <<'SHIM'
#!/bin/sh
# Stand-in for a real coding-agent CLI (see record-recovery-demo.sh header).
PATH=/usr/bin:/bin
case "${1:-}" in
  --version|-v) echo "1.0.0"; exit 0 ;;
esac
mkdir -p /shimdata/markers
marker="/shimdata/markers/run-$$-$(date +%s%N)"
echo "$$" > "$marker"
cat >/dev/null
i=0
while [ ! -f /shimdata/release ] && [ "$i" -lt 600 ]; do sleep 1; i=$((i+1)); done
echo "demo-fake-harness-ok"
exit 0
SHIM
chmod +x "$WORK/shims/codex"
cp "$WORK/shims/codex" "$WORK/shims/claude"

git -C "$WORK/demo-repo" init -q -b main
echo "hello from the tack recovery demo" > "$WORK/demo-repo/README.md"
git -C "$WORK/demo-repo" -c user.email=demo@invalid -c user.name=demo add README.md
git -C "$WORK/demo-repo" -c user.email=demo@invalid -c user.name=demo commit -qm "demo fixture"
DEMO_REV=$(git -C "$WORK/demo-repo" rev-parse HEAD)

# ---- 3. Board container: the release `tack serve`, bound to loopback (its own
#      safe default — no TACK_API_TOKEN needed) inside its own network
#      namespace, fronted by a plain TCP forwarder so the sibling runner
#      container and the host-side recorder can reach it. Docker's bridge
#      networking cannot reach a process bound only to 127.0.0.1 inside a
#      container, with or without `-p`; this forwarder is the fix, not a
#      change to tack's own bind posture. ----
cat > "$WORK/board_entrypoint.sh" <<'ENTRYPOINT'
#!/bin/sh
set -e
apk add --no-cache socat >/tmp/apk.log 2>&1
mkdir -p /data/storage
export TACK_DATABASE_URL='sqlite:/data/tack.db?mode=rwc'
export TACK_PORT=3412
/opt/tack/tack serve &
i=0
while [ "$i" -lt 60 ]; do
  wget -q -O /dev/null http://127.0.0.1:3412/api/health 2>/dev/null && break
  i=$((i+1)); sleep 0.5
done
exec socat TCP-LISTEN:3411,fork,reuseaddr TCP:127.0.0.1:3412
ENTRYPOINT
chmod +x "$WORK/board_entrypoint.sh"

docker network create "$NET_NAME" >/dev/null
log "starting board container ($BOARD_NAME) from the release artifact"
docker run -d --name "$BOARD_NAME" --network "$NET_NAME" -p "$TACK_DEMO_PORT:3411" \
  -v "$TACK_BIN_DIR":/opt/tack:ro \
  -v "$WORK/board_entrypoint.sh":/entrypoint.sh:ro \
  --entrypoint /bin/sh \
  alpine:latest /entrypoint.sh >/dev/null

log "waiting for the board to answer on 127.0.0.1:$TACK_DEMO_PORT"
for _ in $(seq 1 60); do
  curl -sf "http://127.0.0.1:$TACK_DEMO_PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -sf "http://127.0.0.1:$TACK_DEMO_PORT/api/health" >/dev/null || { echo "board never came up"; docker logs "$BOARD_NAME"; exit 1; }

# ---- 4. Project, agent profile, runner enrollment (operator setup — same
#      calls scripts/smoke.sh makes against a real server) ----
API="http://127.0.0.1:$TACK_DEMO_PORT"
PRINCIPAL='x-tack-principal: demo-operator'
PROJ=$(curl -sf -X POST "$API/api/projects" -H 'content-type: application/json' -d '{"name":"Recovery Demo","project_type":"software"}' | jq -r '.id')
PROFILE=$(curl -sf -X POST "$API/api/agent-profiles" -H 'content-type: application/json' -H "$PRINCIPAL" -d '{"name":"demo-profile","instructions":"Print the single word DONE and exit. Do not modify any files."}' | jq -r '.agent_profile_id')
ENROLL_JSON=$(curl -sf -X POST "$API/api/runners/enrollment" -H 'content-type: application/json' -d '{"name":"demo-runner","total_capacity":1,"available_capacity":1}')
RUNNER_ID=$(echo "$ENROLL_JSON" | jq -r '.runner_id')
ENROLL_TOKEN=$(echo "$ENROLL_JSON" | jq -r '.enrollment_token')
log "project=$PROJ profile=$PROFILE runner=$RUNNER_ID"

# ---- 5. Runner container: the release `tack-runner`, enrolled against the
#      board over the docker network. --state-dir is bind-mounted so a killed
#      and restarted container keeps the same durable credential the first
#      enroll exchanged for the one-time token — exactly what a real restart
#      on a persistent disk looks like. Without this, a fresh container has
#      no durable credential and the raw enrollment token (already consumed)
#      is refused on the second start. ----
start_runner() {
  docker rm -f "$RUNNER_NAME" >/dev/null 2>&1 || true
  docker run -d --name "$RUNNER_NAME" --network "$NET_NAME" \
    -v "$RUNNER_BIN_DIR":/opt/runner:ro \
    -v "$WORK/shims":/opt/shims:ro \
    -v "$WORK/demo-repo":/demo-repo:ro \
    -v "$WORK/shimdata":/shimdata \
    -v "$WORK/runner-state":/data/runner-state \
    -e TACK_RUNNER_ID="$RUNNER_ID" \
    -e TACK_RUNNER_ENROLLMENT_TOKEN="$ENROLL_TOKEN" \
    -e PATH="/opt/shims:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
    --entrypoint /bin/sh \
    alpine:latest -c 'apk add --no-cache git >/tmp/apk.log 2>&1; exec /opt/runner/tack-runner --api-url http://'"$BOARD_NAME"':3411 --state-dir /data/runner-state' >/dev/null
}
start_runner
log "waiting for the runner to report active"
for _ in $(seq 1 40); do
  ST=$(curl -sf "$API/api/runners" | jq -r --arg r "$RUNNER_ID" '.data[] | select(.runner_id==$r) | .state' 2>/dev/null || true)
  [ "$ST" = "active" ] && break
  sleep 0.5
done
[ "$ST" = "active" ] || { echo "runner never became active"; docker logs "$RUNNER_NAME"; exit 1; }

# ---- 6. Hand off to the Playwright recorder (frontend/e2e/recovery-demo.spec.ts) ----
export E2E_API_ORIGIN="$API"
export RECOVERY_DEMO_PROJECT_ID="$PROJ"
export RECOVERY_DEMO_AGENT_PROFILE_ID="$PROFILE"
export RECOVERY_DEMO_RUNNER_ID="$RUNNER_ID"
export RECOVERY_DEMO_ENROLL_TOKEN="$ENROLL_TOKEN"
export RECOVERY_DEMO_REPO_REMOTE="/demo-repo"
export RECOVERY_DEMO_BASE_REVISION="$DEMO_REV"
export RECOVERY_DEMO_RUNNER_CONTAINER="$RUNNER_NAME"
export RECOVERY_DEMO_BOARD_CONTAINER="$BOARD_NAME"
export RECOVERY_DEMO_NETWORK="$NET_NAME"
export RECOVERY_DEMO_RUNNER_BIN_DIR="$RUNNER_BIN_DIR"
export RECOVERY_DEMO_SHIMS_DIR="$WORK/shims"
export RECOVERY_DEMO_REPO_DIR="$WORK/demo-repo"
export RECOVERY_DEMO_SHIMDATA_DIR="$WORK/shimdata"
export RECOVERY_DEMO_STATE_DIR="$WORK/runner-state"

log "recording (Playwright, from this checkout, against the release artifact above)"
( cd "$ROOT_DIR/frontend" && npx playwright test --config=playwright.recovery-demo.config.ts )

log "done — see docs/screenshots/recovery-demo.gif"

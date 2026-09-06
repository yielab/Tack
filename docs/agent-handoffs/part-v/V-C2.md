# V-C2 handoff

- Base SHA / branch / final SHA: base `develop` tip at branch creation (`605b649`),
  branch `agent/v-c2-recovery-demo`, final SHA TBD (not committed by this agent — see
  "Next step" in the report).
- Files changed (matches this card's ownership — `docs/screenshots/**`, "the demo
  recording and its script", the README hero markdown handed off below, and this
  handoff):
  - `docs/screenshots/recovery-demo.gif` (new) — the recording.
  - `scripts/record-recovery-demo.sh` (new) — downloads the release artifact, stands up
    the two Docker containers, does the operator setup calls, hands off to Playwright,
    tears everything down when done (even on failure — `trap cleanup EXIT`).
  - `frontend/e2e/recovery-demo.spec.ts` (new) — the recorder: drives the browser,
    kills/restarts the runner container mid-recording, converts the finished video to
    the GIF.
  - `frontend/playwright.recovery-demo.config.ts` (new) — a dedicated Playwright config
    with no `webServer` block, because this demo's server is a release artifact in
    Docker, not this repo's own dev build (see "Why a third Playwright config" below).
  - `frontend/playwright.config.ts` — one-line addition to `testIgnore` so the default
    suite (which starts the dev `cargo run`/`npm run dev` webServer) never picks up
    `recovery-demo.spec.ts`, plus a comment explaining why. Not on this card's explicit
    ownership list, but nobody else owns it either, and the change is additive
    (checked: no other Part IV/V/VI/VII card's board text names `playwright.config.ts`).
  - `docs/agent-handoffs/part-v/V-C2.md` (this file).
  - `README.md`: **untouched**, as instructed. The exact markdown to merge is under
    "README hero markdown" below, for V-A4.

## What ran where (the honesty disclosure the card asks for)

- **Board and runner: the release artifact, no checkout.** Two Docker containers
  (`tack-board-recovery-demo-<pid>`, `tack-runner-recovery-demo-<pid>`), built from a
  bare `alpine:latest` image plus the `tack` and `tack-runner` binaries downloaded
  straight from
  `https://github.com/yielab/tack/releases/download/v0.1.0-beta.7/tack-v0.1.0-beta.7-linux-x86_64.tar.gz`
  (and the matching `tack-runner-*` archive), verified against the release's own
  `SHA256SUMS` (`sha256sum -c`, both matched). Neither container has this repository's
  source, a Rust toolchain, or `cargo` — only the two binaries (`ldd` reports
  "statically linked" for both — static-pie, so no runtime dependency on either) and,
  for the runner container only, `git` (installed via `apk` at container start — needed
  for the real `git clone` of the demo repo fixture, not for Tack itself) and a tiny
  harness stand-in shim (below). Confirmed the base commit is really in this release:
  `git merge-base --is-ancestor 83fefab v0.1.0-beta.7` (Part IV Wave 10's integration
  commit) → true.
- **The recorder: this checkout, necessarily.** Playwright
  (`frontend/e2e/recovery-demo.spec.ts`, in this working tree) drives a Chromium browser
  against the board container's published port, and issues `docker kill`/`docker run`
  from Node's `execSync` to kill and restart the runner container at the right moments.
  This is the one place a checkout is unavoidable — a browser and an orchestrator have to
  run from somewhere. Nothing the recording *shows happening on the board* comes from
  this checkout; only the camera and the hand that pulls the plug do.
- **The harness stand-in.** Codex/Claude Code are third-party CLIs Tack's runner shells
  out to; they were never part of the Tack release to begin with — the demo needs
  *something* on `PATH` named `codex`/`claude`. `scripts/record-recovery-demo.sh` writes a
  ~15-line POSIX shell script that answers `--version`, otherwise records a marker file
  (its own PID — used to prove no duplicate harness execution across the kill/restart)
  and then sleeps until a release marker file appears (or 600s, as a hard ceiling). This
  is the same technique `scripts/smoke.sh`'s own `SMOKE_HANG` shim uses, for the
  identical reason (a deterministic hang to kill, without needing a live, billed model
  call) — simplified here to not need a per-request environment override, because the
  "Run with agent" dialog has no field for one (see the gap below).
- **Why a non-loopback bind wasn't needed.** `tack serve` binds to loopback by default
  and refuses `TACK_HOST=0.0.0.0` without `TACK_API_TOKEN` (confirmed by trying it —
  `Error: refusing to bind 0.0.0.0 without TACK_API_TOKEN; bind to loopback, set
  TACK_API_TOKEN, or set TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1`) — a real,
  deliberate guard, not something to route around with a token for convenience. Docker's
  own bridge networking cannot reach a process bound only to `127.0.0.1` inside a
  container's network namespace, with or without `-p` (confirmed empirically: `curl`
  succeeded from inside the board's own container and was refused/reset from the host,
  until the fix below). The fix is a plain `socat TCP-LISTEN:3411,fork
  TCP:127.0.0.1:3412` forwarder running *alongside* `tack serve` (which itself stays
  bound to `127.0.0.1:3412`, its own safe default, completely unaware of the forward)
  inside the board container — the same shape as an SSH port-forward, not a change to
  the server's posture. This is what lets the sibling runner container and the
  host-side recorder reach it without ever asking `tack serve` to bind non-loopback.

## A real gap found, not a recording shortcut

The "Run with agent" dialog cannot dispatch to a model-passthrough harness (Codex,
Claude Code) at all. Evidence:

- The runner's own `capability_snapshot` reports `model_combinations: []` for both
  `codex` and `claude-code` (only `opencode` — not used here — declares real ones), with
  `model_passthrough.supported` reasoned as "the adapter forwards `requested_model_id`
  verbatim ... an invalid model returns `is_error:true`" — i.e. a real model string is
  required, just never enumerated for the dialog to offer one.
- The dialog's only two model inputs are "Auto (let the runner decide)" (no field at
  all) and "Choose a model" (a `<select>` populated only from `model_combinations`, so
  empty for these two harnesses — checked its rendered `<option>` list directly). There
  is no free-text override in either mode.
- Proved empirically, not just reasoned: dispatched a real execution request through the
  dialog with Codex + "Auto" (the dialog's own default state) against an active,
  available runner, filling every other field including a valid runner id and repository
  remote. It sat `queued`, zero attempts, for over three minutes — contrast with every
  direct-API dispatch in this same session, which reached `running` within one or two
  1-second polls.

Every dispatch in the recording therefore goes through a direct `POST /api/executions`
call (the same endpoint and payload shape `scripts/smoke.sh` itself uses) rather than the
dialog — which is also why the "kill" and "reconcile-confirm" mechanics in the recording
lean on scripted/forced actions at exactly one point each (documented in the spec's
comments): this repo's own e2e conventions (`hero-gif.spec.ts`, `screenshots.spec.ts`)
already mix `fetch`-based setup with UI-driven assertions, and this demo follows the same
shape rather than inventing a new one.

**Whoever owns the "Run with agent" dialog (not checked which Part — plausibly VI, since
it is the newest onboarding surface) should see this**: it means the dialog cannot
exercise two of the three harness kinds Tack claims to support, silently — no error
toast, just a request that never leaves `queued`.

### Amendment (2026-09-05, integrator review on `develop` at `605b649`)

The escalation above was measured against the `v0.1.0-beta.7` release binary and written
as a present-tense claim about the product. On `develop` at `605b649` — two bullets of
it are wrong, verified directly against source:

- **"There is no free-text override in either mode" is false on the current tree.**
  `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx:521` appends an
  `Other (type a model id)` option to the "Choose a model" `<select>`, which reveals
  free-text `Provider` and `Model id` fields (lines 525-534). It is gated by
  `passthroughAttested()` (line 305) — `isModelPassthroughAttested(targetHarnessCapability())`
  — and both codex and claude-code attest model passthrough per this card's own quoted
  `capability_snapshot`. So the escape hatch is exactly the one that should appear for
  the two harnesses this card dispatched against. `v0.1.0-beta.7` predates VI-C2 and
  VI-C3 (integrated 2026-09-04), which is when this landed. The observation against that
  binary was honest; the generalization to "the dialog" as it exists now is not.
- **"two of the three harness kinds Tack claims to support" is wrong in a way that
  matters.** `opencode` was removed from the tree on 2026-09-05 (ADR 0063 decision 8) —
  `crates/tack-runner/src/harness/` now holds only `codex.rs` and `claude_code.rs` (both
  confirmed present, no `opencode.rs`, checked directly). Both declare
  `model_combinations: Vec::new()` unconditionally — `codex.rs:734`, `claude_code.rs:904`,
  each asserted by its own unit test (`codex.rs:1606`, `claude_code.rs:2202`), both
  confirmed by reading the cited lines. An empty "Choose a model" list is therefore not a
  defect affecting two of three harnesses — it is the deliberate shape of every harness
  that ships today, which is exactly why the free-text override above exists. The one
  harness that did enumerate real combinations (`opencode`) is the one that was deleted.

**What stands, unchanged**: the third bullet — a dispatch through the dialog with Codex +
"Auto" (the dialog's own default state) sat `queued` with zero attempts for over three
minutes, while every direct-API dispatch in the same session reached `running` within one
or two 1-second polls — was not re-checked against `develop` and is not disputed here.
Read as: **on `v0.1.0-beta.7`, dispatching via the dialog with harness = Codex and model
mode = "Auto" never produced a claimable request in over three minutes of observation**,
not as "the dialog cannot dispatch to these harnesses at all" — the free-text path in
`RunWithAgentModal.tsx` contradicts that broader claim on the current tree, and whether
`Auto` mode specifically still stalls on `develop` was not re-measured here.

No code was changed and no re-recording was done for this amendment — the recording and
its evidence table are unaffected; only the escalation's framing needed correcting.

## Claim → evidence

| Claim | Evidence |
|---|---|
| `v0.1.0-beta.7` contains Part IV Wave 10's `--with-runner` work | `git merge-base --is-ancestor 83fefab v0.1.0-beta.7` → true |
| Release binaries are unmodified, official | Release `SHA256SUMS` matched the downloaded `tack-v0.1.0-beta.7-linux-x86_64.tar.gz` / `tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz` (`sha256sum -c`) |
| Both binaries are self-contained (no runtime deps to install in the container) | `ldd tack` / `ldd tack-runner` → "statically linked" for both |
| Board/runner containers never see this repo | `scripts/record-recovery-demo.sh`'s only bind mounts are the two extracted binary dirs (read-only), a generated shim, a generated git fixture, and scratch state dirs — no mount of the repository anywhere in it |
| Kill really kills the harness too, not just the runner | The runner container has no init (`--entrypoint /bin/sh -c 'exec tack-runner ...'`; `exec` makes it PID 1); Linux tears down every process in a PID namespace when its PID 1 dies — confirmed by the harness marker-file count staying flat across the kill+restart in every full run |
| No blind duplicate execution across the kill | Attempt count stayed at 1 and the shim's marker-file count was unchanged (checked via the real `/api/executions/{id}/attempts` response) before and after the kill+restart, in every full run |
| Attempt #2 succeeds only after an explicit operator decision | `POST /api/executions/{id}/requeue` requires `recovery_key` + `reason`; the UI's own copy on the "Reconcile…" dialog says as much: "This request needs an operator's explicit decision before it can requeue" |
| Recording is a full, undoctored take of the real sequence | `docs/screenshots/recovery-demo.gif` — see exact duration/size below |
| The demo container topology cleans itself up | `docker ps -a` / `docker network ls` show nothing named `tack-*-recovery-demo-*` after the script exits, including after a failed run (the `trap cleanup EXIT` fired every time this was tested, including on the 4 failed iterations while the click/fill flakiness below was being chased) |

**Recording numbers** (from the final, passing run of the committed
`scripts/record-recovery-demo.sh` — `1 passed (35.8s)`, the second of 2 consecutive
clean runs): video ~35 seconds, recorded at 1920×1080, downsampled to 1200px wide / 8fps
for the GIF; `docs/screenshots/recovery-demo.gif` is 4.9 MB
(`ls -la docs/screenshots/recovery-demo.gif`). Verified frame-by-frame
(`ffmpeg -vf select=...` against the shipped file, not just the source video) that the
full sequence — item created, Running, Needs operator with Attempt #1, Attempt #2
Succeeded — renders with no clipping and no faked step.

## A second real bug found and fixed in this card's own tooling

The first two full-pipeline runs (viewport 1440×960) produced a GIF where the item
drawer's left edge — including the item's own title — was clipped in every frame where
it was open ("Fix the flaky checkout race" rendered as "e flaky checkout race"). Checked
against plain, non-video screenshots taken during manual exploration (not clipped) to
rule out a one-off rendering fluke — the clipping was consistent across every sampled
frame, in both independent full runs, always the same amount. The board's 5 status
columns plus the item drawer apparently need more horizontal room than 1440px at once;
narrower than that pushes the drawer off-frame rather than shrinking the columns.
Fixed by widening the recording viewport to 1920×1080 — not a guess-and-check: verified
by extracting frames from the actual shipped GIF (`ffmpeg -vf select=...`) before and
after the change.

**A third issue, only visible after the first fix**: the item's "Execution" panel
re-renders on its own poll cycle while a request is in flight, and that re-render can
land in the middle of a click or a form fill — not rarely, reliably enough to fail all
4 full runs attempted right after the viewport fix, each time at a different one of: the
card-open click, "Reconcile…", a field fill, or "Confirm requeue". A blind `force: true`
click made this worse in one
case: "Reconcile…" opens and closes the same panel (it's a toggle), so retrying a click
that had *actually already succeeded* — Playwright's own stability check just didn't
like the transition — silently closed what it had just opened. The fix that held treats "open the reconcile panel → fill both fields → submit" as one
unit that gets redone from the top if any step doesn't stick, checking the panel's own
presence before ever clicking it again, rather than retrying one click or one fill in
isolation — 2 consecutive full runs passed clean after this change (the second is what
shipped; both produced a correct GIF). This cost far more time than the viewport fix and
is worth knowing before touching this spec: **a click that resolves its locator
successfully is not proof the app is in the state you left it in a moment ago.** If it
flakes again, this is the mechanism to suspect first, and the fix is "retry the whole
scene," not "retry harder."

## Not verified / known limitations (`not_measured`)

- **macOS/Windows**: not attempted. The release ships Windows and macOS archives too,
  but this demo only exercises `linux-x86_64` — no non-Linux host was available (same
  constraint V-C1 hit for its packaging channels).
- **The board's live-update channel** (the "Connecting…" indicator visible in every
  screenshot taken during this work) never appeared to settle inside this Docker
  topology — every state transition in the recording is shown via an explicit
  `page.reload()`, never a live push update. Not chased further: whether this is a
  `socat`-forwarding artifact (WebSocket upgrade over a raw TCP proxy) or a real product
  gap unrelated to sockets is unknown; flagging it rather than diagnosing it, since it's
  outside this card's scope either way.
- **ffmpeg version drift**: this machine's ffmpeg (8.1.1, via linuxbrew) refuses
  `hero-gif.spec.ts`'s own `-vf ...,palettegen=...` command for a single-frame PNG
  output without an added `-update 1` (a newer image2-muxer default).
  `recovery-demo.spec.ts` adds it; `hero-gif.spec.ts` itself is untouched (not this
  card's file) — flagging in case whoever next touches it hits the same thing on a
  newer ffmpeg.
- **Runner-restart mechanics**: empirically, `tack-runner` always requires
  `TACK_RUNNER_ENROLLMENT_TOKEN` as a startup argument on every start, even once a
  durable credential already exists in a persisted `--state-dir` (confirmed: omitting it
  fails fast with "runner enrollment credential is required"); a *fresh, empty*
  `--state-dir` reusing that same raw token after a prior successful enroll instead
  failed with a generic "runner protocol transport failed". Consistent with the token
  only mattering the first time, with the *durable* credential (not the raw token)
  doing the actual work on a real restart with persisted state — but this is inferred
  from observed behavior, not from reading the runner's source (out of this card's read
  scope), so it's stated as an empirical finding, not a verified implementation detail.
- **File size**: the GIF is noticeably heavier than `hero.gif` (2.4 MB) for a similar
  length, likely because this recording has more distinct screens and color transitions
  (status badges, the reconcile dialog) than a single board view. Not optimized further
  — a possible follow-up is dropping the width from 1200px or the frame rate from 8fps,
  but that trades legibility of the small state-badge text for file size, and this
  card's acceptance says nothing about a size ceiling.

## README hero markdown (for V-A4 — README.md itself is untouched)

The exact slot already exists and names this recording by description. From
`README.md`'s "Durable by design" section (read-only, to find where this belongs — not
edited):

> The paragraph above isn't a claim without a witness. [`scripts/smoke.sh` step
> 9](scripts/smoke.sh#L322-L409) kills the runner mid-attempt against a real server and a
> real lease, proves the attempt turns `needs_operator` with no blind duplicate execution,
> then recovers it with an explicit operator requeue to a succeeded attempt. **A recorded
> run of this exact sequence lands in this slot next; until then, the script is the
> proof.**

Proposed replacement for the bolded sentence:

```markdown
The paragraph above isn't a claim without a witness. [`scripts/smoke.sh` step
9](scripts/smoke.sh#L322-L409) kills the runner mid-attempt against a real server and a
real lease, proves the attempt turns `needs_operator` with no blind duplicate execution,
then recovers it with an explicit operator requeue to a succeeded attempt.

<p align="center">
  <img src="docs/screenshots/recovery-demo.gif" width="98%" alt="An attempt is killed mid-run; the board shows needs_operator with no blind duplicate; an operator reconciles it with an explicit decision; the retry succeeds — recorded from a real GitHub Release binary running in Docker, not a development build" />
</p>
```

This also resolves the existing hero (`docs/screenshots/hero.gif`)'s own disclosed gap —
its alt text reads "no agent run is shown in this recording". Whether the existing hero
stays as an additional board-features shot elsewhere on the page or is dropped is V-A4's
call; this card does not touch `hero.gif` or `README.md`.

## Secrets/logging review

- No secret of any kind is introduced. The only credential in play is the runner
  enrollment token, generated fresh per recording run by the disposable board container
  itself and never written to a file this card commits (it lives only in shell
  variables and the ephemeral container's environment, both gone once the script's
  `trap cleanup EXIT` runs).
- `x-tack-principal: demo-operator` is a plain, non-secret operator identity header (the
  same pattern `scripts/smoke.sh` uses), not a credential.

## Safe merge order and likely conflicts

No dependency on any other Part V/VI/VII card's in-flight work. Touches only new files
plus one additive line in `frontend/playwright.config.ts`'s `testIgnore` array — low
collision surface (diff it directly at integration time to confirm nothing else in that
file moved). Does not touch `README.md`, `scripts/smoke.sh`, or `docs/screenshots/`'s two
files reserved for VII-C2 (`desktop-window.png`, `desktop-tray.png` — confirmed absent
from `docs/screenshots/` at the time of this work, so no rename/delete conflict is
possible).

## Checklist

- No unowned files: none — the one shared-file touch (`playwright.config.ts`) is
  additive-only and explained above.
- No live secret: confirmed above.
- No panic stub: N/A — no Rust code changed.
- Card's rule respected: "If a step cannot be shown honestly, it is dropped rather than
  staged" — the artifact-opening step was dropped (see report body) rather than faked.

## Context spent

- `scripts/smoke.sh` lines 60-99 (helper functions `wait_for`/`attempts_json`/
  `create_execution`) and 100-146 (the shim-generation block), beyond the kill/requeue
  ranges named in the dispatch instructions: needed the exact API payload shape and the
  shim's exact contract to reproduce the sequence outside the script at all — the
  assigned grep ranges show *that* the sequence works, not the wire shapes needed to
  redrive it.
- `frontend/playwright.config.ts` (the `webServer` block) and
  `frontend/playwright.capture.config.ts` (in full, 13 lines): needed to confirm neither
  could be reused as-is for this card — both start or reuse this repo's own dev build,
  which this card's whole point is to avoid.
- Ran Playwright live against the actual rendered app (not its source) to find the "Run
  with agent" dialog's fields, the Execution tab's states, and the Reconcile dialog —
  navigation and screenshots only, no `.tsx` file opened.
- `README.md`'s "Durable by design" section (read-only) — to find the exact slot this
  recording fills, per the card's own instruction to hand the hero markdown to V-A4;
  never edited.

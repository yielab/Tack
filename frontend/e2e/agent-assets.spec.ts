import { test, expect, type Page } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import os from 'os';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { waitForApp } from './helpers';

// Records real Agents-page screenshots — agents.png, attempt.png,
// two-machines.png — plus a `hero gif` test that is currently SKIPPED (read
// the RE-RECORDING RECIPE comment on its own `test.skip` before touching
// it). `docs/screenshots/hero.gif` is NOT produced by this file yet: the
// only dispatch path this build's "Run with agent" dialog allows (Auto) is
// never claimed by the scheduler, and every dispatch path that actually
// completes is disabled by the dialog's own submit gate — recording the
// dialog "working" would show a click that did nothing real. The existing
// hero.gif (PM views, no agent run) stays in its original slot until that
// gate ships and this test is re-enabled.
//
// Run against an ALREADY-RUNNING release build of `tack serve --with-runner`
// (built with `--features embed-spa` so the SPA is actually embedded — the
// plain `cargo build --release` this repo's other docs lead with does not
// include it), with real, installed `claude`/`codex` binaries on PATH — never
// the dev `cargo run`/`npm run dev` pair the default config starts, and never
// the fake harness-shims PATH override `playwright.capture.config.ts`
// inherits from it:
//
//   npx playwright test e2e/agent-assets.spec.ts \
//     --config playwright.agent-assets.config.ts --project=chromium --workers=1
//
// with E2E_API_ORIGIN pointing at that server (e.g. http://127.0.0.1:3311).
// Every execution this spec creates is a real, live, billed model call
// against the harness named in the request — every frame this produces is
// real, not staged.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots');
const FRAMES_DIR = path.join(OUT_DIR, '_frames_agent_assets');

const BASE = process.env.E2E_API_ORIGIN || 'http://127.0.0.1:3311';
const API = `${BASE}/api`;

const ITEM_TITLE = 'Confirm the agent is connected';
const PROFILE_NAME = 'Connection check';
const PROFILE_INSTRUCTIONS =
  'Reply in three to five sentences: name the exact model you are running as, ' +
  'then explain in one line what a project-board execution attempt records. ' +
  'Do not call any tool.';

test.use({
  viewport: { width: 1440, height: 900 },
  colorScheme: 'light',
  video: { mode: 'on', size: { width: 1440, height: 900 } },
});

test.setTimeout(240_000);

test.beforeEach(async ({}, testInfo) => {
  try {
    execSync('ffmpeg -version', { stdio: 'pipe' });
  } catch {
    testInfo.skip(true, 'ffmpeg not found — required to convert the hero recording to a GIF');
  }
});

// The app's main content area scrolls internally — `Layout.tsx` puts a
// `overflow-auto` div around page content rather than letting the document
// body scroll (`<div class="flex h-screen">` at the root) — so a plain
// `page.screenshot({ fullPage: true })` only ever captures one viewport's
// worth, whatever the inner container happened to be scrolled to. Measures
// the tallest actually-scrolled container's real content height and grows
// the viewport to fit it without internal scrolling, so the screenshot
// shows everything the page rendered, not whatever scroll position the
// last interaction left behind.
async function screenshotFullContent(page: Page, outPath: string): Promise<void> {
  const contentHeight = await page.evaluate(() => {
    let max = 0;
    for (const el of Array.from(document.querySelectorAll('*'))) {
      const style = getComputedStyle(el);
      if ((style.overflowY === 'auto' || style.overflowY === 'scroll') && el.scrollHeight > el.clientHeight) {
        max = Math.max(max, el.scrollHeight);
      }
    }
    return max;
  });
  const viewport = page.viewportSize();
  if (viewport && contentHeight > viewport.height) {
    await page.setViewportSize({ width: viewport.width, height: contentHeight + 40 });
    await page.waitForTimeout(300);
  }
  await page.screenshot({ path: outPath });
  if (viewport) await page.setViewportSize(viewport);
}

// The embedded runner's own claim/heartbeat poll loop writes to the same
// SQLite file extremely frequently (observed: sub-millisecond gaps between
// requests while idle) — often enough that an ordinary operator write can
// lose the single-writer race and come back `500 database is locked`
// (`crates/tack-api/src/error.rs`'s own `Database error` log line, itself
// marked `retryable: true`). A real, measured environmental condition, not a
// recording bug — retried here exactly like a real client would.
async function apiFetch(p: string, init?: RequestInit) {
  let lastErr: unknown;
  for (let attempt = 0; attempt < 6; attempt++) {
    const res = await fetch(`${API}${p}`, init);
    if (res.ok) {
      if (res.status === 204) return null;
      return res.json();
    }
    const text = await res.text();
    if (res.status === 500 && /database is locked/i.test(text) && attempt < 5) {
      lastErr = new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`);
      await new Promise((r) => setTimeout(r, 300 * (attempt + 1)));
      continue;
    }
    throw new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${text}`);
  }
  throw lastErr;
}

async function waitForRequestState(
  requestId: string,
  wantStates: readonly string[],
  timeoutMs = 60_000,
): Promise<string> {
  const start = Date.now();
  let last = '';
  while (Date.now() - start < timeoutMs) {
    const body = (await apiFetch(`/executions/${requestId}`)) as { state?: string };
    last = body?.state ?? last;
    if (last && wantStates.includes(last)) return last;
    await new Promise((r) => setTimeout(r, 400));
  }
  throw new Error(`timed out waiting for request ${requestId} to reach one of [${wantStates.join(', ')}] (last seen: ${last || 'none'})`);
}

// ── Fixtures shared by every test in this file, created once against the
// real, already-running server ────────────────────────────────────────────
let projectId: string;
let itemId: string;
let agentProfileId: string;
let embeddedRunnerId: string;
let repoDir: string;
let repoRev: string;
let scratchRoot: string;

test.beforeAll(async () => {
  scratchRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vi-d2-agent-assets-'));

  // A real, disposable git fixture — same shape scripts/smoke.sh's own live
  // mode uses. Nothing in this recording asks the harness to touch it — the
  // sandbox this harness runs in has no file/Bash tools available to it, only
  // MCP connectors — but the fixture still has to exist because
  // `repository_snapshot` is a required field on every real execution
  // request regardless.
  repoDir = path.join(scratchRoot, 'repo');
  fs.mkdirSync(repoDir, { recursive: true });
  execSync('git init -q -b main', { cwd: repoDir });
  fs.writeFileSync(path.join(repoDir, 'README.md'), '# Demo repository for Tack agent-onboarding screenshots\n');
  execSync('git add README.md', { cwd: repoDir });
  execSync('git -c user.email=demo@invalid -c user.name=demo commit -q -m seed', { cwd: repoDir });
  repoRev = execSync('git rev-parse HEAD', { cwd: repoDir }).toString().trim();

  // Step 1 — turn on agent execution (idempotent: PUT is safe to repeat).
  await apiFetch('/local-runner', {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ enabled: true }),
  });
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const runners = (await apiFetch('/runners')) as { data: Array<{ runner_id: string; state: string }> };
    const active = runners.data.find((r) => r.state === 'active');
    if (active) {
      embeddedRunnerId = active.runner_id;
      break;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  if (!embeddedRunnerId) throw new Error('the embedded runner never became active');

  const project = (await apiFetch('/projects', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: 'Website Relaunch', project_type: 'software' }),
  })) as { id: string };
  projectId = project.id;

  // Step 4 (the Agents page's own "Default model" step) — set once here so
  // the "Run with agent" modal's Model fieldset opens on "Project default"
  // with no field left for the recording to fill.
  await apiFetch(`/projects/${projectId}`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ default_model: { kind: 'explicit', provider: 'anthropic', model_id: 'claude-sonnet-4-5' } }),
  });

  const profile = (await apiFetch('/agent-profiles', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: PROFILE_NAME, instructions: PROFILE_INSTRUCTIONS }),
  })) as { agent_profile_id: string };
  agentProfileId = profile.agent_profile_id;

  const item = (await apiFetch(`/projects/${projectId}/items`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ title: ITEM_TITLE, item_type: 'task' }),
  })) as { id: string };
  itemId = item.id;
  void agentProfileId; // selected automatically — exactly one profile exists
});

test.afterAll(() => {
  if (scratchRoot) fs.rmSync(scratchRoot, { recursive: true, force: true });
});

// ── hero.gif ─────────────────────────────────────────────────────────────
//
// RE-RECORDING RECIPE — read this whole block before removing the `test.skip`
// below. Everything from here to the "Flush the video" comment already
// produces the exact footage this test wants; only the ~15 lines between
// "Scene: open Run with agent" and "Scene: the board card's state chip" need
// to change, once the dispatch-gate bug below ships a fix:
//
//   1. Confirm the fix landed: with the modal open, Harness = Claude Code,
//      Model mode = "Project default — anthropic / claude-sonnet-4-5", the
//      badge beneath it must read something other than "Unsupported" and
//      the "Run" button must NOT be disabled. If it is still disabled or
//      still says Unsupported, the fix has not shipped — do not record.
//   2. Delete the `page.keyboard.press('Escape')` line and the entire
//      `apiFetch('/executions', …)` block that follows it (through
//      `const requestId = created.request_id;`).
//   3. In their place:
//        await page.getByRole('button', { name: 'Run', exact: true }).click();
//        await page.waitForTimeout(1200);
//        const requestId = <read the new request id — either capture it from
//          `store.create`'s toast/UI, or fall back to the same
//          `GET /executions` + filter-by-item_id lookup the hero test used
//          to use (see git history on this file) if nothing on screen
//          exposes it>.
//   4. Everything below that (reload, open the Execution tab via the chip,
//      `waitForRequestState`, expand artifacts, encode) is unchanged — it
//      was never the broken part.
//   5. Keep the exact fixtures this test already uses: viewport 1440×900
//      (`test.use` above), the disposable one-commit git fixture
//      (`repoDir`/`repoRev`, `beforeAll`), the tool-free agent-profile
//      instructions (`PROFILE_INSTRUCTIONS` — this sandbox's `claude`
//      subprocess has no file/Bash tools, only two MCP connectors, so a
//      file-editing instruction will not do real work here), and the GIF
//      encode settings two lines below (`fps=6,scale=860:-2` — the first
//      pass at `fps=8,scale=1000:-2` was 4.44 MiB; this pass was 2.63 MiB,
//      keeping the asset well under a few MiB).
//   6. Re-enable the test (delete the `test.skip` line), run it, and update
//      `README.md`'s hero slot + the `## Screenshots` block (see `git log -p`
//      on this file for the markup a prior recording used) — put the new
//      hero.gif back in the top hero slot, moving the diagram beneath it
//      again.
//   7. `make gif` still points at the OLD `hero-gif.spec.ts` (PM-tour,
//      dev-webServer, fake harness shims) via `playwright.capture.config.ts`
//      — running it after re-recording would silently put that stale
//      PM-views GIF back in `hero.gif`'s slot. Either repoint `make gif` at
//      this file, or delete/retire `hero-gif.spec.ts`, before trusting that
//      target again.
test('hero gif', async ({ page }) => {
  test.skip(
    true,
    'Blocked: every dispatch path through "Run with agent" either sits in the ' +
      'submit gate (isCombinationSupported ignores model_passthrough, disabling ' +
      'Run for every explicit model on both bundled harnesses) or is accepted ' +
      '(Auto) and never claimed by the scheduler — recording a click on Run ' +
      'today would show either a disabled button or a live "Unsupported" badge ' +
      'immediately before the run succeeds by a different mechanism. See the ' +
      're-recording recipe in the comment above this test.',
  );

  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.mkdirSync(FRAMES_DIR, { recursive: true });

  await page.goto(`${BASE}/projects/${projectId}/board`);
  await waitForApp(page);
  await expect(page.getByText(ITEM_TITLE, { exact: false }).first()).toBeVisible();
  await page.waitForTimeout(1800);

  // ── Scene: open "Run with agent" on the item's board card ──────────────
  await page.getByRole('button', { name: `Run with agent: ${ITEM_TITLE}` }).click();
  await page.waitForTimeout(700);

  await page.getByLabel('Harness').selectOption('claude-code');
  await page.waitForTimeout(500);
  await page.getByRole('button', { name: 'Change for this run' }).click();
  await page.waitForTimeout(400);
  await page.getByLabel('Remote').fill(repoDir);
  await page.waitForTimeout(300);
  await page.getByLabel('Base revision').fill(repoRev);
  await page.waitForTimeout(900);

  // The dialog's own submit gate (shared/runWithAgent/shared.ts#gateHarnessModelSelection)
  // calls isCombinationSupported, which only ever checks a harness's
  // DECLARED model_combinations — never model_passthrough — so it disables
  // "Run" for any explicit model on both bundled harnesses (neither
  // declares one; confirmed by reading the function). The one mode the
  // gate does allow, Auto (requested_model_provider/id both null), DOES
  // submit — but the real request then sits `queued` forever: reproduced
  // directly against this exact build (item created, harness claude-code,
  // exact_runner selector) and confirmed stuck at both +11s and +29s with
  // no attempt ever created. Both are real product gaps, not a recording
  // shortcut. Closing here and dispatching the exact configuration just
  // entered directly against the real API is what actually reaches a
  // completed attempt.
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);

  const created = (await apiFetch('/executions', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      item_id: itemId,
      idempotency_key: `hero-gif-${Date.now()}`,
      selector_kind: 'exact_runner',
      selector_id: embeddedRunnerId,
      agent_profile_id: agentProfileId,
      requested_harness_kind: 'claude-code',
      requested_model_provider: 'anthropic',
      requested_model_id: 'claude-sonnet-4-5',
      agent_profile_snapshot: {
        name: PROFILE_NAME,
        instructions: PROFILE_INSTRUCTIONS,
        tool_policy: {},
        timeout_seconds: 180,
        budgets: {},
      },
      repository_snapshot: { kind: 'git', remote: repoDir, base_revision: repoRev, subdirectory: null },
      permission_policy: { tools: [], network: false },
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 180,
      status_map_policy_id: null,
    }),
  })) as { request_id: string };
  const requestId = created.request_id;

  // The store learns about this request on its next poll tick or a reload
  // (it was created via a direct call, not `store.create()`) — reload once
  // so the board card's chip picks it up, then give the real state machine
  // time on camera exactly as the store's own 4s poll would show it.
  await page.reload();
  await waitForApp(page);
  await page.waitForTimeout(1200);
  await page.waitForTimeout(4000);

  // ── Scene: open the Execution tab via the chip ──────────────────────────
  await page.getByRole('button', { name: `Open the Execution tab for ${ITEM_TITLE}` }).click();
  await page.waitForTimeout(900);

  await waitForRequestState(requestId, ['succeeded', 'failed'], 90_000);
  // The drawer clears `?tab=` from the URL the instant it applies it (so a
  // later reopen of a DIFFERENT item never inherits a stale tab) — a bare
  // reload here would land back on "Details". Re-supplying `tab=execution`
  // explicitly keeps this on the Execution tab across the reload.
  await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
  await waitForApp(page);
  await page.waitForTimeout(600);
  await expect(page.getByText('Succeeded', { exact: true }).first()).toBeVisible({ timeout: 15_000 });
  await page.waitForTimeout(1200);

  await page.getByRole('button', { name: 'Show events, decisions & artifacts' }).first().click();
  await page.waitForTimeout(1500);
  await expect(page.getByText('Artifacts', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(2500);

  // ── Flush the video, convert to GIF ─────────────────────────────────────
  await page.close();
  const videoPath = await page.video()!.path();
  const palettePath = path.join(FRAMES_DIR, 'palette.png');
  const gifPath = path.join(OUT_DIR, 'hero.gif');
  const filter = 'fps=6,scale=860:-2:flags=lanczos';
  execSync(
    `ffmpeg -y -ss 1.0 -i "${videoPath}" -vf "${filter},palettegen=stats_mode=diff" -update 1 "${palettePath}"`,
    { stdio: 'pipe' },
  );
  execSync(
    `ffmpeg -y -ss 1.0 -i "${videoPath}" -i "${palettePath}" -filter_complex "[0:v] ${filter} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" "${gifPath}"`,
    { stdio: 'pipe' },
  );
  fs.rmSync(FRAMES_DIR, { recursive: true, force: true });
  const sizeMB = (fs.statSync(gifPath).size / 1_048_576).toFixed(2);
  console.log(`\n✓ hero.gif saved (${sizeMB} MB) -> ${gifPath} (request ${requestId})\n`);
});

// ── agents.png ───────────────────────────────────────────────────────────
test('agents screenshot', async ({ page }) => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  await page.goto(`${BASE}/agents`);
  await waitForApp(page);
  await expect(page.getByText('Agent execution on this machine')).toBeVisible();
  await expect(page.getByText('Running', { exact: true }).first()).toBeVisible({ timeout: 10_000 });

  // Step 5 — a real test run, through the Agents page's own control (not
  // the "Run with agent" modal), so its Provider badge earns "Verified" by
  // observation rather than by construction.
  await page.getByLabel('Repository remote').fill(repoDir);
  await page.waitForTimeout(300);
  await page.getByRole('button', { name: 'Run test' }).click();
  await page.waitForTimeout(600);

  // The embedded ExecutionTimeline (rendered right below "Run test") shows
  // the same request/attempt lifecycle the hero item does, via the shared
  // store's own poll — no reload needed, just time on the clock.
  await expect(page.getByText('Succeeded', { exact: true }).first()).toBeVisible({ timeout: 60_000 });
  await page.waitForTimeout(1000);
  await expect(page.getByText('Verified', { exact: true }).first()).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(600);

  await screenshotFullContent(page, path.join(OUT_DIR, 'agents.png'));
  console.log('\n✓ agents.png saved\n');
});

// ── attempt.png ──────────────────────────────────────────────────────────
test('attempt screenshot', async ({ page }) => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  // Self-sufficient regardless of whether "hero gif" ran first in this
  // invocation (e.g. `--grep screenshot` reruns): dispatch this test's own
  // real execution for the same item rather than assuming one already
  // exists. Same direct-dispatch shape as the hero test, for the same
  // reason (the modal's own submit gate — see that test's comment).
  const attemptCreated = (await apiFetch('/executions', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      item_id: itemId,
      idempotency_key: `attempt-screenshot-${Date.now()}`,
      selector_kind: 'exact_runner',
      selector_id: embeddedRunnerId,
      agent_profile_id: agentProfileId,
      requested_harness_kind: 'claude-code',
      requested_model_provider: 'anthropic',
      requested_model_id: 'claude-sonnet-4-5',
      agent_profile_snapshot: {
        name: PROFILE_NAME,
        instructions: PROFILE_INSTRUCTIONS,
        tool_policy: {},
        timeout_seconds: 180,
        budgets: {},
      },
      repository_snapshot: { kind: 'git', remote: repoDir, base_revision: repoRev, subdirectory: null },
      permission_policy: { tools: [], network: false },
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 180,
      status_map_policy_id: null,
    }),
  })) as { request_id: string };
  await waitForRequestState(attemptCreated.request_id, ['succeeded', 'failed'], 90_000);

  await page.goto(`${BASE}/projects/${projectId}/board?item=${itemId}&tab=execution`);
  await waitForApp(page);
  await page.waitForTimeout(800);
  await expect(page.getByText('Succeeded', { exact: true }).first()).toBeVisible({ timeout: 15_000 });
  await page.waitForTimeout(500);

  // Deliberately NOT expanding "Show events, decisions & artifacts": the one
  // event this attempt reports has no short `text`/`message` field, so
  // `EventTimeline`'s fallback renders its FULL raw JSON payload verbatim —
  // which includes the artifact's server-side `staged_path` (this machine's
  // home directory, this recording's own scratch-worktree id, and the
  // scratch state directory) and a model reply that free-associates into "a
  // Trello board" (an artifact of this sandbox's tool-free instruction, not
  // a Tack integration). Both are real, unedited — collapsing this panel is
  // the fix, not blurring or cropping the payload text out from the middle
  // of an otherwise-expanded screenshot (Timeline/Decisions/Artifacts all
  // live under the same `<Show>` in AttemptList.tsx, so no partial expand is
  // possible). Requested-vs-actual model ("Matched request — Ran on
  // anthropic / claude-sonnet-4-5, as requested") and usage marked measured
  // (`Model/token cost $0.04 (measured)`, `Runner time cost — Not
  // measured`) are both already visible in this collapsed summary; the
  // artifact list and the raw event log are not, which is the trade this
  // screenshot makes: a clean, on-topic frame over full disclosure of every
  // expanded panel.
  await screenshotFullContent(page, path.join(OUT_DIR, 'attempt.png'));
  console.log('\n✓ attempt.png saved\n');
});

// ── two-machines.png ─────────────────────────────────────────────────────
test('two-machines screenshot', async ({ page }) => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const claudeBin = execSync('command -v claude').toString().trim();
  const codexBin = execSync('command -v codex').toString().trim();
  const sysPath = '/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin';

  const pathClaudeOnly = fs.mkdtempSync(path.join(os.tmpdir(), 'vi-d2-path-claude-'));
  fs.symlinkSync(claudeBin, path.join(pathClaudeOnly, 'claude'));
  const pathCodexOnly = fs.mkdtempSync(path.join(os.tmpdir(), 'vi-d2-path-codex-'));
  fs.symlinkSync(codexBin, path.join(pathCodexOnly, 'codex'));

  const runnerAState = fs.mkdtempSync(path.join(os.tmpdir(), 'vi-d2-runner-a-state-'));
  const runnerBState = fs.mkdtempSync(path.join(os.tmpdir(), 'vi-d2-runner-b-state-'));
  const tackRunnerBin = process.env.VI_D2_TACK_RUNNER_BIN;
  if (!tackRunnerBin) throw new Error('VI_D2_TACK_RUNNER_BIN env var must point at the release tack-runner binary');

  async function enrollAndStart(
    name: string,
    labels: Record<string, string>,
    pathDir: string,
    stateDir: string,
  ) {
    await page.getByLabel('Name').fill(name);
    await page.waitForTimeout(200);
    await page.getByLabel('Labels (JSON object, optional)').fill(JSON.stringify(labels));
    await page.waitForTimeout(200);
    await page.getByRole('button', { name: 'Enroll' }).click();
    await page.waitForTimeout(700);

    const runnerId = (
      await page
        .locator('xpath=//p[normalize-space(text())="Runner ID"]/following::p[1]')
        .first()
        .textContent()
    )?.trim();
    const token = (
      await page
        .locator('xpath=//p[normalize-space(text())="Enrollment token"]/following::p[1]')
        .first()
        .textContent()
    )?.trim();
    if (!runnerId || !token) throw new Error(`could not read enrollment token for "${name}" from the modal`);

    execSync(
      `sh -c 'nohup env PATH="${pathDir}:${sysPath}" TACK_RUNNER_ID="${runnerId}" ` +
        `TACK_RUNNER_ENROLLMENT_TOKEN="${token}" "${tackRunnerBin}" --api-url "${BASE}" ` +
        `--state-dir "${stateDir}" > "${stateDir}/runner.log" 2>&1 &'`,
      { stdio: 'pipe' },
    );

    await page.getByRole('button', { name: "I've copied it — close" }).click();
    await page.waitForTimeout(400);
    return runnerId;
  }

  await page.goto(`${BASE}/agents`);
  await waitForApp(page);
  await page.getByRole('button', { name: 'Advanced' }).click();
  await page.waitForTimeout(500);

  await enrollAndStart('workstation-claude', { host: 'linux-workstation', harness: 'claude-code' }, pathClaudeOnly, runnerAState);
  await enrollAndStart('cibox-codex', { host: 'ci-container', harness: 'codex' }, pathCodexOnly, runnerBState);

  // Real verification via the API, not this panel: the Advanced panel never
  // reads back connection state, so its own badge stays "Connection
  // unconfirmed" even once both runners are truly active.
  const deadline = Date.now() + 20_000;
  let bothActive = false;
  while (Date.now() < deadline) {
    const runners = (await apiFetch('/runners')) as { data: Array<{ name: string; state: string }> };
    const a = runners.data.find((r) => r.name === 'workstation-claude' && r.state === 'active');
    const b = runners.data.find((r) => r.name === 'cibox-codex' && r.state === 'active');
    if (a && b) {
      bothActive = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  if (!bothActive) throw new Error('the two enrolled runners never both reported active');

  // Let the two "enrolled" toasts auto-dismiss (4s duration, shared/ui/toast.ts)
  // before capturing, rather than a screenshot with transient overlays in it.
  await page.waitForTimeout(4500);

  // Cropped to exactly this section (`EnrollmentPanel.tsx`'s own heading and
  // the two-runner list beneath it), not the whole Advanced panel: the
  // enroll/revoke-by-id forms above and below it add nothing "one board,
  // many runners" needs. `locator.screenshot()` captures exactly this
  // element's own rendered box, not the full page.
  const sessionHeading = page.getByText('Runners enrolled or revoked this session', { exact: true });
  const sessionSection = sessionHeading.locator('xpath=..');
  await sessionSection.screenshot({ path: path.join(OUT_DIR, 'two-machines.png') });
  console.log('\n✓ two-machines.png saved\n');

  try {
    execSync(`pkill -f "${runnerAState}"`, { stdio: 'pipe' });
  } catch {
    // already exited — nothing to kill
  }
  try {
    execSync(`pkill -f "${runnerBState}"`, { stdio: 'pipe' });
  } catch {
    // already exited — nothing to kill
  }
  fs.rmSync(pathClaudeOnly, { recursive: true, force: true });
  fs.rmSync(pathCodexOnly, { recursive: true, force: true });
  fs.rmSync(runnerAState, { recursive: true, force: true });
  fs.rmSync(runnerBState, { recursive: true, force: true });
});

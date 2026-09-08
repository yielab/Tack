import { test, expect } from '@playwright/test';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { waitForApp } from './helpers';

// Records the durable-recovery demo for the README hero asset. Run via
// scripts/record-recovery-demo.sh — it downloads a real GitHub Release
// artifact, runs the board and runner from it in disposable Docker
// containers, and only then invokes this spec through
// playwright.recovery-demo.config.ts (no webServer — this needs the release
// container, not a dev build; running this file directly, or through the
// default config, will skip for missing env).
//
//   ./scripts/record-recovery-demo.sh
//
// This spec is the recorder only: everything it drives (create an item,
// dispatch it, watch the attempt, see the recovery after a kill, requeue,
// see it succeed) happens against that release-artifact server. The kill and
// restart of the runner container are shell commands run from here — not
// clicks — because a container's death is not a browser action; the state
// change they cause is what the recording shows.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, '../../docs/screenshots');
const FRAMES_DIR = path.join(OUT_DIR, '_frames_recovery_demo');

test.use({
  // Wide enough that the board's 5 status columns and the item drawer never
  // both need more width than the viewport at once — narrower than this
  // (tried 1440x960 first) pushes the drawer's left edge off-frame instead
  // of shrinking the columns, clipping "Fix the flaky checkout race" to
  // "e flaky checkout race" in every frame where the drawer is open.
  viewport: { width: 1920, height: 1080 },
  colorScheme: 'light',
  video: { mode: 'on', size: { width: 1920, height: 1080 } },
});

test.setTimeout(300_000);

test.beforeEach(async ({}, testInfo) => {
  try {
    execSync('ffmpeg -version', { stdio: 'pipe' });
  } catch {
    testInfo.skip(true, 'ffmpeg not found — required to convert the recording to a GIF');
  }
  const required = [
    'RECOVERY_DEMO_PROJECT_ID', 'RECOVERY_DEMO_AGENT_PROFILE_ID', 'RECOVERY_DEMO_RUNNER_ID',
    'RECOVERY_DEMO_ENROLL_TOKEN', 'RECOVERY_DEMO_REPO_REMOTE', 'RECOVERY_DEMO_BASE_REVISION',
    'RECOVERY_DEMO_RUNNER_CONTAINER', 'RECOVERY_DEMO_BOARD_CONTAINER', 'RECOVERY_DEMO_NETWORK',
    'RECOVERY_DEMO_RUNNER_BIN_DIR', 'RECOVERY_DEMO_SHIMS_DIR', 'RECOVERY_DEMO_REPO_DIR',
    'RECOVERY_DEMO_SHIMDATA_DIR', 'RECOVERY_DEMO_STATE_DIR',
  ];
  const missing = required.filter((k) => !process.env[k]);
  if (missing.length) {
    testInfo.skip(true, `missing env: ${missing.join(', ')} — run via scripts/record-recovery-demo.sh, not directly`);
  }
});

function restartRunnerContainer() {
  const {
    RECOVERY_DEMO_RUNNER_CONTAINER: name,
    RECOVERY_DEMO_NETWORK: net,
    RECOVERY_DEMO_RUNNER_BIN_DIR: runnerBin,
    RECOVERY_DEMO_SHIMS_DIR: shims,
    RECOVERY_DEMO_REPO_DIR: repo,
    RECOVERY_DEMO_SHIMDATA_DIR: shimdata,
    RECOVERY_DEMO_STATE_DIR: state,
    RECOVERY_DEMO_RUNNER_ID: runnerId,
    RECOVERY_DEMO_ENROLL_TOKEN: token,
    RECOVERY_DEMO_BOARD_CONTAINER: board,
  } = process.env as Record<string, string>;
  execSync(`docker rm -f ${name}`, { stdio: 'pipe' });
  execSync(
    `docker run -d --name ${name} --network ${net} ` +
      `-v "${runnerBin}":/opt/runner:ro -v "${shims}":/opt/shims:ro ` +
      `-v "${repo}":/demo-repo:ro -v "${shimdata}":/shimdata -v "${state}":/data/runner-state ` +
      `-e TACK_RUNNER_ID=${runnerId} -e TACK_RUNNER_ENROLLMENT_TOKEN=${token} ` +
      `-e PATH=/opt/shims:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin ` +
      `--entrypoint /bin/sh alpine:latest -c ` +
      `"apk add --no-cache git >/tmp/apk.log 2>&1; exec /opt/runner/tack-runner --api-url http://${board}:3411 --state-dir /data/runner-state"`,
    { stdio: 'pipe' },
  );
}

async function apiFetch(base: string, p: string, init?: RequestInit) {
  const res = await fetch(`${base}${p}`, init);
  if (!res.ok) throw new Error(`${init?.method ?? 'GET'} ${p} -> ${res.status}: ${await res.text()}`);
  return res.json();
}

// This panel re-renders on a tight poll interval while a request is in
// flight, so a single click (even force: true) can land between "resolved"
// and "detached" and hang on Playwright's own retry forever — seen on both
// the "Confirm requeue" button and the card-open click, at different times,
// not tied to one specific element. Bound each attempt instead of trusting
// one unbounded click; a fresh locator lookup each loop survives a remount
// that an in-flight click's retry does not.
async function clickResilient(
  locator: import('@playwright/test').Locator,
  attempts = 10,
) {
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      await locator.click({ timeout: 4000, force: true });
      return;
    } catch (e) {
      lastErr = e;
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  throw lastErr;
}

// The item drawer's open/closed state survives a page reload (it's in the
// URL), so after the first time this opens it, later reloads land with it
// already open — clicking the board card again then fights the drawer's own
// backdrop for the same click. Check first instead of assuming either state.
async function openExecutionTab(page: import('@playwright/test').Page) {
  await page.waitForTimeout(500); // let a just-finished reload settle first
  const execTab = page.getByText('Execution', { exact: true }).first();
  const alreadyOpen = await execTab.isVisible().catch(() => false);
  if (!alreadyOpen) {
    await clickResilient(page.getByText('Fix the flaky checkout race', { exact: false }).first());
    await page.waitForTimeout(400);
  }
  await clickResilient(execTab);
  await page.waitForTimeout(600);
}

test('recovery demo', async ({ page }) => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.mkdirSync(FRAMES_DIR, { recursive: true });

  const base = process.env.E2E_API_ORIGIN as string;
  const proj = process.env.RECOVERY_DEMO_PROJECT_ID as string;
  const profile = process.env.RECOVERY_DEMO_AGENT_PROFILE_ID as string;
  const runnerId = process.env.RECOVERY_DEMO_RUNNER_ID as string;
  const remote = process.env.RECOVERY_DEMO_REPO_REMOTE as string;
  const rev = process.env.RECOVERY_DEMO_BASE_REVISION as string;
  const shimdataDir = process.env.RECOVERY_DEMO_SHIMDATA_DIR as string;
  const principal = { 'x-tack-principal': 'demo-operator', 'content-type': 'application/json' };

  // ── Scene 1: board, empty, then create the item live ─────────────────────
  await page.goto(`${base}/projects/${proj}/board`);
  await waitForApp(page);
  await expect(page.getByText('Backlog').first()).toBeVisible();
  await page.waitForTimeout(3000);

  await page.getByRole('button', { name: 'New' }).first().click();
  await page.waitForTimeout(300);
  const titleInput = page.getByPlaceholder('What needs to be done?');
  await titleInput.fill('Fix the flaky checkout race');
  await page.waitForTimeout(400);
  await page.getByRole('button', { name: 'Create Item' }).click();
  await page.waitForTimeout(1200);

  const items = (await apiFetch(base, `/api/projects/${proj}/items`)) as { data?: Array<{ id: string; title: string }> } | Array<{ id: string; title: string }>;
  const list = Array.isArray(items) ? items : (items.data ?? []);
  const item = list.find((i) => i.title === 'Fix the flaky checkout race');
  if (!item) throw new Error('created item not found via API — cannot proceed without its id');

  // The board's live-update channel doesn't reliably reach across the
  // container topology this demo runs in (see handoff); reload rather than
  // trust a push update, matching how screenshots.spec.ts treats every
  // state change — a fresh navigation, not a live assertion.
  await page.reload();
  await waitForApp(page);
  await expect(page.getByText('Fix the flaky checkout race', { exact: false }).first()).toBeVisible();
  await page.waitForTimeout(2000);

  // ── Scene 2: dispatch to the enrolled runner (see header: not through the
  // "Run with agent" dialog — Codex/Claude Code declare no model_combinations
  // and the dialog has no free-text override, so neither "Auto" nor "Choose
  // a model" can ever produce a claimable request for them; a real gap, not
  // a recording shortcut) ─────────────────────────────────────────────────
  const body = {
    item_id: item.id,
    idempotency_key: `recovery-demo-${Date.now()}`,
    selector_kind: 'exact_runner',
    selector_id: runnerId,
    agent_profile_id: profile,
    requested_harness_kind: 'codex',
    requested_model_provider: 'fake',
    requested_model_id: 'demo-model',
    agent_profile_snapshot: {
      name: 'demo-profile',
      instructions: 'Print the single word DONE and exit. Do not modify any files.',
      tool_policy: {}, timeout_seconds: 600, budgets: {},
    },
    repository_snapshot: { kind: 'git', remote, base_revision: rev, subdirectory: null },
    permission_policy: { tools: [], network: false },
    budgets: {}, environment: {}, metadata: {}, timeout_seconds: 600,
  };
  const created = await apiFetch(base, '/api/executions', { method: 'POST', headers: principal, body: JSON.stringify(body) });
  const reqId = created.request_id as string;

  // Wait for the attempt to actually be running server-side before opening
  // the panel — the item modal fetches once on open, it does not poll while
  // open (same reason every scene below reloads instead of waiting live).
  await expect
    .poll(async () => {
      const attempts = await apiFetch(base, `/api/executions/${reqId}/attempts`, { headers: principal });
      return attempts.data?.[0]?.state ?? null;
    }, { timeout: 20_000, intervals: [500] })
    .toBe('running');

  // ── Scene 3: open the item, see it Running ────────────────────────────────
  await page.reload();
  await waitForApp(page);
  await openExecutionTab(page);
  await expect(page.getByText('Running', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(4000);

  // ── Scene 4: kill the runner mid-attempt (a container death, not a click) ─
  execSync(`docker kill ${process.env.RECOVERY_DEMO_RUNNER_CONTAINER}`, { stdio: 'pipe' });
  restartRunnerContainer();

  await expect
    .poll(async () => {
      const attempts = await apiFetch(base, `/api/executions/${reqId}/attempts`, { headers: principal });
      return attempts.data?.[0]?.state ?? null;
    }, { timeout: 45_000, intervals: [1000] })
    .toBe('needs_operator');

  // ── Scene 5: reload to show the reconciled state — needs operator, one
  // attempt, no silent duplicate ────────────────────────────────────────────
  await page.reload();
  await waitForApp(page);
  await openExecutionTab(page);
  await expect(page.getByText('Needs operator', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(4500);

  // ── Scene 6: reconcile — an explicit operator decision, not a silent retry ─
  // This whole sequence (open the dialog, fill it, submit) is retried as one
  // unit, not click-by-click: the panel remounts on its own poll cycle (an
  // attempts refetch), which can wipe the dialog's local open/filled state
  // between one step and the next — a step that worked a moment ago is not
  // proof the next one still sees it. Re-running the whole thing survives a
  // remount landing at an inconvenient moment; retrying only the stuck step
  // does not, since the state it depends on (the dialog being open) may
  // itself be gone. No <label for> association exists on these fields
  // (checked by hand), so they're found by their own visible text.
  const recoveryKeyLabel = page.getByText('Recovery key', { exact: false }).first();
  const recoveryKeyInput = page.locator(
    'xpath=//*[contains(normalize-space(text()), "Recovery key")]/following::input[1]',
  );
  const reasonInput = page.locator(
    'xpath=//*[contains(normalize-space(text()), "Reason")]/following::textarea[1] ' +
      '| //*[contains(normalize-space(text()), "Reason")]/following::input[1]',
  );
  const confirmBtn = page.getByRole('button', { name: 'Confirm requeue' });

  let reconciled = false;
  for (let attempt = 0; attempt < 6 && !reconciled; attempt++) {
    for (let i = 0; i < 6; i++) {
      if (await recoveryKeyLabel.isVisible().catch(() => false)) break;
      await page.getByText('Reconcile…', { exact: true }).first()
        .click({ force: true, timeout: 4000 }).catch(() => {});
      await page.waitForTimeout(700);
    }
    if (!(await recoveryKeyLabel.isVisible().catch(() => false))) continue;
    try {
      await recoveryKeyInput.first().fill('recovery-demo-requeue', { timeout: 3000 });
      await reasonInput.first().fill('Operator-confirmed restart recovery (recorded demo)', { timeout: 3000 });
      if (attempt === 0) await page.waitForTimeout(1500); // pacing, worth doing once
      // Let the harness stand-in finish quickly on the retry.
      if (!fs.existsSync(path.join(shimdataDir, 'release'))) {
        fs.writeFileSync(path.join(shimdataDir, 'release'), '');
      }
      await confirmBtn.click({ timeout: 3000, force: true });
      reconciled = true;
    } catch {
      // Panel likely remounted mid-sequence — loop and redo the whole thing.
    }
  }
  if (!reconciled) throw new Error('could not submit the reconcile dialog after repeated attempts');
  await page.waitForTimeout(1500);

  await expect
    .poll(async () => {
      const attempts = await apiFetch(base, `/api/executions/${reqId}/attempts`, { headers: principal });
      const rows = (attempts.data ?? []) as Array<{ state: string }>;
      return rows.some((a) => a.state === 'succeeded');
    }, { timeout: 60_000, intervals: [1000] })
    .toBe(true);

  // ── Scene 7: succeeded ─────────────────────────────────────────────────
  await page.reload();
  await waitForApp(page);
  await openExecutionTab(page);
  await expect(page.getByText('Succeeded', { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(5000);

  // ── Flush the video, convert to GIF ────────────────────────────────────
  await page.close();
  const videoPath = await page.video()!.path();
  const palettePath = path.join(FRAMES_DIR, 'palette.png');
  const gifPath = path.join(OUT_DIR, 'recovery-demo.gif');
  const filter = 'fps=8,scale=1200:-2:flags=lanczos';
  execSync(
    // -update 1: newer ffmpeg's image2 muxer refuses a single still frame
    // without it ("does not contain an image sequence pattern").
    `ffmpeg -y -ss 1.0 -i "${videoPath}" -vf "${filter},palettegen=stats_mode=diff" -update 1 "${palettePath}"`,
    { stdio: 'pipe' },
  );
  execSync(
    `ffmpeg -y -ss 1.0 -i "${videoPath}" -i "${palettePath}" -filter_complex "[0:v] ${filter} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" "${gifPath}"`,
    { stdio: 'pipe' },
  );
  fs.rmSync(FRAMES_DIR, { recursive: true, force: true });
  const sizeMB = (fs.statSync(gifPath).size / 1_048_576).toFixed(1);
  console.log(`\n✓ recovery-demo.gif saved (${sizeMB} MB) -> ${gifPath}\n`);
});

import { test, expect, waitForApp } from './helpers';

// ADR 0061 decision 6 — a UI-only user turns the embedded runner on/off,
// with no restart. The e2e webServer runs a plain `cargo run -p tack-cli --
// serve` (no `--with-runner`) on loopback (`playwright.config.ts`), so
// `/api/local-runner` is genuinely mounted and this exercises the real
// `EmbeddedRunnerControl` lifecycle end to end, not a mock.
//
// This is the whole server-wide switch `provider-key-panel.spec.ts` and
// `agents-page.spec.ts`'s first test also drive — `executionToggleLock`
// (`./helpers.ts`) keeps this test's own on/off round trip from racing
// either of theirs under full parallel load. See that fixture's own doc
// comment before removing it.

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

test('turning agent execution on and off round-trips the observed state', async ({ page, executionToggleLock }) => {
  await page.goto('/agents');
  await waitForApp(page);

  await expect(page.getByRole('heading', { name: 'Agent execution on this machine' })).toBeVisible();

  const toggleButton = page.getByRole('button', { name: /Turn (on|off)/ });
  await expect(toggleButton).toBeVisible();

  // Start from a known state: if a previous run left it on, turn it off first.
  if ((await toggleButton.textContent())?.includes('Turn off')) {
    await toggleButton.click();
    await expect(page.getByText('Stopped', { exact: true })).toBeVisible();
  }

  await expect(toggleButton).toHaveText('Turn on');
  await toggleButton.click();

  // A real runner self-provisions and starts — the badge and the button
  // label both flip once the server actually reports it running.
  await expect(page.getByText('Running', { exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole('button', { name: 'Turn off' })).toBeVisible();

  await page.getByRole('button', { name: 'Turn off' }).click();
  await expect(page.getByText('Stopped', { exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole('button', { name: 'Turn on' })).toBeVisible();
});

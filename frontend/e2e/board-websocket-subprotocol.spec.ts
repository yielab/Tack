import { test, expect } from '@playwright/test';
import { getOrCreateProject } from './helpers';

// A raw handshake reader (curl, or a Rust integration test that opens a TCP
// socket) only ever checks the status line: RFC 6455 §4.1's rule about a
// missing `Sec-WebSocket-Protocol` response header is enforced by the
// client's own WebSocket implementation, not by anything server-visible in
// the handshake response line. Only a real browser can prove the client side
// of that rule, so this spec drives one — connecting exactly the way the
// app's own `shared/realtime/boardSocket.ts` does (same-origin, through the
// dev proxy, offering `tack.v1`) — and asserts it both stays open and
// receives a live event, rather than asserting anything about the HTTP
// status code.
test.skip(
  ({ browserName }) => browserName !== 'chromium',
  'the defect is in a server response header a spec-compliant client enforces identically; one real engine is enough'
);

test('a browser WebSocket offering tack.v1 stays connected and receives a live board event', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);

  // A real page (not `about:blank`, whose `location.host` is empty) so the
  // WebSocket URL below resolves same-origin through the dev proxy exactly
  // the way the app's own `boardLiveUrl()` does — no app UI interaction is
  // needed beyond that.
  await page.goto('/');

  await page.evaluate(() => {
    (window as unknown as { __wsEvents: string[] }).__wsEvents = [];
    (window as unknown as { __wsState: string }).__wsState = 'connecting';
  });

  await page.evaluate(({ projectId }) => {
    const url = `ws://${location.host}/api/projects/${projectId}/boards/live`;
    const ws = new WebSocket(url, ['tack.v1']);
    (window as unknown as { __ws: WebSocket }).__ws = ws;
    ws.onopen = () => {
      (window as unknown as { __wsState: string }).__wsState = 'open';
    };
    ws.onclose = (ev) => {
      (window as unknown as { __wsState: string }).__wsState = 'closed';
      (window as unknown as { __wsCloseCode: number }).__wsCloseCode = ev.code;
    };
    ws.onmessage = (ev: MessageEvent) => {
      (window as unknown as { __wsEvents: string[] }).__wsEvents.push(ev.data as string);
    };
  }, { projectId });

  // Against the unfixed server the handshake response carries no
  // Sec-WebSocket-Protocol header; a spec-compliant browser fails the
  // connection per RFC 6455 §4.1 and `onopen` never fires — `__wsState`
  // stays `connecting` (or reaches `closed` first) and this assertion times
  // out and fails, which is the point: it fails against the current server.
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { __wsState: string }).__wsState), {
      timeout: 5_000,
    })
    .toBe('open');

  const created = await request.post(`${process.env.E2E_API_ORIGIN}/api/projects/${projectId}/items`, {
    data: { title: 'ws subprotocol probe', item_type: 'task' },
  });
  expect(created.ok(), `create item failed: ${created.status()}`).toBeTruthy();
  const item = await created.json();
  const itemId: string = item.id ?? item.item?.id;
  expect(itemId, 'created item must carry an id').toBeTruthy();

  await expect
    .poll(
      () => page.evaluate(() => (window as unknown as { __wsEvents: string[] }).__wsEvents.length),
      { timeout: 5_000 }
    )
    .toBeGreaterThan(0);

  const events = await page.evaluate(() =>
    (window as unknown as { __wsEvents: string[] }).__wsEvents.map((raw) => JSON.parse(raw))
  );
  expect(
    events.some((event) => event.type === 'item_created' && event.item_id === itemId),
    `expected an item_created event for ${itemId}, got: ${JSON.stringify(events)}`
  ).toBeTruthy();

  await page.evaluate(() => (window as unknown as { __ws: WebSocket }).__ws.close());
});

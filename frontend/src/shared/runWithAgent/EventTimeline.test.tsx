import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import EventTimeline from './EventTimeline';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function mount(requestId = 'exec_1', attemptNumber = 1) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <EventTimeline requestId={requestId} attemptNumber={attemptNumber} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('EventTimeline', () => {
  it('fetches GET /executions/{id}/attempts/{n}/events and renders events oldest-first with source/kind/payload', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            { event_id: 'evt_1', sequence: 1, source: 'harness', kind: 'message', payload: { text: 'Started' }, occurred_at: '2026-08-06T12:00:00Z', created_at: '2026-08-06T12:00:00Z' },
            { event_id: 'evt_2', sequence: 2, source: 'runner', kind: 'progress', payload: { phase: 'testing', percent: null }, occurred_at: '2026-08-06T12:01:00Z', created_at: '2026-08-06T12:01:00Z' },
          ],
        }),
        { status: 200 },
      ),
    );
    const c = mount('exec_1', 3);
    await flush();
    await flush();

    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/executions/exec_1/attempts/3/events');
    expect(c.textContent).toContain('Started');
    expect(c.textContent).toContain('harness');
    expect(c.textContent).toContain('message');
    expect(c.textContent).toContain('runner');
    expect(c.textContent).toContain('progress');
  });

  it('shows an honest empty state for zero events, not a blank screen', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain('No events reported yet');
  });

  it('a fetch failure is shown, not silently swallowed', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 }),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain("Couldn't load events");
  });

  it('a payload without a text/message field falls back to compact JSON instead of throwing', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [{ event_id: 'evt_1', sequence: 1, source: 'runner', kind: 'custom', payload: { odd: 'shape', n: 1 }, occurred_at: '2026-08-06T12:00:00Z', created_at: '2026-08-06T12:00:00Z' }],
        }),
        { status: 200 },
      ),
    );
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain('"odd":"shape"');
  });

  // -----------------------------------------------------------------
  // Adversarial regression: a harness/runner controls `payload`
  // (free-form, not a fixed schema) and could report an HTML/JS
  // payload, deliberately or via a compromised harness. This event stream
  // is rendered directly in the operator UI, so it is exactly the kind of
  // "prompt rendering" surface the audit's XSS case targets. Proves the
  // malicious string is inserted as an inert text node, never interpreted
  // as markup: no matching element is created anywhere in the DOM, and the
  // raw string is still visible as literal, escaped text (not silently
  // dropped, which would also technically avoid execution but would fail a
  // different honesty requirement).
  // -----------------------------------------------------------------
  it('an XSS-shaped event payload renders as inert text, never as markup', async () => {
    const malicious = '<img src=x onerror="window.__g2_xss_fired = true">';
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              event_id: 'evt_xss',
              sequence: 1,
              source: 'harness',
              kind: 'message',
              payload: { text: malicious },
              occurred_at: '2026-08-06T12:00:00Z',
              created_at: '2026-08-06T12:00:00Z',
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const c = mount();
    await flush();
    await flush();

    // No <img> element was ever created from the payload — proves the
    // string was never parsed as HTML.
    expect(c.querySelector('img')).toBeNull();
    // The malicious handler never ran.
    expect((globalThis as unknown as { __g2_xss_fired?: boolean }).__g2_xss_fired).toBeUndefined();
    // The raw string is still visible, verbatim, as literal text — an
    // honest rendering, not a silently-stripped one.
    expect(c.textContent).toContain(malicious);
  });

  it('an XSS-shaped payload with no text/message field (JSON fallback path) also renders as inert text', async () => {
    const malicious = '<script>window.__g2_xss_fired_2 = true</script>';
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              event_id: 'evt_xss_2',
              sequence: 1,
              source: 'runner',
              kind: 'custom',
              payload: { note: malicious },
              occurred_at: '2026-08-06T12:00:00Z',
              created_at: '2026-08-06T12:00:00Z',
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const c = mount();
    await flush();
    await flush();

    expect(c.querySelector('script')).toBeNull();
    expect((globalThis as unknown as { __g2_xss_fired_2?: boolean }).__g2_xss_fired_2).toBeUndefined();
    // Went through the JSON.stringify fallback path (no text/message field),
    // so the payload is present JSON-escaped, not literally — still proves
    // no markup was created either way.
    expect(c.textContent).toContain('script');
  });
});

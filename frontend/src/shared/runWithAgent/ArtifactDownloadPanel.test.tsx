import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { ArtifactRecord } from '../execution';
import ArtifactDownloadPanel from './ArtifactDownloadPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const VERIFIED: ArtifactRecord = {
  artifact_id: 'art_1',
  kind: 'diff',
  name: 'patch.diff',
  media_type: 'text/plain',
  size_bytes: 1234,
  content_verified: true,
  created_at: '2026-08-06T12:00:00Z',
};

const UNVERIFIED: ArtifactRecord = {
  ...VERIFIED,
  artifact_id: 'art_2',
  name: 'log.txt',
  content_verified: false,
};

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ArtifactDownloadPanel requestId="exec_1" attemptNumber={2} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function jsonOk(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200 });
}

function jsonError(status: number, code: string, message: string) {
  return new Response(JSON.stringify({ error: { status, message, code } }), { status });
}

/** Routes a mocked `fetch` by URL shape: the list call
 *  (`.../artifacts`) vs. a per-artifact content download
 *  (`.../artifacts/{id}/content`) — the two GETs this panel makes. */
function mockFetch(list: ArtifactRecord[], download: () => Response) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    if (url.endsWith('/artifacts')) {
      return Promise.resolve(jsonOk({ protocol_version: 1, data: list }));
    }
    return Promise.resolve(download());
  });
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('ArtifactDownloadPanel — discovers artifacts, never asks for an id', () => {
  it('renders every listed artifact as a real row with a Download button, no id field anywhere', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.querySelector('input')).toBeNull();
    expect(c.textContent).toContain('patch.diff');
    expect(c.textContent).toContain('log.txt');
    expect(c.querySelectorAll('button')).toHaveLength(2);
  });

  it('an unverified artifact is visibly marked distinct from a verified one', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Not verified yet');
    const rows = [...c.querySelectorAll('li')];
    const unverifiedRow = rows.find((li) => li.textContent?.includes('log.txt'));
    const verifiedRow = rows.find((li) => li.textContent?.includes('patch.diff'));
    expect(unverifiedRow?.textContent).toContain('Not verified yet');
    expect(verifiedRow?.textContent).not.toContain('Not verified yet');
  });

  it('an empty list renders a real empty state, not a silently blank panel', async () => {
    mockFetch([], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('No artifacts yet');
  });
});

describe('ArtifactDownloadPanel — every download outcome is a distinct, visible state (artifact failure stays visible)', () => {
  it('clicking Download calls GET .../artifacts/{artifact_id}/content for that exact row and shows "Downloaded." on success', async () => {
    const fetchMock = mockFetch([VERIFIED], () => new Response('artifact bytes', { status: 200 }));
    const c = mount();
    await flush();
    await flush();

    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    const downloadCall = fetchMock.mock.calls.find((call) => String(call[0]).includes('/content'));
    expect(String(downloadCall?.[0])).toBe('/api/executions/exec_1/attempts/2/artifacts/art_1/content');
    expect(c.textContent).toContain('Downloaded.');
  });

  it('a 404 shows "No artifact with that id exists" — distinct from the 409 message', async () => {
    mockFetch([VERIFIED], () => jsonError(404, 'not_found', 'Artifact not found'));
    const c = mount();
    await flush();
    await flush();

    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('No artifact with that id exists for this attempt.');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });

  it('a 409 shows the "content has not been verified yet" state — distinct from 404, and framed as retryable', async () => {
    mockFetch([UNVERIFIED], () => jsonError(409, 'conflict', 'not verified'));
    const c = mount();
    await flush();
    await flush();

    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain("hasn't been verified yet");
    expect(c.textContent).not.toContain('No artifact with that id exists');
  });

  it('a generic 500 shows a distinct error message rather than either the 404 or 409 text', async () => {
    mockFetch([VERIFIED], () => jsonError(500, 'internal_error', 'server exploded'));
    const c = mount();
    await flush();
    await flush();

    (c.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await flush();

    expect(c.textContent).toContain('server exploded');
    expect(c.textContent).not.toContain('No artifact with that id exists');
    expect(c.textContent).not.toContain("hasn't been verified yet");
  });
});

describe('ArtifactDownloadPanel — keyboard accessibility', () => {
  it('every Download button is a real, natively-focusable form control with no negative tabindex', async () => {
    mockFetch([VERIFIED, UNVERIFIED], () => new Response('bytes'));
    const c = mount();
    await flush();
    await flush();

    const buttons = [...c.querySelectorAll('button')];
    expect(buttons.length).toBeGreaterThan(0);
    for (const btn of buttons) {
      const tabindex = btn.getAttribute('tabindex');
      if (tabindex !== null) expect(Number(tabindex)).toBeGreaterThanOrEqual(0);
    }
    buttons[0].focus();
    expect(document.activeElement).toBe(buttons[0]);
  });
});

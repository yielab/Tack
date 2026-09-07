import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import FleetsPanel from './FleetsPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <FleetsPanel />, container);
  return { container, dispose };
}

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200 });
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('FleetsPanel', () => {
  it('shows an empty state when no fleets exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No fleets yet');
  });

  it('lists a fleet with its concurrency cap and an empty roster', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ fleet_id: 'fleet_1', name: 'backend-fleet', concurrency_limit: 3, default_policy: {} }],
          }),
        );
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        return Promise.resolve(jsonResponse({ protocol_version: 1, data: [] }));
      }
      return Promise.resolve(jsonResponse({}));
    });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('backend-fleet');
    expect(container.textContent).toContain('cap 3');
    expect(container.textContent).toContain('No members yet.');
    expect(container.textContent).toContain('No other runners available to add.');
  });

  it('shows "no concurrency cap" rather than a bare 0 or blank for a null limit', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ fleet_id: 'fleet_1', name: 'unbounded', concurrency_limit: null, default_policy: {} }],
          }),
        );
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        return Promise.resolve(jsonResponse({ protocol_version: 1, data: [] }));
      }
      return Promise.resolve(jsonResponse({}));
    });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('no concurrency cap');
  });

  it('creates a fleet via the form and shows it immediately', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(jsonResponse({ protocol_version: 1, data: [] }));
      }
      if (url.endsWith('/api/runner-fleets') && method === 'POST') {
        return Promise.resolve(jsonResponse({ protocol_version: 1, fleet_id: 'fleet_new', name: 'new-fleet' }));
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        return Promise.resolve(jsonResponse({ protocol_version: 1, data: [] }));
      }
      return Promise.resolve(jsonResponse({}));
    });

    const { container } = mount();
    await flush();

    const showFormBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Create fleet'),
    )!;
    showFormBtn.click();
    await flush();

    const nameInput = container.querySelector<HTMLInputElement>('input[placeholder="backend-fleet"]')!;
    nameInput.value = 'new-fleet';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));

    const submitBtn = Array.from(container.querySelectorAll('button[type="submit"]')).find((b) =>
      b.textContent?.includes('Create'),
    )!;
    submitBtn.click();
    await flush();

    expect(container.textContent).toContain('new-fleet');
    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/runner-fleets') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(postCall).toBeTruthy();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({ name: 'new-fleet', concurrency_limit: null, default_policy: {} });
  });

  it('derives the roster from fleet_ids: a runner in two fleets appears under both', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [
              { fleet_id: 'fleet_1', name: 'backend-fleet', concurrency_limit: null, default_policy: {} },
              { fleet_id: 'fleet_2', name: 'frontend-fleet', concurrency_limit: null, default_policy: {} },
            ],
          }),
        );
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [
              { runner_id: 'runr_shared', name: 'shared-box', state: 'active', fleet_ids: ['fleet_1', 'fleet_2'] },
            ],
          }),
        );
      }
      return Promise.resolve(jsonResponse({}));
    });

    const { container } = mount();
    await flush();

    const fleetItems = Array.from(container.querySelectorAll('ul.space-y-2 > li'));
    expect(fleetItems).toHaveLength(2);
    for (const item of fleetItems) {
      expect(item.textContent).toContain('shared-box');
      expect(item.textContent).toContain('Active');
    }
  });

  it('add calls POST /api/runner-fleets/{fleet_id}/members with the runner id and refetches the roster', async () => {
    let runnersCallCount = 0;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ fleet_id: 'fleet_1', name: 'backend-fleet', concurrency_limit: null, default_policy: {} }],
          }),
        );
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        runnersCallCount += 1;
        const fleet_ids = runnersCallCount === 1 ? [] : ['fleet_1'];
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ runner_id: 'runr_2', name: 'box-2', state: 'active', fleet_ids }],
          }),
        );
      }
      if (url.endsWith('/api/runner-fleets/fleet_1/members') && method === 'POST') {
        return Promise.resolve(
          jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', runner_id: 'runr_2', state: 'added' }),
        );
      }
      return Promise.resolve(jsonResponse({}));
    });

    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No members yet.');

    const select = container.querySelector<HTMLSelectElement>('select')!;
    select.value = 'runr_2';
    select.dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    const addBtn = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Add')!;
    addBtn.click();
    await flush();

    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/runner-fleets/fleet_1/members') && (c[1] as RequestInit)?.method === 'POST',
    );
    expect(postCall).toBeTruthy();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({ runner_id: 'runr_2' });

    expect(runnersCallCount).toBeGreaterThanOrEqual(2);
    expect(container.textContent).not.toContain('No members yet.');
    expect(container.textContent).toContain('box-2');
  });

  it('remove calls DELETE /api/runner-fleets/{fleet_id}/members/{runner_id} and refetches the roster', async () => {
    let runnersCallCount = 0;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ fleet_id: 'fleet_1', name: 'backend-fleet', concurrency_limit: null, default_policy: {} }],
          }),
        );
      }
      if (url.endsWith('/api/runners') && method === 'GET') {
        runnersCallCount += 1;
        const fleet_ids = runnersCallCount === 1 ? ['fleet_1'] : [];
        return Promise.resolve(
          jsonResponse({
            protocol_version: 1,
            data: [{ runner_id: 'runr_3', name: 'box-3', state: 'active', fleet_ids }],
          }),
        );
      }
      if (url.endsWith('/api/runner-fleets/fleet_1/members/runr_3') && method === 'DELETE') {
        return Promise.resolve(
          jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', runner_id: 'runr_3', state: 'removed' }),
        );
      }
      return Promise.resolve(jsonResponse({}));
    });

    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('box-3');

    const removeBtn = Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Remove')!;
    removeBtn.click();
    await flush();

    const deleteCall = fetchMock.mock.calls.find(
      (c) =>
        String(c[0]).endsWith('/api/runner-fleets/fleet_1/members/runr_3') &&
        (c[1] as RequestInit)?.method === 'DELETE',
    );
    expect(deleteCall).toBeTruthy();

    expect(runnersCallCount).toBeGreaterThanOrEqual(2);
    expect(container.textContent).toContain('No members yet.');
  });
});

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

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('FleetsPanel', () => {
  it('shows an empty state when no fleets exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No fleets yet');
  });

  it('lists a fleet with its concurrency cap and states the membership gap explicitly', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [{ fleet_id: 'fleet_1', name: 'backend-fleet', concurrency_limit: 3, default_policy: {} }],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('backend-fleet');
    expect(container.textContent).toContain('cap 3');
    expect(container.textContent).toMatch(/Membership isn't readable/);
  });

  it('shows "no concurrency cap" rather than a bare 0 or blank for a null limit', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [{ fleet_id: 'fleet_1', name: 'unbounded', concurrency_limit: null, default_policy: {} }],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('no concurrency cap');
  });

  it('creates a fleet via the form and shows it immediately', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/runner-fleets') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
      }
      if (url.endsWith('/api/runner-fleets') && method === 'POST') {
        return Promise.resolve(
          new Response(JSON.stringify({ protocol_version: 1, fleet_id: 'fleet_new', name: 'new-fleet' }), {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
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
});

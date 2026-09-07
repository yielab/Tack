import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ExecutionToggle from './ExecutionToggle';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ExecutionToggle />, container);
  return { container, dispose };
}

const STOPPED_STATUS = {
  enabled: false,
  state: 'stopped',
  since: null,
  catalog: { status: 'not_configured' },
};

function mockFetch(initial: Record<string, unknown>, opts: { status?: number } = {}) {
  let current = { ...initial };
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit | undefined)?.method ?? 'GET';
    if (url.endsWith('/api/local-runner') && method === 'GET') {
      return Promise.resolve(new Response(JSON.stringify(current), { status: opts.status ?? 200 }));
    }
    if (url.endsWith('/api/local-runner') && method === 'PUT') {
      const body = JSON.parse((init as RequestInit).body as string);
      current = { ...current, enabled: body.enabled, state: body.enabled ? 'running' : 'stopped' };
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ExecutionToggle', () => {
  it('renders the Stopped badge and a Turn on button when off', async () => {
    mockFetch(STOPPED_STATUS);
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain('Stopped');
    expect(container.textContent).toContain('Turn on');
  });

  it('turning it on PUTs enabled:true and reflects the new state', async () => {
    mockFetch(STOPPED_STATUS);
    const { container } = mount();
    await flush();

    const button = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Turn on'),
    );
    expect(button).toBeTruthy();
    button!.click();
    await flush();
    await flush();

    expect(container.textContent).toContain('Running');
    expect(container.textContent).toContain('Turn off');
  });

  it('renders the remote-runner explanation instead of an error on a genuine 404', async () => {
    mockFetch(STOPPED_STATUS, { status: 404 });
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain('remote-runner deployment');
    expect(container.textContent).not.toContain('Turn on');
  });

  it('renders a retry affordance on a real load failure (500)', async () => {
    mockFetch(STOPPED_STATUS, { status: 500 });
    const { container } = mount();
    await flush();

    expect(container.textContent).toContain("Couldn't load");
    expect(container.textContent).not.toContain('remote-runner deployment');
  });
});

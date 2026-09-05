import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ModelProfilesPanel from './ModelProfilesPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <ModelProfilesPanel />, container);
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ModelProfilesPanel', () => {
  it('shows an empty state when no profiles exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No model profiles yet');
  });

  it('lists a profile with provider/model and an enabled badge', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              model_profile_id: 'mp_1',
              name: 'sonnet-default',
              model_provider: 'anthropic',
              model_id: 'claude-sonnet-5',
              config_reference: null,
              enabled: true,
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('sonnet-default');
    expect(container.textContent).toContain('anthropic / claude-sonnet-5');
    expect(container.textContent).toContain('enabled');
  });

  it('shows a "disabled" badge distinctly from "enabled"', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              model_profile_id: 'mp_1',
              name: 'retired',
              model_provider: 'openai',
              model_id: 'gpt-x',
              config_reference: null,
              enabled: false,
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('disabled');
  });

  it('creates a model profile via the form', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/model-profiles') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
      }
      if (url.endsWith('/api/model-profiles') && method === 'POST') {
        return Promise.resolve(
          new Response(
            JSON.stringify({
              protocol_version: 1,
              model_profile_id: 'mp_new',
              name: 'haiku-fast',
              model_provider: 'anthropic',
              model_id: 'claude-haiku-5',
            }),
            { status: 200 },
          ),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    const showFormBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Create model profile'),
    )!;
    showFormBtn.click();
    await flush();

    const nameInput = container.querySelector<HTMLInputElement>('input[placeholder="sonnet-default"]')!;
    nameInput.value = 'haiku-fast';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const providerInput = container.querySelector<HTMLInputElement>('input[placeholder="anthropic"]')!;
    providerInput.value = 'anthropic';
    providerInput.dispatchEvent(new Event('input', { bubbles: true }));
    const modelIdInput = container.querySelector<HTMLInputElement>('input[placeholder="claude-sonnet-5"]')!;
    modelIdInput.value = 'claude-haiku-5';
    modelIdInput.dispatchEvent(new Event('input', { bubbles: true }));

    const submitBtn = Array.from(container.querySelectorAll('button[type="submit"]')).find((b) =>
      b.textContent?.includes('Create'),
    )!;
    submitBtn.click();
    await flush();

    expect(container.textContent).toContain('haiku-fast');
    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/model-profiles') && (c[1] as RequestInit)?.method === 'POST',
    );
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({
      name: 'haiku-fast',
      model_provider: 'anthropic',
      model_id: 'claude-haiku-5',
      config_reference: null,
    });
  });
});

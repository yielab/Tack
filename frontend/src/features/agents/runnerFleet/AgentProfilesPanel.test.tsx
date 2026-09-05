import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentProfilesPanel from './AgentProfilesPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AgentProfilesPanel />, container);
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('AgentProfilesPanel', () => {
  it('shows an empty state when no profiles exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No agent profiles yet');
  });

  it('lists a profile with its instructions', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              agent_profile_id: 'ap_1',
              name: 'reviewer',
              instructions: 'Review the diff.',
              tool_policy: {},
              limits: {},
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('reviewer');
    expect(container.textContent).toContain('Review the diff.');
  });

  it('creates a profile via the form, sending well-formed JSON policy/limits', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/agent-profiles') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
      }
      if (url.endsWith('/api/agent-profiles') && method === 'POST') {
        return Promise.resolve(
          new Response(JSON.stringify({ protocol_version: 1, agent_profile_id: 'ap_new', name: 'builder' }), {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    const showFormBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Create agent profile'),
    )!;
    showFormBtn.click();
    await flush();

    const nameInput = container.querySelector<HTMLInputElement>('input[placeholder="reviewer"]')!;
    nameInput.value = 'builder';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const instructionsInput = container.querySelector<HTMLInputElement>(
      'input[placeholder="Review the diff for correctness and style."]',
    )!;
    instructionsInput.value = 'Build the feature.';
    instructionsInput.dispatchEvent(new Event('input', { bubbles: true }));

    const submitBtn = Array.from(container.querySelectorAll('button[type="submit"]')).find((b) =>
      b.textContent?.includes('Create'),
    )!;
    submitBtn.click();
    await flush();

    expect(container.textContent).toContain('builder');
    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/agent-profiles') && (c[1] as RequestInit)?.method === 'POST',
    );
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({ name: 'builder', instructions: 'Build the feature.', tool_policy: {}, limits: {} });
  });

  it('rejects an invalid tool_policy JSON body before calling the API', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    const showFormBtn = Array.from(container.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Create agent profile'),
    )!;
    showFormBtn.click();
    await flush();

    const nameInput = container.querySelector<HTMLInputElement>('input[placeholder="reviewer"]')!;
    nameInput.value = 'builder';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    const instructionsInput = container.querySelector<HTMLInputElement>(
      'input[placeholder="Review the diff for correctness and style."]',
    )!;
    instructionsInput.value = 'do it';
    instructionsInput.dispatchEvent(new Event('input', { bubbles: true }));
    const policyInput = container.querySelector<HTMLInputElement>('input[placeholder="{}"]')!;
    policyInput.value = 'not json';
    policyInput.dispatchEvent(new Event('input', { bubbles: true }));

    const submitBtn = Array.from(container.querySelectorAll('button[type="submit"]')).find((b) =>
      b.textContent?.includes('Create'),
    )!;
    submitBtn.click();
    await flush();

    expect(fetchMock.mock.calls.some((c) => (c[1] as RequestInit | undefined)?.method === 'POST')).toBe(false);
  });
});

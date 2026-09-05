import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import RunnerFleetSection from './RunnerFleetSection';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <RunnerFleetSection />, container);
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('RunnerFleetSection — tabs', () => {
  it('defaults to the Runners tab (enrollment panel), firing no network request on mount', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 200 }));
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Enroll a runner');
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('is an accessible tablist: role=tablist, one aria-selected=true tab, others false', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    const tablist = container.querySelector('[role="tablist"]')!;
    expect(tablist).toBeTruthy();
    const tabs = Array.from(container.querySelectorAll('[role="tab"]'));
    expect(tabs).toHaveLength(4);
    const selected = tabs.filter((t) => t.getAttribute('aria-selected') === 'true');
    expect(selected).toHaveLength(1);
    expect(selected[0].textContent).toBe('Runners');
  });

  it('switches to the Fleets tab on click and loads fleets', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({ protocol_version: 1, data: [{ fleet_id: 'f1', name: 'my-fleet', concurrency_limit: null, default_policy: {} }] }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();

    const fleetsTab = Array.from(container.querySelectorAll('[role="tab"]')).find((t) => t.textContent === 'Fleets')!;
    (fleetsTab as HTMLButtonElement).click();
    await flush();

    expect(container.textContent).toContain('my-fleet');
  });

  it('moves focus between tabs with the arrow keys (keyboard navigation)', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();

    const tablist = container.querySelector('[role="tablist"]')! as HTMLElement;
    const tabs = Array.from(container.querySelectorAll('[role="tab"]'));
    expect(tabs[0].getAttribute('aria-selected')).toBe('true');

    tablist.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    await flush();

    const tabsAfter = Array.from(container.querySelectorAll('[role="tab"]'));
    expect(tabsAfter[1].getAttribute('aria-selected')).toBe('true');
    expect(tabsAfter[0].getAttribute('aria-selected')).toBe('false');
  });
});

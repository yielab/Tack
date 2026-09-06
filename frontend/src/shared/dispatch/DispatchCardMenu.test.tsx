import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import DispatchCardMenu from './DispatchCardMenu';

const flush = () => new Promise((r) => setTimeout(r, 0));

const disposers: Array<() => void> = [];

function mount(props: { available: boolean; onDispatched?: () => void }) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => <DispatchCardMenu itemId="item-1" itemTitle="Fix login bug" {...props} />,
    container,
  );
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

describe('DispatchCardMenu', () => {
  it('renders no trigger at all when unavailable — no dispatch controls when orchestration is off', () => {
    const c = mount({ available: false });
    expect(c.querySelector('button')).toBeNull();
  });

  it('renders a trigger when available, closed by default', () => {
    const c = mount({ available: true });
    const trigger = c.querySelector('button[aria-haspopup="menu"]');
    expect(trigger).toBeTruthy();
    expect(trigger?.getAttribute('aria-expanded')).toBe('false');
    expect(c.querySelector('[role="menu"]')).toBeNull();
  });

  it('opens the menu on click, revealing "Dispatch to agents"', async () => {
    const c = mount({ available: true });
    (c.querySelector('button[aria-haspopup="menu"]') as HTMLButtonElement).click();
    await flush();
    expect(c.querySelector('[role="menu"]')).toBeTruthy();
    expect(c.textContent).toContain('Dispatch to agents');
  });

  it('closes on Escape', async () => {
    const c = mount({ available: true });
    (c.querySelector('button[aria-haspopup="menu"]') as HTMLButtonElement).click();
    await flush();
    expect(c.querySelector('[role="menu"]')).toBeTruthy();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flush();
    expect(c.querySelector('[role="menu"]')).toBeNull();
  });

  it('closes on an outside click', async () => {
    const c = mount({ available: true });
    (c.querySelector('button[aria-haspopup="menu"]') as HTMLButtonElement).click();
    await flush();
    expect(c.querySelector('[role="menu"]')).toBeTruthy();
    document.body.click();
    await flush();
    expect(c.querySelector('[role="menu"]')).toBeNull();
  });

  it('dispatching calls POST /items/{id}/dispatch, closes the menu, and calls onDispatched', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ outcome: 'dispatched', task: null }), { status: 200 }));
    const onDispatched = vi.fn();

    const c = mount({ available: true, onDispatched });
    (c.querySelector('button[aria-haspopup="menu"]') as HTMLButtonElement).click();
    await flush();
    (c.querySelector('[role="menuitem"]') as HTMLButtonElement).click();
    await flush();

    expect(fetchMock.mock.calls.some((call) => String(call[0]).endsWith('/api/items/item-1/dispatch'))).toBe(true);
    expect(c.querySelector('[role="menu"]')).toBeNull();
    expect(onDispatched).toHaveBeenCalledTimes(1);
  });

  it('a failed dispatch still calls onDispatched (finally) and does not throw', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { message: 'boom' } }), { status: 500 }),
    );
    const onDispatched = vi.fn();

    const c = mount({ available: true, onDispatched });
    (c.querySelector('button[aria-haspopup="menu"]') as HTMLButtonElement).click();
    await flush();
    (c.querySelector('[role="menuitem"]') as HTMLButtonElement).click();
    await flush();

    expect(onDispatched).toHaveBeenCalledTimes(1);
  });
});

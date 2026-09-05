import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import FirstRunBanner from './FirstRunBanner';

const flush = () => new Promise((r) => setTimeout(r, 0));

function mockRunners(data: unknown[]) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data }), { status: 200 })),
  );
}

function mount(hasItems: boolean) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <FirstRunBanner hasItems={hasItems} />} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('FirstRunBanner', () => {
  it('shows when the board has items and no runner is active anywhere observable', async () => {
    mockRunners([]);
    const { container } = mount(true);
    await flush();
    await flush();
    expect(container.textContent).toContain('This board can run its items with an agent.');
  });

  it('stays hidden on an empty board even with no active runner', async () => {
    mockRunners([]);
    const { container } = mount(false);
    await flush();
    await flush();
    expect(container.textContent ?? '').not.toContain('This board can run its items with an agent.');
  });

  it('stays hidden once any runner reports active, regardless of name', async () => {
    mockRunners([{ runner_id: 'r1', name: 'remote-box', state: 'active' }]);
    const { container } = mount(true);
    await flush();
    await flush();
    expect(container.textContent ?? '').not.toContain('This board can run its items with an agent.');
  });

  it('dismissal is remembered per browser and survives a remount', async () => {
    mockRunners([]);
    const first = mount(true);
    await flush();
    await flush();
    const dismissButton = Array.from(first.container.querySelectorAll('button')).find((b) => b.textContent === 'Dismiss')!;
    dismissButton.click();
    await flush();
    expect(first.container.textContent ?? '').not.toContain('This board can run its items with an agent.');
    first.dispose();
    document.body.innerHTML = '';

    const second = mount(true);
    await flush();
    await flush();
    expect(second.container.textContent ?? '').not.toContain('This board can run its items with an agent.');
  });
});

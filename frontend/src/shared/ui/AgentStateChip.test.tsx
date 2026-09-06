import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentStateChip, { AGENT_STATE_LABEL, AGENT_STATE_TONE, type AgentChipState } from './AgentStateChip';

const disposers: Array<() => void> = [];

function mount(state: AgentChipState, title?: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AgentStateChip state={state} title={title} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('AgentStateChip', () => {
  it('covers exactly 5 states, each with a distinct tone', () => {
    const states = Object.keys(AGENT_STATE_LABEL) as AgentChipState[];
    expect(states.sort()).toEqual(['done', 'failed', 'queued', 'running', 'waiting_approval'].sort());
    const tones = new Set(Object.values(AGENT_STATE_TONE));
    expect(tones.size).toBe(5);
  });

  it('renders a text label for every state, never color alone (WCAG 1.4.1)', () => {
    for (const state of Object.keys(AGENT_STATE_LABEL) as AgentChipState[]) {
      const c = mount(state);
      expect(c.textContent).toContain(AGENT_STATE_LABEL[state]);
    }
  });

  it('carries an optional title so a raw remote_status (e.g. "blocked") can surface even though it visually folds into "failed"', () => {
    const c = mount('failed', 'blocked');
    const titled = c.querySelector('[title="blocked"]');
    expect(titled).toBeTruthy();
  });

  it('animates only the "running" state', () => {
    const running = mount('running');
    const dot = running.querySelector('span[aria-hidden="true"]') as HTMLElement;
    expect(dot.style.animation).toContain('tk-pulse');

    const done = mount('done');
    const doneDot = done.querySelector('span[aria-hidden="true"]') as HTMLElement;
    expect(doneDot.style.animation).toBe('none');
  });
});

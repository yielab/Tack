import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import CapabilityNote from './CapabilityNote';
import { gatePause, type Capabilities } from './capabilities';

const disposers: Array<() => void> = [];

function mount(label: string, capabilities: Capabilities) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => <CapabilityNote label={label} gate={gatePause(capabilities)} />,
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
});

const REAL_DOCKET_PAUSE_REASON =
  'docket exposes no pause endpoint over HTTP in either direction; from the docket CLI, ' +
  'run `docket profile <pod-id> --resume` to clear a budget-triggered pause';

function capabilitiesWithPause(reason: string, level: 'unsupported' | 'supported'): Capabilities {
  return {
    dispatch: true,
    cancel: false,
    pause: { level, reason },
    resume: { level: 'unsupported', reason: 'unrelated' },
    event_scope: { level: 'project', reason: 'unrelated' },
    artifacts: false,
    decisions: { level: 'poll', reason: 'unrelated' },
    usage: { level: 'from_provider', reason: 'unrelated' },
    model_selection: { level: 'unsupported', reason: 'unrelated' },
    runtimes: true,
    plane_metrics: true,
    provisioning: true,
  };
}

/**
 * The regression this file exists to prove: a control whose
 * capability is Unsupported renders a reason string that came from the
 * capability payload — asserted on the reason TEXT itself, not merely on
 * the control being disabled. A component that special-cased the plane's
 * `kind` string to decide whether to render, or that hard-coded its own
 * "not supported" copy, would also make the control disabled — this test
 * only passes if the rendered text actually traces back to the payload's
 * own `reason` field, proven by swapping in two DIFFERENT payload reasons
 * and asserting each one's exact text appears.
 */
describe('CapabilityNote — Unsupported renders the reason FROM THE PAYLOAD', () => {
  it("renders docket's real, pinned pause reason verbatim", () => {
    const caps = capabilitiesWithPause(REAL_DOCKET_PAUSE_REASON, 'unsupported');
    const c = mount('Pause', caps);
    expect(c.textContent).toContain(REAL_DOCKET_PAUSE_REASON);
    expect(c.textContent).toContain('docket profile <pod-id> --resume');
  });

  it('renders a DIFFERENT reason verbatim when the payload carries one — proving the text is read from the capability, not hard-coded per provider', () => {
    const differentReason =
      'a hypothetical second adapter has no pause endpoint either, for an entirely ' +
      'different, adapter-specific reason than docket';
    const caps = capabilitiesWithPause(differentReason, 'unsupported');
    const c = mount('Pause', caps);
    expect(c.textContent).toContain(differentReason);
    // And explicitly NOT docket's own reason — if this component had a
    // hard-coded "docket does not support pause" string anywhere, this
    // assertion is what would catch it.
    expect(c.textContent).not.toContain('docket profile');
  });

  it('renders nothing once the capability reports Supported — no reason to show for an enabled control', () => {
    const caps = capabilitiesWithPause('moot — this is Supported', 'supported');
    const c = mount('Pause', caps);
    expect(c.textContent?.trim()).toBe('');
  });
});

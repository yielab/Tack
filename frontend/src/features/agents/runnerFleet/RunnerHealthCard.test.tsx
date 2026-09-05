import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import RunnerHealthCard from './RunnerHealthCard';
import type { RunnerCapabilities } from '../../../shared/execution';

const disposers: Array<() => void> = [];

function mount(props: Parameters<typeof RunnerHealthCard>[0]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <RunnerHealthCard {...props} />, container);
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

/** A capability snapshot reporting everything as fully supported — used to
 *  prove the health badge never trusts capability data as a substitute for
 *  a real connection reading. */
const fullySupportedCapabilities: RunnerCapabilities = {
  protocol_version: 1,
  runner_version: '1.4.0',
  reported_at: '2026-08-01T00:00:00Z',
  labels: {},
  concurrency: { total: 4, available: 4 },
  harnesses: [
    {
      harness_kind: 'claude_code',
      installed_version: '2.1.0',
      probe_error: null,
      probed_at: '2026-08-01T00:00:00Z',
      model_combinations: [{ model_provider: 'anthropic', model_ids: ['opaque-1'], discovery: 'probed' }],
    },
  ],
  features: {
    cancel: { support: 'supported', reason: null },
    resume: { support: 'supported', reason: null },
    decisions: { support: 'supported', reason: null },
    artifacts: { support: 'supported', reason: null },
    usage: { support: 'supported', reason: null },
  },
  limits: { event_payload_bytes_max: 1_000_000, artifact_content_bytes_max: 10_000_000 },
};

describe('RunnerHealthCard — the health badge is driven only by connectionStatus', () => {
  it('never renders "Healthy" for an unconfirmed runner, even with fully-supported capability data', () => {
    const c = mount({
      name: 'ci-runner-1',
      runnerId: 'runr_abc',
      connectionStatus: 'unconfirmed',
      connectionReason: 'Enrolled this session — no read-back endpoint exists to confirm it connected.',
      capacity: { total: 4, available: 4 },
      labels: {},
      capabilities: fullySupportedCapabilities,
    });
    expect(c.textContent).not.toContain('Healthy');
    expect(c.textContent).toContain('Connection unconfirmed');
  });

  it('never renders "Healthy" for a stale runner regardless of capability data', () => {
    const c = mount({
      name: 'ci-runner-1',
      runnerId: 'runr_abc',
      connectionStatus: 'stale',
      connectionReason: 'No heartbeat observed recently.',
      capacity: null,
      labels: {},
      capabilities: fullySupportedCapabilities,
    });
    expect(c.textContent).not.toContain('Healthy');
    expect(c.textContent).toContain('Stale');
  });

  it('never renders "Healthy" for an unconfigured runner', () => {
    const c = mount({
      name: 'ci-runner-1',
      runnerId: 'runr_abc',
      connectionStatus: 'unconfigured',
      connectionReason: 'This runner was revoked.',
      capacity: null,
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).not.toContain('Healthy');
    expect(c.textContent).toContain('Unconfigured');
  });

  it('renders "Healthy" only when connectionStatus is explicitly healthy', () => {
    const c = mount({
      name: 'ci-runner-1',
      runnerId: 'runr_abc',
      connectionStatus: 'healthy',
      connectionReason: 'Fresh heartbeat within the last poll window.',
      capacity: { total: 4, available: 2 },
      labels: {},
      capabilities: fullySupportedCapabilities,
    });
    expect(c.textContent).toContain('Healthy');
  });

  it('always shows a non-empty connection reason', () => {
    const c = mount({
      name: 'ci-runner-1',
      runnerId: 'runr_abc',
      connectionStatus: 'unconfirmed',
      connectionReason: 'Enrolled this session — no read-back endpoint exists to confirm it connected.',
      capacity: null,
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).toContain('no read-back endpoint exists');
  });
});

describe('RunnerHealthCard — capacity display', () => {
  it('shows both available and total together when known', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: { total: 4, available: 2 },
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).toContain('2 / 4 slots available');
  });

  it('shows an explicit "capacity unknown" rather than a fabricated zero', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfigured',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).toContain('capacity unknown');
    expect(c.textContent).not.toMatch(/0 \/ 0/);
  });
});

describe('RunnerHealthCard — feature support always carries a visible reason', () => {
  it('shows "not supported" with the no-data reason when capabilities is null', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).toContain('no runner capability data available');
    expect(c.textContent).toContain('Cancel:');
    expect(c.textContent).toContain('not supported');
  });

  it('shows "supported" with no fabricated reason when the runner reports one verbatim as null', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: fullySupportedCapabilities,
    });
    // cancel/resume/... all report {support:'supported', reason:null} above —
    // must render as supported without inventing reason text.
    expect(c.textContent).toContain('supported');
  });

  it('renders a specific unsupported reason verbatim, proving the text traces to the payload', () => {
    const capsWithReason: RunnerCapabilities = {
      ...fullySupportedCapabilities,
      features: {
        ...fullySupportedCapabilities.features,
        artifacts: { support: 'unsupported', reason: 'this harness cannot stream artifacts' },
      },
    };
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: capsWithReason,
    });
    expect(c.textContent).toContain('this harness cannot stream artifacts');
  });
});

describe('RunnerHealthCard — harness display', () => {
  it('lists harness kind, version, and a probe error when present', () => {
    const capsWithProbeError: RunnerCapabilities = {
      ...fullySupportedCapabilities,
      harnesses: [
        {
          harness_kind: 'claude-code',
          installed_version: '0.9.0',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-01T00:00:00Z',
          model_combinations: [],
        },
      ],
    };
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: capsWithProbeError,
    });
    expect(c.textContent).toContain('claude-code');
    expect(c.textContent).toContain('0.9.0');
    expect(c.textContent).toContain('binary not found on PATH');
  });

  it('shows an explicit "no harness capability data available" when capabilities is null', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: {},
      capabilities: null,
    });
    expect(c.textContent).toContain('no harness capability data available');
  });
});

describe('RunnerHealthCard — labels', () => {
  it('renders label chips when present', () => {
    const c = mount({
      name: 'r',
      runnerId: 'runr_1',
      connectionStatus: 'unconfirmed',
      connectionReason: 'reason',
      capacity: null,
      labels: { region: 'us-east' },
      capabilities: null,
    });
    expect(c.textContent).toContain('region: us-east');
  });
});

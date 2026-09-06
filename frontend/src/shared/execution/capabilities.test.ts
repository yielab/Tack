import { describe, it, expect } from 'vitest';
import {
  gateFeature,
  gateFeatureAcrossRunners,
  listReportedHarnessKinds,
  harnessProbeStatus,
  listModelCombinationsForHarness,
  isCombinationSupported,
} from './capabilities';
import type { RunnerCapabilities } from './types';

// Fixture shaped exactly like docs/contracts/runner-v1/capabilities.json
// (III.1.6's frozen authority), so this test exercises real wire shapes,
// not an invented convenience shape.
function fixtureCapabilities(overrides: Partial<RunnerCapabilities> = {}): RunnerCapabilities {
  return {
    protocol_version: 1,
    runner_version: '0.1.0',
    reported_at: '2026-08-06T12:00:00Z',
    labels: { os: 'linux', arch: 'x86_64', trust: 'local' },
    concurrency: { total: 2, available: 1 },
    harnesses: [
      {
        harness_kind: 'codex',
        installed_version: '1.2.3',
        probe_error: null,
        probed_at: '2026-08-06T11:59:55Z',
        model_combinations: [
          { model_provider: 'openai', model_ids: ['opaque/model-alpha', 'opaque:model-beta'], discovery: 'reported' },
        ],
      },
    ],
    features: {
      cancel: { support: 'supported', reason: null },
      resume: { support: 'unsupported', reason: 'installed harness has no resumable session contract' },
      decisions: { support: 'supported', reason: null },
      artifacts: { support: 'supported', reason: null },
      usage: { support: 'advisory', reason: 'token totals may be absent from harness output' },
    },
    limits: { event_payload_bytes_max: 65536, artifact_content_bytes_max: 52428800 },
    ...overrides,
  };
}

describe('gateFeature', () => {
  it('enables a supported feature and passes through a null reason unfabricated', () => {
    const gate = gateFeature(fixtureCapabilities(), 'cancel');
    expect(gate).toEqual({ enabled: true, reason: null });
  });

  it('disables an unsupported feature and keeps the runner-supplied reason', () => {
    const gate = gateFeature(fixtureCapabilities(), 'resume');
    expect(gate).toEqual({
      enabled: false,
      reason: 'installed harness has no resumable session contract',
    });
  });

  it('enables an advisory feature (advisory is not the same as unsupported)', () => {
    const gate = gateFeature(fixtureCapabilities(), 'usage');
    expect(gate.enabled).toBe(true);
  });
});

describe('gateFeatureAcrossRunners', () => {
  it('is disabled with an explicit reason when there is no capability data at all', () => {
    const gate = gateFeatureAcrossRunners([], 'cancel');
    expect(gate).toEqual({
      enabled: false,
      reason: 'no runner capability data available',
      supportingRunnerCount: 0,
      totalRunnerCount: 0,
    });
  });

  it('is enabled the moment at least one runner supports it, and counts supporters', () => {
    const supporting = fixtureCapabilities();
    const notSupporting = fixtureCapabilities({
      features: {
        ...fixtureCapabilities().features,
        cancel: { support: 'unsupported', reason: 'no cancellation hook' },
      },
    });
    const gate = gateFeatureAcrossRunners([notSupporting, supporting], 'cancel');
    expect(gate.enabled).toBe(true);
    expect(gate.supportingRunnerCount).toBe(1);
    expect(gate.totalRunnerCount).toBe(2);
  });

  it('is disabled only when every runner reports unsupported, and surfaces a reason', () => {
    const a = fixtureCapabilities({
      features: { ...fixtureCapabilities().features, cancel: { support: 'unsupported', reason: 'reason a' } },
    });
    const b = fixtureCapabilities({
      features: { ...fixtureCapabilities().features, cancel: { support: 'unsupported', reason: 'reason b' } },
    });
    const gate = gateFeatureAcrossRunners([a, b], 'cancel');
    expect(gate.enabled).toBe(false);
    expect(gate.supportingRunnerCount).toBe(0);
    expect(['reason a', 'reason b']).toContain(gate.reason);
  });
});

describe('listReportedHarnessKinds', () => {
  it('is empty for no snapshots', () => {
    expect(listReportedHarnessKinds([])).toEqual([]);
  });

  it('deduplicates and sorts across multiple runners', () => {
    const codex = fixtureCapabilities();
    const claude = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'claude-code',
          installed_version: '0.5.0',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
        },
      ],
    });
    expect(listReportedHarnessKinds([codex, claude, codex])).toEqual(['claude-code', 'codex']);
  });
});

describe('harnessProbeStatus', () => {
  it('reports probed:true when at least one runner reports a clean probe', () => {
    expect(harnessProbeStatus([fixtureCapabilities()], 'codex')).toEqual({ probed: true, lastError: null });
  });

  it('reports the last error when every report for the harness failed to probe', () => {
    const failed = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
        },
      ],
    });
    expect(harnessProbeStatus([failed], 'codex')).toEqual({
      probed: false,
      lastError: 'binary not found on PATH',
    });
  });

  it('a harness never reported at all is neither probed nor errored', () => {
    expect(harnessProbeStatus([fixtureCapabilities()], 'claude_code')).toEqual({
      probed: false,
      lastError: null,
    });
  });
});

describe('listModelCombinationsForHarness', () => {
  it('merges and counts identical combinations across runners', () => {
    const a = fixtureCapabilities();
    const b = fixtureCapabilities();
    const combos = listModelCombinationsForHarness([a, b], 'codex');
    // Plain codepoint order, not locale collation (opaque ids — see
    // capabilities.ts's comment): '/' (0x2F) sorts before ':' (0x3A).
    expect(combos).toEqual([
      { model_provider: 'openai', model_id: 'opaque/model-alpha', supportingRunnerCount: 2 },
      { model_provider: 'openai', model_id: 'opaque:model-beta', supportingRunnerCount: 2 },
    ]);
  });

  it('excludes combinations from a harness report carrying a probe error', () => {
    const failed = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [{ model_provider: 'openai', model_ids: ['stale/model'], discovery: 'cached' }],
        },
      ],
    });
    expect(listModelCombinationsForHarness([failed], 'codex')).toEqual([]);
  });
});

describe('isCombinationSupported', () => {
  it('supported: true with a supporter count when at least one runner reports it', () => {
    const result = isCombinationSupported([fixtureCapabilities()], 'codex', 'openai', 'opaque/model-alpha');
    expect(result).toEqual({
      supported: true,
      reason: '1 runner reports this combination',
      supportingRunnerCount: 1,
    });
  });

  it('pluralizes the reason for multiple supporting runners', () => {
    const result = isCombinationSupported(
      [fixtureCapabilities(), fixtureCapabilities()],
      'codex',
      'openai',
      'opaque/model-alpha',
    );
    expect(result.reason).toBe('2 runners report this combination');
  });

  it('is unsupported with an explicit reason when no snapshot exists at all', () => {
    expect(isCombinationSupported([], 'codex', 'openai', 'opaque/model-alpha')).toEqual({
      supported: false,
      reason: 'no runner capability data available',
      supportingRunnerCount: 0,
    });
  });

  it('distinguishes "harness never reported" from "provider unmatched" from "model id unmatched"', () => {
    const snap = fixtureCapabilities();
    expect(isCombinationSupported([snap], 'claude-code', 'openai', 'opaque/model-alpha').reason).toBe(
      'no runner reports this harness',
    );
    expect(isCombinationSupported([snap], 'codex', 'anthropic', 'opaque/model-alpha').reason).toBe(
      'no runner reports this model provider for this harness',
    );
    expect(isCombinationSupported([snap], 'codex', 'openai', 'opaque/model-nonexistent').reason).toBe(
      'no runner reports this model id for this harness/provider',
    );
  });

  it('never reports a combination from a harness with a probe error as supported', () => {
    const failed = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'], discovery: 'cached' }],
        },
      ],
    });
    const result = isCombinationSupported([failed], 'codex', 'openai', 'opaque/model-alpha');
    expect(result.supported).toBe(false);
    // The harness was reported (it exists), so this must not be
    // misclassified as "harness never reported" — it's specifically the
    // provider that never showed up in a *trustworthy* report.
    expect(result.reason).toBe('no runner reports this model provider for this harness');
  });

  // Mirrors `crates/tack-orch/src/scheduler/select.rs`'s own arm: an
  // undeclared pairing still schedules when the harness attests
  // `model_passthrough: supported` — both bundled harnesses (codex,
  // claude-code) ship empty `model_combinations` and rely entirely on this.
  it('an undeclared pairing is supported when the harness attests model_passthrough: supported', () => {
    const passthrough = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
          model_passthrough: { support: 'supported', reason: null },
        },
      ],
    });
    const result = isCombinationSupported([passthrough], 'codex', 'anthropic', 'opaque/model-unlisted');
    expect(result.supported).toBe(true);
    expect(result.reason).toContain('model passthrough');
    expect(result.supportingRunnerCount).toBe(1);
  });

  it('an "advisory" (not "supported") model_passthrough attestation does not unlock an undeclared pairing', () => {
    const advisoryOnly = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
          model_passthrough: { support: 'advisory', reason: 'unverified' },
        },
      ],
    });
    const result = isCombinationSupported([advisoryOnly], 'codex', 'anthropic', 'opaque/model-unlisted');
    expect(result.supported).toBe(false);
  });

  it('a harness with a probe error never counts as passthrough-supported either', () => {
    const failedPassthrough = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: 'binary not found on PATH',
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [],
          model_passthrough: { support: 'supported', reason: null },
        },
      ],
    });
    const result = isCombinationSupported([failedPassthrough], 'codex', 'anthropic', 'opaque/model-unlisted');
    expect(result.supported).toBe(false);
  });

  it('a declared combination is not double-counted when the same runner also attests passthrough', () => {
    const both = fixtureCapabilities({
      harnesses: [
        {
          harness_kind: 'codex',
          installed_version: '1.2.3',
          probe_error: null,
          probed_at: '2026-08-06T12:00:00Z',
          model_combinations: [{ model_provider: 'openai', model_ids: ['opaque/model-alpha'], discovery: 'reported' }],
          model_passthrough: { support: 'supported', reason: null },
        },
      ],
    });
    const result = isCombinationSupported([both], 'codex', 'openai', 'opaque/model-alpha');
    expect(result.supportingRunnerCount).toBe(1);
    expect(result.reason).toBe('1 runner reports this combination');
  });
});

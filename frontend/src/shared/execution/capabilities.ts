// Pure capability-selector logic for the Part III execution surface
// (TODO.md III-E2 task: "capability selector — what harness/provider/model
// combinations are actually usable, driven by real runner-reported
// capability data, not assumptions").
//
// Every function here takes `RunnerCapabilities[]` (see `types.ts`) as a
// plain argument — it performs no I/O and knows nothing about HTTP, caching,
// or where the snapshots came from. This mirrors TODO.md's Wave 4 sibling
// card III-E1 ("Pure selection performs no I/O and never grants the
// authoritative lease") applied to the frontend's read-side equivalent: a
// component decides whether to let an operator pick a harness/provider/model
// combination, never whether to actually grant a lease.
//
// **There is currently no operator-facing endpoint that returns
// `RunnerCapabilities` for a registered runner** (see `api.ts`'s header
// note on the missing `GET /runners`). Every function below is fully usable
// and fully tested against the frozen `docs/contracts/runner-v1/
// capabilities.json` fixture shape today; only the data source is missing.
// A future `GET /runners` (or per-fleet roster) response can be fed straight
// into these functions without any further design work here.
//
// No function in this file ever reports `supported: true` without at least
// one runner's real capability data backing it, and never reports
// `supported: false` without a specific, typed reason — "unsupported is
// typed, unknown is explicit" (TODO.md III.2 rule 7), extended to this
// module's own vocabulary: an empty snapshot list is `false` with the
// explicit reason `'no runner capability data available'`, never a
// structural zero indistinguishable from "checked, and it's unsupported."

import type { CapabilitySupport, CapabilityValue, FeatureCapabilities, ModelCombination, RunnerCapabilities } from './types';

/** What a gated control needs to render itself. `reason` stays nullable
 *  (unlike `shared/orch/capabilities.ts`'s `CapabilityGate`, whose backend
 *  always supplies a reason) because `CapabilityValue.reason` is genuinely
 *  `null` on the wire when a runner reports support without qualification
 *  (`docs/contracts/runner-v1/capabilities.json`'s `cancel` entry) — a
 *  component must not fabricate text where the runner gave none. */
export interface CapabilityGate {
  enabled: boolean;
  reason: string | null;
}

const NO_DATA_REASON = 'no runner capability data available';

/** A single runner capability value is "enabled" at every level except
 *  `'unsupported'` — matching `shared/orch/capabilities.ts`'s `gate()`
 *  convention (`'advisory'` still lets a control fire). */
function gateValue(value: CapabilityValue | undefined): CapabilityGate {
  if (!value) return { enabled: false, reason: NO_DATA_REASON };
  return { enabled: value.support !== 'unsupported', reason: value.reason };
}

export type FeatureName = keyof FeatureCapabilities;

/** Gates one feature (`cancel`/`resume`/`decisions`/`artifacts`/`usage`) for
 *  a single runner's capability report. */
export function gateFeature(capabilities: RunnerCapabilities, feature: FeatureName): CapabilityGate {
  return gateValue(capabilities.features[feature]);
}

/**
 * Gates one feature across every runner that could serve a fleet/`any`
 * selector. `enabled` is true the moment at least one runner supports it
 * (advisory or supported) — the scheduler (III-E1), not this function,
 * decides which runner within the fleet actually gets picked; this is only
 * "could the operator ever expect this control to do something." Reports
 * `supportingRunnerCount`/`totalRunnerCount` so a caller can render
 * "supported by 2 of 3 runners" rather than a bare boolean.
 */
export function gateFeatureAcrossRunners(
  snapshots: RunnerCapabilities[],
  feature: FeatureName,
): CapabilityGate & { supportingRunnerCount: number; totalRunnerCount: number } {
  if (snapshots.length === 0) {
    return { enabled: false, reason: NO_DATA_REASON, supportingRunnerCount: 0, totalRunnerCount: 0 };
  }
  const gates = snapshots.map((snapshot) => gateValue(snapshot.features[feature]));
  const supporting = gates.filter((gate) => gate.enabled);
  const supportingRunnerCount = supporting.length;
  const totalRunnerCount = snapshots.length;
  if (supportingRunnerCount > 0) {
    const reason = supporting.find((gate) => gate.reason !== null)?.reason ?? null;
    return { enabled: true, reason, supportingRunnerCount, totalRunnerCount };
  }
  const reason = gates.find((gate) => gate.reason !== null)?.reason ?? null;
  return { enabled: false, reason, supportingRunnerCount: 0, totalRunnerCount };
}

/** Every distinct `harness_kind` reported by at least one runner, sorted for
 *  a stable render order. A harness whose only report carries a
 *  `probe_error` is still listed — the operator should see it exists, even
 *  disabled; use {@link harnessProbeStatus} to gate the picker entry. */
export function listReportedHarnessKinds(snapshots: RunnerCapabilities[]): string[] {
  const kinds = new Set<string>();
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) kinds.add(harness.harness_kind);
  }
  return [...kinds].sort();
}

/** Whether at least one runner reports this harness with no `probe_error`,
 *  and the most recent probe error seen for it otherwise (never silently
 *  hidden — a component must be able to say why a harness is greyed out). */
export function harnessProbeStatus(
  snapshots: RunnerCapabilities[],
  harnessKind: string,
): { probed: boolean; lastError: string | null } {
  let lastError: string | null = null;
  let sawClean = false;
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) {
      if (harness.harness_kind !== harnessKind) continue;
      if (harness.probe_error === null) sawClean = true;
      else lastError = harness.probe_error;
    }
  }
  return { probed: sawClean, lastError: sawClean ? null : lastError };
}

/** One provider/model-id combination merged across every reporting runner,
 *  with how many distinct runners reported it — the shape a picker needs to
 *  render "openai / opaque/model-alpha (2 runners)". */
export interface AggregatedModelCombination {
  model_provider: string;
  model_id: string;
  supportingRunnerCount: number;
}

/** Every provider/model-id pair reported for `harnessKind` across all
 *  snapshots, deduplicated and counted. Harness reports with a
 *  `probe_error` are excluded — a failed probe's `model_combinations` (if
 *  any were stale-cached) are not trustworthy evidence of what's usable
 *  right now. */
export function listModelCombinationsForHarness(
  snapshots: RunnerCapabilities[],
  harnessKind: string,
): AggregatedModelCombination[] {
  const counts = new Map<string, AggregatedModelCombination>();
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) {
      if (harness.harness_kind !== harnessKind || harness.probe_error !== null) continue;
      for (const combo of harness.model_combinations as ModelCombination[]) {
        for (const modelId of combo.model_ids) {
          const key = `${combo.model_provider} ${modelId}`;
          const existing = counts.get(key);
          if (existing) existing.supportingRunnerCount += 1;
          else {
            counts.set(key, {
              model_provider: combo.model_provider,
              model_id: modelId,
              supportingRunnerCount: 1,
            });
          }
        }
      }
    }
  }
  // Plain string comparison, not `localeCompare` — model ids are opaque
  // (`types.ts`'s `ModelCombination` doc comment: "never inspect or split
  // it"), so locale-aware collation (which can treat punctuation like `/`
  // vs `:` inconsistently across environments) is both meaningless for
  // these values and a source of non-deterministic ordering across ICU
  // data versions. A stable, codepoint-based sort keeps this function's
  // output reproducible everywhere it runs.
  return [...counts.values()].sort((a, b) => {
    if (a.model_provider !== b.model_provider) return a.model_provider < b.model_provider ? -1 : 1;
    if (a.model_id !== b.model_id) return a.model_id < b.model_id ? -1 : 1;
    return 0;
  });
}

export interface CombinationAvailability {
  supported: boolean;
  reason: string;
  supportingRunnerCount: number;
}

/**
 * The single function a "Run with agent" submit gate (E4) needs: is this
 * exact harness/provider/model combination usable right now, across every
 * runner capability snapshot the caller has. Always returns a typed reason,
 * whether supported or not — never a bare boolean (TODO.md III.2 rule 7).
 *
 * Mirrors `crates/tack-orch/src/scheduler/select.rs`'s `evaluate_candidate`
 * exactly: a pairing counts as supported when a runner either declares it in
 * `model_combinations`, OR attests `model_passthrough: supported` for that
 * harness — the scheduler's own arm is `!declared && !passthrough`, checked
 * with no regard to which provider/model was actually requested, because a
 * passthrough harness forwards whatever it's given and validates it itself
 * at run time. `'advisory'` and an absent attestation both mean "not
 * attested" and reject exactly like `'unsupported'`, same as the scheduler.
 */
export function isCombinationSupported(
  snapshots: RunnerCapabilities[],
  harnessKind: string,
  modelProvider: string,
  modelId: string,
): CombinationAvailability {
  if (snapshots.length === 0) {
    return { supported: false, reason: NO_DATA_REASON, supportingRunnerCount: 0 };
  }
  let harnessSeen = false;
  let providerSeen = false;
  let declaredSupportingRunnerCount = 0;
  let passthroughOnlySupportingRunnerCount = 0;
  for (const snapshot of snapshots) {
    for (const harness of snapshot.harnesses) {
      if (harness.harness_kind !== harnessKind) continue;
      harnessSeen = true;
      if (harness.probe_error !== null) continue;
      let declaredHere = false;
      for (const combo of harness.model_combinations) {
        if (combo.model_provider !== modelProvider) continue;
        providerSeen = true;
        if (combo.model_ids.includes(modelId)) declaredHere = true;
      }
      // Counted once per reporting harness, never both ways for the same
      // report — a runner is either supporting evidence or it isn't, exactly
      // the per-candidate `declared || passthrough` the scheduler evaluates.
      if (declaredHere) declaredSupportingRunnerCount += 1;
      else if (harness.model_passthrough?.support === 'supported') passthroughOnlySupportingRunnerCount += 1;
    }
  }
  const supportingRunnerCount = declaredSupportingRunnerCount + passthroughOnlySupportingRunnerCount;
  if (supportingRunnerCount > 0) {
    const noun = supportingRunnerCount === 1 ? 'runner' : 'runners';
    const verb = supportingRunnerCount === 1 ? 'reports' : 'report';
    const reason =
      declaredSupportingRunnerCount > 0
        ? `${supportingRunnerCount} ${noun} ${verb} this combination`
        : `${supportingRunnerCount} ${noun} ${verb} this harness forwards an operator-chosen model verbatim (model passthrough)`;
    return { supported: true, reason, supportingRunnerCount };
  }
  if (!harnessSeen) {
    return { supported: false, reason: 'no runner reports this harness', supportingRunnerCount: 0 };
  }
  if (!providerSeen) {
    return {
      supported: false,
      reason: 'no runner reports this model provider for this harness',
      supportingRunnerCount: 0,
    };
  }
  return {
    supported: false,
    reason: 'no runner reports this model id for this harness/provider',
    supportingRunnerCount: 0,
  };
}

export type { CapabilitySupport };

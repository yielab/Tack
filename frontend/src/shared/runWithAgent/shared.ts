// Pure, framework-agnostic logic behind the "Run with agent" UI (TODO.md
// III-E4, Wave 4 / Phase 54: "item/sprint Run with agent and activity").
//
// Kept separate from the SolidJS components below so that payload
// construction, capability gating, and default-provenance logic are unit
// testable without mounting anything — and, more importantly, so that
// `RunWithAgentModal.tsx` (the ONE shared modal all three surfaces mount) has
// a single, provably-shared code path from form state to wire body. That is
// what backs this card's acceptance bar "all three surfaces (Board,
// item-detail, Sprint) create the same payload shape when launching a run —
// no divergent DTOs between entry points": Board/item-detail/Sprint never
// build a `CreateExecutionInput` themselves, they only collect a
// `RunWithAgentFormValues` and hand it to the one modal, which calls
// {@link buildCreateExecutionInput} exactly once.
//
// Vocabulary note (III.0): this module is about the NEW, neutral Part III
// execution domain (`ExecutionRequest` via `tack-runner`) — never the legacy
// `shared/dispatch/**` Docket "dispatch" concept. Nothing here imports from
// or is compatible with `shared/dispatch/**`; see that folder's own files
// for the older, unrelated feature.

import type {
  CreateExecutionInput,
  RunnerCapabilities,
  HarnessCapability,
} from '../execution';
import { isCombinationSupported } from '../execution';
import type { ProjectModelDefault } from '../types';

// ─── Harness kinds ──────────────────────────────────────────────────────────

/**
 * The in-tree v1 harness adapters, with their real,
 * verified `harness_kind` wire values — read directly from each adapter's own
 * constant, not guessed:
 *   - `codex.rs`'s `CODEX_HARNESS_KIND = "codex"`
 *   - `claude_code.rs`'s `HARNESS_KIND = "claude-code"` (a HYPHEN)
 *
 * This list is not the whole harness vocabulary — a runner's own
 * `harness_kind` is an opaque string (`HarnessKind::Other(String)` on the
 * runner side), so a runner can report a kind not in this list; it is just
 * not one this build has a bundled adapter for.
 *
 * Worth calling out explicitly: `crates/tack-cli/src/execution.rs`'s own
 * unit tests use `"claude_code"` (an UNDERSCORE) as an arbitrary example
 * value for what is, on the wire, an untyped `String` field
 * (`CreateExecution::requested_harness_kind`) — that is a test fixture
 * string, not the runner's real reported `harness_kind`. Copying the
 * underscore form here would silently make Claude Code unselectable against
 * any runner that actually reports itself as `"claude-code"`, since matching
 * against a real runner's capability report is a byte-exact string compare
 * (`capabilities.ts#isCombinationSupported`). Verified by reading the
 * harness adapter source files directly, not by inference.
 */
export const HARNESS_KINDS: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'codex', label: 'Codex' },
  { value: 'claude-code', label: 'Claude Code' },
];

// ─── Payload construction ───────────────────────────────────────────────────

/**
 * Everything the modal collects from an operator, in a shape independent of
 * any particular form widget. `agentProfileSnapshot` deliberately carries
 * only `name`/`instructions`/`tool_policy` — the fields copied verbatim from
 * the chosen `AgentProfileSummary` (`shared/execution/api.ts`) — because
 * `timeout_seconds`/`budgets` are collected once at the top level of the form
 * and folded in by {@link buildCreateExecutionInput}, not duplicated as a
 * second set of inputs.
 */
export interface RunWithAgentFormValues {
  itemId: string;
  selectorKind: 'fleet' | 'exact_runner';
  selectorId: string;
  agentProfileId: string;
  agentProfileSnapshot: { name: string; instructions: string; tool_policy: unknown };
  harnessKind: string;
  /** `null`/`null` together mean "auto — let the runner decide" (III.1.2:
   *  "requested model-provider and opaque model id, each nullable when
   *  auto-selection is allowed"), a first-class, always-legal request shape
   *  distinct from "a specific combination that happens to be unsupported." */
  modelProvider: string | null;
  modelId: string | null;
  timeoutSeconds: number;
  allowNetwork: boolean;
  tools: string[];
  repository: { kind: string; remote: string; baseRevision: string; subdirectory: string | null };
  idempotencyKey: string;
}

/**
 * The one function every entry point calls to go from collected form state
 * to the exact `POST /executions` body (`CreateExecutionInput`,
 * `shared/execution/api.ts`, itself copied field-for-field from
 * `crates/tack-api/src/handlers/executions.rs`'s `CreateExecution`).
 *
 * The nested `agent_profile_snapshot`/`repository_snapshot`/
 * `permission_policy` objects are untyped (`unknown`) on the wire-boundary
 * type because the operator API publishes no OpenAPI schema for this domain
 * yet (see `shared/execution/api.ts`'s header comment) — but they are NOT
 * arbitrary here: their shape is copied field-for-field from the real Rust
 * structs one layer deeper (`crates/tack-orch/src/execution/types.rs`):
 *   - `AgentProfileSnapshot { name, instructions, tool_policy, timeout_seconds, budgets }`
 *   - `RepositorySnapshot { kind, remote, base_revision, subdirectory }`
 *   - `PermissionPolicy { tools, network }`
 * confirmed against `docs/agent-handoffs/part-iii/III-E5.md`'s own
 * adversarial finding (E5's first draft defaulted `agent_profile_snapshot`/
 * `permission_policy` to `{}` and got a live 400 `missing field \`network\`` —
 * this module's fields exist specifically because that mistake is already
 * documented). `budgets`/`environment`/`metadata` are left at `{}`, which
 * E5's same finding confirmed is genuinely safe to default (untyped `Value`
 * fields end-to-end).
 */
export function buildCreateExecutionInput(values: RunWithAgentFormValues): CreateExecutionInput {
  return {
    item_id: values.itemId,
    idempotency_key: values.idempotencyKey,
    selector_kind: values.selectorKind,
    selector_id: values.selectorId,
    agent_profile_id: values.agentProfileId,
    requested_harness_kind: values.harnessKind,
    requested_model_provider: values.modelProvider,
    requested_model_id: values.modelId,
    agent_profile_snapshot: {
      name: values.agentProfileSnapshot.name,
      instructions: values.agentProfileSnapshot.instructions,
      tool_policy: values.agentProfileSnapshot.tool_policy,
      timeout_seconds: values.timeoutSeconds,
      budgets: {},
    },
    repository_snapshot: {
      kind: values.repository.kind,
      remote: values.repository.remote,
      base_revision: values.repository.baseRevision,
      subdirectory: values.repository.subdirectory,
    },
    permission_policy: {
      tools: values.tools,
      network: values.allowNetwork,
    },
    budgets: {},
    environment: {},
    metadata: {},
    timeout_seconds: values.timeoutSeconds,
    status_map_policy_id: null,
  };
}

/** A fresh idempotency key for one create attempt — matches the existing
 *  frontend precedent for client-generated ids (`ActivityTab.tsx`,
 *  `CreateItemModal.tsx`'s subtask ids both use `crypto.randomUUID()`), and
 *  E5's CLI default ("a freshly-generated default" the operator may
 *  override to opt into safe retry). This module never reuses a key across
 *  two different submissions of the same open modal. */
export function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

// ─── Resolved-default provenance ────────────────────────────────────────────

/**
 * Why a default was pre-filled — or the honest statement that nothing was.
 * III-E4's task text asks for "resolved default provenance (show why a
 * default was chosen — profile default vs. project default vs. fleet
 * default)". That resolution precedence (request override → agent profile →
 * project → fleet) is III-F3's job (TODO.md Wave 5, "Model resolution and
 * usage provenance") — a card that has not landed as of Wave 4, and E4 only
 * depends on E2. There is today no server-side default to report and no
 * endpoint that would report one; inventing a client-side "first item in the
 * list wins" convention and labeling it "the default" would be exactly the
 * fabricated-provenance this project's rules forbid (TODO.md III.2 rule 7:
 * "no structural zero standing in for unknown"). This type makes that a
 * typed, explicit state instead — the same pattern `shared/execution/
 * store.ts`'s `AttemptAvailability` already established for a different,
 * also-currently-missing read path.
 */
export type DefaultProvenance =
  | { status: 'resolved'; source: 'request_override' | 'profile' | 'project' | 'fleet'; description: string }
  | { status: 'not_available'; reason: string };

const DEFAULT_RESOLUTION_NOT_AVAILABLE_REASON =
  'No server-side default was resolved for this field — profile/project/fleet default ' +
  'precedence (TODO.md III-F3, Wave 5) has not landed yet. Choose explicitly.';

/** Always `not_available` today — see this module's `DefaultProvenance` doc
 *  comment. A future card wiring III-F3's real resolution output can replace
 *  this function's body with a real `{status: 'resolved', ...}` case without
 *  changing any caller's shape. */
export function resolveDefaultProvenance(): DefaultProvenance {
  return { status: 'not_available', reason: DEFAULT_RESOLUTION_NOT_AVAILABLE_REASON };
}

// ─── Model policy resolution (mirrors crates/tack-orch/src/model_policy/**) ─

/** One precedence tier a default model can come from, in the same order
 *  `crates/tack-orch/src/model_policy/mod.rs`'s `ModelPolicyTier::ORDER`
 *  checks them (request override excluded — this vocabulary only describes
 *  what an Auto request, which never carries an override, resolves to). */
export type ModelPolicyTier = 'agent_profile' | 'project' | 'fleet';

const MODEL_POLICY_TIER_LABEL: Record<ModelPolicyTier, string> = {
  agent_profile: 'agent profile',
  project: 'project',
  fleet: 'fleet',
};

/** The `{"default_model": ...}` convention read out of an agent profile's
 *  `limits` or a fleet's `default_policy` — both untyped JSON blobs on the
 *  wire (`AgentProfileSummary.limits` / `FleetSummary.default_policy`,
 *  `shared/execution/api.ts`). Mirrors
 *  `crates/tack-orch/src/model_policy/wiring.rs`'s
 *  `parse_model_default_convention` field-for-field: a missing key,
 *  malformed shape, or unrecognised literal all mean "this tier expressed no
 *  opinion" (`null`), never a thrown error. */
export type ModelDefaultConvention =
  | { kind: 'auto' }
  | { kind: 'explicit'; provider: string; model_id: string }
  | null;

export function parseModelDefaultConvention(raw: unknown): ModelDefaultConvention {
  if (raw === null || typeof raw !== 'object') return null;
  const defaultModel = (raw as Record<string, unknown>).default_model;
  if (defaultModel === undefined || defaultModel === null) return null;
  if (typeof defaultModel === 'string') return defaultModel === 'auto' ? { kind: 'auto' } : null;
  if (typeof defaultModel !== 'object') return null;
  const provider = (defaultModel as Record<string, unknown>).provider;
  const modelIdValue = (defaultModel as Record<string, unknown>).model_id;
  if (typeof provider !== 'string' || typeof modelIdValue !== 'string') return null;
  return { kind: 'explicit', provider, model_id: modelIdValue };
}

/**
 * What an Auto ("let the runner decide") request actually resolves to for a
 * specific agent profile / project / fleet-or-runner target. Without this,
 * `gateHarnessModelSelection` has no way to tell "Auto will schedule because
 * some tier names an explicit model" from "Auto will queue forever because
 * none does" — both send the identical null/null pair over the wire, and
 * the difference is entirely server-side state this function has to read
 * from the same data the modal already fetches for other fields. Mirrors
 * `crates/tack-orch/src/model_policy/mod.rs`'s `resolve_model_policy`
 * precedence walk (agent profile → project → fleet; request override is
 * never present here, since this is only computed for an Auto selection)
 * applied to the same three tiers `wiring.rs`'s
 * `resolve_request_model_policy` reads at request time — the request's own
 * agent profile, its item's project, and (only when the target IS a fleet)
 * that fleet.
 *
 * `outcome: 'pinned_auto'` is `mod.rs`'s documented nuance that a tier
 * explicitly set to the literal `"auto"` is a real, present value that
 * *stops* the walk at that tier rather than falling through to a
 * less-specific tier that might name a concrete model — so a fleet or
 * agent-profile default beneath it never gets a chance to rescue the
 * request, exactly like the server.
 *
 * This is a hand-written copy of three specific Rust decisions —
 * `ModelPolicyTier::ORDER`'s precedence, `resolve_model_policy`'s walk over
 * it, and `parse_model_default_convention`'s parsing of the `limits`/
 * `default_policy` blobs — not a shared definition. Nothing on either side
 * fails to build or fails a test if they drift apart. Because
 * `gateHarnessModelSelection` now blocks a dispatch on this function's
 * answer instead of only commenting on it, the practical risk runs one
 * direction: this copy falling behind a Rust-side change that would resolve
 * *more* requests (an added tier, a reordered precedence, a convention
 * parsed more leniently) reads as `'unresolved'`/`'pinned_auto'` here after
 * the server would already schedule the same request fine — a dialog that
 * confidently refuses a dispatch that would have worked, not one that lets
 * through a dispatch that won't. That asymmetry follows from
 * `parse_model_default_convention`'s own stated posture (an unrecognised
 * shape reads as "no opinion", never an error) — tightening that posture
 * later, the one change that would point the risk the other way, would be
 * a deliberate, visible break from how it already documents itself, not a
 * quiet drift.
 */
export type AutoModelResolution =
  | { outcome: 'explicit'; source: ModelPolicyTier; provider: string; model_id: string }
  | { outcome: 'pinned_auto'; source: ModelPolicyTier }
  | { outcome: 'unresolved' };

export function resolveAutoModelPolicy(
  agentProfileLimits: unknown,
  projectDefaultModel: ProjectModelDefault | null | undefined,
  fleetDefaultPolicy: unknown,
): AutoModelResolution {
  const projectConvention: ModelDefaultConvention =
    projectDefaultModel == null
      ? null
      : projectDefaultModel.kind === 'auto'
        ? { kind: 'auto' }
        : { kind: 'explicit', provider: projectDefaultModel.provider, model_id: projectDefaultModel.model_id };
  const tiers: Array<[ModelPolicyTier, ModelDefaultConvention]> = [
    ['agent_profile', parseModelDefaultConvention(agentProfileLimits)],
    ['project', projectConvention],
    ['fleet', parseModelDefaultConvention(fleetDefaultPolicy)],
  ];
  for (const [source, value] of tiers) {
    if (!value) continue;
    if (value.kind === 'auto') return { outcome: 'pinned_auto', source };
    return { outcome: 'explicit', source, provider: value.provider, model_id: value.model_id };
  }
  return { outcome: 'unresolved' };
}

// ─── Capability gating ──────────────────────────────────────────────────────

export interface CombinationGate {
  /** Whether the submit control should allow this selection through. */
  allowed: boolean;
  /** Always present — never a silent disable (TODO.md III.2 rule 7). */
  reason: string;
  /** `true` when `allowed` is true only because there is no concrete
   *  evidence either way (an advisory, not a real confirmation) — lets a
   *  caller render a softer, non-blocking notice instead of implying the
   *  combination was actually verified. */
  advisory: boolean;
  /** A concrete next step the operator can take right now, when the block
   *  above is fixable from a page that exists — currently only "set the
   *  project's default model" (the one tier with a settings UI at all; an
   *  agent-profile or fleet default is API-only, so there is nothing to
   *  link to for those). Omitted whenever nothing is actionable, including
   *  whenever the combination is already allowed. */
  fix?: { label: string; href: string };
}

/**
 * The submit-gate this card's acceptance bar requires ("an unsupported
 * harness/provider/model combination cannot be submitted — disabled +
 * reasoned, not merely rejected server-side"), built on top of
 * `shared/execution/capabilities.ts#isCombinationSupported` — the exact
 * function that module's own header comment names as "the single function a
 * 'Run with agent' submit gate (E4) needs."
 *
 * Two cases:
 *
 * 1. **A specific model provider/id was chosen** (including a model chosen
 *    via "Project default"). This is a real, falsifiable claim ("this exact
 *    combination works"), so `isCombinationSupported` is authoritative:
 *    unsupported blocks submission outright.
 *
 * 2. **`modelProvider`/`modelId` are both `null` ("Auto").** Unlike the
 *    advisory this function used to show here, Auto's fate is no longer a
 *    guess: `crates/tack-orch/src/scheduler/select.rs`'s `evaluate_candidate`
 *    unconditionally rejects a request that reaches it as
 *    `ModelSelector::AutoSelect` (`IneligibleReason::AutoSelectNotVerified`
 *    — no runner-v1 capability field lets a runner attest it safely accepts
 *    an unspecified model), and it reaches the scheduler that way if and
 *    only if `autoResolution` (the caller's own
 *    {@link resolveAutoModelPolicy} result, computed against the currently
 *    selected agent profile / project / fleet-or-runner target) is anything
 *    other than `'explicit'`. So: `'explicit'` re-runs the exact same
 *    `isCombinationSupported` check this function uses for an explicit
 *    choice, against the resolved pair (this *is* what the request will
 *    resolve to server-side); `'pinned_auto'`/`'unresolved'` both block,
 *    because both are requests the scheduler always refuses today, and name
 *    the fix instead of the retired advisory that claimed the scheduler
 *    would still validate an unresolved choice at claim time -- it never
 *    does, for either of those two outcomes.
 */
export function gateHarnessModelSelection(
  capabilities: RunnerCapabilities[],
  harnessKind: string,
  modelProvider: string | null,
  modelId: string | null,
  autoResolution: AutoModelResolution = { outcome: 'unresolved' },
  projectSettingsHref?: string,
): CombinationGate {
  if (modelProvider == null || modelId == null) {
    if (autoResolution.outcome === 'explicit') {
      const tierLabel = MODEL_POLICY_TIER_LABEL[autoResolution.source];
      const combo = isCombinationSupported(
        capabilities,
        harnessKind,
        autoResolution.provider,
        autoResolution.model_id,
      );
      return {
        allowed: combo.supported,
        advisory: false,
        reason: `Resolves via the ${tierLabel}'s default model (${autoResolution.provider} / ${autoResolution.model_id}): ${combo.reason}.`,
      };
    }
    const fix: CombinationGate['fix'] =
      projectSettingsHref && (autoResolution.outcome === 'unresolved' || autoResolution.source === 'project')
        ? { label: 'Set a default model for this project', href: projectSettingsHref }
        : undefined;
    if (autoResolution.outcome === 'pinned_auto') {
      const tierLabel = MODEL_POLICY_TIER_LABEL[autoResolution.source];
      return {
        allowed: false,
        advisory: false,
        reason:
          `This ${tierLabel} is explicitly set to Auto, and no runner can attest it safely accepts an ` +
          'unspecified model — this request would queue forever and never run. Choose an explicit model below' +
          (fix ? ', or change that default.' : '.'),
        fix,
      };
    }
    return {
      allowed: false,
      advisory: false,
      reason:
        'No agent profile, project, or fleet default model is configured for this target, and no runner can ' +
        'attest it safely accepts an unspecified model — this request would queue forever and never run. ' +
        'Choose an explicit model below, or set a default model.',
      fix,
    };
  }
  const combo = isCombinationSupported(capabilities, harnessKind, modelProvider, modelId);
  return { allowed: combo.supported, advisory: false, reason: combo.reason };
}

// ─── Lifecycle-state display ────────────────────────────────────────────────

export type StateTone = 'neutral' | 'primary' | 'success' | 'warning' | 'danger' | 'info';

const STATE_LABEL: Record<string, string> = {
  queued: 'Queued',
  leased: 'Leased',
  preparing: 'Preparing',
  running: 'Running',
  waiting_decision: 'Waiting on decision',
  succeeded: 'Succeeded',
  failed: 'Failed',
  cancelled: 'Cancelled',
  lost: 'Lost',
  needs_operator: 'Needs operator',
};

const STATE_TONE: Record<string, StateTone> = {
  queued: 'neutral',
  leased: 'info',
  preparing: 'info',
  running: 'primary',
  waiting_decision: 'warning',
  succeeded: 'success',
  failed: 'danger',
  cancelled: 'neutral',
  lost: 'danger',
  needs_operator: 'warning',
};

/**
 * `ExecutionSummary.state` is kept as a plain `string` on the wire type
 * (`shared/execution/api.ts`'s own doc comment: "so an operator-surface
 * value this build doesn't recognise still renders instead of failing to
 * parse"). This is the display-layer half of that same defensiveness — an
 * unrecognised value still renders (as itself, neutral tone), it never
 * throws and never silently disappears.
 */
export function describeExecutionState(state: string): { label: string; tone: StateTone; known: boolean } {
  const label = STATE_LABEL[state];
  if (label) return { label, tone: STATE_TONE[state], known: true };
  return { label: state, tone: 'neutral', known: false };
}

const TERMINAL_STATES = new Set(['succeeded', 'failed', 'cancelled']);

/** Whether the request is in a terminal state — a plain-string-safe
 *  restatement of `shared/execution/types.ts#isTerminalExecutionState`,
 *  which takes a narrowed `ExecutionState`, not the defensively-widened
 *  `string` this module works with (see `describeExecutionState`'s note). */
export function isTerminalStateString(state: string): boolean {
  return TERMINAL_STATES.has(state);
}

/**
 * A small, self-contained relative-time formatter. `shared/agentActivity/
 * format.ts` already has an equivalent `relativeTime`, but that module
 * belongs to the older, distinct Part II Docket agent-activity domain
 * (III.0's vocabulary rule) — this card's brief is explicit about keeping
 * the new execution UI structurally independent from it, so this ~12-line
 * function is duplicated on purpose rather than importing across that
 * boundary for one date-formatting helper.
 */
export function relativeTimeFromIso(iso: string | null | undefined): string {
  if (!iso) return 'unknown';
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'unknown';
  const diffSec = Math.round((Date.now() - then) / 1000);
  if (diffSec < 5) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d ago`;
  return new Date(iso).toLocaleDateString();
}

// ─── Target selection ("Where it runs") ─────────────────────────────────────

/** `agent_runners.state` values that mean the runner is live and could
 *  actually claim work — `runner_admin.rs::list_runners` reports
 *  `'pending_enrollment' | 'active' | 'revoked'` (plus any future value,
 *  which this treats as inactive rather than guessing). */
export function isActiveRunnerState(state: string): boolean {
  return state === 'active';
}

/** Whether the target picker should stay hidden and the one active runner
 *  used directly, with no free-text id anywhere — this card's "hidden when
 *  exactly one machine is active — the common case" task. More than one
 *  active runner, or a fleet also existing as an alternative, means a real
 *  choice exists and the picker must render. */
export function shouldHideTargetPicker(activeRunnerCount: number, fleetCount: number): boolean {
  return activeRunnerCount === 1 && fleetCount === 0;
}

/**
 * Whether the product should show its "agent execution is off" state
 * instead of the run form.
 *
 * Measured against this branch's base (2026-09-04): no persisted execution
 * on/off flag exists yet (`grep -n "local-runner"
 * crates/tack-api/src/router.rs` is empty — that's VI-B3, not landed here),
 * and `/api/executions` and its sibling routes are mounted unconditionally
 * (`frontend/e2e/run-with-agent.spec.ts`'s own header note: "an always-on
 * operator surface, NOT gated behind TACK_ORCH_ENABLE"). The only
 * currently-observable signal is therefore indirect: the §VI.0 surface map's
 * "Turn on agent execution" row is still a console step
 * (`tack serve --with-runner`) today, and without it no runner ever enrolls
 * — so zero active runners is read as "execution is off" until VI-B3 lands a
 * real flag this function can switch to reading instead.
 */
export function isExecutionOff(activeRunnerCount: number): boolean {
  return activeRunnerCount === 0;
}

// ─── Project-default model (VI-C3's `Project` model-policy tier) ───────────

/** Human label for a project's configured model default — the "Project
 *  default — …" mode text the Model fieldset shows verbatim. `null` means
 *  the project has no opinion (VI-C3: `default_model` absent), which is
 *  distinct from an explicit `Auto` choice. */
export function describeProjectModelDefault(defaultModel: ProjectModelDefault | null | undefined): string | null {
  if (!defaultModel) return null;
  if (defaultModel.kind === 'auto') return 'Auto';
  return `${defaultModel.provider} / ${defaultModel.model_id}`;
}

/** The requested provider/model pair the "Project default" mode submits.
 *  A project default of `Auto`, and no project default at all, both
 *  resolve to the same nullable pair the modal's own Auto mode uses
 *  (III.1.2: `null`/`null` means "auto — let the runner decide"); only an
 *  `Explicit` default copies a concrete pair through. */
export function projectDefaultModelPair(
  defaultModel: ProjectModelDefault | null | undefined,
): { provider: string | null; id: string | null } {
  if (!defaultModel || defaultModel.kind === 'auto') return { provider: null, id: null };
  return { provider: defaultModel.provider, id: defaultModel.model_id };
}

// ─── Model passthrough (free-text model id) ─────────────────────────────────

/**
 * Whether a harness attests it forwards an operator-specified model id
 * verbatim, rather than requiring one of its reported `model_combinations`
 * — the only case the Model "Choose…" mode may accept free text. Mirrors
 * `crates/tack-orch/src/scheduler/select.rs`'s own treatment: only
 * `support === 'supported'` counts; `'advisory'` and an absent
 * `model_passthrough` (an older runner, or the shared fake probe) both mean
 * "not attested" and are rejected identically to `'unsupported'` there, so
 * neither unlocks free text here either.
 */
export function isModelPassthroughAttested(harness: HarnessCapability | undefined): boolean {
  return harness?.model_passthrough?.support === 'supported';
}

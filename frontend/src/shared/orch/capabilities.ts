// Wire-format boundary for `tack_orch::Capabilities` — what a registered
// control plane can actually do, surfaced on
// `GET /api/control-planes/{id}` (`ControlPlaneResponse.capabilities`) and
// `GET /api/fleet` (`FleetEntry.capabilities`). Every field here is a
// snake_case projection of `crates/tack-api/src/handlers/orch.rs`'s
// `CapabilitiesResponse` and its five `*Level` enums — not a guess; the enum
// string values are pinned in `docs/openapi.json`.
//
// A capability is a value, never a provider check: every non-boolean field
// carries a `reason: string` written by the *adapter* (docket, or later
// GitHub Actions), never invented by this API layer or by a component. A
// component that wants to know whether a control should be interactive, and
// what to say when it isn't, imports {@link gate} (or one of the named
// per-field helpers below) from this file — never a hard-coded string, and
// never an equality check against a plane's `kind` string (the grep this
// repo's CI runs to catch a regression stays at zero hits).
//
// Two ad-hoc capability bits predate this module and are retired in favour
// of it: the approvals inbox's old grant-availability boolean
// (`features/approvals/api.ts` — see that file's header for why its
// replacement isn't literally a `Capabilities` read: it was never a
// provider capability, it was a Tack-server auth gate) and
// `useAgentActivityMap`'s `orchAvailable()` used as a dispatch gate (see
// that file's doc comment for the residual wiring gap that was found but
// could not be closed without editing a file outside this module).

/** Wire mirror of `tack_orch::Support` — pause/resume readiness. */
export type SupportLevel = 'unsupported' | 'advisory' | 'supported';

/** Wire mirror of `tack_orch::EventScope` — what a provider's `events()` is
 *  keyed on (docket: per-project; a future run-scoped adapter: per-run). */
export type EventScopeLevel = 'none' | 'run' | 'project' | 'plane';

/** Wire mirror of `tack_orch::DecisionSupport`. */
export type DecisionSupportLevel = 'none' | 'poll' | 'push';

/** Wire mirror of `tack_orch::UsageSupport`. */
export type UsageSupportLevel = 'not_measured' | 'from_provider' | 'from_gateway';

/** Wire mirror of `tack_orch::ModelSelection` — per §II.1.2, "the acceptance
 *  test for the whole mechanism": docket owns its own routing and silently
 *  ignores a caller-supplied model (`unsupported`); a future adapter that
 *  forwards it verbatim would report `honoured`. A picker built against this
 *  field must render three different things, never silently do nothing. */
export type ModelSelectionLevel = 'unsupported' | 'advisory' | 'honoured';

/** `pause`/`resume`/`event_scope`/`decisions`/`usage`/`model_selection`'s
 *  common wire shape — level plus why. `reason` is always adapter-authored
 *  (never invented here), and is present at every level, not only the
 *  disabled one, so a caller can show it as a caveat on an `advisory`-level
 *  control too. Mirrors `tack_orch::Rated<T>` / the API's `*Capability`
 *  wrapper structs (`SupportCapability`, `EventScopeCapability`, ...). */
export interface RatedCapability<Level extends string> {
  level: Level;
  reason: string;
}

/** Wire mirror of `tack_orch::Capabilities` / the API's
 *  `CapabilitiesResponse`. Boolean fields (`dispatch`, `cancel`, `artifacts`,
 *  `runtimes`, `plane_metrics`, `provisioning`) carry no `reason` on the
 *  wire — there is nothing for a component to render beyond the boolean
 *  itself, so {@link gate} is only meaningful for the six `RatedCapability`
 *  fields below. */
export interface Capabilities {
  dispatch: boolean;
  cancel: boolean;
  pause: RatedCapability<SupportLevel>;
  resume: RatedCapability<SupportLevel>;
  event_scope: RatedCapability<EventScopeLevel>;
  artifacts: boolean;
  decisions: RatedCapability<DecisionSupportLevel>;
  usage: RatedCapability<UsageSupportLevel>;
  model_selection: RatedCapability<ModelSelectionLevel>;
  runtimes: boolean;
  plane_metrics: boolean;
  provisioning: boolean;
}

/** What a gated control needs to render itself: whether to allow
 *  interaction, and the reason to show either way. `reason` is always the
 *  adapter-authored string from the capability payload — a control must
 *  never substitute a hard-coded string for it and must never branch on
 *  which provider `kind` is registered. */
export interface CapabilityGate {
  enabled: boolean;
  reason: string;
}

/**
 * Turns any `RatedCapability<Level>` into a {@link CapabilityGate}.
 * `offLevel` names the single level that disables the control — every other
 * level (e.g. a provider that reports `'advisory'` rather than
 * `'unsupported'`) still enables it, with the same adapter-supplied reason
 * carried through as a caveat rather than a block. This is the one place
 * "which level means off" is decided, so a component never has to encode
 * that judgement itself alongside a UI it didn't design against.
 */
export function gate<Level extends string>(
  capability: RatedCapability<Level>,
  offLevel: Level,
): CapabilityGate {
  return { enabled: capability.level !== offLevel, reason: capability.reason };
}

/** `pause`/`resume`: disabled only at `'unsupported'` — `'advisory'` (a
 *  provider that accepts the request but doesn't guarantee it takes effect)
 *  still lets the control fire. */
export const gatePause = (c: Capabilities): CapabilityGate => gate(c.pause, 'unsupported');
export const gateResume = (c: Capabilities): CapabilityGate => gate(c.resume, 'unsupported');

/** `model_selection`: disabled only at `'unsupported'` (docket today). */
export const gateModelSelection = (c: Capabilities): CapabilityGate =>
  gate(c.model_selection, 'unsupported');

/** `decisions`: disabled only at `'none'` — both `'poll'` (docket) and
 *  `'push'` still mean "there is a decision inbox to read." */
export const gateDecisions = (c: Capabilities): CapabilityGate => gate(c.decisions, 'none');

/** `usage`: disabled only at `'not_measured'` — a control that shows a
 *  token/cost figure should hide or grey it out below that level, but a
 *  gateway-sourced figure (`'from_gateway'`) is just as renderable as a
 *  provider-sourced one (`'from_provider'`). */
export const gateUsage = (c: Capabilities): CapabilityGate => gate(c.usage, 'not_measured');

/** `event_scope`: disabled only at `'none'`. */
export const gateEventScope = (c: Capabilities): CapabilityGate => gate(c.event_scope, 'none');

import { type Component, Show } from 'solid-js';
import type { CapabilityGate } from './capabilities';

export interface CapabilityNoteProps {
  /** What the gate is about, e.g. "Pause", "Model selection" — shown as a
   *  plain label prefix, never a provider name. */
  label: string;
  gate: CapabilityGate;
}

/**
 * The one place a `CapabilityGate` becomes visible copy. Renders nothing
 * while the gate is enabled; once disabled, always shows the reason
 * **verbatim from the capability payload** — never a string this component
 * invents. A capability is a value, never a provider check, applied to
 * read-only/informational controls: `features/fleet/FleetRow.tsx` and
 * `features/settings/orchestrationSettings/ControlPlanesManager.tsx` both
 * use this instead of writing their own copy, so there is exactly one place
 * that could regress into a hard-coded string.
 *
 * Deliberately a plain note, not a disabled `<button>`: no pause/resume
 * *action* exists on the wire yet (that lands with the Wave C trait
 * reshape), and rendering an inert button that looks clickable would be a
 * worse failure than rendering none — see the plan's own framing, "the UI
 * disables what a provider cannot do and names why," which a note satisfies
 * without implying an affordance that isn't there yet.
 */
const CapabilityNote: Component<CapabilityNoteProps> = (props) => (
  <Show when={!props.gate.enabled}>
    <div
      style={{
        'font-size': '10.5px',
        color: 'var(--color-text-tertiary)',
        'margin-top': '2px',
      }}
    >
      {props.label}: {props.gate.reason}
    </div>
  </Show>
);

export default CapabilityNote;

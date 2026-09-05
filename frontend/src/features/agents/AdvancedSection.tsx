import { type Component, createSignal, Show } from 'solid-js';
import { IconChevronDown, IconChevronRight } from '../../shared/ui/icons';
import RunnerFleetSection from './runnerFleet/RunnerFleetSection';

/**
 * Everything `RunnerFleetSection.tsx` already provided on the old Fleet
 * page (enrollment, fleets, agent profiles, model profiles) — moved into
 * this feature (`architecture.test.ts` forbids a `features/*` importing
 * another `features/*`, so the whole `runnerFleet/` directory lives here
 * instead of being reached across that boundary) and collapsed by default.
 * Its own vocabulary ("runner", "fleet", "enroll", "capacity"…) is the
 * vocabulary the numbered steps above deliberately never use, so none of
 * it — including this section's own label — renders while collapsed.
 * Nothing inside `runnerFleet/` is edited here.
 */
const AdvancedSection: Component = () => {
  const [open, setOpen] = createSignal(false);

  return (
    <section id="advanced" class="space-y-3 border-t pt-6" style={{ 'border-color': 'var(--color-border-light)' }}>
      <button
        type="button"
        class="flex items-center gap-2 text-sm font-semibold"
        style={{ color: 'var(--color-text-primary)' }}
        onClick={() => setOpen(!open())}
        aria-expanded={open()}
      >
        <Show when={open()} fallback={<IconChevronRight size={14} />}>
          <IconChevronDown size={14} />
        </Show>
        Advanced
      </button>
      <Show when={open()}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          Enroll additional machines, manage fleets, and configure agent/model profiles.
        </p>
        <RunnerFleetSection />
      </Show>
    </section>
  );
};

export default AdvancedSection;

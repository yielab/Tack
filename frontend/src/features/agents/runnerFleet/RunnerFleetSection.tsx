import { type Component, Show, createSignal } from 'solid-js';
import { Tabs } from '../../../shared/ui';
import EnrollmentPanel from './EnrollmentPanel';
import FleetsPanel from './FleetsPanel';
import AgentProfilesPanel from './AgentProfilesPanel';
import ModelProfilesPanel from './ModelProfilesPanel';

const TABS = [
  { id: 'runners', label: 'Runners' },
  { id: 'fleets', label: 'Fleets' },
  { id: 'agent-profiles', label: 'Agent profiles' },
  { id: 'model-profiles', label: 'Model profiles' },
] as const;

type TabId = (typeof TABS)[number]['id'];

/**
 * Part III's runner-fleet management surface (TODO.md III-E3): enrollment/
 * revocation, fleets, agent profiles, model profiles — everything
 * `frontend/src/shared/execution`'s `runnersApi`/`fleetsApi`/
 * `agentProfilesApi`/`modelProfilesApi` (card E2) expose a real, wired
 * endpoint for today. Deliberately does NOT include a live runner health/
 * capacity roster — no `GET /runners` endpoint exists to back one (see
 * `EnrollmentPanel.tsx`'s header comment and this card's handoff, gap 1).
 *
 * Mounted as the PRIMARY content of `FleetPage.tsx`, with the pre-existing
 * Part II Docket control-plane view moved into a clearly labeled legacy
 * section beneath it — see `FleetPage.tsx`'s own header comment for why.
 */
const RunnerFleetSection: Component = () => {
  const [active, setActive] = createSignal<TabId>('runners');

  return (
    <Tabs tabs={[...TABS]} active={active()} onChange={(id) => setActive(id as TabId)}>
      <Show when={active() === 'runners'}>
        <EnrollmentPanel />
      </Show>
      <Show when={active() === 'fleets'}>
        <FleetsPanel />
      </Show>
      <Show when={active() === 'agent-profiles'}>
        <AgentProfilesPanel />
      </Show>
      <Show when={active() === 'model-profiles'}>
        <ModelProfilesPanel />
      </Show>
    </Tabs>
  );
};

export default RunnerFleetSection;

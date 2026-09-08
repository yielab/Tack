import { type Component } from 'solid-js';
import { Badge } from '../../../shared/ui';
import type { OrchLink } from './api';

export interface CompatibilityPanelProps {
  /** The current link — both fields it reads come from the same `GET
   *  /orch-link` fetch `OrchestrationPanel` already made, so this panel
   *  makes no request of its own. */
  link: OrchLink;
}

/**
 * Names which compatibility state this project's Docket bridge is in, and
 * why — both values come straight from the API response
 * (`OrchLink.compatibility_label`/`compatibility_policy`), never a string
 * literal here, so this panel can't drift from the backend's decision.
 */
const CompatibilityPanel: Component<CompatibilityPanelProps> = (props) => (
  <section aria-labelledby="orch-compatibility-heading">
    <h2
      id="orch-compatibility-heading"
      class="text-base font-semibold mb-3"
      style={{ color: 'var(--color-text-primary)' }}
    >
      Compatibility
    </h2>
    <div class="rounded-lg p-4 space-y-2" style={{ border: '1px solid var(--color-border-light)' }}>
      <Badge tone="neutral" class="font-mono">
        {props.link.compatibility_label}
      </Badge>
      <p style={{ 'font-size': '12.5px', color: 'var(--color-text-secondary)' }}>
        {props.link.compatibility_policy}
      </p>
    </div>
  </section>
);

export default CompatibilityPanel;

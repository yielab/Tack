import { type Component, For } from 'solid-js';
import type { EconomicsSlice } from './api';
import {
  formatTokens,
  formatEstimatedCost,
  formatRate,
  describeCostPerItem,
} from './format';

const thStyle = {
  padding: '10px 14px',
  'text-align': 'left' as const,
  'font-size': '10.5px',
  'font-weight': 700,
  'letter-spacing': '.05em',
  'text-transform': 'uppercase' as const,
  color: 'var(--color-text-tertiary)',
  'border-bottom': '1px solid var(--color-border-light)',
  'white-space': 'nowrap' as const,
};

const tdStyle = {
  padding: '10px 14px',
  'font-size': '13px',
  color: 'var(--color-text-primary)',
  'border-bottom': '1px solid var(--color-border-light)',
  'white-space': 'nowrap' as const,
};

/**
 * One breakdown table — reused for both `by_project_type` and `by_item_type`,
 * slicing every metric by that dimension. Tokens
 * render first and more prominently than the cost column.
 * `cost_usd_estimated_per_item` — the headline number — always goes through
 * `describeCostPerItem`, which withholds it below the
 * stated minimum sample size rather than showing a number a handful of items can't
 * support.
 */
const SliceTable: Component<{
  caption: string;
  dimensionLabel: string;
  slices: EconomicsSlice[];
  minSampleSize: number;
}> = (props) => (
  <div
    class="overflow-x-auto rounded-lg"
    style={{ border: '1px solid var(--color-border-light)' }}
    tabindex="0"
    role="region"
    aria-label={props.caption}
  >
    <table class="w-full" style={{ 'border-collapse': 'collapse' }}>
      <caption class="sr-only">{props.caption}</caption>
      <thead>
        <tr>
          <th scope="col" style={thStyle}>{props.dimensionLabel}</th>
          <th scope="col" style={thStyle}>Completed</th>
          <th scope="col" style={thStyle}>Agent / Human</th>
          <th scope="col" style={thStyle}>Tokens in / out</th>
          <th scope="col" style={thStyle}>Est. cost</th>
          <th scope="col" style={thStyle}>Est. cost / item</th>
          <th scope="col" style={thStyle}>Rework rate</th>
        </tr>
      </thead>
      <tbody>
        <For each={props.slices}>
          {(slice) => (
            <tr>
              <td style={{ ...tdStyle, 'font-weight': 600 }}>{slice.key}</td>
              <td style={tdStyle}>{slice.completed_item_count}</td>
              <td style={tdStyle}>
                {slice.agent_completed_count} / {slice.human_completed_count}
              </td>
              <td style={{ ...tdStyle, 'font-weight': 600 }}>
                {formatTokens(slice.tokens_in)} / {formatTokens(slice.tokens_out)}
              </td>
              <td style={{ ...tdStyle, color: 'var(--color-text-secondary)', 'font-size': '12px' }}>
                {formatEstimatedCost(slice.cost_usd_estimated, slice.pricing_snapshot_at)}
              </td>
              <td style={{ ...tdStyle, color: 'var(--color-text-secondary)', 'font-size': '12px' }}>
                {describeCostPerItem(
                  slice.cost_usd_estimated_per_item,
                  slice.pricing_snapshot_at,
                  slice.agent_completed_count,
                  props.minSampleSize,
                )}
              </td>
              <td style={{ ...tdStyle, color: 'var(--color-text-secondary)', 'font-size': '12px' }}>
                {slice.rework.below_min_sample ? 'too few samples' : formatRate(slice.rework.rate)}
              </td>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  </div>
);

export default SliceTable;

import { type Component, type JSX } from 'solid-js';

/** Style for a WIP-limit chip. Turns danger when the count exceeds the limit.
 *
 *  Not-exceeded uses `--color-text-secondary` (not `-tertiary`, despite this
 *  being the "quiet" state): `-tertiary` on `--color-chip` (== bg-subtle)
 *  fails WCAG AA in all three dark-mode palettes (as low as 3.41:1) — the
 *  same class of bug as PriorityDot's old low/none color. `-secondary` on
 *  bg-subtle is the identical pair Badge's `neutral` tone already uses and
 *  clears 4.5:1 everywhere. */
export function wipChipStyle(count: number, limit: number): JSX.CSSProperties {
  const exceeded = count > limit;
  return {
    'font-family': 'var(--font-mono)',
    'font-size': '10px',
    'font-weight': 500,
    padding: '1px 6px',
    'border-radius': '5px',
    'background-color': exceeded ? 'var(--color-danger-100)' : 'var(--color-chip)',
    color: exceeded ? 'var(--color-danger-600)' : 'var(--color-text-secondary)',
  };
}

export interface WipChipProps {
  count: number;
  limit: number;
}

/** `count/limit` chip; the board passes a column's size and `wip_limit`. */
const WipChip: Component<WipChipProps> = (props) => (
  <span style={wipChipStyle(props.count, props.limit)}>
    {props.count}/{props.limit}
  </span>
);

export default WipChip;

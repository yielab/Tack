import { For, type Component, type JSX } from 'solid-js';
import clsx from 'clsx';

export type AvatarSize = 'sm' | 'md';

const PX: Record<AvatarSize, number> = { sm: 22, md: 26 };
const FONT: Record<AvatarSize, string> = { sm: '10px', md: '11px' };

/** Deterministic hue (0–359) from a name, so the same person is always the
 *  same color without storing one. Stable across reloads and machines. */
export function hueFromName(name: string): number {
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) % 360;
  }
  return h;
}

/** The fixed saturation/lightness the avatar background is generated at —
 *  only the hue varies per name. Shared with `textColorForHue` so the
 *  contrast check always matches the color actually painted below. */
const AVATAR_SAT = 45;
const AVATAR_LIGHT = 50;

/** hsl(h, s%, l%) -> sRGB components in 0–1, using the same conversion the
 *  CSS `hsl()` background below resolves to in the browser (CSS Color spec),
 *  so the luminance computed here matches what's actually painted. Exported
 *  for the exhaustive-hue-space contrast test (Avatar.test.tsx). */
export function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const sat = s / 100;
  const light = l / 100;
  const c = (1 - Math.abs(2 * light - 1)) * sat;
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let r1 = 0;
  let g1 = 0;
  let b1 = 0;
  if (hp < 1) [r1, g1, b1] = [c, x, 0];
  else if (hp < 2) [r1, g1, b1] = [x, c, 0];
  else if (hp < 3) [r1, g1, b1] = [0, c, x];
  else if (hp < 4) [r1, g1, b1] = [0, x, c];
  else if (hp < 5) [r1, g1, b1] = [x, 0, c];
  else [r1, g1, b1] = [c, 0, x];
  const m = light - c / 2;
  return [r1 + m, g1 + m, b1 + m];
}

/** WCAG relative luminance of an sRGB color given as 0–1 components. Exported
 *  for Avatar.test.tsx. */
export function relativeLuminance(r: number, g: number, b: number): number {
  const f = (u: number) => (u <= 0.03928 ? u / 12.92 : ((u + 0.055) / 1.055) ** 2.4);
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

/** WCAG contrast ratio between two relative luminances (order-independent).
 *  Exported for Avatar.test.tsx. */
export function contrastRatio(l1: number, l2: number): number {
  const [a, b] = l1 > l2 ? [l1, l2] : [l2, l1];
  return (a + 0.05) / (b + 0.05);
}

/**
 * Pick the initials color (pure white or near-black) that clears WCAG AA
 * (4.5:1, this is small bold text — not "large text" by the 18.66px-bold
 * cutoff) against the per-name generated `hsl(hue, 45%, 50%)` background.
 *
 * The background hue is intentionally NOT palette/mode-aware (it carries
 * per-person identity, independent of theme), so the text pick can't be
 * either — it can't reuse `--color-text-inverse` etc., which flip with
 * `.dark`/`data-palette` and would be right for the *theme's* accent, not for
 * an arbitrary computed hue. `--color-avatar-ink-white` /
 * `--color-avatar-ink-black` in index.css are deliberately defined once, only
 * in the base `:root` block, and never overridden — true constants.
 *
 * This always finds an AA-passing choice: picking whichever of pure white
 * (L=1) / pure black (L=0) has higher contrast against a background of
 * luminance L guarantees >=4.5:1 for every possible L, verified exhaustively
 * across all 360 integer hues at this saturation/lightness (worst case
 * 4.59:1 at hue 10 — see `Avatar.test.tsx` for the exhaustive proof).
 */
export function textColorForHue(hue: number): string {
  const [r, g, b] = hslToRgb(hue, AVATAR_SAT, AVATAR_LIGHT);
  const bgLum = relativeLuminance(r, g, b);
  const whiteContrast = contrastRatio(bgLum, 1);
  const blackContrast = contrastRatio(bgLum, 0);
  return whiteContrast >= blackContrast
    ? 'var(--color-avatar-ink-white)'
    : 'var(--color-avatar-ink-black)';
}

/** Up to two uppercase initials from a display name. */
export function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export interface AvatarProps {
  /** Display name (drives the color) or pre-computed initials. */
  name: string;
  size?: AvatarSize;
  /** Render an outline ring (used inside stacks over a surface). */
  ring?: boolean;
  title?: string;
  style?: JSX.CSSProperties;
}

const Avatar: Component<AvatarProps> = (props) => {
  const size = () => props.size ?? 'md';
  const px = () => PX[size()];
  const hue = () => hueFromName(props.name);
  return (
    <span
      title={props.title ?? props.name}
      style={{
        width: `${px()}px`,
        height: `${px()}px`,
        'border-radius': '99px',
        background: `hsl(${hue()} ${AVATAR_SAT}% ${AVATAR_LIGHT}%)`,
        color: textColorForHue(hue()),
        display: 'inline-flex',
        'align-items': 'center',
        'justify-content': 'center',
        'font-size': FONT[size()],
        'font-weight': 700,
        'flex-shrink': 0,
        ...(props.ring ? { border: '2px solid var(--color-bg-base)' } : {}),
        ...props.style,
      }}
    >
      {initialsOf(props.name)}
    </span>
  );
};

export interface AvatarStackProps {
  names: string[];
  max?: number;
  size?: AvatarSize;
  class?: string;
}

/** Overlapping avatars with a `+N` overflow chip. */
export const AvatarStack: Component<AvatarStackProps> = (props) => {
  const max = () => props.max ?? 4;
  const size = () => props.size ?? 'md';
  const shown = () => props.names.slice(0, max());
  const overflow = () => Math.max(0, props.names.length - max());
  const px = () => PX[size()];
  return (
    <div class={clsx('flex items-center', props.class)}>
      <For each={shown()}>
        {(name, i) => (
          <Avatar name={name} size={size()} ring style={i() === 0 ? {} : { 'margin-left': '-8px' }} />
        )}
      </For>
      {overflow() > 0 && (
        <span
          style={{
            width: `${px()}px`,
            height: `${px()}px`,
            'border-radius': '99px',
            background: 'var(--color-chip)',
            color: 'var(--color-text-secondary)',
            display: 'inline-flex',
            'align-items': 'center',
            'justify-content': 'center',
            'font-size': FONT[size()],
            'font-weight': 700,
            border: '2px solid var(--color-bg-base)',
            'margin-left': '-8px',
          }}
        >
          +{overflow()}
        </span>
      )}
    </div>
  );
};

export default Avatar;

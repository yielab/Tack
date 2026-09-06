import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import Avatar, {
  hueFromName,
  initialsOf,
  textColorForHue,
  hslToRgb,
  relativeLuminance,
  contrastRatio,
} from './Avatar';

// Avatar's per-name background is `hsl(hue, 45%, 50%)` — a
// color generated outside the design-token system, independent of the
// active palette/mode. Fixed white initials text used to fail WCAG AA
// (4.5:1, this is small bold text, not "large text") against ~56% of the
// possible hues. `textColorForHue` picks between two fixed extremes (pure
// white / pure black, via `--color-avatar-ink-white` / `-black` in
// index.css) by the *actual computed luminance* of the background that hue
// paints, so it must never fail — for ANY hue, not just the handful an axe
// scan happens to render. This test proves that over the full 360° space,
// independently recomputing the contrast for whichever token the function
// picked, rather than re-trusting its internal decision.

const MIN_AA_CONTRAST = 4.5;

describe('textColorForHue', () => {
  it('clears WCAG AA (4.5:1) against the generated background for every one of the 360 possible hues', () => {
    const failures: Array<{ hue: number; contrast: number; picked: string }> = [];

    for (let hue = 0; hue < 360; hue++) {
      const [r, g, b] = hslToRgb(hue, 45, 50);
      const bgLum = relativeLuminance(r, g, b);
      const picked = textColorForHue(hue);

      // The function only ever returns one of these two tokens — assert that
      // invariant too, since the contrast check below assumes it.
      expect(['var(--color-avatar-ink-white)', 'var(--color-avatar-ink-black)']).toContain(picked);

      const textLum = picked === 'var(--color-avatar-ink-white)' ? 1 : 0;
      const contrast = contrastRatio(bgLum, textLum);
      if (contrast < MIN_AA_CONTRAST) {
        failures.push({ hue, contrast, picked });
      }
    }

    expect(failures, JSON.stringify(failures, null, 2)).toEqual([]);
  });

  it('regression guard: fixed white text (the pre-fix behavior) does fail AA for a majority of hues', () => {
    // Documents *why* this needed a fix at all — if this ever starts
    // failing, the background generation formula changed and the fix above
    // (and its "verified exhaustively" claim in Avatar.tsx) needs re-checking.
    let whiteFailures = 0;
    for (let hue = 0; hue < 360; hue++) {
      const [r, g, b] = hslToRgb(hue, 45, 50);
      const bgLum = relativeLuminance(r, g, b);
      if (contrastRatio(bgLum, 1) < MIN_AA_CONTRAST) whiteFailures++;
    }
    expect(whiteFailures).toBeGreaterThan(180); // >50% of 360
  });

  it('picks white for a known-dark background hue and black for a known-light one', () => {
    // hue 240 (blue, low luminance at s45/l50) vs hue 60 (yellow, high
    // luminance) — sanity check the direction of the decision, not just the
    // magnitude.
    expect(textColorForHue(240)).toBe('var(--color-avatar-ink-white)');
    expect(textColorForHue(60)).toBe('var(--color-avatar-ink-black)');
  });
});

const disposers: Array<() => void> = [];

function mount(name: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Avatar name={name} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});

describe('Avatar', () => {
  it('renders initials and sets the text color from textColorForHue for the same name', () => {
    const name = 'Avery Green';
    const c = mount(name);
    const span = c.querySelector('span') as HTMLElement;
    expect(span.textContent).toBe(initialsOf(name));
    expect(span.style.color).toBe(textColorForHue(hueFromName(name)));
  });

  it('never emits a raw hex color for text or background (lint:tokens ratchet)', () => {
    const c = mount('Someone Random');
    const span = c.querySelector('span') as HTMLElement;
    // Text must be a token reference. Background is a computed hsl() — jsdom
    // normalizes the inline `hsl(...)` string to an equivalent `rgb(...)`
    // computed value, so assert the invariant the token gate actually
    // enforces (no raw hex literal) rather than the exact serialization.
    expect(span.style.color).toMatch(/^var\(--color-/);
    expect(span.style.background).not.toMatch(/#[0-9a-fA-F]{3,8}/);
  });
});

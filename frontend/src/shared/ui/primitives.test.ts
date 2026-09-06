import { describe, it, expect } from 'vitest';
import { hueFromName, initialsOf } from './Avatar';
import { typeKey, typeBadgeTone } from './TypeBadge';
import { priorityColor, priorityLabel } from './PriorityDot';
import { wipChipStyle } from './WipChip';

describe('hueFromName', () => {
  it('is deterministic and in range [0, 360)', () => {
    for (const name of ['Sara Reyes', 'Milo Kade', '', 'x']) {
      const h = hueFromName(name);
      expect(h).toBeGreaterThanOrEqual(0);
      expect(h).toBeLessThan(360);
      expect(hueFromName(name)).toBe(h);
    }
  });

  it('gives different names different hues (typically)', () => {
    expect(hueFromName('Sara Reyes')).not.toBe(hueFromName('Jun Park'));
  });
});

describe('initialsOf', () => {
  it('takes first + last initials', () => {
    expect(initialsOf('Sara Reyes')).toBe('SR');
  });
  it('handles single word', () => {
    expect(initialsOf('Cher')).toBe('CH');
  });
  it('falls back to ? when empty', () => {
    expect(initialsOf('   ')).toBe('?');
  });
});

describe('typeKey', () => {
  it('passes through string types', () => {
    expect(typeKey('epic')).toBe('epic');
  });
  it('unwraps custom types', () => {
    expect(typeKey({ custom: 'spike' })).toBe('spike');
  });
});

describe('typeBadgeTone', () => {
  it('colors epic/feature/bug distinctly and neutralizes the rest', () => {
    expect(typeBadgeTone('epic').fg).toContain('accent-ink');
    expect(typeBadgeTone('feature').fg).toContain('accent2');
    expect(typeBadgeTone('bug').fg).toContain('danger');
    expect(typeBadgeTone('task').bg).toContain('chip');
    expect(typeBadgeTone('spike').bg).toContain('chip');
  });
});

describe('priorityColor / priorityLabel', () => {
  it('maps each priority to a token', () => {
    expect(priorityColor('critical')).toContain('danger');
    expect(priorityColor('high')).toContain('warning');
    expect(priorityColor('medium')).toContain('accent2');
    // -secondary, not -tertiary: this value also gets used as a solid fill
    // behind `--color-text-inverse` text (Calendar/Timeline priority chips),
    // and -tertiary fails WCAG AA there in all three dark-mode palettes
    // (see src/shared/ui/PriorityDot.tsx).
    expect(priorityColor('low')).toContain('secondary');
    expect(priorityColor('none')).toContain('secondary');
  });
  it('capitalizes labels', () => {
    expect(priorityLabel('high')).toBe('High');
  });
});

describe('wipChipStyle', () => {
  it('stays neutral within the limit', () => {
    const s = wipChipStyle(3, 4);
    expect(s['background-color']).toContain('chip');
    // -secondary, not -tertiary: see wipChipStyle's doc comment (WCAG AA).
    expect(s.color).toContain('secondary');
  });
  it('turns danger when exceeded', () => {
    const s = wipChipStyle(5, 4);
    expect(s['background-color']).toContain('danger');
    expect(s.color).toContain('danger');
  });
  it('treats equal-to-limit as not exceeded', () => {
    expect(wipChipStyle(4, 4)['background-color']).toContain('chip');
  });
});

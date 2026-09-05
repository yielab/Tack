import { describe, it, expect } from 'vitest';
import { parseOptionalJsonObject, formatCapacity, formatExpiresIn, formatLabelChips } from './format';

describe('parseOptionalJsonObject', () => {
  it('treats a blank input as an empty object, not an error', () => {
    expect(parseOptionalJsonObject('   ', 'Tool policy')).toEqual({ ok: true, value: {} });
  });

  it('parses a valid JSON object', () => {
    expect(parseOptionalJsonObject('{"allow":["read"]}', 'Tool policy')).toEqual({
      ok: true,
      value: { allow: ['read'] },
    });
  });

  it('rejects invalid JSON with a field-labeled message', () => {
    const result = parseOptionalJsonObject('{not json', 'Tool policy');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('Tool policy');
  });

  it('rejects a JSON array — must be an object', () => {
    const result = parseOptionalJsonObject('[1,2,3]', 'Limits');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain('JSON object');
  });

  it('rejects a bare JSON scalar', () => {
    const result = parseOptionalJsonObject('42', 'Limits');
    expect(result.ok).toBe(false);
  });
});

describe('formatCapacity', () => {
  it('always shows both available and total, never available alone', () => {
    expect(formatCapacity(4, 2)).toBe('2 / 4 slots available (as declared at enrollment)');
  });

  it('singularizes "slot" for a total of 1', () => {
    expect(formatCapacity(1, 1)).toBe('1 / 1 slot available (as declared at enrollment)');
  });
});

describe('formatExpiresIn', () => {
  const now = new Date('2026-08-10T12:00:00Z').getTime();

  it('reads "expired" for a past timestamp, never a negative duration', () => {
    expect(formatExpiresIn('2026-08-10T11:00:00Z', now)).toBe('expired');
  });

  it('reads minutes for under an hour', () => {
    expect(formatExpiresIn('2026-08-10T12:45:00Z', now)).toBe('expires in 45m');
  });

  it('reads hours for an hour or more', () => {
    expect(formatExpiresIn('2026-08-10T15:00:00Z', now)).toBe('expires in 3h');
  });

  it('reads "under a minute" rather than "0m"', () => {
    expect(formatExpiresIn('2026-08-10T12:00:30Z', now)).toBe('expires in under a minute');
  });

  it('reads an explicit "unknown expiry" for an unparsable timestamp', () => {
    expect(formatExpiresIn('not-a-date', now)).toBe('unknown expiry');
  });
});

describe('formatLabelChips', () => {
  it('formats a flat string map as "key: value" chips', () => {
    expect(formatLabelChips({ region: 'us-east', gpu: 'true' })).toEqual([
      'region: us-east',
      'gpu: true',
    ]);
  });

  it('stringifies a non-string value rather than crashing', () => {
    expect(formatLabelChips({ count: 3 })).toEqual(['count: 3']);
  });

  it('returns an empty list for null, an array, or a non-object — never fabricates chips', () => {
    expect(formatLabelChips(null)).toEqual([]);
    expect(formatLabelChips([1, 2])).toEqual([]);
    expect(formatLabelChips('not an object')).toEqual([]);
  });
});

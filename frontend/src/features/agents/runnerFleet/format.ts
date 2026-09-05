// Pure formatting/parsing helpers for the Part III runner-fleet UI
// (TODO.md III-E3). Kept isolated and unit-tested, separate from
// `../format.ts` (the pre-existing Part II Docket-fleet formatters) —
// deliberately not shared, per III.0's vocabulary rule: `Runner`/`Fleet`
// (this file) and Docket's `FleetAgent`/control-plane roster (`../format.ts`)
// are different domain concepts that happen to reuse the word "fleet."

/** Optional JSON object field (`tool_policy`, `limits`, `default_policy`,
 *  `labels`, `capability_snapshot`) typed by the wire as `unknown`. Every
 *  create form in this folder lets the operator type raw JSON for these —
 *  this is the one shared parse path, so a malformed body always fails the
 *  same explicit way instead of each form inventing its own JSON.parse
 *  try/catch. Returns `{}` for a blank/whitespace-only input rather than
 *  treating "the operator left it empty" as an error. */
export function parseOptionalJsonObject(
  raw: string,
  fieldLabel: string,
): { ok: true; value: Record<string, unknown> } | { ok: false; error: string } {
  const trimmed = raw.trim();
  if (!trimmed) return { ok: true, value: {} };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return { ok: false, error: `${fieldLabel} must be valid JSON (or left blank)` };
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    return { ok: false, error: `${fieldLabel} must be a JSON object (or left blank)` };
  }
  return { ok: true, value: parsed as Record<string, unknown> };
}

/** `total`/`available` capacity as one compact phrase — always both numbers
 *  together, never `available` alone (which would read as a live occupancy
 *  reading rather than what the operator declared at enrollment time). */
export function formatCapacity(total: number, available: number): string {
  const slot = total === 1 ? 'slot' : 'slots';
  return `${available} / ${total} ${slot} available (as declared at enrollment)`;
}

/** Countdown to an enrollment token's expiry, or an explicit "expired"
 *  rather than a negative duration — this token can no longer be redeemed
 *  by a runner either way, so the two must read the same to an operator. */
export function formatExpiresIn(expiresAtIso: string, nowMs: number = Date.now()): string {
  const expiresAt = new Date(expiresAtIso).getTime();
  if (Number.isNaN(expiresAt)) return 'unknown expiry';
  const diffMs = expiresAt - nowMs;
  if (diffMs <= 0) return 'expired';
  if (diffMs < 60_000) return 'expires in under a minute';
  const diffMin = Math.floor(diffMs / 60_000);
  if (diffMin < 60) return `expires in ${diffMin}m`;
  const diffHr = Math.floor(diffMin / 60);
  return `expires in ${diffHr}h`;
}

/** `labels` is `Record<string, string>` by convention (the runner-protocol
 *  fixtures' own shape — `types.ts`'s `RunnerCapabilities.labels`) but
 *  arrives here as `unknown` off a hand-typed JSON field
 *  (`EnrollRunnerInput.labels`). Never assume the shape; render nothing
 *  fabricated for a value that isn't actually a flat string map. */
export function formatLabelChips(labels: unknown): string[] {
  if (typeof labels !== 'object' || labels === null || Array.isArray(labels)) return [];
  const chips: string[] = [];
  for (const [key, value] of Object.entries(labels as Record<string, unknown>)) {
    if (typeof value === 'string') chips.push(`${key}: ${value}`);
    else chips.push(`${key}: ${JSON.stringify(value)}`);
  }
  return chips;
}

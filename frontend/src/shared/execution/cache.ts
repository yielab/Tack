// Framework-agnostic request/attempt cache primitives: a stale event can
// never overwrite a newer snapshot. No SolidJS dependency and no I/O —
// plain data structures, independently unit-testable, that `store.ts`
// composes with reactivity and network calls layered on top.

/**
 * A per-key cache where a write only applies if it is at least as new as
 * whatever is already stored for that key. This is the entire mechanism
 * behind "a stale event can never overwrite a newer snapshot": two
 * in-flight fetches for the same key can resolve in either order over the
 * network, but only the one carrying the higher `version` — assigned by
 * {@link SequenceAllocator} at *request-issue* time, not response time — is
 * ever allowed to win.
 */
export class VersionedCache<T> {
  private readonly entries = new Map<string, { value: T; version: number }>();

  get(key: string): T | undefined {
    return this.entries.get(key)?.value;
  }

  versionOf(key: string): number | undefined {
    return this.entries.get(key)?.version;
  }

  has(key: string): boolean {
    return this.entries.has(key);
  }

  /**
   * Applies `value` iff `version` is greater than or equal to the version
   * currently stored for `key` (or no entry exists yet). A strictly older
   * version is dropped silently — the caller can check the boolean return
   * to know whether its write actually landed. Equal versions are allowed
   * to apply (a replay of the same fetch generation overwrites with
   * identical intent, never a correctness issue).
   */
  set(key: string, value: T, version: number): boolean {
    const current = this.entries.get(key);
    if (current && version < current.version) return false;
    this.entries.set(key, { value, version });
    return true;
  }

  delete(key: string): void {
    this.entries.delete(key);
  }

  keys(): string[] {
    return [...this.entries.keys()];
  }
}

/**
 * Issues a monotonically increasing sequence number per key. `store.ts`
 * calls {@link next} once per key at the moment it *starts* a fetch (or
 * applies an optimistic local mutation) and uses that number as the
 * `version` passed to {@link VersionedCache.set} once the corresponding
 * result is known — so ordering is fixed by *when the operation began*, not
 * by whenever the network happens to deliver its response.
 */
export class SequenceAllocator {
  private readonly counters = new Map<string, number>();

  next(key: string): number {
    const value = (this.counters.get(key) ?? 0) + 1;
    this.counters.set(key, value);
    return value;
  }

  current(key: string): number {
    return this.counters.get(key) ?? 0;
  }
}

// Binds `resolveAutoModelPolicy` (this folder's own hand-written mirror of
// `crates/tack-orch/src/model_policy`'s precedence walk) to the shared
// fixture at `docs/contracts/model-policy/precedence-table.json`, the one
// file both this test and `crates/tack-orch/tests/model_policy_contract.rs`
// read against their own implementation. The rows below are never
// hand-typed here — `docs/contracts/model-policy/README.md` documents how
// the fixture is generated from the real Rust behavior.

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveAutoModelPolicy } from './shared';
import type { AutoModelResolution } from './shared';
import type { ProjectModelDefault } from '../types';

interface FixtureRow {
  id: string;
  description: string;
  agent_profile_limits: unknown;
  project_default_model: ProjectModelDefault | null;
  fleet_default_policy: unknown;
  expected: AutoModelResolution;
}

const fixturePath = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../docs/contracts/model-policy/precedence-table.json',
);

const rows: FixtureRow[] = JSON.parse(readFileSync(fixturePath, 'utf8'));

describe('resolveAutoModelPolicy against the shared Rust/TypeScript precedence fixture', () => {
  it('loaded the full three-tier x three-state table', () => {
    expect(rows.length).toBe(27);
  });

  for (const row of rows) {
    it(`${row.id}: ${row.description}`, () => {
      const actual = resolveAutoModelPolicy(
        row.agent_profile_limits,
        row.project_default_model,
        row.fleet_default_policy,
      );
      expect(actual).toEqual(row.expected);
    });
  }
});

# Model-policy precedence fixture

`precedence-table.json` is the language-neutral authority for what an *Auto* execution
request resolves to. Both the Rust precedence walk
(`crates/tack-orch/src/model_policy/mod.rs`'s `resolve_model_policy` over
`ModelPolicyTier::ORDER`, and `wiring.rs`'s `parse_model_default_convention`) and its
TypeScript mirror (`frontend/src/shared/runWithAgent/shared.ts`'s
`resolveAutoModelPolicy`) are checked against this one file — neither implementation reads
the other's source.

The table is exhaustive over three tiers (agent profile, project, fleet — the ones an Auto
request can resolve through; a request override is never present in that case) and three
states each (absent, pinned to the literal `"auto"`, explicit provider/model), 27 rows.
`agent_profile_limits`/`fleet_default_policy` carry the raw `{"default_model": ...}`
convention exactly as it sits in `agent_profiles.limits`/`agent_fleets.default_policy`;
`project_default_model` carries `projects.default_model`'s own typed JSON shape
(`{"kind": "auto"}` or `{"kind": "explicit", "provider", "model_id"}`). `expected` names the
winning tier and, for `explicit`, the resolved provider/model pair; `pinned_auto` marks a
tier whose own `"auto"` opinion stops the walk before a more distant tier is ever consulted.

The file is generated from the real Rust behavior, never hand-typed:

```sh
UPDATE_MODEL_POLICY_FIXTURE=1 cargo nextest run --workspace -E 'binary(model_policy_contract)'
```

`crates/tack-orch/tests/model_policy_contract.rs` regenerates it in that mode and otherwise
asserts the committed file still matches `resolve_model_policy`'s current output byte for
byte. `frontend/src/shared/runWithAgent/modelPolicyContract.test.ts` reads the same
committed file and asserts `resolveAutoModelPolicy` agrees with every row.

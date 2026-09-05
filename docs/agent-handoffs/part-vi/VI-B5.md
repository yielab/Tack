# VI-B5 handoff

- Base SHA / branch / final SHA: base `129c6c6` (confirmed the `develop` tip at dispatch
  time via `git rev-parse develop`), branch `agent/vi-b5-catalog-on-the-wire`, final `bbbaed6`.
- Files changed (must equal ownership list):
  - Owned, as the card names them: `crates/tack-orch/src/execution/capabilities.rs` (the
    field itself — `ModelCombination` and `HarnessCapability` live here, not in an
    `execution.rs`, matching the dispatch block's correction to the board text),
    `crates/tack-orch/src/execution/mod.rs` (re-export `ModelMetadata`),
    `docs/contracts/runner-v1/capabilities.json`,
    `crates/tack-orch/tests/runner_contract/fixtures.rs` (pin table),
    `crates/tack-runner/src/provider/mod.rs`, `anthropic.rs`, `vercel_ai_gateway.rs` (the
    runner side that fills it).
  - **Not on the ownership list, touched anyway, mechanically:**
    `crates/tack-cli/src/doctor.rs`, `crates/tack-orch/src/scheduler/batch.rs`,
    `crates/tack-orch/src/scheduler/select.rs`,
    `crates/tack-orch/tests/scheduling/scheduler.rs`. Each has exactly one line added
    (`model_metadata: BTreeMap::new()` / `Default::default()`) to an existing
    `ModelCombination { .. }` struct literal that does not compile once the field is
    required. `doctor.rs`'s own comment on that literal already anticipated this
    ("so a future field addition to `ModelCombination` fails loudly here instead of only
    in a manual `--json` read") — this is that failure firing as designed, not scope
    creep chosen freely. No behavior in those four files changed beyond making the new
    field explicit at each existing call site.
- Contract fixtures consumed: `docs/contracts/runner-v1/capabilities.json` (owned,
  revised). Read-only: `enrollment.request.json` and `refresh.request.json` — both
  already have a `model_combinations` entry with **no** `model_metadata` key, and both
  already pass `embedded_capability_snapshot_parses_full_and_sparse_fixtures` and
  `every_json_fixture_parses_and_value_round_trips_without_loss` unchanged. That is a
  real-fixture proof of the old-runner round trip, not just the synthetic one below.
- Behavior implemented: `ModelCombination` gains `model_metadata: BTreeMap<ModelId,
  ModelMetadata>`, keyed by model id, carrying `context_window` / `price` / `modality`
  exactly as a provider's catalog published them (`Option`, never a default, never a
  flattened field). `attach_one_catalog` in `provider/mod.rs` now builds this map from the
  `CatalogEntry` values it already fetched and discarded after computing doctor counts —
  the parsing was already there (VI-B4); this card is what keeps the result instead of
  throwing it away. A model absent from the map is one the provider's catalog said
  nothing about, never a claim of zero. `CatalogEntry` gained a fourth field, `modality:
  Option<serde_json::Value>`, alongside the existing `price`/`context_window`.
- Tests added and exact commands/results:
  - `cargo nextest run --workspace -E 'binary(runner_contract)'` → `18 tests run: 18
    passed, 0 skipped`.
  - `cargo nextest run --workspace` → `1414 tests run: 1414 passed, 7 skipped`.
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean, no output beyond the
    build log (no warnings, no errors).
  - `./scripts/check-comments.sh` → `✓ no board archaeology in crates/`.
  - `./scripts/check-test-hygiene.sh` → `✓ tests take their temporary paths from a guard`.
  - `cargo fmt --check` → clean (ran once, found three formatting issues from my own new
    test code, fixed with `cargo fmt`, re-ran clean).
  - New unit tests, all in this run: `model_metadata_absent_on_an_older_runner_defaults_and_round_trips`
    and `model_metadata_round_trips_price_context_window_and_modality_per_model`
    (`capabilities.rs`); `modality_passes_through_opaquely_when_the_catalog_publishes_it`
    (`vercel_ai_gateway.rs`); the existing
    `one_providers_unresolvable_secret_never_suppresses_the_others_catalog` extended to
    assert the attached `model_metadata` entry.
- Failure/adversarial case proved: reverted the `skip_serializing_if` attribute on
  `model_metadata` locally and re-ran `model_metadata_absent_on_an_older_runner_defaults_and_round_trips`
  — it failed (`round_trip` gained a `"model_metadata": {}` key absent from `raw`),
  confirming the test is load-bearing for the "old runner round-trips unchanged" claim,
  then restored the attribute and re-confirmed green.
- Schema/API/contract change requested from another owner: none required — ran
  `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'` (`5 tests
  run: 5 passed, 0 skipped`) and `git status --porcelain docs/openapi.json` came back
  empty. The API exposes `capability_snapshot` as an opaque `serde_json::Value`
  (`crates/tack-api/src/handlers/runner_admin.rs:117`, `:175`), never a typed struct
  utoipa introspects, so this field addition changes no OpenAPI schema and needs no
  frontend regeneration. `cd frontend && npm run gen:api` itself could not be run in this
  environment (`openapi-typescript: not found` — `frontend/node_modules` is not
  installed here), but since its only input (`docs/openapi.json`) is confirmed
  byte-identical, there is nothing for it to regenerate. **FYI for VI-C1** (not a
  request): `model_metadata` exists on the wire now, under `capability_snapshot` per
  harness/combination, if you want to render price/context window/modality.
- Known limitations or `not_measured` fields:
  - Anthropic's `/v1/models` publishes `context_window` but never `price` or `modality`
    — always `None`, by design, not a parser gap (matches the vendor's actual response
    schema: id, display name, `max_input_tokens`/`max_tokens`, nothing else).
  - Vercel AI Gateway's catalog field name for modality (`modalities`) and its general
    existence come from ADR 0063's own dated measurement (2026-09-04, against a live
    fetch of 373 models) — **not independently re-measured this session**. No
    `TACK_LIVE_VERCEL_AI_GATEWAY_KEY` was available in this sandboxed environment, so the
    `#[ignore]`-gated live test could not be run, and the exact internal shape of
    `modalities` (array of strings? `{input, output}`? something else?) was never
    captured from a real body by anyone, including this card. `modality` is therefore
    stored as an opaque `serde_json::Value`, the same treatment as `price`, precisely
    because its shape is unverified — never typed or normalized.
  - A model whose catalog entry has all three fields absent gets no `model_metadata`
    entry at all, which is indistinguishable on the wire from a model this runner's
    catalog fetch never named. This follows decision 7 (absence is not zero) but is a
    rendering-time gotcha worth flagging to whoever builds the UI: "not in the map" must
    read as "nothing published," never as "not offered."
- Secrets/logging review: no secret material is touched by this change. The values
  carried in `model_metadata` are vendor-published catalog data (prices, limits,
  modality), not credentials, and no new `tracing` call sites were added.
- Safe merge order and likely conflicts: this card is the only one in Part VI allowed to
  touch `docs/contracts/runner-v1/**` and the pin table — no other in-flight card should
  conflict there. Sibling parallel cards (`agent/vi-c1-agents-page`,
  `agent/vii-c3-window-or-reason`, both present as branches at dispatch time) touch
  `frontend/**` and `crates/tack-desktop/**` respectively — no file overlap with this
  diff. Generated files (`Cargo.lock`, `docs/openapi.json`,
  `frontend/src/shared/api/schema.gen.ts`) are unaffected (verified above), so the
  `tack-generated` merge driver has nothing to resolve here.
- Checklist: no unowned files (four mechanical one-line fixups explained above, required
  by the compiler, not new scope), no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `ModelCombination` carries per-model price, context window and modality, keyed by model id | `crates/tack-orch/src/execution/capabilities.rs` (`ModelMetadata`, `ModelCombination::model_metadata`); `cargo nextest run --workspace -E 'binary(runner_contract)'` → 18 passed |
| The `additional` flatten map was not used to smuggle this field | `model_metadata` is its own named, typed field on `ModelCombination`, not a key inside `additional` — visible directly in the diff of `capabilities.rs` |
| An older runner's `capabilities.json` (no `model_metadata` key) still parses and round-trips byte-for-byte | `model_metadata_absent_on_an_older_runner_defaults_and_round_trips` (synthetic) plus the real, unmodified `enrollment.request.json`/`refresh.request.json` fixtures passing `embedded_capability_snapshot_parses_full_and_sparse_fixtures` and `every_json_fixture_parses_and_value_round_trips_without_loss` |
| Populated per-model metadata (price + context window + modality) round-trips exactly, and a model the catalog quoted nothing for stays absent, not zeroed | `model_metadata_round_trips_price_context_window_and_modality_per_model` |
| The fixture revision and the pin-table hash update land in the same change as the field | `git show bbbaed6 --stat` shows `docs/contracts/runner-v1/capabilities.json` and `crates/tack-orch/tests/runner_contract/fixtures.rs` in one commit |
| The runner fills `model_metadata` from both real providers' catalogs, not just the type existing | `attach_one_catalog` in `crates/tack-runner/src/provider/mod.rs`; `one_providers_unresolvable_secret_never_suppresses_the_others_catalog` now asserts the attached map's contents |
| Anthropic's catalog publishes no price and no modality; Vercel's publishes both, plus a context window | `anthropic.rs::parse_catalog` (always `None`) and its tests; `vercel_ai_gateway.rs::parse_catalog` and `modality_passes_through_opaquely_when_the_catalog_publishes_it` |
| The whole workspace still builds, tests, lints and passes hygiene gates | commands and results listed above |

## Measured numbers

- `cargo nextest run --workspace -E 'binary(runner_contract)'`: 18 tests run, 18 passed.
- `cargo nextest run --workspace`: 1414 tests run, 1414 passed, 7 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.
- `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'`: 5 tests
  run, 5 passed; `git status --porcelain docs/openapi.json` empty (no drift).
- New `capabilities.json` FNV-1a 64 pin: `0xecb0_79e8_21b5_c12f`, computed by replicating
  the exact algorithm in `crates/tack-orch/tests/runner_contract/fixtures.rs::fnv1a64`
  against the revised file's bytes in Python, then confirmed by the passing
  `fixture_field_state_or_error_mutation_fails_the_frozen_manifest` test (one of the 18
  above).
- Frozen fixture count unchanged at 46 (`every_json_fixture_parses_and_value_round_trips_without_loss`
  still asserts and passes `paths.len() == 46` — no fixture files were added or removed).
- `git diff --stat` against base: 11 files changed, 219 insertions(+), 13 deletions(-).

## What a stranger still cannot do

See a model's price, context window or modality anywhere reachable in the running
product. This card puts the data on the wire and nowhere else — not in `tack runner
doctor`'s own output, not in any API response a person reads directly, not in the web UI.
A stranger who asks "what does this model cost" still gets no answer from anything they
can click or read today; that answer exists now only inside `capability_snapshot`'s raw
JSON, for a future card (VI-C1 or later, per this card's own acceptance) to render.
Separately, nobody in this session could confirm live what Vercel's real `modalities`
values actually look like — no credential was available to run the `#[ignore]`-gated
live catalog test — so a stranger with a real Vercel AI Gateway key today would be the
first to see whether the opaque pass-through this card wrote actually matches what ships
out of that endpoint.

## Surface-map delta

None. This card is wire-only — no console-vs-UI surface moved, and the card's own
acceptance says so explicitly ("Rendering it is VI-C1's or a later card's; this card
carries it no further than the API response"). §VI.0's surface-map table itself was not
re-read this session; nothing here should be read as a claim about its current rows.

## Context spent

- Tokens read before the first edit (cold start), against the block's ~14k estimate:
  roughly in that range, plus a deliberate ~2k overrun (see below) — not measured with a
  token-counting tool, estimated from lines read.
- Context size at handoff: well under the 120k mid-card ceiling; this card did not
  approach the 150k stop condition.
- Files opened and not used: none — every file the read list named fed directly into the
  diff or the tests.
- Read-list lines that were wrong: none found to be wrong; the read list was accurate.
  One deliberate deviation beyond it: after reading ADR 0063 decision 5 only (lines
  23–58, as instructed), I also read its "What a catalog actually publishes" section
  (lines 59–115, ADR 0063) — decision 5's own text says a modality field is needed but
  never names what a real vendor calls it on the wire; that section is where the field
  name `modalities` and its 2026-09-04 live-measurement provenance actually live. Recorded
  here per the instruction to note any file read beyond the named list, and why.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

### 2026-09-05 — the live Vercel catalog was measured; the "not checked" gap is now closed

The coordinator ran `live_fetch_catalog_reaches_the_real_gateway_when_opted_in` against a
real Vercel AI Gateway key on the integration machine (this session never had that
credential, so this could not be run here — see "Known limitations" above, which stays
true as written for the reason it was true then). It passed on this branch: 373 models,
352 priced, 355 with a context window.

Reading the raw response body behind that run, over all 373 models:

- **`modalities` is present on 373 of 373**, and its key set is `{input, output}` on
  every single one — no variation anywhere in the catalog. Both values are arrays of
  strings, e.g. `{"input": ["text"], "output": ["text"]}`.
- **`pricing` is present as a key on all 373, but empty on 21** — exactly the 21 this
  card's parser already excludes, which is why `priced_model_count` reports 352 rather
  than 373. That number was honest, not a parser gap.
- **Of the 352 non-empty `pricing` values, only 259 carry both `input` and `output`.**
  The rest vary: 38 add `input_cache_read`, 31 are `video_duration_pricing` alone, 24
  carry `input` only — four distinct key shapes in the largest four groups (the
  coordinator's own count; those four numbers sum to the full 352, so this run found no
  further tail beyond them, though a different sample of the catalog could still turn one
  up).

**What this settles.** `price: Option<serde_json::Value>` — kept opaque in this card
because vendor pricing shapes were known in general to vary (ADR 0063) but never counted
for this specific catalog — is now proven correct by measurement, not just caution: **93
of the 352 priced models — everything except the 259 that carry exactly `{input,
output}` — would have been silently dropped or failed to parse under a typed
`{input: _, output: _}` struct.**
Storing it opaquely was the right call, and it now has a measured count behind it rather
than a general vendor-caution argument.

**Modality is the opposite case.** One shape, 373 of 373, zero exceptions across the
whole catalog this run saw. Unlike `price`, giving `modality` a typed shape (e.g.
`{input: Vec<String>, output: Vec<String>}` instead of an opaque `serde_json::Value`)
would be safe on this evidence — every model agreed. This card does not make that change:
the branch was reported finished and verified before this measurement arrived, and the
coordinator asked that any code change prompted by it be proposed here rather than made.
Flagging it as a design option for a future card, specifically because "the next person
to look at this field will be tempted to type it" applies to `modality` in a way it
provably does not apply to `price` — and a reader comparing the two fields side by side
without this note would have no way to tell they now rest on different amounts of
evidence.

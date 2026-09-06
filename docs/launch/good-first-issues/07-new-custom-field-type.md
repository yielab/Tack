# good first issue: add a new custom-field type

**Suggested labels:** `good first issue`, `enhancement`

## What exists today, checked directly

Custom fields today support nine types (`crates/tack-core/src/models.rs`, `pub enum
CustomFieldType`): `Text`, `Number`, `Date`, `Boolean`, `Select`, `MultiSelect`, `Url`,
`Email`, `LongText`. Each type has validation logic in `CustomFieldType::validate_value`
(`crates/tack-core/src/models.rs:839` onward) — e.g. `Url` requires the string start
with `http://`/`https://`, `Date` accepts `YYYY-MM-DD` or RFC3339. This is one of the
most self-contained extension points in the codebase: a new variant touches one enum,
one validation match arm, and one serialization mapping.

## Where to start

1. Pick a type that's genuinely missing and has a clear validation rule — a few
   reasonable options, pick one rather than trying all of them: `Phone` (a phone-number
   string, whatever validation strictness you think is reasonable), `Currency` (an
   amount plus an ISO 4217 currency code — note this repo has a strict rule about money
   fields elsewhere, "never render an unmeasured value as $0.00," so if you add this
   think about how a currency field without a value should render, not just how it
   validates), or `Rating` (a bounded integer, e.g. 1–5).
2. Add the variant to `CustomFieldType` in `crates/tack-core/src/models.rs`.
3. Add its validation arm to `validate_value` (same function, right below the existing
   `Url` arm at line 847 is a good, complete example of the shape: match on the JSON
   value's shape, return a specific `Err(String)` naming the field and what's actually
   expected — not a generic "invalid value").
4. Add its string mapping in `crates/tack-db/src/repo/custom_fields.rs` — there are
   **three** separate match arms across two different functions that all need the new
   variant: one serialize-to-string arm, and two independent deserialize-from-string
   arms in two different row-mapping functions elsewhere in the same file. Run
   `grep -n '"email" => CustomFieldType::Email' crates/tack-db/src/repo/custom_fields.rs`
   first — it will show you all three (well, both deserialize ones plus the reverse
   arm nearby) so you don't add the variant in one place and miss the others.
5. Add the type to the frontend's custom-field editor UI (`grep -rln
   'CustomFieldType\|custom_field' frontend/src/features/` to find the current picker —
   confirm the exact file yourself, since this is exactly the kind of location that
   moves between when an issue is written and when someone picks it up).

## What "done" looks like for a first PR

A new field type with real validation (not just "accept anything"), a unit test proving
both valid and invalid values are handled correctly (`crates/tack-core/src/models.rs`
already has tests near `validate_value` — follow the existing pattern), and — if you
did the frontend piece too — the type selectable when creating a custom field, with a
component test. The backend-only version (model + validation + repo mapping, no
frontend) is a completely reasonable, smaller first PR on its own.

## Before you start

Read `CONTRIBUTING.md`'s "Good First Contributions" table (this exact task is listed
there) and "Writing Tests" section. This issue does not require reading this
repository's internal planning board to get started.

# good first issue: SMTP / outbound email notifications

**Suggested labels:** `good first issue`, `enhancement`

## The gap, checked directly

There is no email of any kind in this codebase. `grep -rn 'smtp\|SMTP' crates/` and
`grep -rn 'lettre\|mail' Cargo.toml crates/*/Cargo.toml` both return nothing. The only
`email` hits anywhere in `crates/` are `CustomFieldType::Email` (a form-field *type* for
validation, `crates/tack-core/src/models.rs`), a `contact(email = ...)` line in the
OpenAPI spec metadata, and test fixtures setting `git config user.email` for commits the
runner makes. None of it sends mail. Right now the only way to know an item changed
status or an agent's attempt finished is to have the board open or poll the API.

## Why this is worth doing, and the pattern already in the codebase to follow

Tack already has one outbound-notification mechanism: signed webhooks
(`crates/tack-api/src/webhook.rs`, `WebhookClient`). It already fires on item
create/update/delete, sprint status changes, and due-soon alerts — see the call sites at
`crates/tack-api/src/handlers/items.rs:114,275,541` and
`crates/tack-api/src/handlers/sprints.rs:124` (`if let Some(wh) = &state.webhook`). SMTP
notification is the same shape of problem — "an event happened, tell someone" — with a
different transport. The webhook client is fire-and-forget, HMAC-signs its payload, uses
a 10-second timeout, and explicitly disables following redirects (a webhook URL is
operator-configured, but a redirect response comes from whoever's on the other end of
it — the code has a comment explaining exactly why that's refused). An email notifier
belongs in the same shape: its own client module, registered once on `AppState`
(`crates/tack-api/src/router.rs:42` is where `webhook: Option<WebhookClient>` lives
today), invoked from the same handler call sites rather than adding a second set of
"when do we notify" logic.

## Where to start

1. Pick a mailer crate (`lettre` is the standard choice for SMTP-over-Rust; check its
   current async support before committing to a version).
2. Add `TACK_SMTP_*` config following the existing pattern in
   `crates/tack-api/src/config.rs` and document every new variable in `docs/CONFIG.md`
   in the same PR — that file is the single authority for config tables, and an
   undocumented env var is treated as a bug here, not a minor omission.
3. This project's posture is that anything reaching the network is off by default
   behind an explicit `_ENABLE` gate (see `TACK_ORCH_ENABLE` in `docs/CONFIG.md` for the
   existing convention) — an SMTP notifier should follow the same rule rather than
   silently start sending mail once host/port are configured.
4. Wire it into the same event points the webhook client already uses, rather than
   inventing new ones.

## What "done" looks like for a first PR

Does not need to support every event type on day one — "item moved to Done sends one
templated email, following the existing gate/config conventions, with a test proving no
send occurs when the feature is disabled" is a complete, mergeable first step. Add a
unit test the same way this repo tests other network-reaching code: assert the request
is well-formed and gated correctly rather than requiring a live mail server in CI (see
how `crates/tack-api/tests/` structures handler tests with `axum::Router::oneshot()` for
the general pattern, even though this is a different transport).

## Before you start

Read `CONTRIBUTING.md`'s "Reporting Bugs & Requesting Features" and "Pull Request
Process" sections. Note the repo-wide rule: secrets are write-only over the API, never
logged — an SMTP password is a secret, and any new one needs to be added to
`remote_backup.rs::scrub_snapshot_secrets` in the same commit so it never leaks into a
backup snapshot. This issue does not require reading this repository's internal
planning board to start.

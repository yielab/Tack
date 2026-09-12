# `crates/tack-orch/src/lib.rs`

Moved out of the module preamble; trim or delete freely.

`tack-orch` — the control-plane orchestration client for Tack.

 Defines [`ControlPlane`], the trait every agent-fleet backend (docket today,
 something else tomorrow) implements, plus the DTOs that cross the
 Tack ⇄ control-plane boundary. Concrete adapters (`adapters::docket`)
 and the reconciler poll loop (`reconciler`) build on top of this.

 # Dependency direction

 This crate depends inward on `tack-core` and `tack-db` only. **It must never
 depend on `tack-api`.** `tack-api` depends on `tack-orch` — to spawn the
 reconciler and to expose the `/api/control-planes`, `/api/fleet`, and
 dispatch routes — not the other way around. If you're an agent reaching for
 `tack-api` types from in here (e.g. to reuse a handler DTO), stop: define the
 type here instead and let `tack-api` depend on it, or duplicate the small
 shape rather than inverting the graph. See
 `docs/book/src/developer/orchestration.md`.

 # Money is always an estimate

 Every dollar-valued field in this crate is named `*_usd_estimated` (never
 `*_usd` alone) — token counts are the primary, trustworthy measure; docket's
 own driver does not report real spend (see `docket/core/dispatch.py`'s
 `pod_gating_cost`), so any dollar figure downstream of it is derived, not
 recorded.

 # Unknown enum values never fail a poll

 [`RunState`], [`RunSource`], [`TaskStatus`], and [`ApprovalState`] each carry
 an `Unknown(String)` fallback variant with a hand-written `Deserialize` (a
 plain `#[serde(other)]` only works on unit variants, and we need the
 original string preserved so it can round-trip back out). A docket upgrade
 that adds a new state must degrade to "shown as-is", never to a
 deserialization error that kills the reconciler's poll loop.

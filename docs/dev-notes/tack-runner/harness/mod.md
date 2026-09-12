# `crates/tack-runner/src/harness/mod.rs`

Moved out of the module preamble; trim or delete freely.

Harness process/event infrastructure and the shared adapter-registration
 seam.

 [`crate::client::engine::HarnessAdapter`] (re-exported as
 [`HarnessAdapter`] at this module's root) is the frozen per-attempt
 lifecycle interface each concrete harness adapter (`codex.rs`,
 `claude_code.rs`) implements. This module supplies the
 genuinely separate piece: [`HarnessProbe`], for harness
 discovery/capability reporting, which has no home in
 `engine::HarnessAdapter` at all (see "Why `HarnessProbe` is not a sixth
 `HarnessAdapter` method" below). [`AdapterRegistry`] is the shared
 registry wiring: a struct that itself implements `engine::HarnessAdapter`
 by dispatching to whichever concrete adapter matches a claimed attempt's
 requested harness kind, so `RunnerEngine<P, A, W, C>`'s single `A:
 HarnessAdapter` can serve every registered harness through one engine
 instance. [`process`] and [`event_sink`] are the lower-level primitives
 the concrete adapters compose inside their own
 `validate`/`start`/`cancel`/`wait` implementations.

 ## Why `HarnessProbe` is not a sixth `HarnessAdapter` method

 `engine::HarnessAdapter`'s five methods all take an `ExecutionSpec` or a
 `LocalRunHandle` — both of which only exist once a request has been
 claimed. Capability reporting (`RunnerCapabilities.harnesses`, populated
 at enrollment/refresh — see `tack_orch::execution::capabilities`) has to
 run *before* any attempt exists, to tell the scheduler what this runner
 can even do. There is nowhere on the existing trait to hang that. Rather
 than forcing capability discovery through a method that would need to
 fabricate an `ExecutionSpec` to call, [`HarnessProbe`] is a small,
 separate trait for exactly that: "detect version, report capabilities,
 independent of any specific attempt."

 ## Two open interface gaps, proven by two real adapters

 1. **`LocalRunHandle` cannot name its own harness kind.** `cancel`/`wait`
    take only `&LocalRunHandle { process_id: String }`, with no
    harness-kind field, so a dispatching registry has no way to route a
    bare handle back to the adapter that produced it. [`AdapterRegistry`]
    works around this by encoding the kind into the opaque `process_id`
    string it hands back from `start` (see `encode_handle`/`decode_handle`
    below) and decoding it again in `cancel`/`wait`/`reconcile`. The
    straightforward fix — adding a `harness_kind` field to
    `LocalRunHandle` — is still not made: `LocalRunHandle { process_id:
    ... }` is constructed by literal in `crates/tack-runner/tests/
    crash_matrix.rs:277`, and any new required field breaks that
    construction.
 2. **Kind-key type duplication, still present.** [`AdapterRegistry`] keys
    directly on `tack_orch::execution::HarnessKind` (an opaque string,
    matching `ExecutionRequestSnapshot::requested_harness_kind`).
    `registry.rs` separately defines its own `HarnessKind` enum
    (`Codex`/`ClaudeCode`/`Other(String)`). Whether these two
    types should be unified, and whether `AdapterRegistry` itself belongs
    in `registry.rs` instead of here, remains an open registry-shape
    decision.

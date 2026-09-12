//! Production-wiring proofs: claims about artifact storage, model-policy
//! resolution, and retention/expiry sweeps are load-bearing in the real
//! `tack_api::router::build_router`/`ExecutionRuntime` wiring, not merely
//! present as source text.

mod common;

#[path = "wiring/artifact.rs"]
mod artifact;
#[path = "wiring/execution_sweep.rs"]
mod execution_sweep;
#[path = "wiring/model.rs"]
mod model;

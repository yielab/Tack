//! Scheduler selection, its wiring against a real repository, and model-policy
//! resolution, grouped into one nextest binary since all three exercise the
//! same subject (which runner a request lands on) at different depths: pure
//! property tests, real-repository wiring, and the policy layer that decides
//! what model a request wires in before scheduling ever sees it.

#[path = "common/mod.rs"]
mod common;
#[path = "scheduling/policy.rs"]
mod policy;
#[path = "scheduling/scheduler.rs"]
mod scheduler;
#[path = "scheduling/support.rs"]
mod support;
#[path = "scheduling/wiring.rs"]
mod wiring;

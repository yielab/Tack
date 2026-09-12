//! Crate-wide shared fixtures for `tack-orch`'s integration test binaries: the
//! migrated in-memory pool and bare workspace/project/item seeds delegate to
//! `tack-test-support`, so no binary hand-rolls its own copy. Anything below
//! is specific to one binary's own domain (docket wiremock stand-ins,
//! scheduler value builders, contract fakes) and stays in that binary's own
//! module.

pub(crate) use tack_test_support::*;

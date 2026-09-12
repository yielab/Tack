//! Shared fixtures for `tack-runner`'s integration tests.

/// A uniquely prefixed temporary directory, deleted when the returned guard
/// drops — including when an assertion panics first.
pub fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// The frozen usage fixture shared by execution tests that don't vary it.
#[allow(dead_code)] // not every binary that includes this module calls it
pub fn usage() -> tack_orch::execution::Usage {
    serde_json::from_str(
        r#"{
            "tokens_in":{"value":1,"source":"measured"},
            "tokens_out":{"value":2,"source":"measured"},
            "duration_ms":{"value":3,"source":"measured"},
            "cost_usd":{"value":null,"source":"not_measured"}
        }"#,
    )
    .expect("usage")
}

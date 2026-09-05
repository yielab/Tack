use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const FROZEN_FIXTURE_FNV1A64: &[(&str, u64)] = &[
    ("accept.request.json", 0x7c41_cf4c_5a0c_50a0),
    ("accept.response.json", 0x9e9d_72b5_565a_783d),
    ("artifact.request.json", 0x40c1_0d30_32db_290f),
    ("artifact.response.json", 0x103d_f88e_44d9_900d),
    ("cancellation.request.json", 0x083e_61d8_a5b4_f80a),
    ("cancellation.response.json", 0x25ac_7703_2f24_298f),
    // The codex harness entry exemplifies the
    // optional `model_passthrough` attestation (operator-specified opaque
    // models forwarded verbatim; scheduler eligibility without a declared
    // combination) and per-model `model_metadata` (one of the two model ids
    // priced and windowed, the other absent from the map entirely — a model
    // the catalog said nothing about, not a claim of zero).
    ("capabilities.json", 0xecb0_79e8_21b5_c12f),
    ("claim.no-work.response.json", 0x67a3_99f9_f7ee_5fc4),
    ("claim.request.json", 0x2b2d_c1f6_2357_bd0b),
    ("claim.response.json", 0x4d27_1810_d5f7_cf48),
    ("completion.request.json", 0x2a0f_8adc_77a0_b06f),
    ("completion.response.json", 0x99b7_7e8d_6afc_1354),
    ("decision.create.request.json", 0x23af_3ef2_1d81_3a7b),
    ("decision.create.response.json", 0xb160_e2d3_f16e_3318),
    ("decision.poll.request.json", 0x7b88_373b_f26d_5a4e),
    ("decision.poll.response.json", 0x2067_05f2_dbb7_7239),
    ("enrollment.request.json", 0xb58e_21c5_d6d0_bfe8),
    ("enrollment.response.json", 0xbb7a_b9c5_8fbf_ee57),
    (
        "errors/artifact-checksum-mismatch.json",
        0x8e1c_ecea_6d2b_ea54,
    ),
    ("errors/conflict.json", 0x331e_9975_5892_fd00),
    ("errors/decision-expired.json", 0x59b5_3108_96fb_20e4),
    ("errors/forbidden.json", 0xb858_15bd_3d89_bab5),
    ("errors/idempotency-conflict.json", 0x7f61_ba4a_c0a0_8d6a),
    ("errors/internal-error.json", 0x6725_873b_7a8a_5b3d),
    ("errors/invalid-request.json", 0x4a64_8995_f2fb_3e18),
    ("errors/invalid-transition.json", 0x312a_85d0_1d0b_8607),
    ("errors/not-found.json", 0x83ad_6515_ae25_0aca),
    ("errors/payload-too-large.json", 0x53ce_0d82_ad8d_a039),
    ("errors/rate-limited.json", 0x552a_b0f1_3d23_f1e9),
    ("errors/runner-revoked.json", 0xea22_e74b_74d4_2d1b),
    ("errors/stale-lease.json", 0x6c3e_0277_7d69_2b67),
    ("errors/unauthorized.json", 0x9501_39e7_11b3_4d86),
    ("errors/unsupported-protocol.json", 0xaa3e_8d64_461f_3a23),
    ("event-batch.request.json", 0xdf25_3a41_3e89_1773),
    ("event-batch.response.json", 0x57c9_0fa3_870c_a709),
    ("heartbeat.request.json", 0xd79b_6787_5f57_ed34),
    ("heartbeat.response.json", 0x63a2_31d5_7dd3_20e3),
    ("lifecycle-transitions.json", 0xca13_1c5f_aa10_a91b),
    ("limits.json", 0xed78_5be0_4be8_ed98),
    ("protocol.json", 0xe879_2dbb_331d_2996),
    ("recovery-observation.request.json", 0xa853_654b_7c9c_3f01),
    ("recovery-observation.response.json", 0x01d2_022b_e95f_3987),
    ("refresh.request.json", 0xc418_5ef4_fc72_fcc8),
    ("refresh.response.json", 0x96eb_7891_d95e_45c7),
    ("start.request.json", 0x26b7_1d95_5b02_3895),
    ("start.response.json", 0x9f07_b1b7_473e_75a3),
];

pub(crate) fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/contracts/runner-v1")
}

pub(crate) fn fixture_paths() -> Vec<PathBuf> {
    fn visit(directory: &Path, paths: &mut Vec<PathBuf>) {
        let mut entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            .map(|entry| {
                entry
                    .expect("fixture directory entry must be readable")
                    .path()
            })
            .collect::<Vec<_>>();
        entries.sort();

        for path in entries {
            if path.is_dir() {
                visit(&path, paths);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
                paths.push(path);
            }
        }
    }

    let mut paths = Vec::new();
    visit(&fixture_root(), &mut paths);
    paths
}

pub(crate) fn fixture_name(path: &Path) -> String {
    path.strip_prefix(fixture_root())
        .expect("fixture must be under the runner-v1 root")
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn load_value(relative_name: &str) -> Value {
    let path = fixture_root().join(relative_name);
    let bytes = fs::read(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn every_json_fixture_parses_and_value_round_trips_without_loss() {
    let paths = fixture_paths();
    assert_eq!(paths.len(), 46, "the frozen fixture manifest changed");

    for path in paths {
        let name = fixture_name(&path);
        let bytes = fs::read(&path).expect("fixture must remain readable");
        let decoded: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{name} is invalid JSON: {error}"));
        let encoded = serde_json::to_vec(&decoded).expect("JSON value must serialize");
        let round_tripped: Value = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("{name} did not round-trip: {error}"));
        assert_eq!(round_tripped, decoded, "{name} lost an additive field");
    }
}

#[test]
fn fixture_field_state_or_error_mutation_fails_the_frozen_manifest() {
    let actual = fixture_paths()
        .into_iter()
        .map(|path| {
            let name = fixture_name(&path);
            let bytes = fs::read(path).expect("fixture must remain readable");
            (name, fnv1a64(&bytes))
        })
        .collect::<Vec<_>>();
    let expected = FROZEN_FIXTURE_FNV1A64
        .iter()
        .map(|(name, hash)| ((*name).to_owned(), *hash))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "runner-v1 fixture bytes changed");

    let original = fs::read(fixture_root().join("protocol.json"))
        .expect("protocol fixture must remain readable");
    let mut changed = original.clone();
    let position = changed
        .iter()
        .position(|byte| *byte == b'1')
        .expect("test fixture contains a version");
    changed[position] = b'2';

    let expected_protocol_hash = FROZEN_FIXTURE_FNV1A64
        .iter()
        .find_map(|(name, hash)| (*name == "protocol.json").then_some(*hash))
        .expect("protocol hash must be pinned");
    assert_eq!(fnv1a64(&original), expected_protocol_hash);
    assert_ne!(fnv1a64(&changed), expected_protocol_hash);
}

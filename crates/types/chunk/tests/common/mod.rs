//! Fixtures shared by the chunk-path integration tests.
//!
//! Each test binary compiles this module separately, so a helper only one of them needs looks
//! unused in the others.
#![allow(dead_code)]

use sbv_core::BlockWitness;
use sbv_primitives::types::Header;
use std::path::Path;
use types_base::version::Version;

pub const TSUKI_VERSION_BYTE: u8 = Version::tsuki().as_version_byte();

/// Reads the native SBV witness format. Legacy corpora must be regenerated before replay.
fn read_witness(path: &Path) -> BlockWitness {
    let json = std::fs::read(path).unwrap();
    serde_json::from_slice(&json).expect("native SBV block witness")
}

/// Feynman blocks 16525000..=16525004 from the integration test data: a fork whose chunks keep
/// state-root endpoints, used to show that the legacy behaviour is unchanged.
pub fn feynman_blocks() -> Vec<BlockWitness> {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integration/testdata/feynman/witnesses");
    (16_525_000..=16_525_004)
        .map(|n| read_witness(&dir.join(format!("{n}.json"))))
        .collect()
}

/// The Tsuki base-fee corpus and the trusted header of the block before it: a chain a node built
/// while enforcing the base-fee rule, so the chunk path can verify it. Block 4's transaction sets
/// the L2 base-fee overhead, and blocks 5-7 are judged on the new one.
///
/// The chunk starts at block 1, whose parent is the chain's genesis header, so this is also the
/// genesis-anchored chunk.
pub fn base_fee_chunk() -> (Header, Vec<BlockWitness>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../integration/testdata/tsuki-base-fee/witnesses");
    let parent = serde_json::from_slice(&std::fs::read(dir.join("parent.json")).unwrap())
        .expect("the chain's genesis header");
    let blocks = (1..=7)
        .map(|n| read_witness(&dir.join(format!("{n}.json"))))
        .collect();
    (parent, blocks)
}

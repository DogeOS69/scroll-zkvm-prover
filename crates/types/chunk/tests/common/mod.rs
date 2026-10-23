use sbv_core::BlockWitness;
use std::path::Path;
use types_base::version::Version;

pub const TSUKI_VERSION_BYTE: u8 = Version::tsuki().as_version_byte();

/// Tsuki blocks 1..=8 from the integration test data: a real chunk of 4-transaction blocks.
pub fn tsuki_blocks() -> Vec<BlockWitness> {
    tsuki_blocks_range(1..=8)
}

pub fn tsuki_blocks_range(range: std::ops::RangeInclusive<u64>) -> Vec<BlockWitness> {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integration/testdata/tsuki/witnesses");
    range
        .map(|n| {
            let json = std::fs::read(dir.join(format!("{n}.json"))).unwrap();
            serde_json::from_slice(&json).unwrap()
        })
        .collect()
}

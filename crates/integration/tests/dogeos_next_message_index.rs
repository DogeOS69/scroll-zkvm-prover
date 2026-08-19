//! Committed DogeOS fixture test for the `next_message_index` overlay.
//!
//! DogeOS threads Scroll's `nextMessageIndex` out of the (patched)
//! stateless-block-verifier into [`ChunkInfo`]/`BatchInfo`, committing it into the
//! Tsuki / Scroll@v11 public inputs (see `dogeos/changes/next-message-index.md`).
//! Tsuki is DogeOS's production fork; `ForkName::Tsuki` / `Version::tsuki()` are
//! added by PR #9 (`feat/tsuki-hardfork`), which this branch stacks on.
//!
//! This test derives a [`ChunkInfo`] natively from committed DogeOS block witnesses —
//! it runs the SBV state-transition, so it needs no guest program and no GPU — and
//! asserts the committed expected `next_message_index`, chain id and block range. That
//! keeps the DogeOS overlay field threaded and guards against a silent regression.
//!
//! Fixtures + provenance live under
//! `crates/integration/testdata/dogeos/next-message-index/` (see its `README.md`).

use eyre::{Context, ContextCompat, Result};
use sbv_primitives::B256;
use scroll_zkvm_integration::testers::PATH_TESTDATA;
use scroll_zkvm_integration::testers::chunk::read_block_witness;
use scroll_zkvm_integration::utils::metadata_from_chunk_witnesses;
use scroll_zkvm_types::public_inputs::{ForkName, Version};
use scroll_zkvm_types::scroll::chunk::ChunkWitness;
use std::path::{Path, PathBuf};

/// Feature directory under the DogeOS testdata fork tree.
const FEATURE_DIR: &str = "dogeos/next-message-index";

/// Committed description of the fixture and its expected derived values.
#[derive(serde::Deserialize)]
struct Manifest {
    fork_name: String,
    chain_id: u64,
    block_start: u64,
    block_end: u64,
    prev_msg_queue_hash: String,
    expected_next_message_index: u64,
}

fn feature_path() -> PathBuf {
    Path::new(PATH_TESTDATA).join(FEATURE_DIR)
}

fn parse_b256(s: &str) -> Result<B256> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    let bytes =
        hex::decode(hex).wrap_err_with(|| format!("prev_msg_queue_hash is not valid hex: {s}"))?;
    eyre::ensure!(
        bytes.len() == 32,
        "prev_msg_queue_hash must be 32 bytes, got {}",
        bytes.len()
    );
    Ok(B256::from_slice(&bytes))
}

#[test]
fn dogeos_next_message_index_committed_fixture() -> Result<()> {
    let dir = feature_path();

    let manifest_path = dir.join("manifest.json");
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path).wrap_err_with(|| {
            format!("missing DogeOS fixture manifest at {}", manifest_path.display())
        })?,
    )
    .wrap_err("failed to parse DogeOS fixture manifest")?;

    // Tsuki (Scroll@v11) is DogeOS's production fork that commits next_message_index into the PI.
    eyre::ensure!(
        manifest.fork_name == "Tsuki",
        "unexpected fork_name {} (this fixture targets the Tsuki fork)",
        manifest.fork_name
    );
    let version = Version::tsuki();
    let prev_msg_queue_hash = parse_b256(&manifest.prev_msg_queue_hash)?;

    // Read the committed DogeOS block witnesses for the manifest's block range. A failure
    // here most likely means the sbv BlockWitness format drifted from the pinned rev.
    let witnesses_dir = dir.join("witnesses");
    let blocks = (manifest.block_start..=manifest.block_end)
        .map(|block| read_block_witness(witnesses_dir.join(format!("{block}.json"))))
        .collect::<Result<Vec<_>>>()
        .wrap_err("failed to read DogeOS block witnesses (sbv format/rev drift?)")?;

    // Sanity: the witnesses cover exactly the manifest's block range.
    let first = blocks.first().context("no block witnesses")?.header.number;
    let last = blocks.last().context("no block witnesses")?.header.number;
    eyre::ensure!(
        first == manifest.block_start && last == manifest.block_end,
        "block witnesses cover {first}..={last}, manifest declares {}..={}",
        manifest.block_start,
        manifest.block_end
    );

    // Assemble the chunk witness exactly as the tester harness does, then derive the
    // ChunkInfo natively (runs the SBV STF; no guest program / GPU involved).
    let witness = ChunkWitness::new_scroll(
        version.as_version_byte(),
        &blocks,
        prev_msg_queue_hash,
        ForkName::Tsuki,
    );
    let chunk_info = metadata_from_chunk_witnesses(witness)?;

    let derived_block_end =
        chunk_info.initial_block_number + chunk_info.block_ctxs.len() as u64 - 1;
    eprintln!(
        "dogeos_next_message_index: chain_id={} block_range={}..={} next_message_index={}",
        chunk_info.chain_id,
        chunk_info.initial_block_number,
        derived_block_end,
        chunk_info.next_message_index,
    );

    // Committed-fixture assertions: the DogeOS overlay field and its identifying context.
    assert_eq!(chunk_info.chain_id, manifest.chain_id, "chain_id");
    assert_eq!(
        chunk_info.initial_block_number, manifest.block_start,
        "initial block number"
    );
    assert_eq!(derived_block_end, manifest.block_end, "final block number");
    assert_eq!(
        chunk_info.next_message_index, manifest.expected_next_message_index,
        "next_message_index (DogeOS overlay field) regressed"
    );

    Ok(())
}

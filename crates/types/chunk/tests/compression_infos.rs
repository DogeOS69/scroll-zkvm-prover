//! Host-mode regression for per-transaction compression infos.
//!
//! The chunk witness carries one compression info per transaction. If a block's vector is
//! shorter than its transaction list, the trailing transactions must not be skipped: the chunk
//! still commits to every transaction, so the post-state root would come from a prefix only.
//! This test forges exactly that witness (last block truncated, header root set to the prefix
//! root) on a real Tsuki chunk and checks that `execute` rejects it.

mod common;

use common::{TSUKI_VERSION_BYTE, tsuki_blocks};
use sbv_core::{BlockWitness, error::StatelessValidationError, verifier};
use sbv_primitives::{
    B256,
    chainspec::{Chain, build_chain_spec_force_hardfork},
    hardforks::Hardfork,
};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute};
use types_base::fork_name::ForkName;

/// The state root reached by executing the chunk without the last block's final transaction.
fn prefix_state_root(blocks: &[BlockWitness]) -> B256 {
    let mut blocks = blocks.to_vec();
    blocks.last_mut().unwrap().transactions.pop().unwrap();
    let infos = blocks.iter().map(BlockWitness::compression_infos).collect();
    let chain_spec =
        build_chain_spec_force_hardfork(Chain::from_id(blocks[0].chain_id), Hardfork::Tsuki);
    match verifier::run(&blocks, chain_spec, infos) {
        Err(StatelessValidationError::PostStateRootMismatch { got, .. }) => got,
        other => panic!("expected the prefix to reach a different root, got {other:?}"),
    }
}

#[test]
fn honest_tsuki_chunk_executes() {
    let witness = ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        &tsuki_blocks(),
        B256::ZERO,
        ForkName::Tsuki,
    );
    execute(witness).expect("honest chunk must execute");
}

#[test]
fn truncated_compression_infos_with_matching_header_root_are_rejected() {
    let blocks = tsuki_blocks();
    let last_txs = blocks.last().unwrap().transactions.len();
    assert!(
        last_txs > 1,
        "fixture's last block must have several transactions"
    );
    let prefix_root = prefix_state_root(&blocks);
    assert_ne!(prefix_root, blocks.last().unwrap().header.state_root);

    let mut witness =
        ChunkWitness::new_scroll(TSUKI_VERSION_BYTE, &blocks, B256::ZERO, ForkName::Tsuki);
    // The count check fires before execution, so the forged root does not change the outcome
    // here. It makes the witness otherwise consistent, so that without the check it executes.
    witness.blocks.last_mut().unwrap().header.state_root = prefix_root;
    witness.compression_infos.last_mut().unwrap().pop().unwrap();

    let err = execute(witness).expect_err("a chunk that skips a transaction must not execute");
    assert!(
        err.contains(&format!(
            "block {}: compression infos count {} != tx count {last_txs}",
            blocks.last().unwrap().header.number,
            last_txs - 1
        )),
        "unexpected error: {err}"
    );
}

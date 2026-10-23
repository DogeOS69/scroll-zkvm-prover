//! Host-mode regression for per-transaction compression infos.
//!
//! The chunk witness carries one compression info per transaction. If a block's vector is
//! shorter than its transaction list, the trailing transactions must not be skipped: the chunk
//! still commits to every transaction, so the post-state root would come from a prefix only.
//!
//! Dropping the transaction itself is caught independently, by the transactions root in its own
//! header, which is asserted here too: the two rules close the same hole from opposite sides.
//!
//! One thing was traded away in moving to this corpus. The older version of the first test wrote
//! the prefix state root - the root the chunk reaches without the skipped transaction - into the
//! last block's header, so the witness was consistent apart from the truncated list and the test
//! would have failed had the count check been removed. That is not constructible here: this
//! corpus's last block is empty, and rewriting an earlier block's state root breaks the
//! witness-chaining check first. These tests pin the two error paths instead.

mod common;

use common::{TSUKI_VERSION_BYTE, base_fee_chunk};
use sbv_core::BlockWitness;
use sbv_primitives::{B256, types::consensus::Header};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute};
use types_base::fork_name::ForkName;

fn witness(parent: Header, blocks: &[BlockWitness]) -> ChunkWitness {
    ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        blocks,
        B256::ZERO,
        ForkName::Tsuki,
        Some(parent),
    )
}

#[test]
fn honest_tsuki_chunk_executes() {
    let (parent, blocks) = base_fee_chunk();
    execute(witness(parent, &blocks)).expect("honest chunk must execute");
}

/// A block with fewer compression infos than transactions is rejected before execution, so the
/// trailing transactions cannot be skipped while the chunk still commits to them.
#[test]
fn truncated_compression_infos_are_rejected() {
    let (parent, blocks) = base_fee_chunk();
    let mut witness = witness(parent, &blocks);
    // The chunk's last block is empty, so truncate the first block's infos instead.
    let first = witness.compression_infos.first_mut().expect("a block");
    assert!(first.len() > 1, "the first block has several transactions");
    first.pop();

    let err = execute(witness).expect_err("a chunk that skips a transaction must not execute");
    assert!(
        err.contains("invalid compression infos"),
        "unexpected error: {err}"
    );
}

/// Dropping the transaction from the witness instead of its compression info is caught by the
/// transactions root the block's own header commits to.
#[test]
fn a_witness_that_drops_a_transaction_is_rejected() {
    let (parent, blocks) = base_fee_chunk();
    let mut witness = witness(parent, &blocks);
    let first = witness.blocks.first_mut().expect("a block");
    assert!(
        first.transactions.len() > 1,
        "the first block has several transactions"
    );
    first.transactions.pop();
    witness
        .compression_infos
        .first_mut()
        .expect("a block")
        .pop();

    let err = execute(witness).expect_err("a chunk missing a transaction must not execute");
    // Specifically the transactions root, so the test says which layer caught the drop.
    assert!(
        err.starts_with("verify error: body validation failed for block 1:")
            && err.contains("transaction root"),
        "unexpected error: {err}"
    );
}

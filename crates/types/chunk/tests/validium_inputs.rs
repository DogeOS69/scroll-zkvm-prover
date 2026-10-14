//! Host-mode regression for validium inputs on the chunk witness.
//!
//! The validium transactions and secret key are prover-supplied. The rolling message queue hash
//! pairs each block with its own list of validium transactions, so the inputs must be present
//! exactly for the validium domain and must have one list per block. Otherwise a Scroll-domain
//! chunk could commit to a message queue hash over a different set of L1 messages from the ones
//! it executed.

mod common;

use common::{TSUKI_VERSION_BYTE, tsuki_blocks};
use sbv_primitives::B256;
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, ValidiumInputs, execute};
use types_base::{fork_name::ForkName, version::Version};

fn validium_inputs(lists: usize) -> ValidiumInputs {
    ValidiumInputs {
        validium_txs: vec![Vec::new(); lists],
        secret_key: [1u8; 32].into(),
    }
}

#[test]
fn scroll_domain_chunk_with_validium_inputs_is_rejected() {
    let blocks = tsuki_blocks();
    for lists in [0, blocks.len()] {
        let mut witness =
            ChunkWitness::new_scroll(TSUKI_VERSION_BYTE, &blocks, B256::ZERO, ForkName::Tsuki);
        witness.validium = Some(validium_inputs(lists));

        let err = execute(witness).expect_err("a Scroll chunk must not take validium inputs");
        assert_eq!(err, "validium inputs supplied for a Scroll domain chunk");
    }
}

#[test]
fn validium_domain_chunk_without_validium_inputs_is_rejected() {
    let witness = ChunkWitness::new(
        Version::validium_v1().as_version_byte(),
        &tsuki_blocks(),
        B256::ZERO,
        ForkName::Tsuki,
        None,
    );

    let err = execute(witness).expect_err("a validium chunk needs validium inputs");
    assert_eq!(err, "validium inputs missing for a validium domain chunk");
}

#[test]
fn validium_inputs_need_one_list_per_block() {
    let blocks = tsuki_blocks();
    for lists in [0, blocks.len() - 1, blocks.len() + 1] {
        let witness = ChunkWitness::new(
            Version::validium_v1().as_version_byte(),
            &blocks,
            B256::ZERO,
            ForkName::Tsuki,
            Some(validium_inputs(lists)),
        );

        let err = execute(witness).expect_err("validium lists must match the blocks");
        assert_eq!(
            err,
            format!(
                "validium transaction lists count {lists} != block count {}",
                blocks.len()
            )
        );
    }
}

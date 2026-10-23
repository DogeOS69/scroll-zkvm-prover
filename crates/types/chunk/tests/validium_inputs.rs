//! Host-mode regression for the witness fields that select which rules a chunk is held to.
//!
//! The validium transactions and secret key are prover-supplied. The rolling message queue hash
//! pairs each block with its own list of validium transactions, so the inputs must be present
//! exactly for the validium domain and must have one list per block. Otherwise a Scroll-domain
//! chunk could commit to a message queue hash over a different set of L1 messages from the ones
//! it executed.
//!
//! `version` and `fork_name` are prover-supplied too, and they have to agree: the fork decides
//! which rules apply and the version decides the layout the result is serialised into.

mod common;

use common::{TSUKI_VERSION_BYTE, base_fee_chunk};
use sbv_primitives::B256;
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, ValidiumInputs, execute};
use types_base::{fork_name::ForkName, version::Version};

fn validium_inputs(lists: usize) -> ValidiumInputs {
    ValidiumInputs {
        validium_txs: vec![Vec::new(); lists],
        secret_key: [1u8; 32].into(),
    }
}

/// The validium version byte's own fork, which a validium witness has to claim. It is before
/// Tsuki, so a validium chunk is not anchored and carries no parent header.
fn validium_fork() -> ForkName {
    Version::validium_v1().fork
}

#[test]
fn scroll_domain_chunk_with_validium_inputs_is_rejected() {
    let (parent, blocks) = base_fee_chunk();
    for lists in [0, blocks.len()] {
        let mut witness = ChunkWitness::new_scroll(
            TSUKI_VERSION_BYTE,
            &blocks,
            B256::ZERO,
            ForkName::Tsuki,
            Some(parent.clone()),
        );
        witness.validium = Some(validium_inputs(lists));

        let err = execute(witness).expect_err("a Scroll chunk must not take validium inputs");
        assert_eq!(err, "validium inputs supplied for a Scroll domain chunk");
    }
}

#[test]
fn validium_domain_chunk_without_validium_inputs_is_rejected() {
    let (_, blocks) = base_fee_chunk();
    let witness = ChunkWitness::new(
        Version::validium_v1().as_version_byte(),
        &blocks,
        B256::ZERO,
        validium_fork(),
        None,
        None,
    );

    let err = execute(witness).expect_err("a validium chunk needs validium inputs");
    assert_eq!(err, "validium inputs missing for a validium domain chunk");
}

#[test]
fn validium_inputs_need_one_list_per_block() {
    let (_, blocks) = base_fee_chunk();
    for lists in [0, blocks.len() - 1, blocks.len() + 1] {
        let witness = ChunkWitness::new(
            Version::validium_v1().as_version_byte(),
            &blocks,
            B256::ZERO,
            validium_fork(),
            Some(validium_inputs(lists)),
            None,
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

/// A fork that is not the one the version byte names is rejected. The guest asserts the same
/// equality before it commits anything, so a proof cannot pair the legacy state-root path with a
/// Tsuki public input; this is the host-side half.
#[test]
fn a_fork_that_is_not_the_versions_own_is_rejected() {
    let (parent, blocks) = base_fee_chunk();
    let witness = ChunkWitness::new(
        Version::validium_v1().as_version_byte(),
        &blocks,
        B256::ZERO,
        ForkName::Tsuki,
        Some(validium_inputs(blocks.len())),
        Some(parent),
    );

    let err = execute(witness).expect_err("the fork must be the version byte's own");
    assert_eq!(
        err,
        format!(
            "fork {} does not match the fork of version byte {}, which is {}",
            ForkName::Tsuki,
            Version::validium_v1().as_version_byte(),
            validium_fork()
        )
    );
}

/// An earlier fork is not anchored to anything, so supplying a parent header would hand the chunk
/// anchoring data it then discards. Refused in that direction too.
#[test]
fn a_parent_header_on_an_unanchored_fork_is_rejected() {
    let (parent, blocks) = base_fee_chunk();
    let witness = ChunkWitness::new_scroll(
        Version::feynman().as_version_byte(),
        &blocks,
        B256::ZERO,
        ForkName::Feynman,
        Some(parent),
    );

    let err = execute(witness).expect_err("an unanchored fork must not take a parent header");
    assert_eq!(
        err,
        format!(
            "parent header supplied for a {} chunk, which is not anchored to one",
            ForkName::Feynman
        )
    );
}

//! The proof path must reject headers that the execution client considers non-canonical,
//! including metadata that cannot be detected by replaying transactions and checking state roots.
//!
//! From Tsuki the chunk opens the header of the block before its first block, so the header rules
//! run across that boundary too: a chunk cannot be grafted onto a chain it does not continue,
//! even when its own blocks are internally consistent.
//!
//! The rules themselves live in SBV, which runs them over the chain of headers starting from the
//! trusted parent. These tests pin that the chunk path reaches them and refuses to proceed
//! without the parent header they are rooted in; SBV's own tests cover each rule.

mod common;

use common::{TSUKI_VERSION_BYTE, base_fee_chunk};
use sbv_primitives::{B256, U256, types::consensus::Header};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute};
use types_base::fork_name::ForkName;

fn tsuki_witness() -> ChunkWitness {
    let (parent, blocks) = base_fee_chunk();
    ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        &blocks,
        B256::ZERO,
        ForkName::Tsuki,
        Some(parent),
    )
}

/// A header field that no transaction reads changes the block hash without changing execution,
/// so only the header rules can reject it. The block nonce is one such field: it is not an EVM
/// input, so replaying the chunk would reach the same state root.
#[test]
fn changed_nonce_preserves_execution_but_is_rejected_by_chunk() {
    // Mutate the last block so the existing parent links remain valid.
    let mut witness = tsuki_witness();
    let last = witness.blocks.last_mut().unwrap();
    let original_hash = last.header.hash_slow();
    last.header.nonce = [1; 8].into();
    assert_ne!(last.header.hash_slow(), original_hash);

    let error = execute(witness).expect_err("the chunk must enforce the header nonce rule");
    assert!(error.contains("block nonce is not zero"), "{error}");
}

#[test]
fn noncanonical_headers_are_rejected_before_execution() {
    type Mutation = (&'static str, fn(&mut Header));
    let cases: &[Mutation] = &[
        ("ommers", |h| h.ommers_hash = B256::repeat_byte(1)),
        ("beneficiary", |h| h.beneficiary = [1; 20].into()),
        ("extra data", |h| h.extra_data = vec![1].into()),
        ("mix hash", |h| h.mix_hash = B256::repeat_byte(1)),
        ("nonce", |h| h.nonce = [1; 8].into()),
        ("difficulty", |h| h.difficulty = U256::from(2)),
        ("missing base fee", |h| h.base_fee_per_gas = None),
        ("excessive base fee", |h| {
            h.base_fee_per_gas = Some(420_000_000_000_001)
        }),
        ("withdrawals root", |h| {
            h.withdrawals_root = Some(B256::ZERO)
        }),
        ("blob gas used", |h| h.blob_gas_used = Some(0)),
        ("excess blob gas", |h| h.excess_blob_gas = Some(0)),
        ("parent beacon root", |h| {
            h.parent_beacon_block_root = Some(B256::ZERO)
        }),
        ("requests hash", |h| h.requests_hash = Some(B256::ZERO)),
        ("gas used", |h| h.gas_used = h.gas_limit + 1),
    ];
    let blocks = tsuki_witness().blocks;
    // Cover both the first and last header: validating just the chunk boundary is insufficient.
    for index in [0, blocks.len() - 1] {
        for (name, mutate) in cases {
            let mut witness = tsuki_witness();
            mutate(&mut witness.blocks[index].header);
            let error = execute(witness).expect_err(name);
            assert!(
                error.starts_with(&format!(
                    "verify error: invalid header for block {}:",
                    blocks[index].header.number
                )),
                "{name}: {error}"
            );
        }
    }
}

/// A Tsuki chunk has to carry the header it is anchored to.
#[test]
fn a_tsuki_chunk_without_a_parent_header_is_rejected() {
    let (_, blocks) = base_fee_chunk();
    let witness = ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        &blocks,
        B256::ZERO,
        ForkName::Tsuki,
        None,
    );

    assert_eq!(
        execute(witness).expect_err("a Tsuki chunk must carry its parent header"),
        format!("missing parent header for a {} chunk", ForkName::Tsuki)
    );
}

/// The parent header has to be the one the chunk's first block names. Any other header, even a
/// canonical one, hashes differently and so is not the previous proof endpoint.
#[test]
fn a_substituted_parent_header_is_rejected() {
    let mut witness = tsuki_witness();
    let parent = witness.parent_header.as_mut().unwrap();
    // Changing the state root keeps the header's own rules satisfied while changing its hash, so
    // the chunk is rejected for not continuing from it rather than for a malformed parent.
    parent.state_root = B256::repeat_byte(3);

    let error = execute(witness).expect_err("the parent header must be the one the chunk names");
    assert!(
        error.starts_with("verify error: parent header mismatch:"),
        "{error}"
    );
}

/// The rules that relate a header to its parent apply across the boundary into the trusted parent
/// and between the chunk's own blocks: a backwards timestamp and a gas limit that moves further
/// than the bound allows are rejected wherever they occur.
///
/// Heights are not covered here: SBV rejects a non-sequential chunk before it reaches the header
/// rules, and its own tests cover both that check and the parent-link rules.
#[test]
fn headers_are_validated_against_their_parent() {
    type Case = (&'static str, fn(&mut Header), &'static str);
    let cases: &[Case] = &[
        ("timestamp", |h| h.timestamp = 0, "is in the past"),
        ("gas limit", |h| h.gas_limit *= 2, "gas_limit"),
    ];
    let (_, blocks) = base_fee_chunk();

    for (name, mutate, expected) in cases {
        // Each case is applied to a chunk's first block, which is checked against the trusted
        // parent header, and to a mid-chunk block, which is checked against its predecessor. The
        // chunk ends on the mutated block either way: mutating a header changes its hash, so a
        // block after it would be rejected for not linking to it rather than for the rule here.
        for (shape, chunk, parent) in [
            ("first block", &blocks[2..=2], blocks[1].header.clone()),
            ("mid-chunk", &blocks[1..=2], blocks[0].header.clone()),
        ] {
            let mut witness = ChunkWitness::new_scroll(
                TSUKI_VERSION_BYTE,
                chunk,
                B256::ZERO,
                ForkName::Tsuki,
                Some(parent),
            );
            let offending = witness.blocks.len() - 1;
            mutate(&mut witness.blocks[offending].header);
            let number = witness.blocks[offending].header.number;

            let error = execute(witness).expect_err(&format!("{name} at the {shape}"));
            assert!(
                error.starts_with(&format!(
                    "verify error: invalid header for block {number} against its parent:"
                )) || error
                    .starts_with(&format!("verify error: invalid header for block {number}:")),
                "{name} at the {shape}: {error}"
            );
            assert!(
                error.to_lowercase().contains(expected),
                "{name} at the {shape}: {error} does not mention {expected}"
            );
        }
    }
}

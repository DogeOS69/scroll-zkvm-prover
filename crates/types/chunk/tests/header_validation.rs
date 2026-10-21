//! The proof path must reject headers that the execution client considers non-canonical,
//! including metadata that cannot be detected by replaying transactions and checking state roots.

mod common;

use common::{TSUKI_VERSION_BYTE, tsuki_blocks};
use sbv_core::verifier;
use sbv_primitives::{
    B256, U256,
    chainspec::{Chain, build_chain_spec_force_hardfork},
    hardforks::Hardfork,
    types::consensus::Header,
};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute};
use types_base::fork_name::ForkName;

#[test]
fn changed_nonce_preserves_execution_but_is_rejected_by_chunk() {
    // Mutate the last block so the existing parent links remain valid. SBV execution alone
    // accepts this witness with the same state root, but its final block hash changes.
    let mut blocks = tsuki_blocks();
    let last = blocks.last_mut().unwrap();
    let original_hash = last.header.hash_slow();
    last.header.nonce = [1; 8].into();
    assert_ne!(last.header.hash_slow(), original_hash);
    let expected_state_root = last.header.state_root;

    let chain_spec =
        build_chain_spec_force_hardfork(Chain::from_id(blocks[0].chain_id), Hardfork::Tsuki);
    let result = verifier::run_host(&blocks, chain_spec).expect("nonce does not affect execution");
    assert_eq!(result.post_state_root, expected_state_root);

    let witness =
        ChunkWitness::new_scroll(TSUKI_VERSION_BYTE, &blocks, B256::ZERO, ForkName::Tsuki);
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
            h.base_fee_per_gas = Some(10_000_000_001)
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
    let blocks = tsuki_blocks();
    // Cover both the first and last header: validating just the chunk boundary is insufficient.
    for index in [0, blocks.len() - 1] {
        for (name, mutate) in cases {
            let mut witness =
                ChunkWitness::new_scroll(TSUKI_VERSION_BYTE, &blocks, B256::ZERO, ForkName::Tsuki);
            mutate(&mut witness.blocks[index].header);
            let error = execute(witness).expect_err(name);
            assert!(
                error.starts_with(&format!(
                    "invalid header at block {}:",
                    blocks[index].header.number
                )),
                "{name}: {error}"
            );
        }
    }
}

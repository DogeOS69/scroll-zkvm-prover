//! A chunk's public inputs carry its boundary endpoints in the `prev_state_root` and
//! `post_state_root` slots. From Tsuki those slots carry the boundary **L2 block hashes** instead
//! of state roots: the chunk opens the header of the block before its first block, so the hash of
//! that header is the previous endpoint and the hash of its own last block is the final one.
//!
//! The byte layout does not change, which is what keeps the batch and bundle circuits untouched
//! and means nothing has to be migrated. What the published value *means* does change, and it
//! propagates: the batch and the bundle take their own endpoints from the chunk's, so the value a
//! bundle proof finalizes on-chain is a block hash from Tsuki.

mod common;

use common::{TSUKI_VERSION_BYTE, base_fee_chunk, feynman_blocks};
use sbv_core::BlockWitness;
use sbv_primitives::types::consensus::BlockHeader as _;
use sbv_primitives::{B256, U256, types::Header};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute, fork_is_anchored};
use types_base::{
    fork_name::ForkName,
    public_inputs::MultiVersionPublicInputs,
    public_inputs::scroll::chunk::{BlockContextV2, ChunkInfo, SIZE_BLOCK_CTX},
    version::Version,
};

/// The size of a Tsuki chunk's public inputs before the per-block contexts:
/// version || chain id || prev endpoint || post endpoint || withdraw root || next message index ||
/// tx data digest || prev msg queue hash || post msg queue hash || initial block number.
const SIZE_TSUKI_PI_HEADER: usize = 1 + 8 + 32 + 32 + 32 + 8 + 32 + 32 + 32 + 8;

fn tsuki_witness(parent: Header, blocks: &[BlockWitness]) -> ChunkWitness {
    ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        blocks,
        B256::ZERO,
        ForkName::Tsuki,
        parent,
    )
}

/// The Tsuki public-input layout is unchanged: 269 bytes for a one-block chunk, with the two
/// endpoints in the slots that held the state roots.
#[test]
fn the_tsuki_public_input_layout_is_unchanged() {
    let prev = B256::repeat_byte(1);
    let post = B256::repeat_byte(2);
    let chunk_info = ChunkInfo {
        chain_id: 1,
        prev_state_root: prev,
        post_state_root: post,
        withdraw_root: B256::repeat_byte(3),
        next_message_index: 4,
        data_hash: B256::ZERO,
        tx_data_digest: B256::repeat_byte(5),
        prev_msg_queue_hash: B256::repeat_byte(6),
        post_msg_queue_hash: B256::repeat_byte(7),
        tx_data_length: 8,
        initial_block_number: 9,
        block_ctxs: vec![BlockContextV2 {
            timestamp: 10,
            base_fee: U256::from(11),
            gas_limit: 12,
            num_txs: 13,
            num_l1_msgs: 14,
        }],
        prev_blockhash: B256::repeat_byte(15),
        post_blockhash: B256::repeat_byte(16),
        encryption_key: None,
    };

    let pi = chunk_info.pi_tsuki(Version::tsuki());

    assert_eq!(pi.len(), SIZE_TSUKI_PI_HEADER + SIZE_BLOCK_CTX);
    assert_eq!(pi.len(), 269);
    assert_eq!(&pi[9..41], prev.as_slice());
    assert_eq!(&pi[41..73], post.as_slice());
}

/// Endpoint semantics change at Tsuki; parent-header validation applies to every supported fork.
#[test]
fn only_tsuki_and_later_are_anchored() {
    for fork in [
        ForkName::EuclidV1,
        ForkName::EuclidV2,
        ForkName::Feynman,
        ForkName::Galileo,
        ForkName::GalileoV2,
    ] {
        assert!(!fork_is_anchored(fork), "{fork} is not anchored");
    }
    assert!(fork_is_anchored(ForkName::Tsuki));
}

/// Earlier forks keep state-root endpoints while also validating the supplied parent.
#[test]
#[ignore = "Feynman corpus needs native SBV witnesses with L2SystemConfig slot 101 proofs"]
fn legacy_chunk_endpoints_remain_state_roots() {
    let blocks = feynman_blocks();
    let (parent, blocks) = blocks.split_first().unwrap();
    let witness = ChunkWitness::new_scroll(
        Version::feynman().as_version_byte(),
        blocks,
        B256::repeat_byte(1),
        ForkName::Feynman,
        parent.header.clone(),
    );

    let chunk_info = execute(witness).expect("a Feynman chunk with its parent header");

    assert_eq!(chunk_info.prev_state_root, blocks[0].prev_state_root);
    assert_eq!(
        chunk_info.post_state_root,
        blocks.last().unwrap().header.state_root
    );
    // The block-hash fields keep the meaning they always had.
    assert_eq!(chunk_info.prev_blockhash, blocks[0].header.parent_hash);
    assert_eq!(
        chunk_info.post_blockhash,
        blocks.last().unwrap().header.hash_slow()
    );
}

/// A Tsuki chunk's endpoints are the hash of the header it opens and the hash of its last block.
#[test]
fn tsuki_chunk_endpoints_are_block_hashes() {
    let (parent, blocks) = base_fee_chunk();
    let last_hash = blocks.last().unwrap().header.hash_slow();
    let parent_hash = parent.hash_slow();

    let chunk_info = execute(tsuki_witness(parent, &blocks)).expect("an honest Tsuki chunk");

    assert_eq!(chunk_info.prev_state_root, parent_hash);
    assert_eq!(chunk_info.post_state_root, last_hash);
    // The block-hash fields keep their values, so for Tsuki the two pairs agree.
    assert_eq!(chunk_info.prev_blockhash, parent_hash);
    assert_eq!(chunk_info.post_blockhash, last_hash);
}

/// Two consecutive chunks meet at one block: the endpoint the first chunk ends on is the endpoint
/// the second chunk starts from, and both are the hash of that boundary block.
#[test]
fn consecutive_tsuki_chunks_meet_at_the_boundary_block() {
    let (genesis, blocks) = base_fee_chunk();
    // Split at the block whose transaction changes the L2 base-fee overhead, so the second chunk
    // is the one that first has to read the new value out of its own pre-state.
    let boundary = &blocks[3];
    assert_eq!(boundary.header.number, 4);

    let first = execute(tsuki_witness(genesis, &blocks[..4])).expect("the first chunk");
    let second =
        execute(tsuki_witness(boundary.header.clone(), &blocks[4..])).expect("the second chunk");

    assert_eq!(first.post_state_root, boundary.header.hash_slow());
    assert_eq!(second.prev_state_root, first.post_state_root);
    assert_eq!(
        second.initial_block_number,
        first.initial_block_number + first.block_ctxs.len() as u64
    );
}

/// The chain's first chunk opens its genesis header, so the genesis endpoint is the genesis block
/// hash. The corpus carries that header as `witnesses/parent.json`.
#[test]
fn the_first_tsuki_chunk_opens_the_genesis_header() {
    let (genesis, blocks) = base_fee_chunk();
    assert_eq!(blocks[0].header.number, 1, "the corpus starts at block 1");
    assert_eq!(genesis.number, 0);
    assert_eq!(genesis.hash_slow(), blocks[0].header.parent_hash);

    let chunk_info = execute(tsuki_witness(genesis.clone(), &blocks)).expect("the first chunk");

    assert_eq!(chunk_info.prev_state_root, genesis.hash_slow());
    assert_eq!(chunk_info.initial_block_number, 1);
}

/// The chunk's claimed pre-state root has to be the trusted parent's state root. Mutating the
/// witness rather than the parent is what reaches this rule: changing the parent would change its
/// hash, and the chunk would be rejected for not continuing from it instead (which
/// `a_substituted_parent_header_is_rejected` covers).
#[test]
fn a_pre_state_root_that_is_not_the_parents_is_rejected() {
    let (genesis, mut blocks) = base_fee_chunk();
    blocks[0].prev_state_root = B256::repeat_byte(9);

    let error = execute(tsuki_witness(genesis, &blocks))
        .expect_err("the chunk must start from the state the parent committed to");
    assert!(
        error.starts_with("verify error: parent state root mismatch:"),
        "{error}"
    );
}

/// Two chunks that really do follow each other satisfy the contract the batch circuit enforces:
/// `ChunkInfo::validate` chains the endpoints and the rolling message queue hash. Under Tsuki the
/// endpoint half of that is strictly stronger than it was, because a block hash commits to the
/// state root and to everything else in the header.
#[test]
fn a_chained_tsuki_pair_validates() {
    let (genesis, blocks) = base_fee_chunk();
    let boundary = &blocks[3];

    let first = execute(tsuki_witness(genesis, &blocks[..4])).expect("the first chunk");
    // The successor's rolling message queue hash is the one its predecessor ended on; block 3
    // carries an L1 message, so this is not zero and the pair would not chain without it.
    let second = ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        &blocks[4..],
        first.post_msg_queue_hash,
        ForkName::Tsuki,
        boundary.header.clone(),
    );
    let second = execute(second).expect("the second chunk");

    assert_ne!(
        first.post_msg_queue_hash,
        B256::ZERO,
        "block 3 enqueues one"
    );
    second.validate(&first, Version::tsuki());
}

/// The two values a batch publishes as its own endpoints are the first chunk's previous endpoint
/// and the last chunk's final one: `BatchInfoBuilderV7::build` copies them straight across
/// (`crates/types/batch/src/builder/v7.rs`, `parent_state_root: first_chunk.prev_state_root` and
/// `state_root: last_chunk.post_state_root`), and the bundle takes the batch's. So under Tsuki a
/// batch - and the value a bundle proof finalizes on-chain - publishes block hashes.
///
/// This pins the two values that get copied, over the two chunks a batch of this corpus would
/// span. Driving `BatchInfoBuilderV7` itself needs the blob bytes and the KZG point-evaluation
/// witness that go with them, which only the integration harness builds, and its test binaries do
/// not link (see the PR body); this is the part reachable from a crate that does.
#[test]
fn the_endpoints_a_batch_would_publish_are_block_hashes() {
    let (genesis, blocks) = base_fee_chunk();
    let boundary = &blocks[3];

    let first = execute(tsuki_witness(genesis.clone(), &blocks[..4])).expect("the first chunk");
    let last = execute(ChunkWitness::new_scroll(
        TSUKI_VERSION_BYTE,
        &blocks[4..],
        first.post_msg_queue_hash,
        ForkName::Tsuki,
        boundary.header.clone(),
    ))
    .expect("the last chunk");

    // What the batch builder would copy into `BatchInfo::parent_state_root` / `state_root`.
    assert_eq!(first.prev_state_root, genesis.hash_slow());
    assert_eq!(
        last.post_state_root,
        blocks.last().unwrap().header.hash_slow()
    );
    // Neither is a state root any more, which is the whole point.
    assert_ne!(first.prev_state_root, genesis.state_root);
    assert_ne!(
        last.post_state_root,
        blocks.last().unwrap().header.state_root
    );
}

/// The one new wire field has to survive the encoding the guest decodes, in both states, and the
/// decoded valid witness has to execute to the same public inputs. A missing parent stays invalid.
#[rstest::rstest]
#[case::with_parent(true)]
#[case::missing_parent(false)]
fn the_witness_round_trips_through_bincode_and_json(#[case] has_parent: bool) {
    let (genesis, blocks) = base_fee_chunk();
    let witness = if has_parent {
        tsuki_witness(genesis, &blocks)
    } else {
        // Missing parents still round-trip, but no fork may execute such a witness.
        let mut witness = ChunkWitness::new_scroll(
            Version::feynman().as_version_byte(),
            &blocks,
            B256::ZERO,
            ForkName::Feynman,
            genesis,
        );
        witness.parent_header = None;
        witness
    };
    let expected = witness.parent_header.as_ref().map(Header::hash_slow);

    let config = bincode::config::standard();
    let bytes = bincode::serde::encode_to_vec(&witness, config).expect("encode");
    let (decoded, _): (ChunkWitness, usize) =
        bincode::serde::decode_from_slice(&bytes, config).expect("decode");
    assert_eq!(
        decoded.parent_header.as_ref().map(Header::hash_slow),
        expected
    );

    let json = serde_json::to_string(&witness).expect("to json");
    let from_json: ChunkWitness = serde_json::from_str(&json).expect("from json");
    assert_eq!(
        from_json.parent_header.as_ref().map(Header::hash_slow),
        expected
    );

    // The decoded witness has to be the same chunk, not merely the same bytes.
    if has_parent {
        assert_eq!(
            execute(decoded)
                .expect("the decoded chunk")
                .pi_hash_by_version(Version::tsuki()),
            execute(witness)
                .expect("the encoded chunk")
                .pi_hash_by_version(Version::tsuki())
        );
    } else {
        assert_eq!(
            execute(decoded).unwrap_err(),
            "missing parent header for a feynman chunk"
        );
        assert_eq!(
            execute(from_json).unwrap_err(),
            "missing parent header for a feynman chunk"
        );
    }
}

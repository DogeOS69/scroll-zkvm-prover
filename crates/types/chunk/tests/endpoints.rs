//! A chunk's public inputs carry its boundary endpoints in the `prev_state_root` and
//! `post_state_root` slots. From Tsuki those slots carry the boundary **L2 block hashes** instead
//! of state roots: the chunk opens the header of the block before its first block, so the hash of
//! that header is the previous endpoint and the hash of its own last block is the final one. The
//! byte layout does not change, which is what keeps the batch and bundle circuits and the
//! persisted protocol state untouched.

mod common;

use common::{TSUKI_VERSION_BYTE, base_fee_chunk, feynman_blocks};
use sbv_core::BlockWitness;
use sbv_primitives::{B256, U256, types::Header};
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, execute};
use types_base::{
    fork_name::ForkName,
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
        Some(parent),
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

/// Earlier forks keep state-root endpoints, and their chunks still need no parent header.
#[test]
fn legacy_chunk_endpoints_remain_state_roots() {
    let blocks = feynman_blocks();
    let witness = ChunkWitness::new_scroll(
        Version::feynman().as_version_byte(),
        &blocks,
        B256::repeat_byte(1),
        ForkName::Feynman,
        None,
    );

    let chunk_info = execute(witness).expect("a Feynman chunk needs no parent header");

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

/// A Tsuki chunk opened with a header that is not the one its first block names is not that
/// chain's chunk, whatever else is consistent about it.
#[test]
fn a_substituted_genesis_header_is_rejected() {
    let (mut genesis, blocks) = base_fee_chunk();
    genesis.state_root = B256::repeat_byte(9);

    let error = execute(tsuki_witness(genesis, &blocks))
        .expect_err("the genesis header must be the one the chain starts from");
    assert!(
        error.starts_with("verify error: parent header mismatch:"),
        "{error}"
    );
}

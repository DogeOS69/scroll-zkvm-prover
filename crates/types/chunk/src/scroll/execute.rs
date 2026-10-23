use super::{ChunkWitness, types::ChunkExt};
use ecies::SecretKey;
use sbv_core::verifier::{self, VerifyResult};
use sbv_helpers::manually_drop_on_zkvm;
use sbv_primitives::{
    B256, U256,
    chainspec::{Chain, build_chain_spec_force_hardfork},
    hardforks::Hardfork,
    types::{
        consensus::BlockHeader,
        reth::primitives::{Block, RecoveredBlock},
    },
};
use types_base::{
    fork_name::ForkName,
    public_inputs::scroll::chunk::{BlockContextV2, ChunkInfo},
    version::Version,
};

/// Executes the chunk and derives its public inputs.
///
/// `version`, `fork_name`, `parent_header`, `compression_infos` and `validium` are all untrusted
/// prover data, and between them they select which rules the chunk is held to: `fork_name` decides
/// whether the chunk is anchored and whether the base-fee rule applies, and `version` decides the
/// public-input layout and the domain. They are checked against each other and against the blocks
/// before anything is executed or hashed.
pub fn execute(witness: ChunkWitness) -> Result<ChunkInfo, String> {
    if witness.blocks.is_empty() {
        return Err("a chunk has no blocks".to_string());
    }
    check_version_fork(&witness)?;
    check_validium_inputs(&witness)?;
    check_parent_header(&witness)?;

    let chain = Chain::from_id(witness.blocks[0].chain_id);
    let post_blockhash = witness
        .blocks
        .last()
        .expect("witnesses can not be empty")
        .header
        .hash_slow();
    let hardfork = match witness.fork_name {
        ForkName::EuclidV1 | ForkName::EuclidV2 => {
            return Err(format!(
                "{} is no longer supported by the Feynman+ DogeOS execution client",
                witness.fork_name
            ));
        }
        ForkName::Feynman => Hardfork::Feynman,
        ForkName::Galileo => Hardfork::Galileo,
        ForkName::GalileoV2 => Hardfork::GalileoV2,
        ForkName::Tsuki => Hardfork::Tsuki,
    };

    // From Tsuki the chunk opens the header of the block before its first block: that header's
    // hash is the chunk's previous proof endpoint, and its state root is the state the chunk
    // starts from. SBV takes it from here, checks the chunk against it, and runs the shared
    // DogeOS header rules over the whole chain of headers starting from it - including the ones
    // that relate each header to its predecessor, which is why they are not repeated here.
    // `check_parent_header` has already established that it is present exactly when it is needed.
    let parent_header = witness.parent_header.as_ref();

    let chain_spec = build_chain_spec_force_hardfork(chain, hardfork);

    let VerifyResult {
        blocks,
        pre_state_root,
        post_state_root,
        withdraw_root,
        next_message_index,
        ..
    } = verifier::run(
        &witness.blocks,
        parent_header,
        chain_spec,
        witness.compression_infos,
    )
    .map_err(|e| format!("verify error: {e}"))?;

    // Under header-anchored endpoints a chunk's endpoints are the block hashes at its boundary,
    // carried in the slots that held state roots before. The parent's state root is still the
    // state the chunk starts from; SBV checks that against the witness's pre-state root.
    //
    // `prev_blockhash` is taken from the trusted parent too, rather than from the first block's
    // own `parent_hash`: SBV checks the two against each other, and this way the published field
    // is derived from the header that was checked rather than from the witness's copy of its hash.
    let (prev_endpoint, post_endpoint, prev_blockhash) = match parent_header {
        Some(parent) => {
            let parent_hash = parent.hash_slow();
            (parent_hash, post_blockhash, parent_hash)
        }
        None => (
            pre_state_root,
            post_state_root,
            witness.blocks[0].header.parent_hash(),
        ),
    };

    let blocks = manually_drop_on_zkvm!(blocks);
    let mut rlp_buffer = manually_drop_on_zkvm!(Vec::with_capacity(2048));
    let (tx_data_length, tx_data_digest) = blocks.tx_bytes_hash_in(rlp_buffer.as_mut());

    let data_hash = if witness.fork_name < ForkName::EuclidV2 {
        blocks.legacy_data_hash()
    } else {
        B256::default()
    };
    let post_msg_queue_hash = if witness.fork_name >= ForkName::EuclidV2 {
        blocks.rolling_msg_queue_hash(witness.prev_msg_queue_hash, witness.validium.as_ref())
    } else {
        B256::default()
    };

    let chunk_info = ChunkInfo {
        chain_id: chain.id(),
        prev_state_root: prev_endpoint,
        post_state_root: post_endpoint,
        data_hash,
        withdraw_root,
        next_message_index,
        tx_data_digest,
        tx_data_length: tx_data_length as u64,
        initial_block_number: blocks[0].header().number,
        prev_msg_queue_hash: witness.prev_msg_queue_hash,
        post_msg_queue_hash,
        block_ctxs: blocks.iter().map(block_to_context).collect(),
        prev_blockhash,
        post_blockhash,
        encryption_key: witness.validium.map(|input| {
            SecretKey::try_from_bytes(input.secret_key)
                .expect("validium key")
                .public_key()
                .to_bytes(true)
        }),
    };

    #[cfg(target_os = "zkvm")]
    println!("chunk_info = {}", chunk_info);

    Ok(chunk_info)
}

/// Validium inputs must be present exactly when the version's domain is validium, with one list
/// of validium transactions per block. Otherwise the rolling message queue hash would cover a
/// different set of L1 messages from the one that was executed.
/// The fork the chunk claims has to be the fork its version byte names.
///
/// `fork_name` selects the rules - whether the chunk is anchored to a parent header at all, and
/// whether the base-fee rule applies - while `version` selects the public-input layout the result
/// is serialised into. The guest asserts the same equality before it commits anything, so a proof
/// cannot pair the legacy state-root path with a Tsuki public input; this is the host-side half, so
/// that metadata derived outside the guest cannot either.
fn check_version_fork(witness: &ChunkWitness) -> Result<(), String> {
    let version = Version::from(witness.version);
    if version.fork != witness.fork_name {
        return Err(format!(
            "fork {} does not match the fork of version byte {}, which is {}",
            witness.fork_name, witness.version, version.fork
        ));
    }
    Ok(())
}

/// The trusted parent header has to be present exactly when the chunk is anchored to one.
///
/// Both directions matter. Without it a Tsuki chunk has nothing binding its first block, its
/// pre-state root or its published previous endpoint. With it on an earlier fork, anchoring data
/// would be supplied, accepted and then silently discarded - the same mistake
/// [`check_validium_inputs`] refuses for the validium inputs.
fn check_parent_header(witness: &ChunkWitness) -> Result<(), String> {
    let anchored = super::fork_is_anchored(witness.fork_name);
    match (&witness.parent_header, anchored) {
        (None, true) => Err(format!(
            "missing parent header for a {} chunk",
            witness.fork_name
        )),
        (Some(_), false) => Err(format!(
            "parent header supplied for a {} chunk, which is not anchored to one",
            witness.fork_name
        )),
        _ => Ok(()),
    }
}

fn check_validium_inputs(witness: &ChunkWitness) -> Result<(), String> {
    let version = Version::from(witness.version);
    match &witness.validium {
        Some(_) if !version.is_validium() => Err(format!(
            "validium inputs supplied for a {:?} domain chunk",
            version.domain
        )),
        None if version.is_validium() => {
            Err("validium inputs missing for a validium domain chunk".to_string())
        }
        Some(validium) if validium.validium_txs.len() != witness.blocks.len() => Err(format!(
            "validium transaction lists count {} != block count {}",
            validium.validium_txs.len(),
            witness.blocks.len()
        )),
        _ => Ok(()),
    }
}

fn block_to_context(block: &RecoveredBlock<Block>) -> BlockContextV2 {
    BlockContextV2 {
        timestamp: block.timestamp,
        gas_limit: block.gas_limit,
        base_fee: U256::from(block.base_fee_per_gas().expect("base_fee_expected")),
        num_txs: u16::try_from(block.body().transactions.len()).expect("num txs u16"),
        num_l1_msgs: u16::try_from(
            block
                .body()
                .transactions
                .iter()
                .filter(|tx| tx.is_l1_message())
                .count(),
        )
        .expect("num l1 msgs u16"),
    }
}

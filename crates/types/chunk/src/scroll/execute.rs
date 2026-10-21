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
        reth::{
            consensus::{DogeosConsensus, HeaderValidator},
            primitives::{Block, RecoveredBlock, SealedHeader},
        },
    },
};
use types_base::{
    fork_name::ForkName,
    public_inputs::scroll::chunk::{BlockContextV2, ChunkInfo},
    version::Version,
};

/// Executes the chunk and derives its public inputs.
///
/// `compression_infos` and `validium` are untrusted prover data: their shapes are checked against
/// the blocks and the version before anything is executed or hashed.
pub fn execute(witness: ChunkWitness) -> Result<ChunkInfo, String> {
    check_validium_inputs(&witness)?;

    let chain = Chain::from_id(witness.blocks[0].chain_id);
    let prev_blockhash = witness.blocks[0].header.parent_hash();
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

    // Earlier forks retain their historical header format. Tsuki proofs must use the same
    // canonical header rules as the execution client, even though SBV only executes transactions.
    if witness.fork_name >= ForkName::Tsuki {
        for block in &witness.blocks {
            let header = SealedHeader::new_unhashed(block.header.clone());
            DogeosConsensus.validate_header(&header).map_err(|error| {
                format!("invalid header at block {}: {error}", block.header.number)
            })?;
        }
    }

    let chain_spec = build_chain_spec_force_hardfork(chain, hardfork);

    let VerifyResult {
        blocks,
        pre_state_root,
        post_state_root,
        withdraw_root,
        next_message_index,
        ..
    } = verifier::run(&witness.blocks, chain_spec, witness.compression_infos)
        .map_err(|e| format!("verify error: {e}"))?;

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
        prev_state_root: pre_state_root,
        post_state_root,
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

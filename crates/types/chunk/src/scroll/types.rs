use super::{types::validium::SecretKey, witness::ValidiumInputs};
use alloy_primitives::keccak256;
use alloy_sol_types::sol;
use itertools::Itertools;
use sbv_helpers::manually_drop_on_zkvm;
use sbv_primitives::{
    B256, U256,
    types::{
        consensus::TxL1Message,
        eips::Encodable2718,
        reth::primitives::{Block, RecoveredBlock, TransactionSigned},
    },
};
use std::ops::Deref;

pub mod validium;

sol! {
    #[derive(Debug)]
    function relayMessage(
        address sender,
        address target,
        uint256 value,
        uint256 messageNonce,
        bytes message
    );

    #[derive(Debug)]
    function finalizeDepositERC20(
        address token,
        address l2Token,
        address from,
        address to,
        uint256 amount,
        bytes l2Data
    );

    #[derive(Debug)]
    function finalizeDepositERC20Encrypted(
        address token,
        address l2Token,
        address from,
        bytes to,
        uint256 amount,
        bytes l2Data
    );

}

const LEGACY_DA_HEADER_LEN: usize = size_of::<u64>() // block number
        + size_of::<u64>() // timestamp
        + U256::BYTES // base fee per gas
        + size_of::<u64>() // gas limit
        + size_of::<u16>(); // l1 tx count

pub trait ChunkExt {
    /// Hash the transaction bytes.
    ///
    /// Only L2 transactions are considered while computing the digest.
    fn tx_bytes_hash_in(&self, rlp_buffer: &mut Vec<u8>) -> (usize, B256);
    /// Data hash before Euclid V2
    fn legacy_data_hash(&self) -> B256;
    /// Rolling message queue hash after Euclid V2
    fn rolling_msg_queue_hash(
        &self,
        rolling_hash: B256,
        validium_inputs: Option<&ValidiumInputs>,
    ) -> B256;
}

impl<T: Deref<Target = [RecoveredBlock<Block>]>> ChunkExt for T {
    #[inline]
    fn tx_bytes_hash_in(&self, rlp_buffer: &mut Vec<u8>) -> (usize, B256) {
        let blocks = self.as_ref();
        blocks
            .iter()
            .flat_map(|b| b.body().transactions.iter())
            .tx_bytes_hash_in(rlp_buffer)
    }

    #[inline]
    fn legacy_data_hash(&self) -> B256 {
        let blocks = self.as_ref();

        let num_l1_txs: usize = blocks
            .iter()
            .map(|b| {
                b.body()
                    .transactions
                    .iter()
                    .filter(|tx| tx.is_l1_message())
                    .count()
            })
            .sum();

        let mut buffer = manually_drop_on_zkvm!(Vec::with_capacity(
            blocks.len() * LEGACY_DA_HEADER_LEN + num_l1_txs * size_of::<B256>(),
        ));

        for block in blocks.iter() {
            block.encode_legacy_da_header(&mut buffer);
        }
        for block in blocks.iter() {
            block.encode_legacy_l1_msg(&mut buffer);
        }
        keccak256(&*buffer)
    }

    #[inline]
    fn rolling_msg_queue_hash(
        &self,
        mut rolling_hash: B256,
        validium_inputs: Option<&ValidiumInputs>,
    ) -> B256 {
        let blocks = self.as_ref();

        if let Some(ValidiumInputs {
            validium_txs,
            secret_key,
        }) = validium_inputs
        {
            let secret_key = SecretKey::try_from_bytes(secret_key).expect("valid secret key");

            for (block, validium_txs) in blocks.iter().zip_eq(validium_txs.iter()) {
                rolling_hash = block
                    .hash_msg_queue(&rolling_hash, Some((validium_txs.as_slice(), &secret_key)));
            }
        } else {
            for block in blocks.iter() {
                rolling_hash = block.hash_msg_queue(&rolling_hash, None);
            }
        }
        rolling_hash
    }
}

/// Helper trait for hashing transaction bytes.
trait TxBytesHashExt {
    /// Hash the transaction bytes.
    ///
    /// Only L2 transactions are considered while computing the digest.
    fn tx_bytes_hash_in(self, rlp_buffer: &mut Vec<u8>) -> (usize, B256);
}

impl<'a, I: Iterator<Item = &'a TransactionSigned>> TxBytesHashExt for I {
    #[inline]
    fn tx_bytes_hash_in(self, rlp_buffer: &mut Vec<u8>) -> (usize, B256) {
        rlp_buffer.clear();
        // Ignore L1 msg txs.
        for tx in self.filter(|&tx| !tx.is_l1_message()) {
            tx.encode_2718(rlp_buffer);
        }
        let hash = keccak256(&rlp_buffer);
        let len = rlp_buffer.len();
        rlp_buffer.clear();
        (len, hash)
    }
}

/// Chunk related extension methods for Block
trait BlockExt {
    /// Get an iterator over L1 message transactions in the block.
    fn l1_txs_iter(&self) -> impl Iterator<Item = &TxL1Message>;

    /// Hash the header of the block
    fn encode_legacy_da_header(&self, buffer: &mut Vec<u8>);
    /// Hash the l1 messages of the block
    fn encode_legacy_l1_msg(&self, buffer: &mut Vec<u8>);
    /// Hash the l1 messages of the block
    fn hash_msg_queue(
        &self,
        initial_queue_hash: &B256,
        validium_txs: Option<(&[TxL1Message], &SecretKey)>,
    ) -> B256;
}

impl BlockExt for RecoveredBlock<Block> {
    #[inline]
    fn l1_txs_iter(&self) -> impl Iterator<Item = &TxL1Message> {
        self.body()
            .transactions
            .iter()
            .filter_map(|tx| tx.as_l1_message())
            .map(|tx| tx.inner())
    }

    #[inline]
    fn encode_legacy_da_header(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&self.number.to_be_bytes());
        buffer.extend_from_slice(&self.timestamp.to_be_bytes());
        buffer.extend_from_slice(
            &U256::from_limbs([self.base_fee_per_gas.unwrap_or_default(), 0, 0, 0])
                .to_be_bytes::<{ U256::BYTES }>(),
        );
        buffer.extend_from_slice(&self.gas_limit.to_be_bytes());
        // FIXME: l1 tx could be skipped, the actual tx count needs to be calculated
        buffer.extend_from_slice(&(self.body().transactions.len() as u16).to_be_bytes());
    }

    #[inline]
    fn encode_legacy_l1_msg(&self, buffer: &mut Vec<u8>) {
        for tx in self
            .body()
            .transactions
            .iter()
            .filter_map(|tx| tx.as_l1_message())
        {
            buffer.extend_from_slice(tx.hash_ref().as_ref());
        }
    }

    #[inline]
    fn hash_msg_queue(
        &self,
        initial_queue_hash: &B256,
        validium_txs: Option<(&[TxL1Message], &SecretKey)>,
    ) -> B256 {
        let mut rolling_hash = *initial_queue_hash;

        let mut hash_tx = |tx_hash: B256| {
            rolling_hash = hash_msg_queue_step(rolling_hash, tx_hash);
        };

        if let Some((txs, secret_key)) = validium_txs {
            for (validium_tx, tx_in_block) in txs.iter().zip_eq(self.l1_txs_iter()) {
                match validium::decrypt(validium_tx, secret_key) {
                    Ok(decrypted) => {
                        assert_eq!(decrypted, *tx_in_block);
                    }
                    Err(_) => {
                        // when decryption has failed, the validium_tx is just used for execution with its encrypted bytes
                        assert_eq!(*validium_tx, *tx_in_block);
                    }
                }

                hash_tx(validium_tx.tx_hash());
            }
        } else {
            for tx in self.l1_txs_iter() {
                hash_tx(tx.tx_hash());
            }
        };

        rolling_hash
    }
}

/// DogeOS uses the entire digest, including the bytes Scroll reserves for timestamps.
#[inline]
fn hash_msg_queue_step(rolling_hash: B256, tx_hash: B256) -> B256 {
    let mut buffer = [0u8; 64];
    buffer[..32].copy_from_slice(rolling_hash.as_slice());
    buffer[32..].copy_from_slice(tx_hash.as_slice());
    keccak256(buffer)
}

#[cfg(test)]
mod queue_hash_tests {
    use super::*;
    use alloy_primitives::{Address, Bytes, b256};

    #[test]
    fn full_width_queue_hash_vectors() {
        // Same independent vectors as dogeos-core and rollup-node.
        let first = hash_msg_queue_step(B256::ZERO, B256::ZERO);
        assert_eq!(
            first,
            b256!("ad3228b676f7d3cd4284a5443f17f1962b36e491b30a40b2405849e597ba5fb5")
        );
        assert_eq!(
            hash_msg_queue_step(first, B256::repeat_byte(1)),
            b256!("760121498932758a8b6e34be15314cb69ad8a8a80f94f1d332797f4d61a8bfc6")
        );
    }

    #[test]
    fn block_queue_hash_preserves_empty_seed_and_full_intermediate_digest() {
        let seed = B256::repeat_byte(0xAB);
        let empty = RecoveredBlock::new_unhashed(Block::default(), vec![]);
        assert_eq!(empty.hash_msg_queue(&seed, None), seed);
        let first = TxL1Message {
            queue_index: 0,
            gas_limit: 21000,
            sender: Address::ZERO,
            to: Address::ZERO,
            value: U256::ZERO,
            input: Bytes::new(),
        };
        let second = TxL1Message {
            queue_index: 1,
            ..first.clone()
        };
        let first_hash = keccak256([seed.as_slice(), first.tx_hash().as_slice()].concat());
        let expected = keccak256([first_hash.as_slice(), second.tx_hash().as_slice()].concat());
        assert_ne!(&first_hash[28..], &[0; 4]);
        assert_ne!(&expected[28..], &[0; 4]);
        let mut block = Block::default();
        let messages = [first, second];
        block.body.transactions = messages.iter().cloned().map(Into::into).collect();
        let recovered = RecoveredBlock::new_unhashed(block, vec![Address::ZERO; 2]);
        assert_eq!(recovered.hash_msg_queue(&seed, None), expected);
        // Validium messages follow the same full-width hash rule.
        let secret_key = SecretKey::try_from_bytes([1u8; 32]).unwrap();
        assert_eq!(
            recovered.hash_msg_queue(&seed, Some((&messages, &secret_key))),
            expected
        );
    }
}

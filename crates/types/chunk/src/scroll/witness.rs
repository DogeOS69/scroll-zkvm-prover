use super::types::validium::SecretKey;
use alloy_primitives::B256;
use sbv_core::witness::BlockWitness;
use sbv_primitives::types::consensus::TxL1Message;
use sbv_primitives::types::evm::ScrollTxCompressionInfos;
use std::collections::HashSet;
use types_base::version::Version;
use types_base::{fork_name::ForkName, public_inputs::scroll::chunk::ChunkInfo};

/// The witness type accepted by the chunk-circuit.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChunkWitness {
    /// Version byte as per [version][types_base::version].
    pub version: u8,
    /// The block witness for each block in the chunk.
    pub blocks: Vec<BlockWitness>,
    /// The on-chain rolling L1 message queue hash before enqueueing any L1 msg tx from the chunk.
    pub prev_msg_queue_hash: B256,
    /// The code version specify the chain spec
    pub fork_name: ForkName,
    /// The compression info for each block in the chunk.
    pub compression_infos: Vec<ScrollTxCompressionInfos>,
    /// Validium encrypted txs and secret key if this is a validium chain.
    pub validium: Option<ValidiumInputs>,
}

/// The validium inputs for the chunk witness.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ValidiumInputs {
    /// The validium transactions for each block in the chunk.
    pub validium_txs: Vec<Vec<TxL1Message>>,
    /// The secret key used for decrypting validium transactions.
    pub secret_key: Box<[u8]>,
}

#[derive(Clone, Debug)]
pub struct ChunkDetails {
    pub num_blocks: usize,
    pub num_txs: usize,
    pub total_gas_used: u64,
}

impl ChunkWitness {
    /// Deserialize a witness whose input is retained for the process lifetime.
    ///
    /// Block code and state byte strings borrow directly from binary input when
    /// supported by the deserializer. Normal [`serde::Deserialize`] remains an
    /// owning operation and does not require a static input.
    pub fn deserialize_from_static<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'static>,
    {
        static_input::ChunkWitness::deserialize(deserializer)
    }

    pub fn new_scroll(
        version: u8,
        blocks: &[BlockWitness],
        prev_msg_queue_hash: B256,
        fork_name: ForkName,
    ) -> Self {
        Self::new(version, blocks, prev_msg_queue_hash, fork_name, None)
    }

    pub fn new_validium(
        version: u8,
        blocks: &[BlockWitness],
        prev_msg_queue_hash: B256,
        fork_name: ForkName,
        validium_txs: Vec<Vec<TxL1Message>>,
        secret_key: SecretKey,
    ) -> Self {
        Self::new(
            version,
            blocks,
            prev_msg_queue_hash,
            fork_name,
            Some(ValidiumInputs {
                validium_txs,
                secret_key: secret_key.to_bytes(),
            }),
        )
    }

    pub fn new(
        version: u8,
        blocks: &[BlockWitness],
        prev_msg_queue_hash: B256,
        fork_name: ForkName,
        validium: Option<ValidiumInputs>,
    ) -> Self {
        let num_codes = blocks.iter().map(|w| w.codes.len()).sum();
        let mut codes = HashSet::with_capacity(num_codes);

        let num_states = blocks.iter().map(|w| w.states.len()).sum();
        let mut states = HashSet::with_capacity(num_states);

        let blocks: Vec<BlockWitness> = blocks
            .iter()
            .map(|block| BlockWitness {
                chain_id: block.chain_id,
                header: block.header.clone(),
                prev_state_root: block.prev_state_root,
                transactions: block.transactions.clone(),
                withdrawals: block.withdrawals.clone(),
                states: block
                    .states
                    .iter()
                    .filter(|s| states.insert(*s))
                    .cloned()
                    .collect(),
                codes: block
                    .codes
                    .iter()
                    .filter(|c| codes.insert(*c))
                    .cloned()
                    .collect(),
            })
            .collect();

        let compression_infos = blocks
            .iter()
            .map(|block| block.compression_infos())
            .collect();

        Self {
            version,
            blocks,
            prev_msg_queue_hash,
            fork_name,
            compression_infos,
            validium,
        }
    }

    pub fn stats(&self) -> ChunkDetails {
        let num_blocks = self.blocks.len();
        let num_txs = self
            .blocks
            .iter()
            .map(|b| b.transactions.len())
            .sum::<usize>();
        let total_gas_used = self.blocks.iter().map(|b| b.header.gas_used).sum::<u64>();

        ChunkDetails {
            num_blocks,
            num_txs,
            total_gas_used,
        }
    }

    pub fn version(&self) -> Version {
        Version::from(self.version)
    }
}

impl TryFrom<ChunkWitness> for ChunkInfo {
    type Error = String;

    fn try_from(value: ChunkWitness) -> Result<Self, Self::Error> {
        super::execute(value)
    }
}

mod static_input {
    use super::{B256, BlockWitness, ForkName, ScrollTxCompressionInfos, ValidiumInputs};
    use serde::de::{DeserializeSeed, Deserializer, SeqAccess, Visitor};

    // Keep the serialized fields in the same order as the public witness. Remote
    // derive constructs the public type without changing its owning Deserialize.
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ChunkWitness", bound(deserialize = "'de: 'static"))]
    pub(super) struct ChunkWitness {
        version: u8,
        #[serde(deserialize_with = "deserialize_blocks")]
        blocks: Vec<BlockWitness>,
        prev_msg_queue_hash: B256,
        fork_name: ForkName,
        compression_infos: Vec<ScrollTxCompressionInfos>,
        validium: Option<ValidiumInputs>,
    }

    fn deserialize_blocks<D>(deserializer: D) -> Result<Vec<BlockWitness>, D::Error>
    where
        D: Deserializer<'static>,
    {
        struct Blocks;

        impl Visitor<'static> for Blocks {
            type Value = Vec<BlockWitness>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a sequence of block witnesses")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'static>,
            {
                let mut blocks = Vec::new();
                while let Some(block) = sequence.next_element_seed(StaticBlock)? {
                    blocks.push(block);
                }
                Ok(blocks)
            }
        }

        deserializer.deserialize_seq(Blocks)
    }

    struct StaticBlock;

    impl DeserializeSeed<'static> for StaticBlock {
        type Value = BlockWitness;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'static>,
        {
            BlockWitness::deserialize_from_static(deserializer)
        }
    }
}

#[cfg(test)]
mod static_input_tests {
    use super::*;
    use sbv_primitives::{Bytes, U256, types::Header};

    fn witness() -> ChunkWitness {
        let block = BlockWitness {
            chain_id: 6_281_971,
            header: Header::default(),
            prev_state_root: B256::repeat_byte(1),
            transactions: Vec::new(),
            withdrawals: Some(Default::default()),
            states: vec![Bytes::from(vec![0xc1, 0x80])],
            codes: vec![Bytes::from(vec![0x60, 0x01, 0x00])],
        };
        ChunkWitness {
            version: 11,
            blocks: vec![block.clone(), block],
            prev_msg_queue_hash: B256::repeat_byte(2),
            fork_name: ForkName::Tsuki,
            compression_infos: vec![vec![(U256::from(3), 4)], Vec::new()],
            validium: Some(ValidiumInputs {
                validium_txs: vec![Vec::new()],
                secret_key: vec![5; 32].into_boxed_slice(),
            }),
        }
    }

    #[test]
    fn static_chunk_roundtrip_borrows_block_bytes() {
        let config = bincode::config::standard();
        let input: &'static [u8] = bincode::serde::encode_to_vec(witness(), config)
            .unwrap()
            .leak();
        let mut decoder = bincode::serde::BorrowedSerdeDecoder::from_slice(input, config, ());
        let witness = ChunkWitness::deserialize_from_static(decoder.as_deserializer()).unwrap();
        assert_eq!(
            bincode::serde::encode_to_vec(&witness, config).unwrap(),
            input
        );

        let range = input.as_ptr_range();
        for block in &witness.blocks {
            for bytes in block.codes.iter().chain(&block.states) {
                let bytes_range = bytes.as_ptr_range();
                assert!(bytes_range.start >= range.start && bytes_range.end <= range.end);
                assert_eq!(bytes.clone().as_ptr(), bytes.as_ptr());
            }
        }
    }

    #[test]
    fn ordinary_chunk_decode_keeps_ownership() {
        let config = bincode::config::standard();
        let expected = bincode::serde::encode_to_vec(witness(), config).unwrap();
        let mut input = expected.clone();
        let (witness, read): (ChunkWitness, _) =
            bincode::serde::decode_from_slice(&input, config).unwrap();
        assert_eq!(read, input.len());
        input.fill(0);
        drop(input);
        assert_eq!(
            bincode::serde::encode_to_vec(witness, config).unwrap(),
            expected
        );
    }
}

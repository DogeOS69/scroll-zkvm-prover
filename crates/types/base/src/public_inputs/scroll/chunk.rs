use crate::{
    public_inputs::{ForkName, MultiVersionPublicInputs},
    version::{Domain, STFVersion, Version},
};
use alloy_primitives::{B256, U256};
use sbv_primitives::types::reth::consensus::validate_gas_limit_against_parent;

/// Number of bytes used to serialise [`BlockContextV2`].
pub const SIZE_BLOCK_CTX: usize = 52;

/// Represents the version 2 of block context.
///
/// The difference between v2 and v1 is that the block number field has been removed since v2.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct BlockContextV2 {
    /// The timestamp of the block.
    pub timestamp: u64,
    /// The base fee of the block.
    pub base_fee: U256,
    /// The gas limit of the block.
    pub gas_limit: u64,
    /// The number of transactions in the block, including both L1 msg txs as well as L2 txs.
    pub num_txs: u16,
    /// The number of L1 msg txs in the block.
    pub num_l1_msgs: u16,
}

impl From<&[u8]> for BlockContextV2 {
    fn from(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), SIZE_BLOCK_CTX);

        let timestamp = u64::from_be_bytes(bytes[0..8].try_into().expect("should not fail"));
        let base_fee = U256::from_be_slice(&bytes[8..40]);
        let gas_limit = u64::from_be_bytes(bytes[40..48].try_into().expect("should not fail"));
        let num_txs = u16::from_be_bytes(bytes[48..50].try_into().expect("should not fail"));
        let num_l1_msgs = u16::from_be_bytes(bytes[50..52].try_into().expect("should not fail"));

        Self {
            timestamp,
            base_fee,
            gas_limit,
            num_txs,
            num_l1_msgs,
        }
    }
}

impl BlockContextV2 {
    /// Serialize the block context in packed form.
    pub fn to_bytes(&self) -> Vec<u8> {
        std::iter::empty()
            .chain(self.timestamp.to_be_bytes())
            .chain(self.base_fee.to_be_bytes::<32>())
            .chain(self.gas_limit.to_be_bytes())
            .chain(self.num_txs.to_be_bytes())
            .chain(self.num_l1_msgs.to_be_bytes())
            .collect()
    }
}

/// Represents header-like information for the chunk.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ChunkInfo {
    /// The EIP-155 chain ID for all txs in the chunk.
    pub chain_id: u64,
    /// The state root before applying the chunk.
    pub prev_state_root: B256,
    /// The state root after applying the chunk.
    pub post_state_root: B256,
    /// The withdrawals root after applying the chunk.
    pub withdraw_root: B256,
    /// The next message index after applying the chunk.
    pub next_message_index: u64,
    /// Digest of L1 message txs force included in the chunk.
    /// It is a legacy field and can be omitted in new defination
    #[serde(default)]
    pub data_hash: B256,
    /// Digest of L2 tx data flattened over all L2 txs in the chunk.
    pub tx_data_digest: B256,
    /// The L1 msg queue hash at the end of the previous chunk.
    pub prev_msg_queue_hash: B256,
    /// The L1 msg queue hash at the end of the current chunk.
    pub post_msg_queue_hash: B256,
    /// The length of rlp encoded L2 tx bytes flattened over all L2 txs in the chunk.
    pub tx_data_length: u64,
    /// The block number of the first block in the chunk.
    pub initial_block_number: u64,
    /// The block contexts of the blocks in the chunk.
    pub block_ctxs: Vec<BlockContextV2>,
    /// The blockhash of the last block in the previous chunk.
    pub prev_blockhash: B256,
    /// The blockhash of the last block in the current chunk.
    pub post_blockhash: B256,
    /// Optional encryption key for encrypted L1 msgs, which is used in case of domain=Validium.
    pub encryption_key: Option<Box<[u8]>>,
}

impl std::fmt::Display for ChunkInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Create a wrapper struct that implements Debug
        struct DisplayWrapper<'a>(&'a ChunkInfo);

        impl<'a> std::fmt::Debug for DisplayWrapper<'a> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("ChunkInfo")
                    .field("chain_id", &self.0.chain_id)
                    .field("prev_state_root", &self.0.prev_state_root)
                    .field("post_state_root", &self.0.post_state_root)
                    .field("withdraw_root", &self.0.withdraw_root)
                    .field("next_message_index", &self.0.next_message_index)
                    .field("data_hash", &self.0.data_hash)
                    .field("tx_data_digest", &self.0.tx_data_digest)
                    .field("prev_msg_queue_hash", &self.0.prev_msg_queue_hash)
                    .field("post_msg_queue_hash", &self.0.post_msg_queue_hash)
                    .field("tx_data_length", &self.0.tx_data_length)
                    .field("initial_block_number", &self.0.initial_block_number)
                    .field("prev_blockhash", &self.0.prev_blockhash)
                    .field("post_blockhash", &self.0.post_blockhash)
                    .field("block_ctxs", &"<omitted>")
                    .finish()
            }
        }

        // Use the Debug implementation with pretty formatting
        write!(f, "{:#?}", DisplayWrapper(self))
    }
}

impl ChunkInfo {
    /// Public inputs encoded for a given chunk (euclidv1 or da-codec@v6) is defined as
    ///
    /// concat(
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     chunk data hash ||
    ///     tx data hash
    /// )
    pub fn pi_euclidv1(&self) -> Vec<u8> {
        std::iter::empty()
            .chain(&self.chain_id.to_be_bytes())
            .chain(self.prev_state_root.as_slice())
            .chain(self.post_state_root.as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.data_hash.as_slice())
            .chain(self.tx_data_digest.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a given chunk (euclidv2 or da-codec@v7) is defined as
    ///
    /// concat(
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     tx data digest ||
    ///     prev msg queue hash ||
    ///     post msg queue hash ||
    ///     initial block number ||
    ///     block_ctx for block_ctx in block_ctxs
    /// )
    pub fn pi_euclidv2(&self) -> Vec<u8> {
        std::iter::empty()
            .chain(&self.chain_id.to_be_bytes())
            .chain(self.prev_state_root.as_slice())
            .chain(self.post_state_root.as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.tx_data_digest.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .chain(&self.initial_block_number.to_be_bytes())
            .chain(
                self.block_ctxs
                    .iter()
                    .flat_map(|block_ctx| block_ctx.to_bytes())
                    .collect::<Vec<u8>>()
                    .as_slice(),
            )
            .copied()
            .collect()
    }

    /// Feynman chunk public inputs are the same as EuclidV2.
    pub fn pi_feynman(&self) -> Vec<u8> {
        self.pi_euclidv2()
    }

    /// Public inputs encoded for a given chunk (galileo or da-codec@v9) is defined as
    ///
    /// concat(
    ///     version ||
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     tx data digest ||
    ///     prev msg queue hash ||
    ///     post msg queue hash ||
    ///     initial block number ||
    ///     block_ctx for block_ctx in block_ctxs
    /// )
    pub fn pi_galileo(&self, version: Version) -> Vec<u8> {
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(&self.chain_id.to_be_bytes())
            .chain(self.prev_state_root.as_slice())
            .chain(self.post_state_root.as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.tx_data_digest.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .chain(&self.initial_block_number.to_be_bytes())
            .chain(
                self.block_ctxs
                    .iter()
                    .flat_map(|block_ctx| block_ctx.to_bytes())
                    .collect::<Vec<u8>>()
                    .as_slice(),
            )
            .copied()
            .collect()
    }

    /// Public inputs encoded for a given chunk for Scroll@v10 (GalileoV2) is defined as
    ///
    /// concat(
    ///     version ||
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     tx data digest ||
    ///     prev msg queue hash ||
    ///     post msg queue hash ||
    ///     initial block number ||
    ///     block_ctx for block_ctx in block_ctxs
    /// )
    pub fn pi_galileo_v2(&self, version: Version) -> Vec<u8> {
        self.pi_galileo(version)
    }

    /// Public inputs encoded for a given chunk for Scroll@v11 (Tsuki) is defined as
    ///
    /// concat(
    ///     version ||
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     next message index ||
    ///     tx data digest ||
    ///     prev msg queue hash ||
    ///     post msg queue hash ||
    ///     initial block number ||
    ///     block_ctx for block_ctx in block_ctxs ||
    ///     prev blockhash ||
    ///     post blockhash
    /// )
    pub fn pi_tsuki(&self, version: Version) -> Vec<u8> {
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(&self.chain_id.to_be_bytes())
            .chain(self.prev_state_root.as_slice())
            .chain(self.post_state_root.as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(&self.next_message_index.to_be_bytes())
            .chain(self.tx_data_digest.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .chain(&self.initial_block_number.to_be_bytes())
            .chain(
                self.block_ctxs
                    .iter()
                    .flat_map(|block_ctx| block_ctx.to_bytes())
                    .collect::<Vec<u8>>()
                    .as_slice(),
            )
            .chain(self.prev_blockhash.as_slice())
            .chain(self.post_blockhash.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a given chunk for L3 validium @ v1:
    ///
    /// concat(
    ///     version ||
    ///     chain id ||
    ///     prev state root ||
    ///     post state root ||
    ///     withdraw root ||
    ///     tx data digest ||
    ///     prev msg queue hash ||
    ///     post msg queue hash ||
    ///     initial block number ||
    ///     block_ctx for block_ctx in block_ctxs ||
    ///     prev blockhash ||
    ///     post blockhash ||
    ///     encryption key
    /// )
    pub fn pi_validium(&self, version: Version) -> Vec<u8> {
        // Validium keeps the upstream PI layout and intentionally excludes next_message_index.
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(&self.chain_id.to_be_bytes())
            .chain(self.prev_state_root.as_slice())
            .chain(self.post_state_root.as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.tx_data_digest.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .chain(&self.initial_block_number.to_be_bytes())
            .chain(
                self.block_ctxs
                    .iter()
                    .flat_map(|block_ctx| block_ctx.to_bytes())
                    .collect::<Vec<u8>>()
                    .as_slice(),
            )
            .chain(self.prev_blockhash.as_slice())
            .chain(self.post_blockhash.as_slice())
            .chain(self.encryption_key.as_ref().expect("domain=Validium"))
            .copied()
            .collect()
    }
}

pub type VersionedChunkInfo = (ChunkInfo, Version);

impl MultiVersionPublicInputs for ChunkInfo {
    /// Compute the public input hash for the chunk given the version tuple.
    fn pi_by_version(&self, version: Version) -> Vec<u8> {
        match (version.domain, version.stf_version) {
            (Domain::Scroll, STFVersion::V6) => {
                assert_ne!(self.data_hash, B256::ZERO, "v6 must have valid data_hash");
                self.pi_euclidv1()
            }
            (Domain::Scroll, STFVersion::V7) => self.pi_euclidv2(),
            (Domain::Scroll, STFVersion::V8) => self.pi_feynman(),
            (Domain::Scroll, STFVersion::V9) => self.pi_galileo(version),
            (Domain::Scroll, STFVersion::V10) => self.pi_galileo_v2(version),
            (Domain::Scroll, STFVersion::V11) => self.pi_tsuki(version),
            (Domain::Validium, STFVersion::V1) => self.pi_validium(version),
            (domain, stf_version) => {
                unreachable!("unsupported version=({domain:?}, {stf_version:?})")
            }
        }
    }

    /// Validate public inputs between 2 contiguous chunks.
    ///
    /// - chain id MUST match
    /// - state roots MUST be chained
    /// - L1 msg queue hash MUST be chained
    ///
    /// Tsuki also checks block-number, timestamp, gas-limit, and blockhash continuity.
    /// Validiums always chain blockhashes and encryption keys.
    fn validate(&self, prev_pi: &Self, version: Version) {
        assert_eq!(self.chain_id, prev_pi.chain_id);
        assert_eq!(self.prev_state_root, prev_pi.post_state_root);
        assert_eq!(self.prev_msg_queue_hash, prev_pi.post_msg_queue_hash);

        // Scroll@v11 commits next_message_index into the chunk PI, so it must not regress.
        if version.domain == Domain::Scroll && matches!(version.stf_version, STFVersion::V11) {
            assert!(
                self.next_message_index >= prev_pi.next_message_index,
                "next_message_index must not regress"
            );
        }

        if version.fork >= ForkName::Tsuki {
            let parent = prev_pi
                .block_ctxs
                .last()
                .expect("previous chunk must contain blocks");
            let child = self.block_ctxs.first().expect("chunk must contain blocks");
            let previous_block_count =
                u64::try_from(prev_pi.block_ctxs.len()).expect("block count fits in u64");
            let expected_number = prev_pi
                .initial_block_number
                .checked_add(previous_block_count)
                .expect("chunk block number overflow");
            assert_eq!(
                self.initial_block_number, expected_number,
                "non-contiguous chunk heights"
            );
            assert!(
                child.timestamp >= parent.timestamp,
                "chunk timestamp moved backwards"
            );
            validate_gas_limit_against_parent(child.gas_limit, parent.gas_limit)
                .expect("invalid gas limit across chunks");
        }

        if (version.domain == Domain::Scroll && matches!(version.stf_version, STFVersion::V11))
            || version.domain == Domain::Validium
        {
            assert_eq!(
                self.prev_blockhash, prev_pi.post_blockhash,
                "non-contiguous chunk blockhashes"
            );
        }

        // message queue hash is used only after euclidv2 (da-codec@v7)
        if version.fork == ForkName::EuclidV1 {
            assert_eq!(self.prev_msg_queue_hash, B256::ZERO);
            assert_eq!(prev_pi.prev_msg_queue_hash, B256::ZERO);
            assert_eq!(self.post_msg_queue_hash, B256::ZERO);
            assert_eq!(prev_pi.post_msg_queue_hash, B256::ZERO);
        }

        // Encryption keys must be the same between contiguous validium chunks in a batch.
        if version.domain == Domain::Validium {
            assert!(self.encryption_key.is_some());
            assert_eq!(self.encryption_key, prev_pi.encryption_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockContextV2, ChunkInfo};
    use crate::{
        public_inputs::{MultiVersionPublicInputs, Version},
        version::Domain,
    };
    use alloy_primitives::{B256, U256};

    fn sample_chunk_info(next_message_index: u64) -> ChunkInfo {
        ChunkInfo {
            chain_id: 534352,
            prev_state_root: B256::repeat_byte(0x11),
            post_state_root: B256::repeat_byte(0x22),
            withdraw_root: B256::repeat_byte(0x33),
            next_message_index,
            data_hash: B256::repeat_byte(0x44),
            tx_data_digest: B256::repeat_byte(0x55),
            prev_msg_queue_hash: B256::repeat_byte(0x66),
            post_msg_queue_hash: B256::repeat_byte(0x77),
            tx_data_length: 123,
            initial_block_number: 456,
            block_ctxs: vec![BlockContextV2 {
                timestamp: 789,
                base_fee: U256::from(321u64),
                gas_limit: 20_000_000,
                num_txs: 3,
                num_l1_msgs: 1,
            }],
            prev_blockhash: B256::repeat_byte(0x88),
            post_blockhash: B256::repeat_byte(0x99),
            encryption_key: None,
        }
    }

    fn next_contiguous_chunk(prev: &ChunkInfo, next_message_index: u64) -> ChunkInfo {
        ChunkInfo {
            prev_state_root: prev.post_state_root,
            prev_msg_queue_hash: prev.post_msg_queue_hash,
            prev_blockhash: prev.post_blockhash,
            initial_block_number: prev.initial_block_number + prev.block_ctxs.len() as u64,
            ..sample_chunk_info(next_message_index)
        }
    }

    fn panic_message(err: Box<dyn std::any::Any + Send>) -> String {
        match err.downcast::<String>() {
            Ok(msg) => *msg,
            Err(err) => match err.downcast::<&'static str>() {
                Ok(msg) => (*msg).to_string(),
                Err(_) => panic!("unexpected non-string panic payload"),
            },
        }
    }

    #[test]
    fn chunk_json_requires_next_message_index() {
        let mut value = serde_json::to_value(sample_chunk_info(42)).unwrap();
        value
            .as_object_mut()
            .expect("chunk info object")
            .remove("next_message_index");

        let err = serde_json::from_value::<ChunkInfo>(value)
            .expect_err("chunk info must require next_message_index");
        assert!(err.to_string().contains("next_message_index"));
    }

    #[test]
    fn galileov2_chunk_pi_layout_excludes_next_message_index() {
        let pi = sample_chunk_info(0x0102_0304_0506_0708).pi_galileo_v2(Version::galileo_v2());
        let changed_pi =
            sample_chunk_info(0x1112_1314_1516_1718).pi_galileo_v2(Version::galileo_v2());

        assert_eq!(pi.len(), 261);
        assert_eq!(pi[0], Version::galileo_v2().as_version_byte());
        assert_eq!(&pi[105..137], B256::repeat_byte(0x55).as_slice());
        assert_eq!(pi, changed_pi);
    }

    #[test]
    fn tsuki_chunk_pi_layout_commits_next_message_index() {
        let pi = sample_chunk_info(0x0102_0304_0506_0708).pi_tsuki(Version::tsuki());

        assert_eq!(pi.len(), 333);
        assert_eq!(pi[0], Version::tsuki().as_version_byte());
        assert_eq!(&pi[105..113], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&pi[113..145], B256::repeat_byte(0x55).as_slice());
        assert_eq!(&pi[269..301], B256::repeat_byte(0x88).as_slice());
        assert_eq!(&pi[301..333], B256::repeat_byte(0x99).as_slice());
    }

    #[test]
    fn tsuki_commits_both_blockhashes_without_changing_older_pi() {
        let original = sample_chunk_info(22);
        for previous_hash in [false, true] {
            let mut changed = original.clone();
            if previous_hash {
                changed.prev_blockhash = B256::repeat_byte(1);
            } else {
                changed.post_blockhash = B256::repeat_byte(2);
            }
            assert_ne!(
                original.pi_hash_by_version(Version::tsuki()),
                changed.pi_hash_by_version(Version::tsuki())
            );
            assert_eq!(
                original.pi_hash_by_version(Version::galileo_v2()),
                changed.pi_hash_by_version(Version::galileo_v2())
            );
        }
    }

    #[test]
    fn tsuki_chunk_boundaries_accept_equal_timestamps_and_gas_limit_bounds() {
        let mut prev = sample_chunk_info(22);
        prev.block_ctxs.resize(3, prev.block_ctxs[0].clone());
        for gas_limit in [20_000_000 - 19_531, 20_000_000, 20_000_000 + 19_531] {
            let mut current = next_contiguous_chunk(&prev, 22);
            current.block_ctxs[0].gas_limit = gas_limit;
            current.validate(&prev, Version::tsuki());
        }
    }

    #[test]
    fn tsuki_rejects_noncontiguous_chunk_boundaries() {
        type Mutation = (&'static str, fn(&mut ChunkInfo));
        let cases: &[Mutation] = &[
            ("blockhashes", |c| c.prev_blockhash = B256::ZERO),
            ("heights", |c| c.initial_block_number += 1),
            ("timestamp", |c| c.block_ctxs[0].timestamp -= 1),
            ("gas limit", |c| c.block_ctxs[0].gas_limit += 19_532),
            ("gas limit", |c| c.block_ctxs[0].gas_limit -= 19_532),
            ("must contain blocks", |c| c.block_ctxs.clear()),
        ];
        let prev = sample_chunk_info(22);
        for (message, mutate) in cases {
            let mut current = next_contiguous_chunk(&prev, 22);
            mutate(&mut current);
            let error = std::panic::catch_unwind(|| current.validate(&prev, Version::tsuki()))
                .expect_err(message);
            assert!(panic_message(error).contains(message));
        }
    }

    #[test]
    fn tsuki_rejects_invalid_parent_boundaries_and_subminimum_gas() {
        let mut prev = sample_chunk_info(22);
        prev.block_ctxs.clear();
        let current = next_contiguous_chunk(&prev, 22);
        assert!(std::panic::catch_unwind(|| current.validate(&prev, Version::tsuki())).is_err());

        let mut prev = sample_chunk_info(22);
        prev.initial_block_number = u64::MAX;
        let mut current = sample_chunk_info(22);
        current.prev_state_root = prev.post_state_root;
        current.prev_msg_queue_hash = prev.post_msg_queue_hash;
        current.prev_blockhash = prev.post_blockhash;
        current.initial_block_number = 0;
        let error = std::panic::catch_unwind(|| current.validate(&prev, Version::tsuki()))
            .expect_err("height must not wrap around");
        assert!(panic_message(error).contains("block number overflow"));

        let mut prev = sample_chunk_info(22);
        prev.block_ctxs[0].gas_limit = 5_000;
        let mut current = next_contiguous_chunk(&prev, 22);
        current.block_ctxs[0].gas_limit = 4_999;
        let error = std::panic::catch_unwind(|| current.validate(&prev, Version::tsuki()))
            .expect_err("gas limit below the minimum must be rejected");
        assert!(panic_message(error).contains("invalid gas limit"));
    }

    #[test]
    fn tsuki_chunk_validate_reports_regression() {
        let version = Version::tsuki();
        let prev = sample_chunk_info(22);
        let current = next_contiguous_chunk(&prev, 21);

        let err = std::panic::catch_unwind(|| current.validate(&prev, version))
            .expect_err("v11 validation must reject regressions");

        let message = panic_message(err);
        assert!(message.contains("next_message_index must not regress"));
    }

    #[test]
    fn pre_v11_chunk_validate_ignores_next_message_index_regression() {
        let version = Version::galileo_v2();
        assert_eq!(version.domain, Domain::Scroll);

        let prev = sample_chunk_info(22);
        let current = next_contiguous_chunk(&prev, 21);
        current.validate(&prev, version);
    }
}

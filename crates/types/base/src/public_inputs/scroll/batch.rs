use alloy_primitives::B256;

use crate::{
    public_inputs::{ForkName, MultiVersionPublicInputs},
    version::{Domain, STFVersion, Version},
};

/// Represents public-input values for a batch.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BatchInfo {
    /// The state root before applying the batch.
    pub parent_state_root: B256,
    /// The batch hash of the parent batch.
    pub parent_batch_hash: B256,
    /// The state root after applying txs in the batch.
    pub state_root: B256,
    /// The batch header hash of the batch.
    pub batch_hash: B256,
    /// The EIP-155 chain ID of all txs in the batch.
    pub chain_id: u64,
    /// The withdraw root of the last block in the last chunk in the batch.
    pub withdraw_root: B256,
    /// The next message index of the last block in the last chunk in the batch.
    #[serde(default)]
    pub next_message_index: u64,
    /// The L1 msg queue hash at the end of the previous batch.
    pub prev_msg_queue_hash: B256,
    /// The L1 msg queue hash at the end of the current batch.
    pub post_msg_queue_hash: B256,
    /// Optional encryption key, used in case of domain=Validium.
    pub encryption_key: Option<Box<[u8]>>,
}

impl BatchInfo {
    /// Public inputs encoded for a batch (euclidv1 or da-codec@v6) is defined as
    ///
    /// concat(
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    /// )
    fn pi_euclidv1(&self) -> Vec<u8> {
        std::iter::empty()
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a batch (euclidv2 or da-codec@v7) is defined as
    ///
    /// concat(
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    ///     prev msg queue hash ||
    ///     post msg queue hash
    /// )
    fn pi_euclidv2(&self) -> Vec<u8> {
        std::iter::empty()
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a batch (feynman or da-codec@v8).
    ///
    /// Unchanged from euclid-v2.
    fn pi_feynman(&self) -> Vec<u8> {
        self.pi_euclidv2()
    }

    /// Public inputs encoded for a batch (galileo or da-codec@v9) is defined as
    ///
    /// concat(
    ///     version ||
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    ///     prev msg queue hash ||
    ///     post msg queue hash
    /// )
    fn pi_galileo(&self, version: Version) -> Vec<u8> {
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a batch for Scroll@v10 (GalileoV2) is defined as
    ///
    /// concat(
    ///     version ||
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    ///     prev msg queue hash ||
    ///     post msg queue hash
    /// )
    pub fn pi_galileo_v2(&self, version: Version) -> Vec<u8> {
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .copied()
            .collect()
    }

    /// Public inputs encoded for a L3 validium @ v1.
    ///
    /// concat(
    ///     version ||
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    ///     prev msg queue hash ||
    ///     post msg queue hash
    ///     encryption key
    /// )
    fn pi_validium(&self, version: Version) -> Vec<u8> {
        // Validium keeps the upstream PI layout and intentionally excludes next_message_index.
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .chain(self.encryption_key.as_ref().expect("domain=Validium"))
            .copied()
            .collect()
    }

    /// Public inputs encoded for a batch for Scroll@v11 (Tsuki) is defined as
    ///
    /// concat(
    ///     version ||
    ///     parent state root ||
    ///     parent batch hash ||
    ///     state root ||
    ///     batch hash ||
    ///     chain id ||
    ///     withdraw root ||
    ///     next message index ||
    ///     prev msg queue hash ||
    ///     post msg queue hash
    /// )
    pub fn pi_tsuki(&self, version: Version) -> Vec<u8> {
        std::iter::empty()
            .chain(&[version.as_version_byte()])
            .chain(self.parent_state_root.as_slice())
            .chain(self.parent_batch_hash.as_slice())
            .chain(self.state_root.as_slice())
            .chain(self.batch_hash.as_slice())
            .chain(self.chain_id.to_be_bytes().as_slice())
            .chain(self.withdraw_root.as_slice())
            .chain(self.next_message_index.to_be_bytes().as_slice())
            .chain(self.prev_msg_queue_hash.as_slice())
            .chain(self.post_msg_queue_hash.as_slice())
            .copied()
            .collect()
    }
}

pub type VersionedBatchInfo = (BatchInfo, Version);

impl MultiVersionPublicInputs for BatchInfo {
    fn pi_by_version(&self, version: Version) -> Vec<u8> {
        match (version.domain, version.stf_version) {
            (Domain::Scroll, STFVersion::V6) => self.pi_euclidv1(),
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

    /// Validate public inputs between 2 contiguous batches.
    ///
    /// - chain id MUST match
    /// - state roots MUST be chained
    /// - batch hashes MUST be chained
    /// - L1 msg queue hashes MUST be chained
    fn validate(&self, prev_pi: &Self, version: Version) {
        assert_eq!(self.chain_id, prev_pi.chain_id);
        assert_eq!(self.parent_state_root, prev_pi.state_root);
        assert_eq!(self.parent_batch_hash, prev_pi.batch_hash);
        assert_eq!(self.prev_msg_queue_hash, prev_pi.post_msg_queue_hash);

        // Scroll@v11 commits next_message_index into the PI, so it must not regress.
        if version.domain == Domain::Scroll && matches!(version.stf_version, STFVersion::V11) {
            assert!(
                self.next_message_index >= prev_pi.next_message_index,
                "next_message_index must not regress"
            );
        }

        if version.fork == ForkName::EuclidV1 {
            assert_eq!(self.prev_msg_queue_hash, B256::ZERO);
            assert_eq!(prev_pi.prev_msg_queue_hash, B256::ZERO);
            assert_eq!(self.post_msg_queue_hash, B256::ZERO);
            assert_eq!(prev_pi.post_msg_queue_hash, B256::ZERO);
        }

        if version.domain == Domain::Validium {
            assert!(self.encryption_key.is_some());
            assert_eq!(self.encryption_key, prev_pi.encryption_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BatchInfo;
    use crate::{
        public_inputs::{MultiVersionPublicInputs, Version},
        version::Domain,
    };
    use alloy_primitives::B256;

    fn sample_batch_info(next_message_index: u64) -> BatchInfo {
        BatchInfo {
            parent_state_root: B256::repeat_byte(0x11),
            parent_batch_hash: B256::repeat_byte(0x22),
            state_root: B256::repeat_byte(0x33),
            batch_hash: B256::repeat_byte(0x44),
            chain_id: 534352,
            withdraw_root: B256::repeat_byte(0x55),
            next_message_index,
            prev_msg_queue_hash: B256::repeat_byte(0x66),
            post_msg_queue_hash: B256::repeat_byte(0x77),
            encryption_key: None,
        }
    }

    fn next_contiguous_batch(prev: &BatchInfo, next_message_index: u64) -> BatchInfo {
        BatchInfo {
            parent_state_root: prev.state_root,
            parent_batch_hash: prev.batch_hash,
            prev_msg_queue_hash: prev.post_msg_queue_hash,
            ..sample_batch_info(next_message_index)
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
    fn batch_json_requires_next_message_index() {
        let mut value = serde_json::to_value(sample_batch_info(42)).unwrap();
        value
            .as_object_mut()
            .expect("batch info object")
            .remove("next_message_index");

        let err = serde_json::from_value::<BatchInfo>(value)
            .expect_err("batch info must require next_message_index");
        assert!(err.to_string().contains("next_message_index"));
    }

    #[test]
    fn tsuki_batch_pi_layout_commits_next_message_index() {
        let pi = sample_batch_info(0x0102_0304_0506_0708).pi_tsuki(Version::tsuki());

        assert_eq!(pi.len(), 241);
        assert_eq!(pi[0], Version::tsuki().as_version_byte());
        assert_eq!(&pi[169..177], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&pi[177..209], B256::repeat_byte(0x66).as_slice());
    }

    #[test]
    fn tsuki_batch_validate_reports_regression() {
        let version = Version::tsuki();
        let prev = sample_batch_info(22);
        let current = next_contiguous_batch(&prev, 21);

        let err = std::panic::catch_unwind(|| current.validate(&prev, version))
            .expect_err("v11 validation must reject regressions");

        let message = panic_message(err);
        assert!(message.contains("next_message_index must not regress"));
    }

    #[test]
    fn pre_v11_batch_validate_ignores_next_message_index_regression() {
        let version = Version::galileo_v2();
        assert_eq!(version.domain, Domain::Scroll);

        let prev = sample_batch_info(22);
        let current = next_contiguous_batch(&prev, 21);
        current.validate(&prev, version);
    }
}

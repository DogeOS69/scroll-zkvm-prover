//! Header fields authenticated by a chunk proof and carried through batch aggregation.
use alloy_primitives::B256;
use sbv_primitives::types::reth::consensus::validate_gas_limit_against_parent;

use super::chunk::ChunkInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockHeaderContext {
    pub number: u64,
    pub timestamp: u64,
    pub gas_limit: u64,
}

impl BlockHeaderContext {
    pub fn to_bytes(self) -> Vec<u8> {
        self.number
            .to_be_bytes()
            .into_iter()
            .chain(self.timestamp.to_be_bytes())
            .chain(self.gas_limit.to_be_bytes())
            .collect()
    }

    pub fn validate_parent(self, parent: Self) -> Result<(), String> {
        if parent.number.checked_add(1) != Some(self.number) {
            return Err("non-contiguous block numbers".into());
        }
        if self.timestamp < parent.timestamp {
            return Err("block timestamp moved backwards".into());
        }
        validate_gas_limit_against_parent(self.gas_limit, parent.gas_limit)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockBoundary {
    pub hash: B256,
    pub context: BlockHeaderContext,
}

impl BlockBoundary {
    pub fn to_bytes(self) -> Vec<u8> {
        self.hash
            .into_iter()
            .chain(self.context.to_bytes())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BatchBoundary {
    pub prev_blockhash: B256,
    pub first_block: BlockHeaderContext,
    pub last_block: BlockBoundary,
}

impl BatchBoundary {
    /// Inputs must come from the chunks whose proofs and PI hashes are verified by the batch guest.
    pub fn from_chunks(first: &ChunkInfo, last: &ChunkInfo) -> Self {
        let first_context = first.block_ctxs.first().expect("nonempty first chunk");
        let last_context = last.block_ctxs.last().expect("nonempty last chunk");
        let last_offset = u64::try_from(last.block_ctxs.len() - 1).expect("block count fits u64");
        Self {
            prev_blockhash: first.prev_blockhash,
            first_block: BlockHeaderContext {
                number: first.initial_block_number,
                timestamp: first_context.timestamp,
                gas_limit: first_context.gas_limit,
            },
            last_block: BlockBoundary {
                hash: last.post_blockhash,
                context: BlockHeaderContext {
                    number: last
                        .initial_block_number
                        .checked_add(last_offset)
                        .expect("block number overflow"),
                    timestamp: last_context.timestamp,
                    gas_limit: last_context.gas_limit,
                },
            },
        }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        self.prev_blockhash
            .into_iter()
            .chain(self.first_block.to_bytes())
            .chain(self.last_block.to_bytes())
            .collect()
    }

    pub fn validate_parent(self, parent: BlockBoundary) -> Result<(), String> {
        if self.prev_blockhash != parent.hash {
            return Err("non-contiguous block hashes".into());
        }
        self.first_block.validate_parent(parent.context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary() -> BatchBoundary {
        BatchBoundary {
            prev_blockhash: B256::repeat_byte(1),
            first_block: BlockHeaderContext {
                number: 11,
                timestamp: 100,
                gas_limit: 20_000_000,
            },
            last_block: BlockBoundary {
                hash: B256::repeat_byte(2),
                context: BlockHeaderContext {
                    number: 20,
                    timestamp: 110,
                    gas_limit: 20_000_000,
                },
            },
        }
    }

    #[test]
    fn boundary_rejects_each_invalid_parent_relationship() {
        let child = boundary();
        let parent = BlockBoundary {
            hash: child.prev_blockhash,
            context: BlockHeaderContext {
                number: 10,
                timestamp: 100,
                gas_limit: 20_000_000,
            },
        };
        assert!(child.validate_parent(parent).is_ok());
        let mut bad = child;
        bad.prev_blockhash = B256::ZERO;
        assert!(bad.validate_parent(parent).is_err());
        let mut bad = child;
        bad.first_block.number += 1;
        assert!(bad.validate_parent(parent).is_err());
        let mut bad = child;
        bad.first_block.timestamp -= 1;
        assert!(bad.validate_parent(parent).is_err());
        for gas in [20_000_000 - 19_532, 20_000_000 + 19_532, 4_999] {
            let mut bad = child;
            bad.first_block.gas_limit = gas;
            assert!(bad.validate_parent(parent).is_err());
        }
        let mut overflow = parent;
        overflow.context.number = u64::MAX;
        let mut wrapped = child;
        wrapped.first_block.number = 0;
        assert!(wrapped.validate_parent(overflow).is_err());
        for gas in [20_000_000 - 19_531, 20_000_000 + 19_531] {
            let mut valid = child;
            valid.first_block.gas_limit = gas;
            assert!(valid.validate_parent(parent).is_ok());
        }
    }

    #[test]
    fn encoding_commits_every_boundary_field() {
        let original = boundary();
        type Mutation = fn(&mut BatchBoundary);
        let mutations: &[Mutation] = &[
            |b| b.prev_blockhash = B256::ZERO,
            |b| b.first_block.number += 1,
            |b| b.first_block.timestamp += 1,
            |b| b.first_block.gas_limit += 1,
            |b| b.last_block.hash = B256::ZERO,
            |b| b.last_block.context.number += 1,
            |b| b.last_block.context.timestamp += 1,
            |b| b.last_block.context.gas_limit += 1,
        ];
        assert_eq!(original.to_bytes().len(), 112);
        for mutate in mutations {
            let mut changed = original;
            mutate(&mut changed);
            assert_ne!(original.to_bytes(), changed.to_bytes());
        }
    }
}

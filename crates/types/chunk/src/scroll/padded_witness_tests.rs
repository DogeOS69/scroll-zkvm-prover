use super::{ChunkWitness, ValidiumInputs, padded_witness::*};
use sbv_core::BlockWitness;
use sbv_primitives::{B256, Bytes, U256, types::Header};
use sbv_trie::PaddedCodeError;
use types_base::fork_name::ForkName;

fn witness() -> ChunkWitness {
    let blocks = (0..3)
        .map(|index| BlockWitness {
            chain_id: 6_281_971,
            header: Header {
                number: index + 1,
                gas_limit: 30_000_000,
                gas_used: 21_000 + index,
                ..Header::default()
            },
            prev_state_root: B256::repeat_byte(index as u8 + 1),
            transactions: Vec::new(),
            withdrawals: Some(Default::default()),
            states: if index == 2 {
                Vec::new()
            } else {
                [0, 1, 2, 3, 4, 7, 8]
                    .map(|len| Bytes::from(vec![0xa0 + index as u8; len]))
                    .to_vec()
            },
            codes: if index == 2 {
                Vec::new()
            } else {
                [0, 1, 2, 3, 4, 7, 31, 32, 33, 34]
                    .map(|len| {
                        let mut code = vec![0x60; len];
                        if let Some(last) = code.last_mut() {
                            *last = 0;
                        }
                        Bytes::from(code)
                    })
                    .to_vec()
            },
        })
        .collect();
    ChunkWitness {
        version: 11,
        blocks,
        prev_msg_queue_hash: B256::repeat_byte(4),
        fork_name: ForkName::Tsuki,
        compression_infos: vec![vec![(U256::from(3), 4)], Vec::new(), Vec::new()],
        validium: Some(ValidiumInputs {
            validium_txs: vec![Vec::new(), Vec::new(), Vec::new()],
            secret_key: vec![5; 32].into_boxed_slice(),
        }),
    }
}

fn logical_encoding(witness: &ChunkWitness) -> Vec<u8> {
    bincode::serde::encode_to_vec(witness, bincode::config::standard()).unwrap()
}

fn encode(witness: &ChunkWitness) -> Vec<u8> {
    let mut bytes = Vec::new();
    PaddedChunkWitness::encode_to_writer(witness, &mut bytes).unwrap();
    bytes
}

fn replace_metadata(encoded: &[u8], metadata: &[u8]) -> Vec<u8> {
    let original_len = u64::from_le_bytes(encoded[8..16].try_into().unwrap()) as usize;
    let records_start = (16 + original_len).div_ceil(4) * 4;
    let mut result = encoded[..8].to_vec();
    result.extend_from_slice(&(metadata.len() as u64).to_le_bytes());
    result.extend_from_slice(metadata);
    result.resize(result.len().div_ceil(4) * 4, 0);
    result.extend_from_slice(&encoded[records_start..]);
    result
}

fn static_input(bytes: &[u8], misalign: bool) -> &'static [u8] {
    let storage = vec![0u8; bytes.len() + 4].leak();
    let offset = (storage.as_ptr() as usize).wrapping_neg() % 4 + usize::from(misalign);
    storage[offset..offset + bytes.len()].copy_from_slice(bytes);
    &storage[offset..offset + bytes.len()]
}

fn offset_in(input: &[u8], bytes: &[u8]) -> usize {
    let start = input.as_ptr() as usize;
    let bytes_start = bytes.as_ptr() as usize;
    assert!(bytes_start >= start);
    let offset = bytes_start - start;
    assert!(offset + bytes.len() <= input.len());
    offset
}

#[test]
fn static_roundtrip_reuses_aligned_payloads_and_preserves_the_logical_witness() {
    let expected = witness();
    let input = static_input(&encode(&expected), false);
    assert!(PaddedChunkWitness::is_encoded(input));
    let decoded = PaddedChunkWitness::deserialize_from_static(input).unwrap();
    assert_eq!(
        logical_encoding(decoded.witness()),
        logical_encoding(&expected)
    );
    assert_eq!(encode(decoded.witness()), input);

    for block in &decoded.witness().blocks {
        for bytes in block
            .codes
            .iter()
            .chain(&block.states)
            .filter(|bytes| !bytes.is_empty())
        {
            let offset = offset_in(input, bytes);
            assert_eq!(bytes.as_ptr() as usize % 4, 0);
            assert_eq!(bytes.as_ref(), &input[offset..offset + bytes.len()]);
            assert_eq!(bytes.clone().as_ptr(), bytes.as_ptr());
        }
    }
    let logical_codes: Vec<_> = decoded
        .witness()
        .blocks
        .iter()
        .flat_map(|block| &block.codes)
        .collect();
    assert_eq!(decoded.padded_codes().len(), logical_codes.len());
    for (padded, logical) in decoded.padded_codes().iter().zip(logical_codes) {
        let bytes = padded.padded_bytes();
        offset_in(input, bytes);
        assert_eq!(bytes.as_ptr() as usize % 4, 0);
        assert_eq!(padded.original_bytes(), *logical);
        assert_eq!(padded.original_len(), logical.len());
        assert_eq!(bytes.len(), logical.len() + 33);
        assert!(bytes[logical.len()..].iter().all(|byte| *byte == 0));
        if !logical.is_empty() {
            assert_eq!(bytes.as_ptr(), logical.as_ptr());
        }
    }
}

#[test]
fn host_decode_owns_payloads_after_source_mutation_and_drop() {
    let expected = witness();
    let mut input = encode(&expected);
    let decoded = PaddedChunkWitness::deserialize_from_slice(&input).unwrap();
    let source_start = input.as_ptr() as usize;
    let source_end = source_start + input.len();
    for padded in decoded.padded_codes() {
        let start = padded.padded_bytes().as_ptr() as usize;
        let end = start + padded.padded_bytes().len();
        assert!(end <= source_start || start >= source_end);
    }
    input.fill(0xff);
    drop(input);
    assert_eq!(
        logical_encoding(decoded.witness()),
        logical_encoding(&expected)
    );
    assert_eq!(encode(decoded.witness()), encode(&expected));
    let retained = decoded.padded_codes()[1].clone();
    drop(decoded);
    assert_eq!(retained.original_byte_slice(), &[0]);
    assert!(retained.padded_bytes().iter().all(|byte| *byte == 0));
}

#[test]
fn roundtrips_empty_chunks_blocks_and_entries() {
    let mut expected = witness();
    expected.blocks.clear();
    expected.compression_infos.clear();
    expected.validium = None;
    let decoded = PaddedChunkWitness::deserialize_from_slice(&encode(&expected)).unwrap();
    assert_eq!(
        logical_encoding(decoded.witness()),
        logical_encoding(&expected)
    );
    assert!(decoded.padded_codes().is_empty());

    expected.blocks.push(witness().blocks.pop().unwrap());
    expected.compression_infos.push(Vec::new());
    for has_empty_entries in [false, true] {
        if has_empty_entries {
            expected.blocks[0].codes.push(Bytes::new());
            expected.blocks[0].states.push(Bytes::new());
        }
        let decoded =
            PaddedChunkWitness::deserialize_from_static(static_input(&encode(&expected), false))
                .unwrap();
        assert_eq!(
            logical_encoding(decoded.witness()),
            logical_encoding(&expected)
        );
        assert_eq!(decoded.padded_codes().len(), usize::from(has_empty_entries));
    }
}

#[test]
fn roundtrips_eip7702_designations_without_exposing_padding_as_code() {
    let mut expected = witness();
    let designation = Bytes::from([&[0xef, 0x01, 0x00][..], &[0x11; 20]].concat());
    expected.blocks[2].codes.push(designation.clone());
    let input = static_input(&encode(&expected), false);
    let decoded = PaddedChunkWitness::deserialize_from_static(input).unwrap();
    assert_eq!(
        logical_encoding(decoded.witness()),
        logical_encoding(&expected)
    );
    let padded = decoded.padded_codes().last().unwrap();
    assert_eq!(padded.original_bytes(), designation);
    assert_eq!(padded.original_len(), 23);
    assert_eq!(padded.padded_bytes().len(), 56);
    offset_in(input, padded.padded_bytes());
}

#[test]
fn rejects_payload_arrays_and_trailing_bytes_in_metadata() {
    let expected = witness();
    let encoded = encode(&expected);
    let mut metadata = expected;
    for block in &mut metadata.blocks {
        block.states.clear();
        block.codes.clear();
    }
    let mut trailing = logical_encoding(&metadata);
    trailing.push(0);
    assert!(matches!(
        PaddedChunkWitness::deserialize_from_slice(&replace_metadata(&encoded, &trailing)),
        Err(PaddedWitnessError::Invalid("trailing metadata bytes"))
    ));
    for insert_code in [false, true] {
        let mut invalid = metadata.clone();
        let payload = if insert_code {
            &mut invalid.blocks[0].codes
        } else {
            &mut invalid.blocks[0].states
        };
        payload.push(Bytes::from(vec![0]));
        assert!(matches!(
            PaddedChunkWitness::deserialize_from_slice(&replace_metadata(
                &encoded,
                &logical_encoding(&invalid)
            )),
            Err(PaddedWitnessError::Invalid(
                "metadata contains code or state"
            ))
        ));
    }
}

#[test]
fn rejects_every_truncated_prefix_and_trailing_bytes() {
    let encoded = encode(&witness());
    let input = static_input(&encoded, false);
    for length in 0..input.len() {
        assert!(
            PaddedChunkWitness::deserialize_from_static(&input[..length]).is_err(),
            "accepted truncated prefix of length {length}"
        );
    }
    for trailing in [vec![0], vec![0; 4], vec![0xff; 4]] {
        let mut invalid = encoded.clone();
        invalid.extend(trailing);
        assert!(matches!(
            PaddedChunkWitness::deserialize_from_slice(&invalid),
            Err(PaddedWitnessError::Invalid("trailing input bytes"))
        ));
    }
}

#[test]
fn rejects_nonzero_execution_and_record_padding() {
    let input = static_input(&encode(&witness()), false);
    let decoded = PaddedChunkWitness::deserialize_from_static(input).unwrap();
    for padded in decoded.padded_codes() {
        let offset = offset_in(input, padded.padded_bytes());
        for padding_byte in [
            offset + padded.original_len(),
            offset + padded.padded_bytes().len() - 1,
        ] {
            let mut invalid = input.to_vec();
            invalid[padding_byte] = 1;
            assert!(matches!(
                PaddedChunkWitness::deserialize_from_slice(&invalid),
                Err(PaddedWitnessError::Code(PaddedCodeError::NonzeroPadding))
            ));
        }
    }

    let mut padding_ranges = Vec::new();
    for state in decoded
        .witness()
        .blocks
        .iter()
        .flat_map(|block| &block.states)
        .filter(|state| !state.is_empty())
    {
        let end = offset_in(input, state) + state.len();
        padding_ranges.push(end..end.div_ceil(4) * 4);
    }
    for padded in decoded.padded_codes() {
        let end = offset_in(input, padded.padded_bytes()) + padded.padded_bytes().len();
        padding_ranges.push(end..end.div_ceil(4) * 4);
    }
    assert!(padding_ranges.iter().any(|range| !range.is_empty()));
    for padding_byte in padding_ranges.into_iter().flatten() {
        let mut invalid = input.to_vec();
        invalid[padding_byte] = 1;
        assert!(matches!(
            PaddedChunkWitness::deserialize_from_slice(&invalid),
            Err(PaddedWitnessError::Invalid("nonzero alignment padding"))
        ));
    }
}

#[test]
fn rejects_nonzero_metadata_alignment_padding() {
    let mut expected = witness();
    for secret_len in 1..=4 {
        expected.validium.as_mut().unwrap().secret_key = vec![5; secret_len].into_boxed_slice();
        let mut encoded = encode(&expected);
        let metadata_len = u64::from_le_bytes(encoded[8..16].try_into().unwrap()) as usize;
        let metadata_end = 16 + metadata_len;
        if !metadata_end.is_multiple_of(4) {
            encoded[metadata_end] = 1;
            assert!(matches!(
                PaddedChunkWitness::deserialize_from_slice(&encoded),
                Err(PaddedWitnessError::Invalid("nonzero alignment padding"))
            ));
            return;
        }
    }
    panic!("fixture did not exercise metadata alignment padding");
}

#[test]
fn rejects_unsupported_version_and_misaligned_static_input() {
    let encoded = encode(&witness());
    let mut invalid = encoded.clone();
    invalid[7] ^= 1;
    assert!(!PaddedChunkWitness::is_encoded(&invalid));
    assert!(matches!(
        PaddedChunkWitness::deserialize_from_slice(&invalid),
        Err(PaddedWitnessError::Invalid("unsupported input version"))
    ));

    let misaligned = static_input(&encoded, true);
    assert_ne!(misaligned.as_ptr() as usize % 4, 0);
    assert!(PaddedChunkWitness::is_encoded(misaligned));
    assert!(matches!(
        PaddedChunkWitness::deserialize_from_static(misaligned),
        Err(PaddedWitnessError::Invalid("input address is not aligned"))
    ));
    // Host input is copied into an aligned allocation before decoding.
    let copied = PaddedChunkWitness::deserialize_from_slice(misaligned).unwrap();
    assert_eq!(
        logical_encoding(copied.witness()),
        logical_encoding(&witness())
    );
}

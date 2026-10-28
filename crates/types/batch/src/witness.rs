use halo2curves_axiom::CurveAffine;
use types_base::{
    aggregation::{AggregationInput, ProofCarryingWitness},
    public_inputs::{
        ForkName,
        scroll::{batch::BatchInfo, chunk::ChunkInfo},
    },
    version::{Codec, Domain, Version},
};

use crate::{
    blob_consistency::{ToIntrinsic, is_in_g1_subgroup},
    builder::{
        BatchInfoBuilder, BatchInfoBuilderV6, BatchInfoBuilderV7, BuilderArgsV6, BuilderArgsV7,
        validium::{ValidiumBatchInfoBuilder, ValidiumBuilderArgs},
    },
    header::ReferenceHeader,
};

/// Simply rewrap byte48 to avoid unnecessary dep
pub type Bytes48 = [u8; 48];
mod array48 {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(array: &[u8; 48], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(array)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 48], D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: &[u8] = serde::Deserialize::deserialize(deserializer)?;
        bytes.try_into().map_err(|_| {
            let msg = format!("expected a byte array of length 48 but got {}", bytes.len());
            serde::de::Error::custom(msg)
        })
    }
}

/// Witness required by applying point evaluation
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PointEvalWitness {
    #[serde(with = "array48")]
    pub kzg_commitment_x: Bytes48,
    #[serde(with = "array48")]
    pub kzg_commitment_y: Bytes48,
    #[serde(with = "array48")]
    pub kzg_proof_x: Bytes48,
    #[serde(with = "array48")]
    pub kzg_proof_y: Bytes48,
}

pub fn build_point_eval_witness(kzg_commitment: Bytes48, kzg_proof: Bytes48) -> PointEvalWitness {
    use halo2curves_axiom::bls12_381::G1Affine;
    let kzg_commitment = G1Affine::from_compressed_be(&kzg_commitment).expect("invalid");
    let kzg_proof = G1Affine::from_compressed_be(&kzg_proof).expect("invalid");
    PointEvalWitness {
        kzg_commitment_x: kzg_commitment.x.to_bytes_be(),
        kzg_commitment_y: kzg_commitment.y.to_bytes_be(),
        kzg_proof_x: kzg_proof.x.to_bytes_be(),
        kzg_proof_y: kzg_proof.y.to_bytes_be(),
    }
}

#[allow(dead_code)]
pub fn build_intrinsic_point(
    x: Bytes48,
    y: Bytes48,
) -> Option<openvm_pairing::bls12_381::G1Affine> {
    use openvm_algebra_guest::IntMod;
    use openvm_ecc_guest::weierstrass::WeierstrassPoint;
    use openvm_pairing::bls12_381::{Fp, G1Affine};
    let x = Fp::from_be_bytes(&x)?;
    let y = Fp::from_be_bytes(&y)?;
    let p = unsafe { G1Affine::from_xy(x, y) }?;
    is_in_g1_subgroup(&p).then_some(p)
}

pub fn build_point(x: Bytes48, y: Bytes48) -> Option<halo2curves_axiom::bls12_381::G1Affine> {
    use halo2curves_axiom::bls12_381::{Fq, G1Affine};
    let x = Fq::from_bytes_be(&x).into_option()?;
    let y = Fq::from_bytes_be(&y).into_option()?;
    let p = G1Affine::from_xy(x, y).into_option()?;

    let is_in_g1_subgroup = is_in_g1_subgroup(&p.to_intrinsic());

    #[cfg(feature = "host")]
    assert_eq!(is_in_g1_subgroup, bool::from(p.is_torsion_free()));

    is_in_g1_subgroup.then_some(p)
}

/// Witness to the batch circuit.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BatchWitness {
    /// The version byte as per [version][types_base::version].
    pub version: u8,
    /// Flattened root proofs from all chunks in the batch.
    pub chunk_proofs: Vec<AggregationInput>,
    /// Chunk infos.
    pub chunk_infos: Vec<ChunkInfo>,
    /// Blob bytes.
    pub blob_bytes: Vec<u8>,
    /// Witness for point evaluation.
    ///
    /// Optional field as some domains (for eg. Validium) may not utilise EIP-4844 for DA,
    /// in case of which there is no point-eval witness.
    pub point_eval_witness: Option<PointEvalWitness>,
    /// Header for reference.
    pub reference_header: ReferenceHeader,
    /// The code version specify the chain spec
    pub fork_name: ForkName,
}

impl ProofCarryingWitness for BatchWitness {
    fn get_proofs(&self) -> Vec<AggregationInput> {
        self.chunk_proofs.clone()
    }
}

impl From<&BatchWitness> for BatchInfo {
    fn from(witness: &BatchWitness) -> Self {
        let chunk_infos = witness.chunk_infos.to_vec();

        // The reference header variant is derived from the committed version byte rather than
        // taken from the witness, so a header of the wrong codec or domain is rejected.
        let version = Version::from(witness.version);
        match (version.domain, version.codec, &witness.reference_header) {
            (Domain::Scroll, Codec::V6, ReferenceHeader::V6(header)) => {
                let args = BuilderArgsV6 {
                    header: *header,
                    chunk_infos,
                    blob_bytes: witness.blob_bytes.to_vec(),
                    point_eval_witness: None,
                };
                BatchInfoBuilderV6::build(witness.version, args)
            }
            (Domain::Scroll, Codec::V7, ReferenceHeader::V7_V8_V9_V10(header)) => {
                let point_eval_witness = witness
                    .point_eval_witness
                    .as_ref()
                    .expect("point_eval_witness missing for header::v7");
                let args = BuilderArgsV7 {
                    header: *header,
                    chunk_infos,
                    blob_bytes: witness.blob_bytes.to_vec(),
                    point_eval_witness: Some(point_eval_witness.clone()),
                };
                BatchInfoBuilderV7::build(witness.version, args)
            }
            (_, _, ReferenceHeader::V8(_)) => {
                unreachable!("Unexpected ReferenceHeader::V8 from 0.7.0 onwards");
            }
            (Domain::Validium, _, ReferenceHeader::Validium(header)) => {
                ValidiumBatchInfoBuilder::build(ValidiumBuilderArgs::new(
                    witness.version,
                    *header,
                    chunk_infos,
                ))
            }
            (domain, codec, _) => panic!(
                "reference header does not match version: domain={domain:?}, codec={codec:?}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use types_base::version::{VALIDIUM_V1, Version};

    use super::*;
    use crate::header::{
        v6::BatchHeaderV6,
        v7::BatchHeaderV7,
        validium::{BatchHeaderValidium, BatchHeaderValidiumV1},
    };

    const TSUKI: u8 = Version::tsuki().as_version_byte();
    const EUCLID_V1: u8 = Version::euclid_v1().as_version_byte();

    fn witness(version: u8, reference_header: ReferenceHeader) -> BatchWitness {
        BatchWitness {
            version,
            chunk_proofs: vec![],
            chunk_infos: vec![],
            blob_bytes: vec![],
            point_eval_witness: None,
            reference_header,
            fork_name: ForkName::Tsuki,
        }
    }

    fn v6_header(version: u8) -> ReferenceHeader {
        ReferenceHeader::V6(BatchHeaderV6 {
            version,
            ..Default::default()
        })
    }

    fn v7_header(version: u8) -> ReferenceHeader {
        ReferenceHeader::V7_V8_V9_V10(BatchHeaderV7 {
            version,
            ..Default::default()
        })
    }

    fn validium_header(version: u8) -> ReferenceHeader {
        ReferenceHeader::Validium(BatchHeaderValidium::V1(BatchHeaderValidiumV1 {
            version,
            ..Default::default()
        }))
    }

    #[test]
    #[should_panic(expected = "reference header does not match version")]
    fn tsuki_version_rejects_v6_header() {
        let _ = BatchInfo::from(&witness(TSUKI, v6_header(TSUKI)));
    }

    #[test]
    #[should_panic(expected = "reference header does not match version")]
    fn tsuki_version_rejects_validium_header() {
        let _ = BatchInfo::from(&witness(TSUKI, validium_header(TSUKI)));
    }

    #[test]
    #[should_panic(expected = "Unexpected ReferenceHeader::V8")]
    fn tsuki_version_rejects_v8_header() {
        let header = v7_header(TSUKI).into_v8_feynman();
        let _ = BatchInfo::from(&witness(TSUKI, header));
    }

    #[test]
    #[should_panic(expected = "reference header does not match version")]
    fn validium_version_rejects_v7_header() {
        let _ = BatchInfo::from(&witness(VALIDIUM_V1, v7_header(TSUKI)));
    }

    #[test]
    #[should_panic(expected = "reference header does not match version")]
    fn v6_version_rejects_v7_header() {
        let _ = BatchInfo::from(&witness(EUCLID_V1, v7_header(EUCLID_V1)));
    }

    /// The V7 arm is the only one that requires a point-evaluation witness, so reaching that
    /// check shows the Tsuki version selected the V7 header and passed it to the V7 builder.
    #[test]
    #[should_panic(expected = "point_eval_witness missing for header::v7")]
    fn tsuki_version_accepts_v7_header() {
        let _ = BatchInfo::from(&witness(TSUKI, v7_header(TSUKI)));
    }
}

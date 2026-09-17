use openvm::init;
use scroll_zkvm_types_chunk::scroll::{ChunkWitness, PaddedChunkWitness};
use scroll_zkvm_types_circuit::{
    Circuit,
    io::read_witnesses,
    public_inputs::{
        Version,
        scroll::chunk::{ChunkInfo, VersionedChunkInfo},
    },
};

#[allow(unused_imports, clippy::single_component_path_imports)]
use {
    openvm::platform as openvm_platform,
    openvm_algebra_guest::IntMod,
    openvm_bigint_guest, // trigger extern u256 (this may be unneeded)
    openvm_k256::Secp256k1Point,
    openvm_keccak256_guest, // trigger extern native-keccak256
    openvm_p256::P256Point,
    openvm_pairing::bn254::Bn254G1Affine,
};

init!();

pub struct ChunkCircuit;

pub enum Witness {
    Standard(ChunkWitness),
    Padded(PaddedChunkWitness),
}

impl Witness {
    pub fn witness(&self) -> &ChunkWitness {
        match self {
            Self::Standard(witness) => witness,
            Self::Padded(witness) => witness.witness(),
        }
    }
}

impl ChunkCircuit {
    /// Decode input retained for the lifetime of the guest without copying block bytes.
    #[cfg(target_os = "zkvm")]
    pub fn deserialize_witness_from_static(witness_bytes: &'static [u8]) -> Witness {
        if PaddedChunkWitness::is_encoded(witness_bytes) {
            return Witness::Padded(
                PaddedChunkWitness::deserialize_from_static(witness_bytes)
                    .expect("ChunkCircuit: deserialisation of padded static witness failed"),
            );
        }
        let mut decoder = bincode::serde::BorrowedSerdeDecoder::from_slice(
            witness_bytes,
            bincode::config::standard(),
            (),
        );
        Witness::Standard(
            ChunkWitness::deserialize_from_static(decoder.as_deserializer())
                .expect("ChunkCircuit: deserialisation of static witness bytes failed"),
        )
    }
}

impl Circuit for ChunkCircuit {
    type Witness = Witness;
    type PublicInputs = VersionedChunkInfo;

    fn read_witness_bytes() -> Vec<u8> {
        read_witnesses()
    }

    fn deserialize_witness(witness_bytes: &[u8]) -> Self::Witness {
        if PaddedChunkWitness::is_encoded(witness_bytes) {
            return Witness::Padded(
                PaddedChunkWitness::deserialize_from_slice(witness_bytes)
                    .expect("ChunkCircuit: deserialisation of padded witness failed"),
            );
        }
        let config = bincode::config::standard();
        let (witness, _): (ChunkWitness, _) =
            bincode::serde::decode_from_slice(witness_bytes, config)
                .expect("ChunkCircuit: deserialisation of witness bytes failed");
        Witness::Standard(witness)
    }

    fn validate(witness: Self::Witness) -> Self::PublicInputs {
        let version = Version::from(witness.witness().version);
        assert_eq!(version.fork, witness.witness().fork_name);

        let chunk_info = match witness {
            Witness::Standard(witness) => ChunkInfo::try_from(witness),
            Witness::Padded(witness) => witness.into_chunk_info(),
        }
        .expect("failed to execute chunk");
        (chunk_info, version)
    }
}

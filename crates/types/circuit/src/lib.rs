pub mod io;
pub use io::read_witnesses;

use alloy_primitives::B256;
use itertools::Itertools;
use public_inputs::PublicInputs;
use scroll_zkvm_types_base as types_base;
pub use types_base::{
    aggregation::{AggregationInput, ProgramCommitment, ProofCarryingWitness},
    public_inputs, utils,
};

/// Reveal the public-input values as openvm public values.
pub fn reveal_pi_hash(pi_hash: B256) {
    openvm::io::println(format!("pi_hash = {pi_hash:?}"));
    openvm::io::reveal_bytes32(*pi_hash);
}

/// Circuit defines the higher-level behaviour to be observed by a [`openvm`] guest program.
pub trait Circuit {
    /// The witness provided to the circuit.
    type Witness;

    /// The public-input values for the circuit.
    type PublicInputs: PublicInputs;

    /// Reads bytes from openvm StdIn.
    fn read_witness_bytes() -> Vec<u8>;

    /// Deserialize raw bytes into the circuit's witness type.
    fn deserialize_witness(witness_bytes: &[u8]) -> Self::Witness;

    /// Validate the witness to produce the circuit's public inputs.
    fn validate(witness: Self::Witness) -> Self::PublicInputs;

    /// Reveal the public inputs.
    fn reveal_pi(pi: &Self::PublicInputs) {
        reveal_pi_hash(pi.pi_hash())
    }
}

const NUM_PUBLIC_VALUES: usize = 32;

/// Circuit that additional aggregates proofs from other [`Circuits`][Circuit].
pub trait AggCircuit: Circuit
where
    Self::Witness: ProofCarryingWitness,
{
    /// The public-input values of the proofs being aggregated.
    type AggregatedPublicInputs: PublicInputs;

    /// Check if the commitment in proof is valid (from program(s)
    /// we have expected)
    fn verify_commitments(commitment: &ProgramCommitment);

    /// Verify the proofs being aggregated.
    ///
    /// Also returns the root proofs being aggregated.
    fn verify_proofs(witness: &Self::Witness) -> Vec<AggregationInput> {
        let proofs = witness.get_proofs();

        for proof in proofs.iter() {
            Self::verify_commitments(&proof.commitment);
            verify_proof(proof);
        }

        proofs
    }

    /// Derive the public-input values of the proofs being aggregated from the witness.
    fn aggregated_public_inputs(witness: &Self::Witness) -> Vec<Self::AggregatedPublicInputs>;

    /// Derive the public-input hashes of the aggregated proofs from the proofs itself.
    fn aggregated_pi_hashes(proofs: &[AggregationInput]) -> Vec<B256>;

    /// Validate that the public-input values of the aggregated proofs are well-formed.
    ///
    /// - That the public-inputs of contiguous chunks/batches are valid
    /// - That the public-input values in fact hash to the pi_hash values from the root proofs.
    fn validate_aggregated_pi(agg_pis: &[Self::AggregatedPublicInputs], agg_pi_hashes: &[B256]) {
        // There should be at least a single proof being aggregated.
        assert!(!agg_pis.is_empty(), "at least 1 pi to aggregate");

        // Validation for the contiguous public-input values.
        for w in agg_pis.windows(2) {
            w[1].validate(&w[0]);
        }

        // Validation for public-input values hash being the pi_hash from root proof.
        for (agg_pi, &agg_pi_hash) in agg_pis.iter().zip_eq(agg_pi_hashes.iter()) {
            assert_eq!(
                agg_pi.pi_hash(),
                agg_pi_hash,
                "pi hash mismatch between proofs and witness computed"
            );
        }
    }
}

/// Verify a root proof. The real "proof" will be loaded from StdIn.
fn verify_proof(proof: &AggregationInput) {
    let commitment = &proof.commitment;
    let public_inputs = proof.public_values.as_slice();

    // Sanity check for the number of public-input values.
    assert_eq!(public_inputs.len(), NUM_PUBLIC_VALUES);

    #[cfg(all(target_os = "zkvm", target_arch = "riscv32"))]
    {
        use openvm::io::read;
        use openvm_verify_stark_guest::{ProofOutput, verify_stark};

        fn u32_words_to_commit(words: &[u32; 8]) -> [u8; 32] {
            let mut bytes = [0u8; 32];
            for (dst, word) in bytes.chunks_exact_mut(4).zip(words.iter()) {
                dst.copy_from_slice(&word.to_le_bytes());
            }
            bytes
        }

        fn u32_words_to_bytes(words: &[u32]) -> Vec<u8> {
            let mut bytes = Vec::with_capacity(words.len() * 4);
            for word in words {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
            bytes
        }

        let expected = ProofOutput {
            app_exe_commit: u32_words_to_commit(&commitment.exe),
            app_vm_commit: u32_words_to_commit(&commitment.vm),
            user_public_values: u32_words_to_bytes(public_inputs),
        };

        let input_commit = proof.input_commit.unwrap_or_else(read);
        verify_stark::<0>(&input_commit, &expected);
    }
}

/// This macro is used to manually drop an expression on zkvm (non x86/aarch64 targets).
#[macro_export]
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
macro_rules! manually_drop_on_zkvm {
    ($e:expr) => {
        std::mem::ManuallyDrop::new($e)
    };
}

/// This macro is used to manually drop an expression on zkvm (non x86/aarch64 targets).
#[macro_export]
#[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
macro_rules! manually_drop_on_zkvm {
    ($e:expr) => {
        $e
    };
}

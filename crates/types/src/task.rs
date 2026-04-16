use crate::proof::StarkProof;
use openvm_sdk::{DeferralInput, openvm_circuit::arch::deferral::DeferralState};
use serde::{Deserialize, Serialize};

/// Host-side metadata needed to replay recursive child proofs through the deferral channel.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AggregatedProofMetadata {
    /// Proof-instance input commit consumed by the verify-stark guest.
    pub input_commit: [u8; 32],
    /// Deferral state that must be attached to `StdIn`.
    pub deferral_state: DeferralState,
    /// Encoded child proof input passed separately to the prover.
    pub deferral_input: DeferralInput,
}

/// Universal task for zkvm-prover, with encoded bytes which can be used
/// as stdin inputs for the app and id data for distinguish
#[derive(Clone, Serialize, Deserialize)]
pub struct ProvingTask {
    /// seralized witness which should be written into stdin first
    pub serialized_witness: Vec<Vec<u8>>,
    /// aggregated proof carried by babybear fields, should be written into stdin
    /// followed `serialized_witness`
    pub aggregated_proofs: Vec<StarkProof>,
    /// Per-child deferral metadata feeding the recursive-proof side-channel.
    #[serde(default)]
    pub aggregated_proof_metadata: Vec<AggregatedProofMetadata>,
    /// Fork name specify
    pub fork_name: String,
    /// The vk of app which is expcted to prove this task
    pub vk: Vec<u8>,
    /// An identifier assigned by coordinator, it should be kept identify for the
    /// same task (for example, using chunk, batch and bundle hashes)
    pub identifier: String,
}

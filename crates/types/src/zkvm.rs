use std::path::Path;

use once_cell::sync::Lazy;
use openvm_sdk::keygen::AggProvingKey;
use openvm_sdk::{F, SC, Sdk, config::AggregationSystemParams};
use openvm_stark_sdk::{
    config::{MAX_APP_LOG_STACKED_HEIGHT, app_params_with_100_bits_security},
    openvm_stark_backend::{keygen::types::MultiStarkVerifyingKey, p3_field::PrimeField32},
};

use types_base::aggregation::ProgramCommitment;

/// Verification key for the STARK aggregation stage.
pub type AggVerifyingKey = MultiStarkVerifyingKey<SC>;

/// Proving key for STARK aggregation. Primarily used to aggregate
/// [continuation proofs][openvm_sdk::prover::vm::ContinuationVmProof].
pub static AGG_STARK_PROVING_KEY: Lazy<AggProvingKey> =
    Lazy::new(|| default_riscv32_sdk().agg_pk());

/// Build the default OpenVM host SDK used across build, prove, and verify tooling.
///
/// All host crates that need a fresh `Sdk` for aggregation bootstrapping should go through
/// this helper so the parameters stay in sync.
pub fn default_riscv32_sdk() -> Sdk {
    let app_params = app_params_with_100_bits_security(MAX_APP_LOG_STACKED_HEIGHT);
    let agg_params = AggregationSystemParams::default();
    Sdk::riscv32(app_params, agg_params)
}

/// Load the aggregation verifying key from `path`, falling back to the one implicit in
/// [`AGG_STARK_PROVING_KEY`] if the file is missing or unreadable.
pub fn load_agg_vk<P: AsRef<Path>>(path: P) -> AggVerifyingKey {
    openvm_sdk::fs::read_object_from_file(path.as_ref()).unwrap_or_else(|_| {
        tracing::warn!(
            "root_verifier_vk not available on disk; computing on-the-fly (may be time consuming)"
        );
        AGG_STARK_PROVING_KEY.internal_recursive.get_vk().clone()
    })
}

/// Pack a pair of BabyBear-field digests (as returned by OpenVM's app-exe / app-vm commits)
/// into a canonical `[u32; 8]` [`ProgramCommitment`].
pub fn program_commitment_from_f_digests(exe: [F; 8], vm: [F; 8]) -> ProgramCommitment {
    ProgramCommitment {
        exe: exe.map(|value| value.as_canonical_u32()),
        vm: vm.map(|value| value.as_canonical_u32()),
    }
}

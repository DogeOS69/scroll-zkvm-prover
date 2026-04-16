use openvm_sdk::{DeferralInput, StdIn};
use scroll_zkvm_types::{public_inputs::ForkName, task::ProvingTask as UniversalProvingTask};

/// Every proving task must have an identifier. The identifier will be appended to a prefix while
/// storing/reading proof to/from disc.
pub trait ProvingTask: serde::de::DeserializeOwned {
    fn identifier(&self) -> String;

    fn build_guest_input_inner(&self, stdin: &mut StdIn);

    fn build_deferral_inputs(&self) -> Vec<DeferralInput> {
        Vec::new()
    }

    fn build_guest_input(&self) -> StdIn {
        let mut stdin = StdIn::default();
        self.build_guest_input_inner(&mut stdin);
        stdin
    }

    fn fork_name(&self) -> ForkName;
}

impl ProvingTask for UniversalProvingTask {
    fn identifier(&self) -> String {
        self.identifier.clone()
    }

    fn build_guest_input_inner(&self, stdin: &mut StdIn) {
        for witness in &self.serialized_witness {
            stdin.write_bytes(witness);
        }

        if self.aggregated_proof_metadata.is_empty() {
            if !self.aggregated_proofs.is_empty() {
                panic!(
                    "legacy aggregated-proof streaming is unsupported on OpenVM beta.2; populate aggregated_proof_metadata"
                );
            }
        } else {
            assert_eq!(
                self.aggregated_proof_metadata.len(),
                self.aggregated_proofs.len(),
                "aggregated proof metadata must line up with aggregated proofs"
            );
            stdin.deferrals = self
                .aggregated_proof_metadata
                .iter()
                .map(|metadata| metadata.deferral_state.clone())
                .collect();
            for metadata in &self.aggregated_proof_metadata {
                stdin.write(&metadata.input_commit);
            }
        }
    }

    fn build_deferral_inputs(&self) -> Vec<DeferralInput> {
        self.aggregated_proof_metadata
            .iter()
            .map(|metadata| metadata.deferral_input.clone())
            .collect()
    }

    fn fork_name(&self) -> ForkName {
        ForkName::from(self.fork_name.as_str())
    }
}

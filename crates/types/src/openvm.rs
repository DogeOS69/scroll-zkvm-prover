use serde::{Deserialize, Serialize};

/// Input structure for OpenVM input json
///
/// ```json
/// {
///   "input": [ "0x...", "0x...", ... ]
/// }
/// ```
///
/// Reference: https://github.com/openvm-org/openvm/blob/7e9488992a74d49fa697359681cd2a7e768b90ef/crates/cli/src/input.rs#L82-L115
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OpenVMInput {
    input: Vec<String>,
}

impl super::ProvingTask {
    pub fn build_openvm_input(&self) -> eyre::Result<OpenVMInput> {
        if !self.aggregated_proofs.is_empty() || !self.aggregated_proof_metadata.is_empty() {
            return Err(eyre::eyre!(
                "OpenVM beta.2 recursive proofs require deferral side inputs; the Axiom/OpenVM JSON input path only supports witness-only jobs"
            ));
        }

        let input = self
            .serialized_witness
            .iter()
            .map(|w| {
                let mut buf = Vec::with_capacity(1 + w.len());
                buf.push(0x01);
                buf.extend_from_slice(w);
                format!("0x{}", hex::encode(&buf))
            })
            .collect();

        Ok(OpenVMInput { input })
    }
}

use eyre::Result;
use openvm_sdk::{Sdk, types::EvmHalo2Verifier};

pub fn generate_evm_verifier(sdk: &Sdk) -> Result<EvmHalo2Verifier> {
    Ok(sdk.generate_halo2_verifier_solidity()?)
}

mod types;
pub use types::{
    finalizeDepositERC20Call, finalizeDepositERC20EncryptedCall, relayMessageCall,
    validium::SecretKey,
};

mod execute;
pub use execute::execute;

mod witness;
pub use witness::{ChunkWitness, ValidiumInputs};

mod padded_witness;
#[cfg(test)]
mod padded_witness_tests;
pub use padded_witness::{PaddedChunkWitness, PaddedWitnessError};

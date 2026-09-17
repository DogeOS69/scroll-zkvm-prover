//! Versioned chunk input with aligned state and pre-padded contract code.

use super::ChunkWitness;
use alloy_primitives::Bytes;
use sbv_trie::PaddedCode;
use std::io::Write;
use types_base::public_inputs::scroll::chunk::ChunkInfo;

const MAGIC: &[u8; 8] = b"SBVPAD01";
const ALIGNMENT: usize = PaddedCode::ALIGNMENT;
const CODE_PADDING: usize = PaddedCode::MIN_PADDING;

/// An error in the versioned, aligned chunk input.
#[derive(Debug, thiserror::Error)]
pub enum PaddedWitnessError {
    /// Invalid framing, sizes, alignment, or padding.
    #[error("invalid padded witness: {0}")]
    Invalid(&'static str),
    /// Metadata could not be decoded.
    #[error(transparent)]
    Decode(#[from] bincode::error::DecodeError),
    /// Metadata could not be encoded.
    #[error(transparent)]
    Encode(#[from] bincode::error::EncodeError),
    /// The output could not be written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A contract-code region was invalid.
    #[error(transparent)]
    Code(#[from] sbv_trie::PaddedCodeError),
}

/// A decoded chunk and validated code regions that share their input allocation.
///
/// Original code remains in `BlockWitness.codes`; execution padding is kept separately so
/// code hashes, lengths, and copy operations retain their original meaning.
#[derive(Debug)]
pub struct PaddedChunkWitness {
    witness: ChunkWitness,
    codes: Vec<PaddedCode>,
}

impl PaddedChunkWitness {
    /// Whether input uses this explicitly versioned envelope.
    pub fn is_encoded(input: &[u8]) -> bool {
        input.starts_with(MAGIC)
    }

    /// Borrow immutable input retained for the lifetime of the guest.
    pub fn deserialize_from_static(input: &'static [u8]) -> Result<Self, PaddedWitnessError> {
        Self::decode(Bytes::from_static(input))
    }

    /// Decode host input while retaining independent ownership of its bytes.
    pub fn deserialize_from_slice(input: &[u8]) -> Result<Self, PaddedWitnessError> {
        Self::decode(Bytes::copy_from_slice(input))
    }

    /// The logical witness, excluding execution padding.
    pub fn witness(&self) -> &ChunkWitness {
        &self.witness
    }

    /// Code regions used by the verifier without allocating padded code copies.
    pub fn padded_codes(&self) -> &[PaddedCode] {
        &self.codes
    }

    /// Execute the chunk using validated, pre-padded code.
    pub fn into_chunk_info(mut self) -> Result<ChunkInfo, String> {
        // Every decoded code has a validated sidecar entry. Remove the duplicate
        // original views before execution so SparseState does not hash them again
        // through its ordinary-code fallback.
        for block in &mut self.witness.blocks {
            block.codes.clear();
        }
        super::execute::execute_with_padded_codes(self.witness, &self.codes)
    }

    /// Write the version-one padded envelope without buffering the full output.
    ///
    /// The header contains the magic and a little-endian u64 metadata length. Bincode metadata
    /// retains the existing chunk fields with empty code/state arrays. After zero alignment
    /// padding, each block has u32-counted state and code arrays. Each entry starts with its
    /// original u32 length and aligned bytes. Code additionally contains 33 zero bytes.
    /// All record boundaries are padded with zeros to four bytes.
    pub fn encode_to_writer<W: Write>(
        witness: &ChunkWitness,
        output: &mut W,
    ) -> Result<(), PaddedWitnessError> {
        let mut metadata = witness.clone();
        for block in &mut metadata.blocks {
            block.states.clear();
            block.codes.clear();
        }
        let metadata = bincode::serde::encode_to_vec(metadata, bincode::config::standard())?;
        let mut writer = Output {
            output,
            position: 0,
        };
        writer.write(MAGIC)?;
        writer.write(&(metadata.len() as u64).to_le_bytes())?;
        writer.write(&metadata)?;
        writer.align()?;
        for block in &witness.blocks {
            writer.length(block.states.len())?;
            for state in &block.states {
                writer.length(state.len())?;
                writer.write(state)?;
                writer.align()?;
            }
            writer.length(block.codes.len())?;
            for code in &block.codes {
                writer.length(code.len())?;
                writer.write(code)?;
                writer.write(&[0; CODE_PADDING])?;
                writer.align()?;
            }
        }
        Ok(())
    }

    fn decode(input: Bytes) -> Result<Self, PaddedWitnessError> {
        if !(input.as_ptr() as usize).is_multiple_of(ALIGNMENT) {
            return Err(PaddedWitnessError::Invalid("input address is not aligned"));
        }
        let mut input = Input { input, position: 0 };
        if input.take(MAGIC.len())?.as_ref() != MAGIC {
            return Err(PaddedWitnessError::Invalid("unsupported input version"));
        }
        let metadata_len = u64::from_le_bytes(input.take(8)?.as_ref().try_into().unwrap());
        let metadata_len = usize::try_from(metadata_len)
            .map_err(|_| PaddedWitnessError::Invalid("metadata length overflow"))?;
        let metadata = input.take(metadata_len)?;
        let (mut witness, consumed): (ChunkWitness, _) =
            bincode::serde::decode_from_slice(&metadata, bincode::config::standard())?;
        if consumed != metadata.len() {
            return Err(PaddedWitnessError::Invalid("trailing metadata bytes"));
        }
        input.align()?;
        let mut codes = Vec::new();
        for block in &mut witness.blocks {
            if !block.states.is_empty() || !block.codes.is_empty() {
                return Err(PaddedWitnessError::Invalid(
                    "metadata contains code or state",
                ));
            }
            for _ in 0..input.length()? {
                let len = input.length()?;
                block.states.push(input.take(len)?);
                input.align()?;
            }
            for _ in 0..input.length()? {
                let original_len = input.length()?;
                let padded_len = original_len
                    .checked_add(CODE_PADDING)
                    .ok_or(PaddedWitnessError::Invalid("code length overflow"))?;
                let code = PaddedCode::new(input.take(padded_len)?, original_len)?;
                block.codes.push(code.original_bytes());
                codes.push(code);
                input.align()?;
            }
        }
        if input.position != input.input.len() {
            return Err(PaddedWitnessError::Invalid("trailing input bytes"));
        }
        Ok(Self { witness, codes })
    }
}

struct Input {
    input: Bytes,
    position: usize,
}

impl Input {
    fn take(&mut self, len: usize) -> Result<Bytes, PaddedWitnessError> {
        let end = self
            .position
            .checked_add(len)
            .filter(|&end| end <= self.input.len())
            .ok_or(PaddedWitnessError::Invalid(
                "truncated input or length overflow",
            ))?;
        let bytes = self.input.slice(self.position..end);
        self.position = end;
        Ok(bytes)
    }

    fn length(&mut self) -> Result<usize, PaddedWitnessError> {
        let value = u32::from_le_bytes(self.take(4)?.as_ref().try_into().unwrap());
        usize::try_from(value).map_err(|_| PaddedWitnessError::Invalid("entry length overflow"))
    }

    fn align(&mut self) -> Result<(), PaddedWitnessError> {
        let padding = self.position.wrapping_neg() & (ALIGNMENT - 1);
        if self.take(padding)?.iter().any(|&byte| byte != 0) {
            return Err(PaddedWitnessError::Invalid("nonzero alignment padding"));
        }
        Ok(())
    }
}

struct Output<'a, W> {
    output: &'a mut W,
    position: usize,
}

impl<W: Write> Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PaddedWitnessError> {
        self.position = self
            .position
            .checked_add(bytes.len())
            .ok_or(PaddedWitnessError::Invalid("output length overflow"))?;
        self.output.write_all(bytes)?;
        Ok(())
    }

    fn length(&mut self, len: usize) -> Result<(), PaddedWitnessError> {
        let len =
            u32::try_from(len).map_err(|_| PaddedWitnessError::Invalid("entry length overflow"))?;
        self.write(&len.to_le_bytes())
    }

    fn align(&mut self) -> Result<(), PaddedWitnessError> {
        let padding = self.position.wrapping_neg() & (ALIGNMENT - 1);
        self.write(&[0; ALIGNMENT][..padding])
    }
}

//! Prepare a retained BlockWitness as padded OpenVM CLI input, entirely offline.
//!
//! Run with `--features scroll,host --example prepare_padded_input -- SOURCE.json
//! OUTPUT.json [--extra-code-corpus CORPUS.bin]`. The optional corpus is a flat
//! sequence of 24 KiB code blobs. Appending unused code creates a local memory
//! stress input; it does not create a new Engine witness or alter the block.

#[cfg(not(all(feature = "scroll", feature = "host")))]
fn main() {
    eprintln!("prepare_padded_input requires --features scroll,host");
    std::process::exit(2);
}

#[cfg(all(feature = "scroll", feature = "host"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    host::run()
}

#[cfg(all(feature = "scroll", feature = "host"))]
mod host {
    use sbv_core::BlockWitness;
    use sbv_primitives::{B256, Bytes};
    use scroll_zkvm_types_chunk::scroll::{ChunkWitness, PaddedChunkWitness};
    use std::{
        fs::File,
        io::{self, BufReader, BufWriter, Read, Write},
        path::{Path, PathBuf},
    };
    use types_base::version::Version;

    const CODE_SIZE: usize = 24 * 1024;
    const USAGE: &str = "Usage: prepare_padded_input SOURCE.json OUTPUT.json \
        [--extra-code-corpus CORPUS.bin]";

    struct HexWriter<W> {
        inner: W,
        encoded_bytes: u64,
    }

    impl<W: Write> Write for HexWriter<W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut buffer = [0u8; 8192];
            for chunk in bytes.chunks(buffer.len() / 2) {
                for (i, byte) in chunk.iter().enumerate() {
                    buffer[i * 2] = HEX[(byte >> 4) as usize];
                    buffer[i * 2 + 1] = HEX[(byte & 15) as usize];
                }
                self.inner.write_all(&buffer[..chunk.len() * 2])?;
            }
            self.encoded_bytes += bytes.len() as u64;
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    fn invalid_input(message: impl Into<String>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, message.into())
    }

    fn append_codes(
        block: &mut BlockWitness,
        path: &Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let corpus_bytes = file.metadata()?.len();
        if corpus_bytes == 0 || !corpus_bytes.is_multiple_of(CODE_SIZE as u64) {
            return Err(invalid_input(
                "extra-code corpus must contain whole, nonempty 24 KiB code chunks",
            )
            .into());
        }
        let count = usize::try_from(corpus_bytes / CODE_SIZE as u64)?;
        block.codes.try_reserve(count)?;
        let mut reader = BufReader::new(file);
        for _ in 0..count {
            let mut code = vec![0; CODE_SIZE];
            reader.read_exact(&mut code)?;
            block.codes.push(Bytes::from(code));
        }
        if reader.read(&mut [0; 1])? != 0 {
            return Err(invalid_input("extra-code corpus changed size while being read").into());
        }
        Ok(count)
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args_os().skip(1);
        let first = args.next().ok_or_else(|| invalid_input(USAGE))?;
        if first == "--help" || first == "-h" {
            println!("{USAGE}");
            return Ok(());
        }
        let input = PathBuf::from(first).canonicalize()?;
        let output = PathBuf::from(args.next().ok_or_else(|| invalid_input(USAGE))?);
        let extra_corpus = match args.next() {
            None => None,
            Some(flag) if flag == "--extra-code-corpus" => Some(
                PathBuf::from(args.next().ok_or_else(|| invalid_input(USAGE))?).canonicalize()?,
            ),
            Some(_) => return Err(invalid_input(USAGE).into()),
        };
        if args.next().is_some() {
            return Err(invalid_input(USAGE).into());
        }

        let mut block: BlockWitness = serde_json::from_reader(BufReader::new(File::open(&input)?))?;
        let original_code_count = block.codes.len();
        let original_code_bytes: usize = block.codes.iter().map(|code| code.len()).sum();
        let state_bytes: usize = block.states.iter().map(|state| state.len()).sum();
        let gas_used = block.header.gas_used;
        let gas_limit = block.header.gas_limit;
        let block_number = block.header.number;
        let chain_id = block.chain_id;
        let tx_count = block.transactions.len();
        let prev_state_root = block.prev_state_root;
        let state_root = block.header.state_root;
        let added_code_count = match &extra_corpus {
            Some(path) => append_codes(&mut block, path)?,
            None => 0,
        };
        let added_code_bytes = added_code_count * CODE_SIZE;
        let version = Version::tsuki();
        let witness = ChunkWitness::new(
            version.as_version_byte(),
            &[block],
            B256::ZERO,
            version.fork,
            None,
        );
        let code_count: usize = witness.blocks.iter().map(|block| block.codes.len()).sum();
        let raw_code_bytes: usize = witness
            .blocks
            .iter()
            .flat_map(|block| &block.codes)
            .map(|code| code.len())
            .sum();

        // Refuse to overwrite either retained evidence or a previous output.
        let mut file = BufWriter::new(File::options().write(true).create_new(true).open(&output)?);
        file.write_all(b"{\"input\":[\"0x01")?;
        let padded_encoded_bytes = {
            let mut hex = HexWriter {
                inner: &mut file,
                encoded_bytes: 0,
            };
            PaddedChunkWitness::encode_to_writer(&witness, &mut hex)?;
            hex.encoded_bytes
        };
        file.write_all(b"\"]}\n")?;
        file.flush()?;

        println!(
            "{}",
            serde_json::json!({
                "classification": if added_code_count == 0 {
                    "offline padded serialization of retained BlockWitness; not a proof"
                } else {
                    "offline memory stress: retained block plus unused code; not a new Engine witness or proof"
                },
                "source_path": input,
                "output": output,
                "extra_code_corpus": extra_corpus,
                "offline_memory_stress": added_code_count != 0,
                "original_code_count": original_code_count,
                "original_code_bytes": original_code_bytes,
                "added_code_count": added_code_count,
                "added_code_bytes": added_code_bytes,
                "code_count": code_count,
                "deduplicated_code_count": original_code_count + added_code_count - code_count,
                "raw_code_bytes": raw_code_bytes,
                "state_bytes": state_bytes,
                "padded_encoded_bytes": padded_encoded_bytes,
                "version_byte": version.as_version_byte(),
                "gas_used": gas_used,
                "gas_limit": gas_limit,
                "block_number": block_number,
                "chain_id": chain_id,
                "tx_count": tx_count,
                "prev_state_root": prev_state_root,
                "state_root": state_root,
            })
        );
        Ok(())
    }
}

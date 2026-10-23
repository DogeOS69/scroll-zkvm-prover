use crate::{
    PartialProvingTask, ProverTester, TaskProver, effective_testdata_fork_directory, prove_verify,
    testdata_fork_directory, tester_execute, testers::PATH_TESTDATA, testing_hardfork,
    testing_version, utils::metadata_from_chunk_witnesses,
};
use eyre::{Context, ContextCompat};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use sbv_core::BlockWitness;
use sbv_primitives::{
    B256,
    types::{Header, consensus::TxL1Message},
};
use scroll_zkvm_prover::utils::read_json;
use scroll_zkvm_prover::utils::vm::ExecutionResult;
use scroll_zkvm_types::{
    proof::ProofEnum,
    public_inputs::{ForkName, Version},
    scroll::chunk::{ChunkInfo, ChunkWitness, SecretKey},
};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

/// Load a file <block_n>.json in the <PATH_BLOCK_WITNESS> directory.
pub fn read_block_witness_from_testdata(block_n: usize) -> eyre::Result<BlockWitness> {
    read_block_witness(
        Path::new(PATH_TESTDATA)
            .join(testdata_fork_directory())
            .join("witnesses")
            .join(format!("{}.json", block_n)),
    )
}

/// Utility function to read and deserialize block witness given the block number.
///
/// Accepts the native SBV format. Legacy corpora must be regenerated with a current node.
pub fn read_block_witness<P>(path_witness: P) -> eyre::Result<BlockWitness>
where
    P: AsRef<Path>,
{
    let path = path_witness.as_ref();
    if !path
        .try_exists()
        .with_context(|| path.display().to_string())?
    {
        return Err(eyre::eyre!("File not found: {}", path.display()));
    }

    serde_json::from_reader(File::open(path)?)
        .with_context(|| format!("{}: expected a native SBV block witness", path.display()))
}

pub struct ChunkProverTester;

impl PartialProvingTask for ChunkWitness {
    fn identifier(&self) -> String {
        let (first, last) = (
            self.blocks.first().expect("MUST NOT EMPTY").header.number,
            self.blocks.last().expect("MUST NOT EMPTY").header.number,
        );
        format!("{first}-{last}")
    }

    fn archive(&self) -> eyre::Result<Vec<u8>>
    where
        Self: Sized,
    {
        let bytes: Vec<u8> = {
            let config = bincode::config::standard();
            bincode::serde::encode_to_vec(self, config)?
        };
        Ok(bytes)
    }

    fn fork_name(&self) -> ForkName {
        ForkName::from(self.fork_name.as_str())
    }
}

impl ProverTester for ChunkProverTester {
    type Metadata = ChunkInfo;

    type Witness = ChunkWitness;

    const NAME: &str = "chunk";

    const PATH_PROJECT_ROOT: &str = "crates/circuits/chunk-circuit";

    const DIR_ASSETS: &str = "chunk";
}
/// Generator collect a range of block witnesses from test data
#[derive(Clone, Default)]
pub struct ChunkTaskGenerator {
    pub version: Version,
    pub block_range: Vec<u64>,
    pub prev_message_hash: Option<B256>,
    pub witness: Option<ChunkWitness>,
    pub proof: Option<ProofEnum>,
}

fn initial_prev_message_hash(version: Version) -> B256 {
    if version.fork == ForkName::Tsuki {
        B256::ZERO
    } else {
        B256::repeat_byte(1u8)
    }
}

/// Load the header of the block before a chunk's first block. SBV validates its hash at execution.
///
/// Prefer the preceding block's native witness, then `<first_block>.parent.json` for sparse
/// corpora, then `parent.json` for the start of a contiguous corpus (including genesis).
/// Header files are RPC block objects; native witnesses use their own header field names.
///
/// Every supported fork needs a parent for SBV validation. A corpus with no parent is reported
/// here, naming all paths that were probed. Successful reads always return a concrete header.
pub fn read_parent_header(
    witnesses_dir: &Path,
    first_block: u64,
    fork: ForkName,
) -> eyre::Result<Header> {
    let parent_number = first_block
        .checked_sub(1)
        .wrap_err("a chunk cannot start at genesis: block 0 has no parent")?;
    let previous = witnesses_dir.join(format!("{parent_number}.json"));
    let per_block = witnesses_dir.join(format!("{first_block}.parent.json"));
    let parent_file = witnesses_dir.join("parent.json");
    let (header, source): (Header, _) = if previous
        .try_exists()
        .with_context(|| previous.display().to_string())?
    {
        (read_block_witness(&previous)?.header, previous)
    } else if per_block
        .try_exists()
        .with_context(|| per_block.display().to_string())?
    {
        (read_json(&per_block)?, per_block)
    } else if parent_file
        .try_exists()
        .with_context(|| parent_file.display().to_string())?
    {
        (read_json(&parent_file)?, parent_file)
    } else {
        eyre::bail!(
            "a {fork} chunk starting at block {first_block} needs its parent header; missing {}, {} and {}",
            previous.display(),
            per_block.display(),
            parent_file.display(),
        );
    };
    eyre::ensure!(
        header.number == parent_number,
        "{} contains block {}, expected parent block {parent_number} for chunk starting at {first_block}",
        source.display(),
        header.number
    );
    Ok(header)
}

impl ChunkTaskGenerator {
    pub fn get_or_build_witness(&mut self) -> eyre::Result<ChunkWitness> {
        if let Some(witness) = &self.witness {
            return Ok(witness.clone());
        }

        let witness = self.calculate_witness()?;
        self.witness = Some(witness.clone());
        Ok(witness)
    }

    pub fn get_or_build_proof(&mut self, prover: &mut impl TaskProver) -> eyre::Result<ProofEnum> {
        if let Some(proof) = &self.proof {
            return Ok(proof.clone());
        }
        let wit = self.get_or_build_witness()?;
        let proof = prove_verify::<ChunkProverTester>(prover, &wit, &[])?;
        self.proof.replace(proof.clone());
        Ok(proof)
    }

    pub fn calculate_witness(&mut self) -> eyre::Result<ChunkWitness> {
        let dir_name = if self.version.is_validium() {
            "validium"
        } else {
            effective_testdata_fork_directory(self.version.fork)
        };
        let paths: Vec<PathBuf> = self
            .block_range
            .iter()
            .map(|block_n| {
                Path::new(PATH_TESTDATA)
                    .join(dir_name)
                    .join("witnesses")
                    .join(format!("{}.json", block_n))
            })
            .collect();

        let block_witnesses = paths
            .iter()
            .map(read_block_witness)
            .collect::<eyre::Result<Vec<BlockWitness>>>()?;

        let first_block = *self
            .block_range
            .first()
            .wrap_err("a chunk task needs at least one block")?;
        let parent_header = read_parent_header(
            &Path::new(PATH_TESTDATA).join(dir_name).join("witnesses"),
            first_block,
            self.version.fork,
        )?;

        let witness = if self.version.is_validium() {
            let base_dir = Path::new(PATH_TESTDATA).join("validium").join("witnesses");
            let secret_key = hex::decode(std::env::var("VALIDIUM_KEY")?)?;
            let secret_key = SecretKey::try_from_bytes(&secret_key)?;
            let validium_txs = self
                .block_range
                .iter()
                .map(|blk| read_json(base_dir.join(format!("{blk}_validium_txs.json"))))
                .collect::<Result<Vec<Vec<TxL1Message>>, _>>()?;
            ChunkWitness::new_validium(
                self.version.as_version_byte(),
                &block_witnesses,
                self.prev_message_hash
                    .unwrap_or_else(|| B256::repeat_byte(1u8)),
                self.version.fork,
                validium_txs,
                secret_key,
                parent_header,
            )
        } else {
            ChunkWitness::new_scroll(
                self.version.as_version_byte(),
                &block_witnesses,
                self.prev_message_hash
                    .unwrap_or_else(|| initial_prev_message_hash(self.version)),
                self.version.fork,
                parent_header,
            )
        };

        Ok(witness)
    }
}

/// helper func to gen a series of proving tasks, specified by the block number
pub fn get_witness_from_env_or_builder(
    fallback_generator: &mut ChunkTaskGenerator,
) -> eyre::Result<ChunkWitness> {
    let paths: Vec<PathBuf> = match std::env::var("TRACE_PATH") {
        // A glob entry that cannot be read is reported rather than dropped: silently skipping one
        // turns a permissions problem into a chunk that is quietly missing a block.
        Ok(paths) => glob::glob(&paths)?
            .collect::<Result<Vec<_>, _>>()
            .wrap_err("TRACE_PATH")?,
        Err(_) => return fallback_generator.get_or_build_witness(),
    };

    read_scroll_chunk_from_paths(paths, testing_version())
}

fn read_scroll_chunk_from_paths(
    paths: Vec<PathBuf>,
    version: Version,
) -> eyre::Result<ChunkWitness> {
    let mut blocks = paths
        .into_iter()
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "parent.json" || name.ends_with(".parent.json"))
        })
        .map(|path| read_block_witness(&path).map(|block| (path, block)))
        .collect::<eyre::Result<Vec<_>>>()?;
    blocks.sort_by_key(|(_, block)| block.header.number);
    let (first_path, first) = blocks.first().wrap_err("TRACE_PATH matched no witnesses")?;

    // The parent comes from the directory the witnesses themselves came from, not from the fork's
    // testdata: a TRACE_PATH run is pointed at some other corpus, whose parent is not the
    // fixtures'.
    let parent_header = read_parent_header(
        first_path
            .parent()
            .wrap_err("TRACE_PATH witnesses need a directory")?,
        first.header.number,
        version.fork,
    )?;
    let block_witnesses = blocks
        .into_iter()
        .map(|(_, block)| block)
        .collect::<Vec<_>>();

    Ok(ChunkWitness::new_scroll(
        version.as_version_byte(),
        &block_witnesses,
        initial_prev_message_hash(version),
        version.fork,
        parent_header,
    ))
}

/// preset examples for single task
pub fn preset_chunk() -> ChunkTaskGenerator {
    let (version, mut block_range) = match testing_hardfork() {
        ForkName::EuclidV1 => (Version::euclid_v1(), 12508460u64..=12508463u64),
        ForkName::EuclidV2 => (Version::euclid_v2(), 1u64..=4u64),
        ForkName::Feynman => (Version::feynman(), 16525000u64..=16525003u64),
        ForkName::Galileo => (Version::galileo(), 20239156..=20239235),
        ForkName::GalileoV2 => (Version::galileo_v2(), 20239240..=20239245),
        ForkName::Tsuki => (Version::tsuki(), 1..=7),
    };

    // If the BLOCK_RANGE env var is set, use that instead.
    if let Ok(r) = std::env::var("BLOCK_RANGE") {
        if r.contains("..=") {
            let parts: Vec<&str> = r.split("..=").collect();
            if parts.len() != 2 {
                panic!("invalid BLOCK_RANGE, expect start..=end, found = {r}");
            }
            let start = parts[0]
                .trim()
                .parse::<u64>()
                .expect("BLOCK_RANGE start not a number");
            let end = parts[1]
                .trim()
                .parse::<u64>()
                .expect("BLOCK_RANGE end not a number");
            block_range = start..=end;
        } else {
            panic!("invalid BLOCK_RANGE, expect start..=end, found = {r}");
        }
    }

    ChunkTaskGenerator {
        version,
        block_range: block_range.collect(),
        ..Default::default()
    }
}

/// create canonical tasks from a series of block range
pub fn create_canonical_tasks(
    version: Version,
    ranges: impl Iterator<Item = std::ops::RangeInclusive<u64>>,
) -> eyre::Result<Vec<ChunkTaskGenerator>> {
    let mut ret = Vec::new();
    let mut prev_message_hash = None;
    for r in ranges {
        let mut canonical_generator = ChunkTaskGenerator {
            version,
            block_range: r.collect(),
            prev_message_hash,
            proof: Default::default(),
            witness: Default::default(),
        };
        let chunk_wit = canonical_generator.get_or_build_witness()?;
        let info = metadata_from_chunk_witnesses(chunk_wit)?;

        prev_message_hash = Some(info.post_msg_queue_hash);
        ret.push(canonical_generator);
    }

    Ok(ret)
}

/// The `tsuki-base-fee` corpus split into two chunks, at the block whose transaction changes the
/// L2 base-fee overhead: the second chunk is then the first one that has to read the new value out
/// of its own pre-state, and its trusted parent is the boundary block.
fn tsuki_golden_chunk_ranges() -> [std::ops::RangeInclusive<u64>; 2] {
    [1..=4, 5..=7]
}

pub fn tsuki_golden_chunk_tasks() -> eyre::Result<Vec<ChunkTaskGenerator>> {
    create_canonical_tasks(Version::tsuki(), tsuki_golden_chunk_ranges().into_iter())
}

/// preset examples for multiple task
pub fn preset_chunk_multiple() -> Vec<ChunkTaskGenerator> {
    let (mut block_range, version) = match testing_hardfork() {
        ForkName::EuclidV1 => (
            vec![
                12508460u64..=12508460u64,
                12508461u64..=12508461u64,
                12508462u64..=12508463u64,
            ],
            Version::euclid_v1(),
        ),
        ForkName::EuclidV2 => (
            vec![1u64..=1u64, 2u64..=2u64, 3u64..=4u64],
            Version::euclid_v2(),
        ),
        ForkName::Feynman => (
            vec![
                16525000u64..=16525000u64,
                16525001u64..=16525001u64,
                16525002u64..=16525003u64,
            ],
            Version::feynman(),
        ),
        ForkName::Galileo => (
            vec![
                20239220..=20239220,
                20239221..=20239221,
                20239222..=20239222,
            ],
            Version::galileo(),
        ),
        ForkName::GalileoV2 => (
            vec![
                20239240..=20239240,
                20239241..=20239241,
                20239242..=20239242,
            ],
            Version::galileo_v2(),
        ),
        ForkName::Tsuki => (
            tsuki_golden_chunk_ranges().into_iter().collect(),
            Version::tsuki(),
        ),
    };
    // If the BLOCK_RANGE env var has been set, use that instead.
    if let Ok(r) = std::env::var("BLOCK_RANGE") {
        block_range = r
            .split(",")
            .map(|part| {
                let p = part.trim();
                if let Some(idx) = p.find("..=") {
                    let start = &p[..idx].trim();
                    let end = &p[idx + 3..].trim();
                    let a = start
                        .parse::<u64>()
                        .expect("BLOCK_RANGE start not a number");
                    let b = end.parse::<u64>().expect("BLOCK_RANGE end not a number");
                    a..=b
                } else {
                    panic!("invalid range syntax: {}", p);
                }
            })
            .collect();
    }
    create_canonical_tasks(version, block_range.into_iter())
        .expect("must success for preset collections")
}

pub fn preset_chunk_validium() -> Vec<ChunkTaskGenerator> {
    let block_range = vec![347..=355, 356..=360, 361..=370, 371..=375, 376..=397];
    create_canonical_tasks(Version::validium_v1(), block_range.into_iter())
        .expect("must succeed for preset collection")
}

pub fn exec_chunk(wit: &ChunkWitness) -> eyre::Result<(ExecutionResult, u64)> {
    let blk = wit.blocks[0].header.number;
    println!(
        "task block num: {}, block[0] idx: {}",
        wit.blocks.len(),
        blk
    );
    let stats = wit.stats();
    println!("chunk stats {:#?}", stats);
    let exec_result = tester_execute::<ChunkProverTester>(wit, &[])?;
    let cycle_count = exec_result.total_cycle as u64;
    let cycle_per_gas = cycle_count as f64 / stats.total_gas_used as f64;
    println!(
        "blk {blk}->{}, cycle {cycle_count}, gas {}, cycle-per-gas {cycle_per_gas}",
        wit.blocks.last().unwrap().header.number,
        stats.total_gas_used,
    );
    eyre::Ok((exec_result, stats.total_gas_used))
}

pub fn execute_multi(
    wits: Vec<ChunkWitness>,
) -> impl FnOnce() -> (u64, u64) + Send + Sync + 'static {
    || {
        wits.into_par_iter()
            .map(|wit| -> (u64, u64) {
                let (exec_result, gas) = exec_chunk(&wit).unwrap();
                (gas, exec_result.total_cycle)
            })
            .reduce(
                || (0u64, 0u64),
                |(gas1, cycle1): (u64, u64), (gas2, cycle2): (u64, u64)| {
                    (gas1 + gas2, cycle1 + cycle2)
                },
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{PATH_TESTDATA, read_parent_header};
    use crate::testers::chunk::{preset_chunk, preset_chunk_multiple};
    use scroll_zkvm_types::public_inputs::ForkName;
    use std::path::Path;

    #[test]
    fn test_presets() {
        std::env::remove_var("BLOCK_RANGE");

        // The Tsuki presets read `tsuki-base-fee`, whose seven blocks split at the one whose
        // transaction changes the L2 base-fee overhead.
        let single = preset_chunk();
        assert_eq!(single.block_range, (1u64..=7).collect::<Vec<u64>>(),);
        let multiple = preset_chunk_multiple();
        assert_eq!(multiple.len(), 2);
        assert_eq!(multiple[0].block_range, (1..=4).collect::<Vec<u64>>(),);
        assert_eq!(multiple[1].block_range, (5..=7).collect::<Vec<u64>>(),);

        // After setting env var.
        std::env::set_var("BLOCK_RANGE", "123..=321");
        assert_eq!(
            preset_chunk().block_range,
            (123..=321).collect::<Vec<u64>>()
        );
        std::env::set_var("BLOCK_RANGE", "1..=2,3..=4,5..=6");
        let multiple = preset_chunk_multiple();
        assert_eq!(multiple[0].block_range, (1u64..=2u64).collect::<Vec<u64>>());
        assert_eq!(multiple[1].block_range, (3u64..=4u64).collect::<Vec<u64>>());
        assert_eq!(multiple[2].block_range, (5u64..=6u64).collect::<Vec<u64>>());
        std::env::remove_var("BLOCK_RANGE");
    }

    /// Parent loading is independent of the public-input endpoint semantics.
    #[test]
    fn a_parent_header_is_read_for_every_supported_fork() {
        let tsuki = Path::new(PATH_TESTDATA)
            .join("tsuki-base-fee")
            .join("witnesses");
        let missing = Path::new(PATH_TESTDATA).join("feynman").join("witnesses");
        for fork in [
            ForkName::Feynman,
            ForkName::Galileo,
            ForkName::GalileoV2,
            ForkName::Tsuki,
        ] {
            for first_block in [1, 2] {
                let parent = read_parent_header(&tsuki, first_block, fork)
                    .expect("genesis and preceding witnesses are available");
                assert_eq!(parent.number, first_block - 1);
            }
            let error = read_parent_header(&missing, 16_525_000, fork)
                .expect_err("every fork requires a parent")
                .to_string();
            assert!(error.contains("16524999.json"), "{error}");
            assert!(error.contains("parent.json"), "{error}");
        }
    }

    #[test]
    fn sparse_validium_blocks_load_their_own_parent_headers() {
        use super::read_block_witness;
        use sbv_primitives::B256;
        use scroll_zkvm_types::{
            public_inputs::Version,
            scroll::chunk::{ChunkWitness, SecretKey},
        };

        let source = Path::new(PATH_TESTDATA).join("tsuki-base-fee/witnesses");
        let dir = tempfile::tempdir().unwrap();
        for number in [2, 5] {
            let parent = read_block_witness(source.join(format!("{}.json", number - 1)))
                .unwrap()
                .header;
            // A sparse validium corpus contains a parent header, not a full preceding witness.
            std::fs::write(
                dir.path().join(format!("{number}.parent.json")),
                serde_json::to_vec(&parent).unwrap(),
            )
            .unwrap();
            let version = Version::validium_v1();
            let loaded = read_parent_header(dir.path(), number, version.fork).unwrap();
            let block = read_block_witness(source.join(format!("{number}.json"))).unwrap();
            assert_eq!(loaded.hash_slow(), block.header.parent_hash);
            let witness = ChunkWitness::new_validium(
                version.as_version_byte(),
                &[block],
                B256::ZERO,
                version.fork,
                vec![vec![]],
                SecretKey::try_from_bytes([1u8; 32]).unwrap(),
                loaded,
            );
            assert_eq!(
                witness.parent_header.unwrap().hash_slow(),
                parent.hash_slow()
            );
        }
    }

    #[test]
    fn parent_files_cannot_silently_select_the_wrong_height() {
        use super::read_block_witness;
        let source = Path::new(PATH_TESTDATA).join("tsuki-base-fee/witnesses");
        let dir = tempfile::tempdir().unwrap();
        let parent = read_block_witness(source.join("1.json")).unwrap().header;
        std::fs::write(
            dir.path().join("parent.json"),
            serde_json::to_vec(&parent).unwrap(),
        )
        .unwrap();
        let error = read_parent_header(dir.path(), 5, ForkName::Tsuki)
            .unwrap_err()
            .to_string();
        assert!(error.contains("expected parent block 4"), "{error}");
        assert!(
            read_parent_header(dir.path(), 0, ForkName::Tsuki)
                .unwrap_err()
                .to_string()
                .contains("genesis")
        );
        // A present but malformed block-specific parent must not fall back to parent.json.
        std::fs::write(dir.path().join("2.parent.json"), b"null").unwrap();
        assert!(read_parent_header(dir.path(), 2, ForkName::Tsuki).is_err());
    }

    #[test]
    fn trace_paths_load_the_first_blocks_parent_and_skip_header_files() {
        use super::{read_block_witness, read_scroll_chunk_from_paths};
        use scroll_zkvm_types::{public_inputs::Version, scroll::chunk::execute};
        let source = Path::new(PATH_TESTDATA).join("tsuki-base-fee/witnesses");
        let dir = tempfile::tempdir().unwrap();
        for name in ["parent.json", "1.json", "2.json"] {
            std::fs::copy(source.join(name), dir.path().join(name)).unwrap();
        }
        let first = read_block_witness(source.join("1.json")).unwrap();
        std::fs::write(
            dir.path().join("2.parent.json"),
            serde_json::to_vec(&first.header).unwrap(),
        )
        .unwrap();
        let witness = read_scroll_chunk_from_paths(
            ["2.json", "parent.json", "2.parent.json", "1.json"]
                .map(|name| dir.path().join(name))
                .to_vec(),
            Version::tsuki(),
        )
        .unwrap();
        assert_eq!(
            witness
                .blocks
                .iter()
                .map(|b| b.header.number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            witness.parent_header.as_ref().unwrap().hash_slow(),
            first.header.parent_hash
        );
        execute(witness).expect("the loaded parent and native witnesses must execute together");
        assert!(
            read_scroll_chunk_from_paths(vec![dir.path().join("parent.json")], Version::tsuki())
                .unwrap_err()
                .to_string()
                .contains("matched no witnesses")
        );
    }
}

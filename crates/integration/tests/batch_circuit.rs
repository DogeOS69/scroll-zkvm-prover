use scroll_zkvm_integration::{
    ProverTester,
    testers::{
        batch::{BatchProverTester, BatchTaskGenerator},
        chunk::{ChunkProverTester, create_canonical_tasks, preset_chunk_multiple},
        load_local_task,
    },
    utils::{build_batch_witnesses, metadata_from_batch_witnesses},
    testing_version,
};
use scroll_zkvm_prover::task::ProvingTask;
use sbv_primitives::B256;
use scroll_zkvm_types::public_inputs::Version;
use std::str::FromStr;

#[ignore = "need local stuff"]
#[test]
fn test_execute() -> eyre::Result<()> {
    BatchProverTester::setup(true)?;
    let u_task = load_local_task("batch-task.json")?;
    let stdin = u_task.build_guest_input();

    let prover = BatchProverTester::load_prover(false)?;

    let _ = prover.execute_and_check(&stdin)?;
    Ok(())
}

#[ignore = "need local stuff"]
#[test]
fn setup_prove_verify_single() -> eyre::Result<()> {
    BatchProverTester::setup(true)?;
    let u_task = load_local_task("batch-task.json")?;

    let mut prover = BatchProverTester::load_prover(false)?;

    let _ = prover.gen_proof_universal(&u_task, false)?;

    Ok(())
}

#[test]
fn test_e2e_execute() -> eyre::Result<()> {
    BatchProverTester::setup(true)?;

    let prover = BatchProverTester::load_prover(false)?;
    let mut chunk_prover = ChunkProverTester::load_prover(false)?;

    let mut task = BatchTaskGenerator::from_chunk_tasks(&preset_chunk_multiple(), None);

    let wit = task.get_or_build_witness()?;
    let agg_proofs = task.get_or_build_child_proofs(&mut chunk_prover)?;

    let stdin = BatchProverTester::build_guest_input(
        &wit,
        agg_proofs.iter().map(|p| p.as_stark_proof().unwrap()),
    )?;
    let _ = prover.execute_and_check_with_full_result(&stdin)?;

    Ok(())
}

#[test]
fn e2e() -> eyre::Result<()> {
    BatchProverTester::setup(true)?;

    let mut prover = BatchProverTester::load_prover(false)?;
    let mut chunk_prover = ChunkProverTester::load_prover(false)?;
    let mut batch = BatchTaskGenerator::from_chunk_tasks(&preset_chunk_multiple(), None);
    let _ = batch.get_or_build_proof(&mut prover, &mut chunk_prover)?;

    Ok(())
}

#[test]
fn axiom_e2e() -> eyre::Result<()> {
    BatchProverTester::setup(true)?;

    let mut prover = BatchProverTester::load_axiom_prover()?;
    let mut chunk_prover = ChunkProverTester::load_axiom_prover()?;
    let mut batch = BatchTaskGenerator::from_chunk_tasks(&preset_chunk_multiple(), None);
    let _ = batch.get_or_build_proof(&mut prover, &mut chunk_prover)?;

    Ok(())
}

#[test]
fn verify_batch_hash_invariant() -> eyre::Result<()> {
    use scroll_zkvm_types::public_inputs::ForkName;
    BatchProverTester::setup(true)?;

    let outcome_1 = preset_chunk_multiple();
    let (version, block_range) = match testing_version().fork {
        ForkName::EuclidV1 => (
            Version::euclid_v1(),
            vec![
                12508460u64..=12508461u64,
                12508462u64..=12508462u64,
                12508463u64..=12508463u64,
            ],
        ),
        ForkName::EuclidV2 => (
            Version::euclid_v2(),
            vec![1u64..=2u64, 3u64..=3u64, 4u64..=4u64],
        ),
        ForkName::Feynman => (
            Version::feynman(),
            vec![
                16525000u64..=16525001u64,
                16525002u64..=16525002u64,
                16525003u64..=16525003u64,
            ],
        ),
        ForkName::Galileo => (
            Version::galileo(),
            vec![
                20239156..=20239162,
                20239163..=20239175,
                20239176..=20239192,
            ],
        ),
        ForkName::GalileoV2 => (
            Version::galileo_v2(),
            vec![
                32144474..=32144475,
                32144476..=32144476,
            ],
        ),
    };
    let outcome_2 = create_canonical_tasks(version, block_range.into_iter())?;

    let mut task_1 = BatchTaskGenerator::from_chunk_tasks(&outcome_1, None);
    let mut task_2 = BatchTaskGenerator::from_chunk_tasks(&outcome_2, None);

    // verify the two task has the same blob bytes
    assert_eq!(
        task_1.get_or_build_witness()?.blob_bytes,
        task_2.get_or_build_witness()?.blob_bytes,
    );

    Ok(())
}

#[test]
fn test_galileov2_batch_metadata() -> eyre::Result<()> {
    let chunks = preset_chunk_multiple()
        .into_iter()
        .map(|mut task| task.get_or_build_witness())
        .collect::<eyre::Result<Vec<_>>>()?;

    let witness = build_batch_witnesses(&chunks, &[0u8; 64], Default::default())?;
    let info = metadata_from_batch_witnesses(&witness)?;

    assert_eq!(
        info.withdraw_root,
        B256::from_str("0x9d42c00e8305f065d4e8df4d073cd9424986b0dcc63becf39b718d9f61f99179")?,
    );
    assert_eq!(info.next_message_index, 221555);

    Ok(())
}

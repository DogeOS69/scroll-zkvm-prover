use alloy_primitives::{B256, b256};
use eyre::Ok;
use sbv_primitives::types::consensus::TxL1Message;
use scroll_zkvm_integration::testers::PATH_TESTDATA;
use scroll_zkvm_integration::testers::chunk::read_block_witness;
use scroll_zkvm_integration::testers::chunk::{exec_chunk, execute_multi};
use scroll_zkvm_integration::utils::get_rayon_threads;
use scroll_zkvm_integration::{
    ProverTester, prove_verify,
    testers::chunk::{
        ChunkProverTester, ChunkTaskGenerator, get_witness_from_env_or_builder, preset_chunk,
        preset_chunk_multiple,
    },
    utils::metadata_from_chunk_witnesses,
};
use scroll_zkvm_prover::utils::read_json;
use scroll_zkvm_types::public_inputs::{MultiVersionPublicInputs, Version};
use scroll_zkvm_types::scroll::chunk::{ChunkWitness, SecretKey};
use std::env;
use std::path::Path;

#[ignore = "can only run under eculidv2 hardfork"]
#[test]
fn test_cycle() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    // use rayon::prelude::*;

    let blocks = 1u64..=8u64;
    for blk in blocks {
        let mut task = ChunkTaskGenerator {
            block_range: (blk..=blk).collect(),
            ..Default::default()
        };

        let (exec_result, gas) = exec_chunk(&task.get_or_build_witness()?)?;
        let cycle_per_gas = exec_result.total_cycle / gas;
        assert!(cycle_per_gas < 30);
    }

    Ok(())
}

#[test]
fn test_execute() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    let wit = get_witness_from_env_or_builder(&mut preset_chunk())?;
    let (exec_result, total_gas_used) = exec_chunk(&wit)?;
    let cycle_per_gas = exec_result.total_cycle / total_gas_used;
    assert_ne!(cycle_per_gas, 0);
    assert!(cycle_per_gas <= 35);
    Ok(())
}

#[test]
fn test_execute_validium() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    let base_dir = Path::new(PATH_TESTDATA).join("validium");

    let secret_key = hex::decode(env::var("VALIDIUM_KEY")?)?;
    let secret_key = SecretKey::try_from_bytes(&secret_key)?;

    for blk in [1019, 1256, 1276, 1141071] {
        let block_witness = read_block_witness(base_dir.join(format!("{blk}.json")))?;
        let validium_txs: Vec<TxL1Message> =
            read_json(base_dir.join(format!("{blk}_validium_txs.json")))?;

        let version = Version::validium_v1();
        let witness = ChunkWitness::new_validium(
            version.as_version_byte(),
            &[block_witness],
            B256::ZERO,
            version.fork,
            vec![validium_txs],
            secret_key.clone(),
        );

        exec_chunk(&witness)?;
    }
    Ok(())
}

#[ignore = "can only run under eculidv2 hardfork"]
#[test]
fn test_autofill_trie_nodes() -> eyre::Result<()> {
    use std::result::Result::Ok;
    ChunkProverTester::setup(true)?;

    let mut template_wit = get_witness_from_env_or_builder(&mut preset_chunk())?;
    template_wit.blocks.truncate(1);
    let wit = ChunkWitness::new_scroll(
        template_wit.version,
        &template_wit.blocks,
        template_wit.prev_msg_queue_hash,
        template_wit.fork_name,
    );
    for index in [10, 13] {
        println!(
            "removing state at index {}: {:?}",
            index, wit.blocks[0].states[index]
        );
        let mut test_wit = wit.clone();
        test_wit.blocks[0].states.remove(index);
        let result = metadata_from_chunk_witnesses(test_wit);

        match result {
            Err(err_str) => {
                let err_str = format!("{}", err_str);
                // https://github.com/scroll-tech/scroll/blob/develop/crates/libzkp/src/tasks/chunk.rs#L155
                let pattern = r"SparseTrieError\(BlindedNode \{ path: Nibbles\((0x[0-9a-fA-F]+)\), hash: (0x[0-9a-fA-F]+) \}\)";
                let err_parse_re = regex::Regex::new(pattern)?;
                match err_parse_re.captures(&err_str) {
                    Some(caps) => {
                        let hash = caps[2].to_string();
                        println!("missing trie hash {hash}");
                        if index == 10 {
                            assert_eq!(
                                hash,
                                "0x3672d4a4951dbf05a8d18c33bd880a640aeb4dc1082bc96c489e3d658659c340"
                            );
                        }
                        if index == 13 {
                            assert_eq!(
                                hash,
                                "0x166a095be91b1f2ffc9d1a8abc0522264f67121086a4ea0b22a0a6bef07b000a"
                            );
                        }
                    }
                    None => {
                        println!("Cannot capture missing trie nodes");
                        panic!("Err msg: {}", err_str);
                    }
                }
            }
            Ok(_) => {
                panic!("Cannot capture missing trie nodes");
            }
        }
    }

    Ok(())
}

#[test]
fn test_execute_multi() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    let tasks = preset_chunk_multiple()
        .into_iter()
        .map(|mut task| task.get_or_build_witness().unwrap())
        .collect::<Vec<_>>();

    // Execute tasks in parallel
    let (total_gas, total_cycle) = rayon::ThreadPoolBuilder::new()
        .num_threads(get_rayon_threads())
        .build()?
        .install(execute_multi(tasks));

    println!(
        "Total gas: {}, Total cycles: {}, Average cycle/gas: {}",
        total_gas,
        total_cycle,
        total_cycle as f64 / total_gas as f64,
    );

    Ok(())
}

#[test]
fn test_tsuki_golden_chunk_metadata() -> eyre::Result<()> {
    let version = Version::tsuki();
    let infos = preset_chunk_multiple()
        .into_iter()
        .map(|mut task| metadata_from_chunk_witnesses(task.get_or_build_witness()?))
        .collect::<eyre::Result<Vec<_>>>()?;

    assert_eq!(infos.len(), 4, "the batch must aggregate four child chunks");
    assert_eq!(
        infos
            .iter()
            .map(|info| info.initial_block_number)
            .collect::<Vec<_>>(),
        [1, 9, 17, 21]
    );
    assert_eq!(
        infos
            .iter()
            .map(|info| info.block_ctxs.len())
            .collect::<Vec<_>>(),
        [8, 8, 4, 6]
    );
    assert_eq!(
        infos
            .iter()
            .map(|info| info.next_message_index)
            .collect::<Vec<_>>(),
        [0, 0, 1, 1]
    );
    assert_eq!(
        infos[0].prev_state_root,
        b256!("8938aed386448da2e825974f29a8f14a862bfa9f94973a8cea261542ff8792a1")
    );
    assert_eq!(
        infos[0].post_state_root,
        b256!("15c80478db61728fc66486ddefcdacac54201cc387fd650c33bff7665040e508")
    );
    assert_eq!(
        infos[1].post_state_root,
        b256!("d91fa7eb65477f108002dfa303f52d4db71cdfd94eab291c5698f8ecadbee89e")
    );
    assert_eq!(
        infos[2].post_state_root,
        b256!("21d953b3b999fd84849a86ab365eca67d68a1ce9c34e816fd746630b8388eb9f")
    );
    assert_eq!(
        infos[3].post_state_root,
        b256!("54097ced498c20c61c9817f44dae4a4cb197c818810aa5a9717c67814f3925f6")
    );

    for pair in infos.windows(2) {
        pair[1].validate(&pair[0], version);
    }

    assert_eq!(
        infos
            .iter()
            .map(|info| info.pi_hash_by_version(version))
            .collect::<Vec<_>>(),
        [
            b256!("ca2da9b9bca7caccda93c8fcac9a34bf0b9c944b8598ac92be40881b7618b787"),
            b256!("b6102e1080723bdec5040facdc096d04049dd1f2ae92285f0bb448cf14dfdd44"),
            b256!("96347767e5c6c67617d5eff9ca0f7bbe210adc6116700647a6eec32166b94110"),
            b256!("fa07464856cdf99805f59f09d53273c459de5b9e15a714fd12c4a08b480ea9b1"),
        ]
    );

    Ok(())
}

#[test]
fn test_tsuki_edge_fixture_contract() -> eyre::Result<()> {
    fn transaction(value: &serde_json::Value) -> &serde_json::Value {
        &value["transactions"][0]["Eip1559"]["transaction"]
    }

    let base_dir = Path::new(PATH_TESTDATA).join("tsuki").join("witnesses");
    let fixture = |block: u64| -> eyre::Result<serde_json::Value> {
        Ok(read_json(base_dir.join(format!("{block}.json")))?)
    };

    let native_success = fixture(21)?;
    assert_eq!(
        transaction(&native_success)["to"],
        "0x530000000000000000000000000000000000d09e"
    );
    assert_eq!(
        transaction(&native_success)["input"],
        "0xa9059cbb000000000000000000000000000000000000000000000000000000000000beef0000000000000000000000000000000000000000000000000000000000003039"
    );
    assert_eq!(native_success["header"]["gas_used"], 33_612);

    let ripemd_limit = fixture(22)?;
    assert_eq!(
        transaction(&ripemd_limit)["to"],
        "0x0000000000000000000000000000000000000003"
    );
    assert_eq!(
        transaction(&ripemd_limit)["input"]
            .as_str()
            .expect("RIPEMD input")
            .len(),
        2 + 32 * 2
    );

    let ripemd_overflow = fixture(23)?;
    assert_eq!(
        transaction(&ripemd_overflow)["input"]
            .as_str()
            .expect("RIPEMD overflow input")
            .len(),
        2 + 33 * 2
    );
    assert_eq!(ripemd_overflow["header"]["gas_used"], 100_000);

    let unauthorized_transfer = fixture(24)?;
    assert_eq!(
        transaction(&unauthorized_transfer)["to"],
        "0x00000000000000000000000000000000000000fd"
    );
    assert_eq!(
        transaction(&unauthorized_transfer)["input"],
        "0x000000000000000000000000ded06046416d6ba20c1e2bad51b3a3e2f267d33f000000000000000000000000000000000000000000000000000000000000beef0000000000000000000000000000000000000000000000000000000000000001"
    );
    assert_eq!(unauthorized_transfer["header"]["gas_used"], 100_000);

    let insufficient_native_balance = fixture(25)?;
    assert_eq!(
        transaction(&insufficient_native_balance)["to"],
        "0x530000000000000000000000000000000000d09e"
    );
    assert_eq!(
        transaction(&insufficient_native_balance)["input"],
        "0xa9059cbb000000000000000000000000000000000000000000000000000000000000beef000000000000000000000000000000000000000c9f2c9cd04674edea40000000"
    );
    assert_eq!(insufficient_native_balance["header"]["gas_used"], 22_189);

    let eip7825_limit = fixture(26)?;
    assert_eq!(
        transaction(&eip7825_limit)["to"],
        "0x0000000000000000000000000000000000000001"
    );
    assert_eq!(transaction(&eip7825_limit)["input"], "0x");
    assert_eq!(transaction(&eip7825_limit)["gas_limit"], 16_777_216);
    assert_eq!(eip7825_limit["header"]["gas_used"], 24_000);

    Ok(())
}

#[test]
fn guest_profiling() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    let wit = get_witness_from_env_or_builder(&mut preset_chunk())?;
    let (exec_result, _) = exec_chunk(&wit)?;
    let total_cycles = exec_result.total_cycle;

    println!(
        "scroll-zkvm-integration(chunk-circuit): total cycles = {:?}",
        total_cycles
    );

    Ok(())
}

#[test]
fn setup_prove_verify_single() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;
    let mut prover = ChunkProverTester::load_prover(false)?;

    let wit = get_witness_from_env_or_builder(&mut preset_chunk())?;
    let _ = prove_verify::<ChunkProverTester>(&mut prover, &wit, &[])?;

    Ok(())
}

#[test]
fn setup_prove_verify_multi() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;
    let mut prover = ChunkProverTester::load_prover(false)?;

    for mut task in preset_chunk_multiple() {
        let _ = task.get_or_build_proof(&mut prover)?;
    }

    Ok(())
}

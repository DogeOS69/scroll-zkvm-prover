use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_client::ClientBuilder;
use alloy_transport::layers::{RetryBackoffLayer, ThrottleLayer};
use clap::Parser;
use eyre::{Context, ContextCompat};
use sbv_core::BlockWitness;
use sbv_primitives::{
    B256,
    types::{Header, Network},
};
use sbv_utils::rpc::ProviderExt;
use scroll_zkvm_integration::{
    ProverTester,
    testers::chunk::{ChunkProverTester, exec_chunk},
    testing_version,
};
use scroll_zkvm_types::{public_inputs::Version, scroll::chunk::ChunkWitness};
use std::collections::HashMap;
use std::{fs::File, path::PathBuf, slice};
use url::Url;

#[derive(Parser)]
struct Cli {
    #[arg(long, env = "RPC_URL", help = "The RPC URL to connect to")]
    rpc_url: Url,
    #[arg(
        long,
        env = "OUT_PATH",
        default_value = "scanner.csv",
        help = "The output CSV file path"
    )]
    out_path: PathBuf,

    #[arg(
        long,
        env = "CHUNK_GAS_TARGET",
        default_value_t = 24,
        help = "Target gas per chunk (in millions of gas)"
    )]
    chunk_gas_target: u64,
    #[arg(
        long,
        env = "N_CHUNKS",
        default_value_t = 50,
        help = "Number of chunks to process"
    )]
    n_chunks: u64,

    #[arg(
        long,
        env = "BLOCK_GAS_ESTIMATED",
        default_value_t = 1,
        help = "Block gas estimated (in millions of gas), used when start_block is not set"
    )]
    block_gas_estimated: u64,
    #[arg(
        long,
        env = "START_BLOCK",
        help = "The start block number (inclusive). If not set, use latest - N_CHUNKS * (CHUNK_GAS_TARGET / BLOCK_GAS_ESTIMATED)"
    )]
    start_block: Option<u64>,

    #[arg(
        long,
        env = "COMPARE_WITH",
        help = "Compare the results with an existing CSV file"
    )]
    compare_with: Option<PathBuf>,

    #[arg(
        long,
        env = "REQUESTS_PER_SECOND",
        default_value_t = 10,
        help = "Max requests per second"
    )]
    requests_per_second: u32,
    #[arg(
        long,
        env = "MAX_RATE_LIMIT_RETRIES",
        default_value_t = 10,
        help = "Max retries on rate limit"
    )]
    max_rate_limit_retries: u32,
    #[arg(
        long,
        env = "RETRIES_INITIAL_BACKOFF",
        default_value_t = 100,
        help = "Initial backoff in ms for retries"
    )]
    retries_initial_backoff: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stats {
    start_block: u64,
    end_block: u64,
    tx_num: usize,
    cycle: u64,
    gas: u64,
    cycle_per_gas: f64,
    diff_percentage: Option<f64>,
    diff_absolute: Option<i64>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    ChunkProverTester::setup(true)?;

    let cli = Cli::parse();

    println!("RPC URL = {}", cli.rpc_url);
    println!("out_path = {}", cli.out_path.display());
    let client = ClientBuilder::default()
        .layer(RetryBackoffLayer::new(
            cli.max_rate_limit_retries,
            cli.retries_initial_backoff,
            u64::MAX,
        ))
        .layer(ThrottleLayer::new(cli.requests_per_second))
        .http(cli.rpc_url);
    let provider = ProviderBuilder::<_, _, Network>::default().connect_client(client);

    let compare_with: Option<HashMap<(u64, u64), Stats>> = cli
        .compare_with
        .map(|compare_with| {
            csv::Reader::from_path(compare_with)?
                .into_deserialize::<Stats>()
                .map(|r| r.map(|stats| ((stats.start_block, stats.end_block), stats)))
                .collect::<Result<_, _>>()
        })
        .transpose()?;

    let out = File::create(&cli.out_path)?;
    let mut writer = csv::Writer::from_writer(out);

    let start_block = if let Some(b) = cli.start_block {
        b
    } else {
        let latest_block = provider
            .get_block_number()
            .await
            .context("Failed to fetch latest block number")?;
        let estimated_blocks_required = cli.chunk_gas_target / cli.block_gas_estimated;
        latest_block
            .checked_sub(estimated_blocks_required)
            .context("estimated start block overflowed; please adjust args")?
    };

    println!(
        "scan blocks from {start_block}; {chunk_gas_target}M Gas chunk, {n_chunks} chunks",
        chunk_gas_target = cli.chunk_gas_target,
        n_chunks = cli.n_chunks,
    );

    eyre::ensure!(
        start_block > 0,
        "cannot scan genesis: block 0 has no parent"
    );
    eyre::ensure!(cli.n_chunks > 0, "N_CHUNKS must be positive");
    let gas_target = cli
        .chunk_gas_target
        .checked_mul(1_000_000)
        .context("CHUNK_GAS_TARGET overflowed")?;
    let version = testing_version();
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let fetch_task = tokio::spawn(fetcher(tx, provider, start_block));

    let mut pending: Option<PendingChunk> = None;
    let mut n_chunks = 0;
    while let Some((parent_header, block)) = rx.recv().await {
        let (_, block_gas) = exec_chunk(&ChunkWitness::new_scroll(
            version.as_version_byte(),
            slice::from_ref(&block),
            B256::ZERO,
            version.fork,
            parent_header.clone(),
        ))?;

        let completed = if let Some(chunk) = &mut pending {
            chunk.push(parent_header, block, block_gas, gas_target)
        } else {
            pending = Some(PendingChunk::new(parent_header, block, block_gas));
            None
        };
        if let Some(chunk) = completed {
            let wit = chunk.into_witness(version);
            let (exec_result, gas) = exec_chunk(&wit)?;
            let blocks = &wit.blocks;

            let start_block = blocks[0].header.number;
            let end_block = blocks.last().unwrap().header.number;
            let cycle = exec_result.total_cycle;

            let old_cycle = compare_with
                .as_ref()
                .and_then(|m| m.get(&(start_block, end_block)))
                .map(|old_stats| old_stats.cycle);
            let diff_absolute = old_cycle.map(|old| cycle as i64 - old as i64);
            let diff_percentage = old_cycle
                .zip(diff_absolute)
                .map(|(old, diff)| (diff as f64) / (old as f64) * 100.0);

            let stats = Stats {
                start_block,
                end_block,
                tx_num: blocks.iter().map(|b| b.transactions.len()).sum::<usize>(),
                cycle,
                gas,
                cycle_per_gas: exec_result.total_cycle as f64 / gas as f64,
                diff_percentage,
                diff_absolute,
            };

            println!(
                "Blocks {}..={} | Tx: {} | Cycle: {} | Gas: {} | Cycle/Gas: {:.2}",
                stats.start_block,
                stats.end_block,
                stats.tx_num,
                stats.cycle,
                stats.gas,
                stats.cycle_per_gas
            );
            writer
                .serialize(stats)
                .context("Failed to write stats to CSV")?;
            writer.flush().context("Failed to flush CSV")?;
            n_chunks += 1;
            if n_chunks >= cli.n_chunks {
                fetch_task.abort();
                return Ok(());
            }
        }
    }

    // A failed RPC task must not turn into a successful, silently truncated scan.
    fetch_task.await.context("block fetcher task failed")??;
    eyre::bail!("block fetcher stopped before the requested chunks were scanned")
}

/// A nonempty chunk retains the parent of its first block as later blocks arrive.
struct PendingChunk {
    parent_header: Header,
    blocks: Vec<BlockWitness>,
    gas_used: u64,
}

impl PendingChunk {
    fn new(parent_header: Header, block: BlockWitness, gas_used: u64) -> Self {
        Self {
            parent_header,
            blocks: vec![block],
            gas_used,
        }
    }

    fn push(
        &mut self,
        parent_header: Header,
        block: BlockWitness,
        gas: u64,
        target: u64,
    ) -> Option<Self> {
        if self.gas_used.saturating_add(gas) > target {
            // Carry this block's own gas and parent into the next chunk, including when a
            // single block exceeds the target. Never try to execute an empty chunk.
            Some(std::mem::replace(
                self,
                Self::new(parent_header, block, gas),
            ))
        } else {
            self.blocks.push(block);
            self.gas_used += gas;
            None
        }
    }

    fn into_witness(self, version: Version) -> ChunkWitness {
        ChunkWitness::new_scroll(
            version.as_version_byte(),
            &self.blocks,
            B256::ZERO,
            version.fork,
            self.parent_header,
        )
    }
}

async fn parent_for_block(
    provider: &impl Provider<Network>,
    child: &Header,
    cached_parent: Option<Header>,
) -> eyre::Result<Header> {
    let parent = match cached_parent {
        Some(parent) => parent,
        None => {
            provider
                .get_block_by_hash(child.parent_hash)
                .await?
                .with_context(|| {
                    format!(
                        "parent {} of block {} not found",
                        child.parent_hash, child.number
                    )
                })?
                .header
                .inner
        }
    };
    eyre::ensure!(
        parent.number.checked_add(1) == Some(child.number),
        "parent block {} does not precede block {}",
        parent.number,
        child.number
    );
    eyre::ensure!(
        parent.hash_slow() == child.parent_hash,
        "parent hash mismatch for block {}; the scanned chain may have reorganized",
        child.number
    );
    Ok(parent)
}

async fn fetcher(
    tx: tokio::sync::mpsc::Sender<(Header, BlockWitness)>,
    provider: impl Provider<Network>,
    start_block: u64,
) -> eyre::Result<()> {
    eyre::ensure!(
        start_block > 0,
        "cannot scan genesis: block 0 has no parent"
    );
    let mut block_number = start_block;
    let mut previous_header = None;
    loop {
        let Some(block) = provider
            .get_block_by_number(block_number.into())
            .full()
            .await?
        else {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        };
        let parent =
            parent_for_block(&provider, &block.header.inner, previous_header.take()).await?;
        let witness = provider
            .dump_block_witness(block_number)
            .with_cached_block(block)
            .with_prev_state_root(parent.state_root)
            .send()
            .await?
            .with_context(|| format!("witness for block {block_number} not found"))?;
        previous_header = Some(witness.header.clone());
        tx.send((parent, witness)).await?;
        block_number = block_number
            .checked_add(1)
            .context("block number overflowed")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_transport::mock::Asserter;
    use scroll_zkvm_integration::testers::chunk::{read_block_witness, read_parent_header};
    use scroll_zkvm_types::scroll::chunk::execute;
    use std::path::Path;

    fn fixture(number: u64) -> BlockWitness {
        read_block_witness(
            Path::new("testdata/tsuki-base-fee/witnesses").join(format!("{number}.json")),
        )
        .unwrap()
    }

    fn genesis() -> Header {
        read_parent_header(
            Path::new("testdata/tsuki-base-fee/witnesses"),
            1,
            Version::tsuki().fork,
        )
        .unwrap()
    }

    #[test]
    fn chunk_boundaries_keep_the_correct_parent_and_incoming_gas() {
        let version = Version::tsuki();
        let first = fixture(1);
        let mut pending = PendingChunk::new(genesis(), first, 1);
        let mut completed = Vec::new();
        for number in 2..=7 {
            let block = fixture(number);
            let parent = fixture(number - 1).header;
            execute(ChunkWitness::new_scroll(
                version.as_version_byte(),
                slice::from_ref(&block),
                B256::ZERO,
                version.fork,
                parent.clone(),
            ))
            .expect("single-block scan has its own parent");
            if let Some(chunk) = pending.push(parent, block, 1, 3) {
                assert_eq!(pending.gas_used, 1);
                completed.push(chunk);
            }
        }
        completed.push(pending);
        let mut previous_hash = genesis().hash_slow();
        for (chunk, (start, end)) in completed.into_iter().zip([(1, 3), (4, 6), (7, 7)]) {
            assert_eq!(chunk.blocks.first().unwrap().header.number, start);
            assert_eq!(chunk.blocks.last().unwrap().header.number, end);
            assert_eq!(chunk.parent_header.hash_slow(), previous_hash);
            let info = execute(chunk.into_witness(version))
                .expect("scanner chunk must pass SBV validation");
            assert_eq!(info.prev_blockhash, previous_hash);
            previous_hash = info.post_blockhash;
        }
        assert_eq!(previous_hash, fixture(7).header.hash_slow());
    }

    #[test]
    fn oversized_blocks_never_emit_an_empty_chunk() {
        let mut pending = PendingChunk::new(genesis(), fixture(1), 5);
        let completed = pending.push(fixture(1).header, fixture(2), 2, 3).unwrap();
        assert_eq!(completed.blocks.len(), 1);
        assert_eq!(completed.parent_header.number, 0);
        assert_eq!(pending.parent_header.number, 1);
        assert_eq!(pending.gas_used, 2);
        assert!(pending.push(fixture(2).header, fixture(3), 1, 3).is_none());
        assert_eq!(pending.blocks.len(), 2);
    }

    #[tokio::test]
    async fn rpc_parent_loading_handles_genesis_missing_and_wrong_headers() {
        let responses = Asserter::new();
        let provider =
            ProviderBuilder::<_, _, Network>::default().connect_mocked_client(responses.clone());
        let rpc_parent: serde_json::Value = serde_json::from_slice(
            &std::fs::read("testdata/tsuki-base-fee/witnesses/parent.json").unwrap(),
        )
        .unwrap();
        let child = fixture(1).header;
        responses.push_success(&rpc_parent);
        let parent = parent_for_block(&provider, &child, None).await.unwrap();
        assert_eq!(parent.hash_slow(), child.parent_hash);
        responses.push_success(&serde_json::Value::Null);
        assert!(
            parent_for_block(&provider, &child, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("not found")
        );
        let mut wrong = rpc_parent;
        wrong["stateRoot"] = serde_json::to_value(B256::repeat_byte(9)).unwrap();
        responses.push_success(&wrong);
        assert!(
            parent_for_block(&provider, &child, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("parent hash mismatch")
        );
    }

    #[tokio::test]
    async fn cached_parents_require_continuity_without_another_rpc() {
        let provider =
            ProviderBuilder::<_, _, Network>::default().connect_mocked_client(Asserter::new());
        let child = fixture(2).header;
        let parent = fixture(1).header;
        assert_eq!(
            parent_for_block(&provider, &child, Some(parent.clone()))
                .await
                .unwrap(),
            parent
        );
        let mut reorg_parent = parent;
        reorg_parent.timestamp += 1;
        assert!(
            parent_for_block(&provider, &child, Some(reorg_parent))
                .await
                .unwrap_err()
                .to_string()
                .contains("parent hash mismatch")
        );
        assert!(
            parent_for_block(&provider, &child, Some(genesis()))
                .await
                .unwrap_err()
                .to_string()
                .contains("does not precede")
        );
    }

    #[tokio::test]
    async fn fetcher_errors_are_available_to_the_scanner_after_channel_closure() {
        let responses = Asserter::new();
        responses.push_failure_msg("RPC unavailable");
        let provider = ProviderBuilder::<_, _, Network>::default().connect_mocked_client(responses);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(fetcher(tx, provider, 1));
        assert!(rx.recv().await.is_none());
        assert!(
            task.await
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("RPC unavailable")
        );
        let provider =
            ProviderBuilder::<_, _, Network>::default().connect_mocked_client(Asserter::new());
        let (tx, _) = tokio::sync::mpsc::channel(1);
        assert!(
            fetcher(tx, provider, 0)
                .await
                .unwrap_err()
                .to_string()
                .contains("genesis")
        );
    }
}

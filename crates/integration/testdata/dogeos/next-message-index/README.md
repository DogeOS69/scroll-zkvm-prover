# DogeOS fixture — `next-message-index`

Committed fixture for the DogeOS `next_message_index` overlay
(`dogeos/changes/next-message-index.md`): Scroll's `nextMessageIndex` is threaded
out of the patched stateless-block-verifier into `ChunkInfo` / `BatchInfo` and
committed into the **Tsuki / Scroll@v11** public inputs. Tsuki is DogeOS's
production fork; `ForkName::Tsuki` / `Version::tsuki()` are introduced by PR #9
(`feat/tsuki-hardfork`), which this branch **stacks on**.

## Contents

- `witnesses/{11,12,13}.json` — real DogeOS Tsuki-era L2 block witnesses (sbv
  `BlockWitness` JSON, identical shape to the sibling `galileov2/witnesses/<n>.json`
  fixtures).
- `manifest.json` — the fixture's expected derived values (`fork_name`, `chain_id`,
  `block_start`/`block_end`, `prev_msg_queue_hash`, `expected_next_message_index`)
  plus source provenance.

## What the test asserts

`crates/integration/tests/dogeos_next_message_index.rs` reads the block witnesses,
assembles a `ChunkWitness` for Tsuki (`ChunkWitness::new_scroll(..., ForkName::Tsuki)`),
derives a `ChunkInfo` natively via `metadata_from_chunk_witnesses` (runs the SBV
state-transition — **no guest program, no GPU**), and asserts:

- `chunk_info.chain_id == 6281971` (DogeOS Tsuki-era L2),
- block range `11..=13`,
- `chunk_info.next_message_index == expected_next_message_index` from the manifest.

These blocks contain **no L1 messages**, so `next_message_index` stays `0`. This
is the cheap, non-advancing base case: it proves the field is threaded and
committed for Tsuki. To exercise the *advancing* case (`next_message_index >= 1`)
a deposit-bearing Tsuki dump is required — see "Advancing case" below.

## Provenance

- **Source:** the `blocks[]` array of a Tsuki materializer chunk-witness,
  `coordinator-staging/witnesses/0x0ab254dff.../scroll-chunk-witness-8654153dc2165857b7909a11d6113503e6d320cadc6f22a3aaecee975bc8e3bf.json`,
  from the run `tsuki-definitive-6a13b33f-vast-retry-20260714T011747Z` (2026-07-14).
- **L2:** DogeOS Tsuki-era L2, `chain_id = 6281971`, blocks `11`, `12`, `13`
  (`prev_msg_queue_hash = 0x0`, no L1 messages).
- **SBV rev:** the source run used sbv `ec6059bd`, which is exactly the rev PR #9
  pins (`Cargo.toml` `[patch]`) — so there is no format drift. The test's
  deserialization step confirms compatibility on the pinned rev.
- **Witness SHA-256:** recorded in `manifest.json` (`provenance.witness_sha256`).

## Regeneration

1. Obtain a Tsuki materializer chunk-witness JSON (from a real-proving run or a
   fresh materializer dump against sbv `ec6059bd`).
2. Extract the per-block witnesses from its `blocks[]` array into
   `witnesses/<block_number>.json` (one object per block, block-number naming to
   match the sibling forks).
3. Recompute the expected value and update `manifest.json`:

   ```
   cargo test -p scroll-zkvm-integration --test dogeos_next_message_index -- --nocapture
   ```

   The test prints the derived line
   `dogeos_next_message_index: chain_id=… block_range=… next_message_index=…`;
   set `manifest.json`'s `expected_next_message_index` (and block range / chain id)
   to the derived values, then re-run to confirm the assertions pass.

## Advancing case (optional, heavier)

The committed blocks do not advance `next_message_index`. Exercising the advancing
case needs a fresh deposit-bearing Tsuki L2 run (>= 1 L1 message / deposit) dumped
against sbv `ec6059bd`. Add it as a second fixture directory (e.g.
`next-message-index-advancing/`) with its own `manifest.json` rather than mutating
this base case.

# Validium witness parents

Every chunk constructor requires the native header immediately before its first
block, including validium chunks. The loader searches beside the block witnesses,
in this order:

1. `<previous_block>.json`, a native SBV block witness.
2. `<first_block>.parent.json`, an RPC block/header object for a sparse corpus.
3. `parent.json`, an RPC block/header object for the start of a contiguous corpus.

The header's number must match the preceding height. SBV checks its hash and state
root against the first block during execution. A missing or mismatched header is
an error; the loader does not synthesize a parent from a hash or state root.

Run `dump-validium-block.sh <block_number>` with `SBV_CLI` and `RPC_URL` set, from
the directory where witnesses should be written. It exports the native witness,
`<block_number>.parent.json`, and `<block_number>_validium_txs.json`. The parent is
fetched by the dumped block's parent hash, so each sparse block retains its own
parent even if the canonical chain changes while exporting.

The isolated-block replay reads files directly from this directory. The range
generator reads from `witnesses/`. Both use the same parent loader. The legacy
committed corpora need regeneration with a current node before full validium
replay; adding parent headers alone does not update their old witness format or
execution rules.

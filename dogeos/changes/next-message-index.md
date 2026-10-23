# Next Message Index Overlay

## Summary

This overlay threads Scroll's `nextMessageIndex` from
`stateless-block-verifier` into chunk and batch public inputs.

## Scope

- Patch all five `sbv-*` crates to the DogeOS SBV fork at one immutable `rev`.
- Extend `ChunkInfo` and `BatchInfo` with `next_message_index`.
- Require `next_message_index` in serialized chunk and batch metadata.
- Append the value only in `tsuki` / `Scroll@v11` chunk and batch public input encodings.
- Keep Scroll v10 / GalileoV2 public inputs compatible with upstream Scroll.
- Leave older PI versions, Validium PI versions, and bundle behavior unchanged.

## Why this stays maintainable

- The SBV dependency change is centralized in one `[patch]` block.
- The feature is threaded through the smallest possible set of types/builders.
- DogeOS-only notes live under `dogeos/`, away from upstream-owned paths.

## Upkeep notes

- If the SBV overlay commit changes, update the pinned `rev` and the matching
  `Cargo.lock` source entries together.
- During local development before that SBV commit is pushed, use temporary CLI
  `cargo --config` path overrides instead of a checked-in `.cargo/config.toml`.
- If upstream adopts this field, remove the DogeOS SBV patch and keep only the
  minimal upstream-compatible wiring that remains necessary.

## Future test coverage

- Add DogeOS-only fixtures under
  `crates/integration/testdata/dogeos/next-message-index/`.
- If proof or task JSON is regenerated for this feature, store it under a
  `dogeos/` subdirectory inside the relevant crate-local `testdata/` tree.

## Tsuki chunk header continuity

Scroll@v11 chunk PI now appends `prev_blockhash || post_blockhash` after the
block contexts. Its length is `281 + 52 * number_of_blocks` bytes. Both hashes
are covered by the PI digest; older Scroll and Validium encodings are unchanged.

The chunk executor applies the shared DogeOS standalone and parent-header
validators. During aggregation, adjacent Tsuki chunks must also have contiguous
block numbers and block hashes, nondecreasing timestamps, and a gas-limit change
accepted by the same DogeOS consensus helper. State-root and message-queue
continuity checks continue to apply.

These boundary checks apply to chunks aggregated in one batch. Batch PI does not
carry boundary block contexts or block hashes, so this change does not add the
same checks across batches. It also does not replace body or post-execution
validation of transaction roots, receipt roots, logs bloom, or gas used.

Changing chunk PI changes the chunk program and its dependent batch/bundle
commitments. Rebuild all guest assets before producing proofs, and do not reuse
proofs or executable commitments generated with the previous Tsuki chunk layout.

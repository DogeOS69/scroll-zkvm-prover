# Next Message Index Overlay

## Summary

This overlay threads Scroll's `nextMessageIndex` from
`stateless-block-verifier` into chunk and batch public inputs.

## Scope

- Patch all five `sbv-*` crates to the DogeOS SBV fork at one immutable `rev`.
- Extend `ChunkInfo` and `BatchInfo` with `next_message_index`.
- Add `#[serde(default)]` so older JSON assets still deserialize during the
  transition.
- Append the value only in `galileo_v2` / `da-codec@v10` public input
  encodings.
- Leave older PI versions and bundle behavior unchanged.

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

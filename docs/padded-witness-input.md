# Padded witness input

The chunk guest accepts the versioned `SBVPAD01` envelope in addition to ordinary
bincode. It borrows immutable code/state bytes from retained guest input, preventing
full payload copies during deserialization, REVM legacy padding, and OpenVM Keccak
alignment. Normal host decoding remains owning. REVM source and versions are unchanged.

## Encoding and validation

`PaddedChunkWitness::encode_to_writer(&witness, &mut output)` writes:

1. Eight-byte magic `SBVPAD01`, little-endian u64 metadata length, and ordinary
   bincode chunk metadata with empty per-block code/state arrays.
2. Zero bytes up to a four-byte boundary.
3. For each metadata block, a little-endian u32 state count; each state has a u32
   original length, raw bytes, and zero alignment padding.
4. A u32 code count; each code has a u32 original length, raw bytes, 33 zero bytes,
   and zero alignment padding.

The decoder checks the actual input address and every boundary, checked length
arithmetic, all zero suffixes, empty metadata payload arrays, and exact consumption.
Every payload begins at a four-byte-aligned address. Padding is kept in validated
SBV `PaddedCode` sidecars, while logical `BlockWitness.codes` retains original slices.
Only original code bytes determine authenticated code hashes and execution-visible
lengths. EIP-7702 uses its checked constructor with original bytes.

The guest computes jump destinations through existing REVM APIs. Its code payload
is retained through analysis. Jump-table cloning still copies an owned bitmap, and
metadata/trie/execution temporaries still allocate. This is payload reuse, not an
allocation-free execution path or a universal capacity guarantee.

## Host and guest lifetimes

`deserialize_from_static` requires a real static input lifetime. The guest explicitly
leaks its owned input buffer, matching OpenVM's non-reclaiming allocator. No unsafe
lifetime extension is used. `deserialize_from_slice` instead copies the envelope into
owned shared bytes so callers may mutate or release their original host input.
`Bytes` slices and clones share the retained payload.

## Local execution

Build the example with `cargo build -p scroll-zkvm-types-chunk --features scroll,host
--example prepare_padded_input`. It converts a BlockWitness JSON file to OpenVM CLI
input without buffering the hex-encoded output:

```sh
cargo run -p scroll-zkvm-types-chunk --features scroll,host \
  --example prepare_padded_input -- witness.json input.json
```

The optional `--extra-code-corpus` argument appends complete 24 KiB blobs for offline
memory tests. That mode is reported as an augmented witness, not a canonical Engine
export. Keep large inputs and generated code outside Git.

For the guest, use the repository's pinned OpenVM CLI and compiler. The optional
`witness-memory-diagnostics` feature checks borrowing, shared clone pointers and
alignment, and records deserialization/validation progress. The example emits the
CLI's input framing byte in addition to the padded envelope; it is not included in
`PaddedChunkWitness::encode_to_writer` output passed to normal proving tasks.

Tests: `cargo test -p scroll-zkvm-types-chunk --features scroll,host`. Tests cover
logical round trips, static/owned lifetimes, all alignment residues, delegation, and
malformed metadata/record lengths and padding. A local Engine block consuming
29,999,112 gas across two cap-compliant transactions completed full OpenVM 1.7
execution with about 267 MiB code and 269 MiB input. This was execution, not proving.

## Rollout requirements

The encoder is explicit. `ChunkWitness::archive` in the integration harness and the
DogeOS worker's chunk-task producer continue to emit ordinary bincode. Migrating
either requires using the encoder above with matching new guest assets and keys.
Universal task forwarding is format-agnostic and must not reinterpret batch or
bundle inputs as chunk witnesses.

Before enabling padded input for production, migrate the producer, rebuild the
complete guest asset/commitment chain through `make build-guest`, and qualify the
result through the repository's integration targets. A native chunk execution does
not qualify production commitments or establish successful STARK/SNARK generation.

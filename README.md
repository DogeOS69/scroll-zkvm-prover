# Scroll zkVM

**zkVM-based Circuits (Guest Programs) with a complete Scroll Prover implementation**

## Repository

This repository contains the following member crates:

- [scroll-zkvm-types](./crates/circuits/types): Primitive and Common types used in project and being exported. It is an aggregation of a series of crates:
  + [scroll-zkvm-types-base](./crates/circuits/types/base): Common types which is used project-wide and expected to be recognized beyond project
  + [scroll-zkvm-types-base](./crates/circuits/types/chunk): Like the base crate, but in the project, these types are only related to chunk circuit
  + [scroll-zkvm-types-base](./crates/circuits/types/batch): Like the base crate, but in the project, these types are only related to batch circuit
  + [scroll-zkvm-types-base](./crates/circuits/types/bundle): Like the base crate, but in the project, these types are only related to bundle circuit
- [scroll-zkvm-chunk-circuit](./crates/circuits/chunk-circuit): Circuit for verification of a Scroll [chunk](TODO:doc)
- [scroll-zkvm-batch-circuit](./crates/circuits/batch-circuit): Circuit for verification of a Scroll [batch](TODO:doc)
- [scroll-zkvm-bundle-circuit](./crates/circuits/bundle-circuit): Circuit for verification of a Scroll [bundle](TODO:doc)
- [scroll-zkvm-prover](./crates/prover): Implementation for a Scroll Prover
- [scroll-zkvm-verifier](./crates/verifier): Implementation for a Verifier-only mode
- [scroll-zkvm-integration](./crates/integration): Integration tests for the Scroll Prover

## Overview

The Scroll zkVM Circuits are [openvm](https://book.openvm.dev/) based Guest Programs.

The [prover](./crates/prover) crate offers a minimalistic API for setting up, generating and verifying proofs for Scroll's zk-rollup.

For a deeper dive into our implementation, please refer the [interfaces](./docs/interfaces.md) doc.

## Testing

For more commands please refer the [Makefile](./Makefile).

### Build Guest Programs

In case you have made any changes to the guest programs, it is important to build them before running the tests.

```shell
$ make build-guest
```

Upon building the guest programs, the child commitments in [batch-circuit](./crates/circuits/batch-circuit/src/child_commitments.rs) and [bundle-circuit](./crates/circuits/bundle-circuit/src/child_commitments.rs) will be overwritten by `build-guest`.

### End-to-end tests for chunk-prover

```shell
$ make test-single-chunk
```

### End-to-end tests for batch-prover

```shell
$ make test-e2e-batch
```

### End-to-end tests for bundle-prover

```shell
$ make test-e2e-bundle
```

*Note*: Configure `RUST_LOG=debug` for debug logs or `RUST_LOG=none,scroll_zkvm_prover=debug` for logs specifically from the `scroll-zkvm-prover` crate.

## Release of prover circuits

All apps of circuits are uploaded into aws s3 storage, and can be download via following urls:

`<s3 base url>/scroll-zkvm/releases/<fork name>/<chunk|batch|bundle>/<vk>`

+ Current the url for s3 storage is `https://circuit-release.s3.us-west-2.amazonaws.com`
+ The fork name can be read via [release-fork](./release-fork) file
+ The circuit app has to be accessed by specifying its proof type (chunk/batch/bundle) and the vk of the circuit.

## Usage of Prover API

### Dependency

Add the following dependency in your `Cargo.toml`:

```toml
[dependencies]
scroll-zkvm-prover = { git = "https://github.com/scroll-tech/zkvm-prover", branch = "master" }
```

### To prove a universal task with STARK proofs

Prover capable of generating STARK proofs for a Scroll [universal task](TODO:doc):

```rust
use std::path::Path;

use scroll_zkvm_prover::{
    Prover,
    task::ProvingTask,
};
use scroll_zkvm_types::{
    public_inputs::ForkName,
    chunk::ChunkWitness,
    task::ProvingTask as UniversalProvingTask,
};

// Paths to the application exe and application config.
let path_exe = Path::new("./path/to/app.vmexe");
let path_app_config = Path::new("./path/to/openvm.toml");

// Optional directory to cache generated proofs on disk.
let cache_dir = Path::new("./path/to/cache/proofs");

let config = scroll_zkvm_prover::ProverConfig {
    path_app_exe,
    path_app_config,
    dir_cache: Some(cache_dir),
    ..Default::default()
};
// Setup prover.
let prover = Prover::setup(config, false, None)?;

let vk = prover.get_app_vk();
let task : UniversalProvingTask = /* a universal task, commonly generated and assigned by coordinator */

// Generate a proof.
let proof = prover.gen_proof_universal(&task, false)?;

// Verify proof.
let verifier = prover.dump_universal_verifier(None::<String>);
assert!(verifier.verify_proof(proof.as_root_proof().expect("should be root proof"), &vk)?);
```

### To prove a universal task with SNARK proofs

Prover capable of generating SNARK proofs aggregating the root proof for a Scroll [universal task](TODO:doc):

```rust
use std::path::Path;

use scroll_zkvm_prover::{
    Prover,
    task::ProvingTask,
};
use scroll_zkvm_types::{
    public_inputs::ForkName,
    chunk::ChunkWitness,
    task::ProvingTask as UniversalProvingTask,
};

// Paths to the application exe and application config.
let path_exe = Path::new("./path/to/app.vmexe");
let path_app_config = Path::new("./path/to/openvm.toml");

// Optional directory to cache generated proofs on disk.
let cache_dir = Path::new("./path/to/cache/proofs");

let config = scroll_zkvm_prover::ProverConfig {
    path_app_exe,
    path_app_config,
    dir_cache: Some(cache_dir),
    ..Default::default()
};
// Setup prover capable to generate SNARK proof.
let prover = Prover::setup(config, true, None)?;

let vk = prover.get_app_vk();
let task : UniversalProvingTask = /* a universal task, commonly generated and assigned by coordinator */

// Generate a SNARK proof.
let proof = prover.gen_proof_universal(&task, true)?;

// Verify proof.
let verifier = prover.dump_universal_verifier(None::<String>);
assert!(verifier.verify_proof_evm(&proof.clone().into_evm_proof().expect("should be evm proof").into(), &vk)?);
```

### Form a universal task for a chunk from block witnesses

The padded input encoder retains four-byte alignment for code and trie payloads and
supplies zero padding for legacy bytecode analysis. Use it only with a chunk guest
built with `SBVPAD01` support and the matching proving key:

```rust
use sbv_core::BlockWitness;
use sbv_primitives::B256;
use scroll_zkvm_types::{
    scroll::chunk::{ChunkWitness, PaddedChunkWitness},
    task::ProvingTask as UniversalProvingTask,
    version::Version,
};

let block_witnesses: Vec<BlockWitness> = /* exported block witnesses */;
let version = Version::tsuki();
let witness = ChunkWitness::new(
    version.as_version_byte(),
    &block_witnesses,
    B256::ZERO,
    version.fork,
    None,
);
let mut serialized_witness = Vec::new();
PaddedChunkWitness::encode_to_writer(&witness, &mut serialized_witness)?;
let task = UniversalProvingTask {
    serialized_witness: vec![serialized_witness],
    aggregated_proofs: Vec::new(),
    fork_name: witness.fork_name.clone(),
    vk: prover.get_app_vk(),
    identifier: String::new(),
};
```

Ordinary bincode inputs remain accepted by the updated guest. Existing integration
and external worker producers still use bincode until explicitly migrated; they do
not gain padding/alignment automatically. Older guest executables cannot decode the
new envelope. See [the padded input guide](docs/padded-witness-input.md) for format,
validation, reproduction, and rollout requirements.

#!/bin/bash

set -e

# Alas, the openvm CLI does not offer `clippy` as a subcommand,
# so we have to use `cargo` directly.  The options are copy-and-pasted from
# `openvm`'s `build` command.
export CARGO_ENCODED_RUSTFLAGS=$'-C\x1fpasses=lower-atomic\x1f-C\x1flink-arg=-Ttext=0x00200800\x1f-C\x1flink-arg=--fatal-warnings\x1f-C\x1fpanic=abort\x1f--cfg\x1fgetrandom_backend="custom"'
OPENVM_RUST_TOOLCHAIN=${OPENVM_RUST_TOOLCHAIN:-nightly-2026-03-17}
clippycmd="cargo +${OPENVM_RUST_TOOLCHAIN} clippy \
  --target riscv32im-risc0-zkvm-elf \
  -Z build-std=alloc,core,proc_macro,panic_abort,std \
  -Z build-std-features=compiler-builtins-mem \
  --all-features \
  -- -D warnings"

cd crates/circuits/chunk-circuit; eval "$clippycmd"; cd ./../../..
cd crates/circuits/batch-circuit; eval "$clippycmd"; cd ./../../..
cd crates/circuits/bundle-circuit; eval "$clippycmd"; cd ./../../..

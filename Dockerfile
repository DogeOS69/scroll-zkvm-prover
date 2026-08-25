FROM rust:1.90

WORKDIR /app

ARG OPENVM_RUST_TOOLCHAIN=nightly-2026-03-17
ENV OPENVM_RUST_TOOLCHAIN=${OPENVM_RUST_TOOLCHAIN}

RUN rustup toolchain install "${OPENVM_RUST_TOOLCHAIN}" --profile minimal \
    --component clippy --component llvm-tools \
    --component rustc-dev --component rustfmt --component rust-src \
    --target riscv32im-unknown-none-elf

RUN wget https://github.com/ethereum/solc-bin/raw/refs/heads/gh-pages/linux-amd64/solc-linux-amd64-v0.8.19+commit.7dd6d404 -O /usr/local/bin/solc && \
    chmod +x /usr/local/bin/solc

COPY . .

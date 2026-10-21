FROM rust:1.86

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates=20250419~deb12u2 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

ARG OPENVM_RUST_TOOLCHAIN=nightly-2026-03-17
ENV OPENVM_RUST_TOOLCHAIN=${OPENVM_RUST_TOOLCHAIN}

RUN rustup toolchain install "${OPENVM_RUST_TOOLCHAIN}" --profile minimal \
    --component clippy --component llvm-tools \
    --component rustc-dev --component rustfmt

RUN wget https://github.com/ethereum/solc-bin/raw/refs/heads/gh-pages/linux-amd64/solc-linux-amd64-v0.8.19+commit.7dd6d404 -O /usr/local/bin/solc && \
    chmod +x /usr/local/bin/solc

COPY . .

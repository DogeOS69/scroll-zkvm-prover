# Tsuki development-chain proving corpus

Imported byte-for-byte from SBV `50ef41efc21b30d85a6425fe61aa19d59eb918c3`, `testdata/dogeos/base-fee-rule/development-chain/`. `provenance.json` retains the original capture identity and `SHA256SUMS` pins the imported bytes.

The four-block chain uses registered development chain ID 1337 and TsukiV2 at zero, with no migration checkpoint. It covers transfers, a block above the gas target, an L1 message and an empty block. Default Tsuki integration/proving tasks split it into blocks 1–2 and 3–4. This corpus is compatible with the standard guest configuration; it is not a mainnet or testnet-cutover qualification.

The historical `tsuki-base-fee` corpus uses Chikyu ID with a synthetic activation at zero. Native unit tests replay it with explicit trusted test configuration, but the standard guest rejects Chikyu until its activation/checkpoint are approved. Do not use those historical fixtures to qualify new deployment proofs.

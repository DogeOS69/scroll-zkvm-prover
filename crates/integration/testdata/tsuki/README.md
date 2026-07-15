# Tsuki integration witnesses

This directory contains a continuous, genuine `BlockWitness` lineage from a local
DogeOS ScrollReth node running Tsuki from genesis. The integration suite combines
the 26 blocks into four child chunks (`1..=8`, `9..=16`, `17..=20`, and
`21..=26`) and then aggregates all four chunks into one v11 batch.

## Provenance

- chain ID: `6281971` (`0x5fdaf3`)
- fork / DA codec: Tsuki / v11
- block range: `1..=26`
- genesis block hash: `0x02fb776ea819e34dd072969300eca4e32820b9a3b4c2cecd2473eef69f24cdd7`
- block 26 hash: `0xbddf37d8e9c966d15a7925cdc9bd58f9c23208aaa0814e6720f180556c907f0c`
- initial state root: `0x8938aed386448da2e825974f29a8f14a862bfa9f94973a8cea261542ff8792a1`
- final state root: `0x54097ced498c20c61c9817f44dae4a4cb197c818810aa5a9717c67814f3925f6`
- rollup-node image: `dogeos69/rollup-node:tsuki-5bce327d-reth-39b31f82`
- image digest: `sha256:ed13066066e22bd5c220827b678a6cf59858fbd0d44463f9fcd3ccd97ec76e5b`
- embedded `reth-scroll-cli`: `39b31f822cc2b4c54db32ba2f0484ca2a157c3f5`
- SBV revision used to dump and replay: `ec6059bdc48fb60d8340ba86b32bbe8d41111cd4`

The node database came from dogeos-core's
`real_scroll_reth_withdrawal_readiness` harness. It was copied before reopening;
the retained source database was not modified. No public Chikyū Tsuki node was
available when these fixtures were collected.

## Coverage

The lineage exercises the Tsuki execution and proof path with:

- protocol and harness deployment/setup transactions in blocks 1–15;
- contract deployment in block 16 and contract calls in blocks 17 and 19;
- ordinary precompile calls in blocks 18–20;
- the first withdrawal queue transition in block 19, where authenticated
  `L2MessageQueue.nextMessageIndex` changes from `0` to `1`;
- the Tsuki NativeDogeToken account/proof material required by replay;
- NativeDogeToken transfer success and insufficient-balance revert;
- RIPEMD-160 success at the 32-byte limit and revert at 33 bytes;
- unauthorized direct access to the native transfer precompile (`0xfd`);
- an ordinary L2 transaction accepted at the exact EIP-7825 gas cap;
- state-root, block-number, and message-queue-hash continuity across four
  independently proven child chunks;
- v11 chunk and batch public inputs, including the committed
  `next_message_index`; and
- blob construction, point-evaluation verification, and multi-child batch
  aggregation.

The focused edge blocks are:

| Block | Transaction | Expected receipt |
| --- | --- | --- |
| 21 | `NativeDogeToken.transfer(..., 12345)` | success |
| 22 | RIPEMD-160 with exactly 32 input bytes | success |
| 23 | RIPEMD-160 with 33 input bytes | revert |
| 24 | unauthorized direct call to precompile `0xfd` | revert |
| 25 | `NativeDogeToken.transfer` above the sender balance | revert |
| 26 | ordinary L2 transaction with gas limit `16777216` | success |

SBV re-execution checks the resulting receipt and state roots. The integration
test additionally pins the target, calldata boundary, and transaction gas limit
for these named cases so a semantically different witness cannot silently
replace one of them.

Rule-rejection cases that cannot occur in a canonical block (for example an
ordinary L2 transaction above the EIP-7825 gas cap) belong in host/node rejection
tests rather than in successful block witnesses. The focused per-rule corpus and
pre-fork boundary vectors are tracked separately by DOG-419; this directory is
the canonical multi-block/multi-chunk proving corpus for DOG-424.

## Reproduction

With the retained node exposed locally as `$RPC`:

```bash
# Submit one transaction per content-driven block. Every command uses an
# explicit gas limit so `eth_estimateGas` cannot erase intentional reverts.
TOKEN=0x530000000000000000000000000000000000d09e
RECIPIENT=0x000000000000000000000000000000000000bEEF

cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 100000 "$TOKEN" 'transfer(address,uint256)(bool)' "$RECIPIENT" 12345

RIPEMD32="0x$(printf '11%.0s' $(seq 1 32))"
RIPEMD33="0x$(printf '22%.0s' $(seq 1 33))"
cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 100000 0x0000000000000000000000000000000000000003 "$RIPEMD32"
cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 100000 0x0000000000000000000000000000000000000003 "$RIPEMD33"

TRANSFER_DATA=$(cast abi-encode 'f(address,address,uint256)' \
  0xded06046416d6ba20c1e2bad51b3a3e2f267d33f "$RECIPIENT" 1)
cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 100000 0x00000000000000000000000000000000000000fd "$TRANSFER_DATA"

cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 100000 "$TOKEN" 'transfer(address,uint256)(bool)' \
  "$RECIPIENT" 1000000000000000000000000000000

cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 16777216 0x0000000000000000000000000000000000000001 0x

cargo run -p sbv-cli --features scroll -- dump \
  --rpc "$RPC" --block 1..27 --out-dir /tmp/tsuki-witnesses

cargo run -p sbv-cli --features scroll -- run --hardfork tsuki \
  /tmp/tsuki-witnesses/*.json

shasum -a 256 /tmp/tsuki-witnesses/*.json
```

The complementary EIP-7825 admission check was run against the same disposable
node. The exact-cap transaction became block 26; the `+1` transaction failed at
RPC admission with `-32003: gas limit too high` and therefore correctly has no
block witness:

```bash
cast send --rpc-url "$RPC" --private-key "$DEPLOYER_PRIVATE_KEY" \
  --gas-limit 16777217 --async \
  0x0000000000000000000000000000000000000001 0x
```

`sbv-cli` ranges are end-exclusive, hence `1..27`. JSON map ordering can vary
between otherwise equivalent dumps, so `SHA256SUMS` pins the exact checked-in
bytes while the integration tests pin the consensus roots and public-input
hashes.

## OpenVM proof identity

The compatible OpenVM 1.7 guests were rebuilt from a clean checkout of
`b38e0dc440be76ce6ad75080b1524429a758adae` on Linux with `solc 0.8.19` and
`nightly-2025-08-18`:

```bash
export OPENVM_RUST_TOOLCHAIN=nightly-2025-08-18
export RUST_MIN_STACK=16777216

BUILD_PROJECT=chunk,batch \
RUSTC="$(rustup which --toolchain nightly-2025-08-18 rustc)" \
cargo +nightly-2025-08-18 run --release -p scroll-zkvm-build-guest -- \
  --mode force --output dogeos-openvm17-tsuki-dog424
```

The resulting identities are:

- chunk `app.vmexe` SHA-256:
  `f9d6882e2b82d42526527ab9981aead8a89029d10dc933c00b3299b4e4ec43cc`;
- batch `app.vmexe` SHA-256:
  `108c3fb3643f981b43c2653cbd42581f5e60a3b3f0676d107db2e9e9f7ff8e17`;
- `root_verifier_vk` SHA-256:
  `593e2fbf17d68f4f4331592492ec7c7965aee7b057840ad1647994b784a030e5`;
- chunk program commitment:
  `8bdace252c289768d5ca0a48f531621b95e2b6130afa7c4dbfbd9c2f75f779213bfd2f7624b51c513cb6873df01bfa673e3e2d24205a3868016c6464ef8a694b`;
- batch program commitment:
  `01ae5d1707fdf01222e13c2830da7d33554bf90f95e1064fd4260645715b395b2990880f5a2046743a5d425c40bb726a5262c65b68fdf72900d74c70fc3c945f`.

These hashes identify the external artifacts used for this proving run. This
fixture-only change does not replace the repository's canonical generated
commitment files: changing that release identity requires the containerized
`make build-guest` path and a complete chunk, batch, and bundle rebuild.

With that release staged under `releases/dogeos-openvm17-tsuki-dog424`, the
following Make targets execute the four chunks, generate and verify all four
child STARK proofs, and recursively verify them in the batch guest:

```bash
make test-tsuki-golden
GUEST_VERSION=dogeos-openvm17-tsuki-dog424 make test-execute-chunk-multi
GUEST_VERSION=dogeos-openvm17-tsuki-dog424 make test-execute-batch
GUEST_VERSION=dogeos-openvm17-tsuki-dog424 make test-e2e-batch
```

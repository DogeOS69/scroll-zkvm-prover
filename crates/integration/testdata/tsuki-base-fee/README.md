# Tsuki base-fee corpus

Seven blocks of a short Tsuki chain, built by a node that enforces the base-fee rule at import and
writes the L2SystemConfig overhead slot into its execution witnesses. It is the corpus the chunk
path's positive Tsuki tests run on, because it is the first one the guest can verify: the overhead
rule makes a chunk unprovable unless its witnesses resolve that slot.

| Blocks | What they cover |
| --- | --- |
| 1 | ordinary transfers |
| 2 | past the 3M gas target, so the base fee rises for block 3 |
| 3 | an L1 message |
| 4 | the transaction that sets the L2 base-fee overhead at slot 101 |
| 5–6 | the first blocks judged on the new overhead |
| 7 | an empty block |

The L2SystemConfig contract is a genesis predeploy with its slot unset, so blocks 1–4 are judged
on the default overhead and 5–7 on the configured one. `witnesses/parent.json` is the chain's
genesis header: the first chunk is anchored to it, so its hash is the chain's first proof endpoint.

`witnesses/parent.json` and `witnesses/N.json` are written differently, and regenerating either by
hand is where that bites: `parent.json` is an `eth_getBlockByNumber` block object (camelCase, with
`hash`, `size`, `transactions` and `uncles` alongside the header fields), while a witness carries
its header in the struct's own field names. Copying a witness's `header` object into `parent.json`
does not parse. `read_parent_header` reads the two with different deserializers for that reason.

`provenance.json` records the node commit, the sbv commit the corpus was taken from, the chain id
and the overhead values; `SHA256SUMS` pins the bytes and `make test-tsuki-golden` checks it. The corpus and
its reproduction live in the stateless-block-verifier repository, under
`testdata/dogeos/base-fee-rule/`, which also carries the two sibling chains covering an absent
L2SystemConfig account and the development chain.

## Why not the 26-block corpus

`../tsuki/` is the canonical multi-block proving corpus and stays so, but it cannot satisfy the
base-fee rule: its witnesses resolve no L2SystemConfig account, and the node that produced them
predates the Tsuki EIP-1559 parameters. Regenerating it needs the harness that built it, with the
NativeDogeToken and withdrawal coverage its own README documents; until then its golden tests are
`#[ignore]`d and this corpus carries the chunk path's positive Tsuki coverage.

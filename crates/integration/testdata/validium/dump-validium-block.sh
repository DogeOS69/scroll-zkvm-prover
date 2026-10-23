#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${SBV_CLI:-}" ]]; then
    echo "Error: SBV_CLI is not set" >&2
    exit 1
fi
echo Using sbv cli from $SBV_CLI

RPC_URL=${RPC_URL:-"http://10.6.11.77:38545"}
echo "Using RPC URL: $RPC_URL"

if [[ -z "${1:-}" ]]; then
    echo "Usage: $0 <block_number>" >&2
    exit 1
fi

BLOCK_NUMBER=$1
HEX_BLOCK_NUMBER=$(printf "0x%x" "$BLOCK_NUMBER")
echo "Dumping block number: $BLOCK_NUMBER ($HEX_BLOCK_NUMBER)"

"$SBV_CLI" dump --rpc "$RPC_URL" --block "$BLOCK_NUMBER"

# Fetch by the dumped witness's parent hash so a reorg cannot select a different parent at
# the same height. Each sparse test block carries its own RPC-format parent header.
PARENT_HASH=$(jq -er '.header.parent_hash' "${BLOCK_NUMBER}.json")
cast rpc --rpc-url "$RPC_URL" eth_getBlockByHash "$PARENT_HASH" false \
    | jq -e 'if . == null then error("parent block not found") else . end' \
    > "${BLOCK_NUMBER}.parent.json"

cast rpc --rpc-url "$RPC_URL" scroll_getL1MessagesInBlock "$HEX_BLOCK_NUMBER" "synced" | jq > "${BLOCK_NUMBER}_validium_txs.json"

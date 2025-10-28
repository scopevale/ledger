#!/usr/bin/env zsh
set -euo pipefail

LEDGER_URL="${LEDGER_URL:-http://localhost:8080}"
MINE_PATH="${MINE_PATH:-/mine}"
COUNT=$1

for i in {1..$COUNT}; do
  ./scripts/data/add_tx_batch.zsh $(od -An -N2 -i /dev/urandom | awk '{print 1 + ($1 % 20)}') && curl -s "${LEDGER_URL}${MINE_PATH}?target=16" | jq
done

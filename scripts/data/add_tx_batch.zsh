#!/usr/bin/env zsh
set -euo pipefail

LEDGER_URL="${LEDGER_URL:-http://localhost:8080}"
TX_PATH="${TX_PATH:-/tx}"
COUNT=$1

rand_hex() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 20
  else
    hexdump -vn20 -e '20/1 "%02x"' /dev/urandom
  fi
}

get_names() {
  fake -n 2 name | awk 'NR>1 && NR<4 { print $1 }'
}

rand_amount() {
  od -An -N2 -i /dev/urandom | awk '{print 1 + ($1 % 250)}'
}

for i in {1..$COUNT}; do
  names=("${(@f)$(get_names)}")
  from="$names[1]"
  to="$names[2]"
  while [[ "$to" == "$from" ]]; do names=("${(@f)$(get_names)}") to="$names[2]"; done
  amount="$(rand_amount)"

  payload=$(printf '{"from":"%s","to":"%s","amount":%s}' "$from" "$to" "$amount")

  code=$(curl -sS -o /dev/null -w "%{http_code}" \
    -X POST "${LEDGER_URL}${TX_PATH}" \
    -H 'Content-Type: application/json' \
    -d "$payload")

  echo "[$i/$COUNT] POST ${TX_PATH} -> ${code} | amount=${amount}"
  sleep 0.01
done


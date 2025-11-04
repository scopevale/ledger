# LGR Capstone — Custom Decentralised Ledger (First Iteration)

This is the **first iteration** of a minimal, PoW‑based ledger written in Rust.
The goal here is to have a clean, compiling workspace with the absolute basics:
types, hashing, a trivial proof‑of‑work, a sled‑backed store, a tiny `axum`
HTTP node, a CLI to submit transactions, and a Criterion benchmark for PoW.

## Crates

- `ledger-core` — core types (`Transaction`, `BlockHeader`, `Block`) and PoW.
- `ledger-storage` — simple `sled` store with a `Storage` trait.
- `ledger-node` — HTTP API (Axum) exposing `/healthz`, `/chain/head`, `/tx`.
- `ledger-cli` — CLI to submit transactions to the node via `reqwest`.

## UI Enhancements

- Ledger-UI (ledger-tui) adds a mempool popup showing details of the selected transaction under the cursor. Toggle with 'p' in the Mempool tab to view From, To, Amount, and Timestamp.

## Build & Run

```bash
# build everything
cargo build

# run the node (listens on 127.0.0.1:8080 by default)
just run-node

# submit a tx
just run-cli tx alice bob 10
```

## Endpoints

- `GET /healthz` → `{ "status": "ok" }`
- `GET /chain/head` → `{ "height": <u64> }`
- `POST /tx` with JSON `{ "from": "...", "to": "...", "amount": 1 }`

## Benchmarks

```bash
cargo bench -p ledger-core
```

## Toolchain

Pinned to stable via `rust-toolchain.toml`. Uses Tokio `1.47.1`, Axum `0.8.6`,
Reqwest `0.12.23`, Sled `0.34.7`, Serde `1.0.228`, Criterion `0.7.0`.

## License

Dual-licensed under MIT or Apache-2.0.

# Changelog
All notable changes to **LGR Capstone — Custom Decentralised Ledger** will be documented in this file.

This project adheres to Semantic Versioning and follows a simplified
“Keep a Changelog” style.

## [0.2.0] — 2025-10-09
### Added
- **Chain façade** in `ledger-core` (`chain` module):
  - `ChainStore` trait (storage interface used by the chain).
  - `Chain<C>` wrapper with `new()`, `ensure_genesis()`, and `tip() -> (height, tip_hash)`.
  - `genesis_block()` helper.
  - Add `mempool` with `/tx` ingestion and `/mine` endpoint to mine pending transactions
  - Expose `/chain/tip` and `/chain/blocks` endpoints; add `SledStore::list_blocks_range`
  - Add `Chain::append_block` with improved error context in `ensure_genesis`

- **Genesis initialization**: `ledger-node` now constructs `Chain` and calls `ensure_genesis()` at startup.
- **/health** endpoint in `ledger-node` (kept `/healthz` for convenience/back-compat).
- **/chain/head** now reads height via `Chain::tip()` (reflects persisted state).

### Changed
- Version bump: `ledger-core`, `ledger-storage`, `ledger-node`, and `ledger-cli` to `0.2.0`.
- Refactored `ledger-storage::SledStore` to **implement `ledger_core::chain::ChainStore`** by delegating to the existing local `Storage` trait.

### Fixed
- **Proof-of-Work leading-zero count**: corrected logic to use byte-level `u8::leading_zeros()` directly and removed the incorrect subtraction; renamed helper to `count_leading_zero_bits()`.
- Removed unused, duplicate leading-zero helper from earlier attempt.

### Notes
- No breaking API changes for consumers of `mine_block()` or the HTTP API. Internals stabilized for future features (mempool, mining, append).

---

## [0.1.0] — 2025-10-08
### Added
- **Workspace scaffolding** with four crates:
  - `ledger-core`: core types (`Transaction`, `BlockHeader` with `previous_hash`, `Block`), `merkle_root()`, and a minimal PoW `mine_block()`.
  - `ledger-storage`: `SledStore` with a simple `Storage` trait (`put_block`, `get_block`, `tip_height`, `tip_hash`).
  - `ledger-node`: Axum HTTP service exposing:
    - `GET /healthz` (basic health check),
    - `GET /chain/head` (returns current height; initial stub),
    - `POST /tx` (accepts tx JSON; no persistence/mining yet).
  - `ledger-cli`: CLI to `submit` a transaction to the node via `reqwest`.
- **Benchmarks**: `benches/pow.rs` using Criterion.
- **Scripts**: `scripts/k6/submit_tx.js` for simple load test (tx submission).
- **Dev tooling**: `Justfile` tasks (`run-node`, `run-cli`, `fmt`, `clippy`, `bench`), `rust-toolchain.toml` (stable + rustfmt + clippy).
- **Pinned dependencies** with full semver (Tokio 1.47.1, Axum 0.8.6, Reqwest 0.12.23, Sled 0.34.7, Serde 1.0.228, etc.).

### Notes
- First runnable skeleton; storage and PoW not yet wired through the node.

set shell := ["bash", "-cu"]

default: build

build:
    cargo build

run-node:
    cargo run -p ledger-node -- --listen 127.0.0.1:8080 --data-dir ./data

run-cli tx sender recipient amount:
    cargo run -p ledger-cli -- submit --node http://127.0.0.1:8080 --from {{sender}} --to {{recipient}} --amount {{amount}}

test:
    cargo test --all

bench:
    cargo bench

fmt:
    cargo fmt --all

clippy:
    cargo clippy --all-targets -- -D warnings

clean:
    cargo clean

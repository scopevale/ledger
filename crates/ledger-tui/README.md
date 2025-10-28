# ledger-tui

A minimal Terminal UI for your **ledger-node**.

## Features

- Dashboard with `/chain/head` and `/chain/tip`
- Chain browser via `/chain/blocks?limit=&dir=&start=`
- Mempool quick TX form posts to `/tx`
- Mining screen calls `/mine?target=&data=`
- Live SHA-256 **hash demo** shows leading-zero bits in real time

## Run

Assuming your node runs at `http://127.0.0.1:3000`:

```bash
cargo run -p ledger-tui -- --node http://127.0.0.1:3000
```

Keys: `Tab` switch tabs, `Enter` submit (TX / Mine), `Esc` quits.

use criterion::{criterion_group, criterion_main, Criterion};
use ledger_core::{pow::mine_block, Block, BlockHeader, Transaction};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::time::{SystemTime, UNIX_EPOCH};

fn bench_pow(c: &mut Criterion) {
    c.bench_function("mine_block_target_20", |b| {
        let mut rng = StdRng::seed_from_u64(42);
        let txs: Vec<Transaction> = (0..10)
            .map(|i| Transaction {
                from: format!("alice-{i}"),
                to: "bob".into(),
                amount: rng.gen_range(1..10),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            })
            .collect();

        let header = BlockHeader::new(0, [0u8; 32], ledger_core::merkle_root(&txs), 0);
        let block = Block { header, txs };

        b.iter(|| {
            let _mined = mine_block(block.clone(), 20);
        });
    });
}

criterion_group!(benches, bench_pow);
criterion_main!(benches);

use axum::{
    extract::Query,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use ledger_core::{chain::Chain, Transaction};
use ledger_storage::sled_store::SledStore;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};

use ledger_core::constants::{BLOCKS_PER_BATCH, HASH_HEX_SIZE, MAX_BLOCKS_PER_REQUEST};

#[derive(Parser, Debug)]
struct Args {
    /// Address to listen on, e.g. 127.0.0.1:8080
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: String,

    /// Data directory for sled
    #[arg(long, default_value = "./data")]
    data_dir: String,
}

#[derive(Clone)]
struct AppState {
    chain: Chain<SledStore>,
    // mempool: Arc<RwLock<Vec<Transaction>>>,
    mempool: Arc<Mutex<Vec<Transaction>>>,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

#[derive(Serialize)]
struct Head {
    height: u64,
}

#[derive(Serialize)]
struct Tip {
    height: u64,
    hash: Option<String>,
}

#[derive(Deserialize)]
struct TxIn {
    from: String,
    to: String,
    amount: u64,
}

#[derive(Deserialize)]
struct MineParams {
    /// Leading zeros required in the hash, default is 20
    target: Option<u32>,
    data: Option<String>,
}
#[derive(Deserialize)]
struct ListParams {
    start: Option<u64>,
    limit: Option<u32>,
    dir: Option<String>,
}

#[derive(Serialize)]
struct BlockRow {
    index: u64,
    ts: u64,
    tx_count: usize,
    hash: String,
    nonce: u64,
    previous_hash: String,
    merkle_root: String,
    data_hash: String,
    data: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let store = Arc::new(SledStore::open(&args.data_dir)?);
    let chain = Chain::new(store.clone());
    chain.ensure_genesis()?;

    let state = AppState {
        chain,
        mempool: Arc::new(Mutex::new(Vec::new())),
    };

    let app = Router::new()
        .route("/health", get(|| async { Json(Health { status: "ok" }) }))
        .route("/healthz", get(|| async { Json(Health { status: "ok" }) }))
        .route(
            "/chain/head",
            get({
                let state = state.clone();
                move || async move {
                    let (height, _hash) = state.chain.tip().unwrap_or((0, None));
                    Json(Head { height })
                }
            }),
        )
        .route(
            "/chain/tip",
            get({
                let state = state.clone();
                move || async move {
                    let (height, hash) = state.chain.tip().unwrap_or((0, None));
                    Json(Tip {
                        height,
                        hash: hash.map(hex::encode),
                    })
                }
            }),
        )
        .route(
            "/tx",
            post({
                let state = state.clone();
                move |Json(tx): Json<TxIn>| {
                    let _state = state.clone();
                    async move {
                        let tx = Transaction {
                            from: tx.from,
                            to: tx.to,
                            amount: tx.amount,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };
                        state.mempool.lock().await.push(tx);
                        Json(serde_json::json!({ "accepted": true}))
                    }
                }
            }),
        )
        .route(
            "/mine",
            get({
                let state = state.clone();
                move |Query(params): Query<MineParams>| {
                    let mut state = state.clone();
                    async move {
                        let target_zeros = params.target.unwrap_or(20);
                        let data = params.data;
                        let txs = {
                            let mut mp = state.mempool.lock().await;
                            if mp.is_empty() {
                                Vec::new()
                            } else {
                                std::mem::take(&mut *mp)
                            }
                        };
                        info!(
                            "/mine endpoint called - mining a new block with {} txs",
                            txs.len()
                        );

                        match state.chain.mine_with_txs_parallel(txs, data, target_zeros) {
                            Ok((block, hash)) => Json(serde_json::json!({
                                "mined": true,
                                "height": block.header.index,
                                "nonce": block.header.nonce,
                                "hash": hex::encode(hash),
                                "previous_hash": hex::encode(block.header.previous_hash),
                                "merkle_root": hex::encode(block.header.merkle_root),
                                "data_hash": hex::encode(block.header.data_hash),
                                "tx_count": block.txs.len(),
                                "target": target_zeros,
                                "data": block.data.clone().unwrap_or_else(|| "No Data".to_string()),
                            })),
                            Err(e) => Json(serde_json::json!({
                                "mined": false,
                                "error": e.to_string(),
                            })),
                        }
                    }
                }
            }),
        )
        .route(
            "/chain/blocks",
            get({
                let state = state.clone();
                move |Query(p): Query<ListParams>| {
                    let state = state.clone();
                    async move {
                        let (height, _) = state.chain.tip().unwrap_or((0, None));
                        let limit = p
                            .limit
                            .unwrap_or(BLOCKS_PER_BATCH)
                            .min(MAX_BLOCKS_PER_REQUEST);
                        let desc = p.dir.as_deref() != Some("asc");
                        let start = p.start.unwrap_or(height);

                        // call through to storage impl
                        let blocks = state
                            .chain
                            .store() // Arc<SledStore>
                            .list_blocks_range(start, limit, desc)
                            .unwrap_or_default();

                        let rows: Vec<BlockRow> = blocks
                            .into_iter()
                            .map(|b| BlockRow {
                                index: b.header.index,
                                ts: b.header.timestamp,
                                tx_count: b.txs.len(),
                                hash: hex::encode(b.hash()),
                                nonce: b.header.nonce,
                                previous_hash: hex::encode(b.header.previous_hash),
                                merkle_root: hex::encode(b.header.merkle_root),
                                data_hash: if b.data.is_some() {
                                    hex::encode(b.header.data_hash)
                                } else {
                                    "0".repeat(HASH_HEX_SIZE)
                                },
                                data: b.data.clone().unwrap_or_else(|| "No Data".to_string()),
                            })
                            .collect();

                        Json(rows)
                    }
                }
            }),
        )
        .route(
            "/mempool",
            get({
                let state = state.clone();
                move || {
                    let _state = state.clone();
                    async move {
                        let mp = state.mempool.lock().await;
                        Json(mp.clone())
                    }
                }
            }),
        )
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = args.listen.parse()?;
    info!("ledger-node listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

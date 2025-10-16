use axum::{routing::{get, post}, Json, Router};
use clap::Parser;
use ledger_core::{Transaction, chain::Chain};
use ledger_storage::sled_store::SledStore;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::{info, Level};

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
}

#[derive(Serialize)]
struct Health { status: &'static str }

#[derive(Serialize)]
struct Head { height: u64 }

#[derive(Deserialize)]
struct TxIn {
    from: String,
    to: String,
    amount: u64,
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

    let state = AppState { chain };

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
                        Json(serde_json::json!({ "accepted": true, "tx": tx }))
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

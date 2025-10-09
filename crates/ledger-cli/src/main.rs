use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Serialize;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "ledger-cli")]
#[command(about = "CLI client for the minimal ledger node")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Submit a transaction
    Submit {
        /// Node base URL (e.g. http://127.0.0.1:8080)
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        node: String,
        /// Sender
        #[arg(long)]
        from: String,
        /// Recipient
        #[arg(long)]
        to: String,
        /// Amount
        #[arg(long)]
        amount: u64,
    },
}

#[derive(Serialize)]
struct Tx {
    from: String,
    to: String,
    amount: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .pretty()
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Command::Submit { node, from, to, amount } => {
            let tx = Tx { from, to, amount };
            let client = reqwest::Client::new();
            let res = client.post(format!("{node}/tx")).json(&tx).send().await?;
            let status = res.status();
            let body = res.text().await?;
            println!("status: {}", status);
            println!("{body}");
        }
    }
    Ok(())
}

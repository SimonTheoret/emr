use clap::Parser;

mod cli;
mod ir;
mod match_id;
mod proxy;
mod record;
mod replay;
mod server;
mod storage;

use crate::cli::{Cli, Mode};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt().init();
    match Cli::parse().mode {
        Mode::Record(args) => record::run(args).await,
        Mode::Replay(args) => replay::run(args).await,
    }
}

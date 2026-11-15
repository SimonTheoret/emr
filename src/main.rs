use clap::Parser;

mod cli;
mod ir;
// The old recorder's storage and matching, deleted by the next changes.
#[allow(dead_code)]
mod legacy_storage;
mod match_id;
#[allow(dead_code)]
pub mod matching;
mod proxy;
mod record;
mod server;
// The replay mode starts using its replayer later in this stack.
#[allow(dead_code)]
mod storage;

use crate::cli::{Cli, Mode};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt().init();
    match Cli::parse().mode {
        Mode::Record(args) => record::run(args).await,
    }
}

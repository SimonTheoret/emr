use clap::Parser;

mod cli;
mod ir;
mod match_id;
mod proxy;
mod record;
mod replay;
mod server;
// TODO(sketch): drop this allow once `loader.rs` and `recorder.rs` land. Their stubs never
// call `Format::serialize` or `Format::deserialize`, which `dead_code = "deny"` rejects.
#[allow(dead_code)]
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

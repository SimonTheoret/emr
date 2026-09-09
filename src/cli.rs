use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version="0.1.0", about= "Record and replay proxy", long_about = None)]
pub struct Args {
    #[arg(long = "destination", short = 'd')]
    destination: String,
    #[arg(short, long, default_value = "mocks")]
    mock_dir: PathBuf,
    #[arg(short, default_value = "4321")]
    port: u16,
}

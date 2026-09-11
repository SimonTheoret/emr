use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version="0.1.0", about= "Record and replay proxy", long_about = None)]
pub struct Args {
    #[arg(long = "destination", short = 'd')]
    pub destination: String,
    #[arg(short, long, default_value = "mocks")]
    pub mock_dir: PathBuf,
    #[arg(short, default_value = "4321")]
    pub port: u16,
}

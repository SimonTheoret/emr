use clap::Parser;
use std::path::PathBuf;
use url::Url;

use crate::proxy::parse_destination;

#[derive(Debug, Parser)]
#[command(version="0.1.0", about= "Record and replay proxy", long_about = None)]
pub struct Args {
    #[arg(long = "destination", short = 'd', value_parser = parse_destination)]
    pub destination: Url,
    #[arg(short, long, default_value = "mocks")]
    pub mock_dir: PathBuf,
    #[arg(short, default_value = "4321")]
    pub port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parses_destination_as_a_url() {
        for flag in ["-d", "--destination"] {
            for destination in ["http://example.test/api", "https://example.test:8443/api/"] {
                let args = Args::try_parse_from(["erm", flag, destination]).unwrap();

                assert_eq!(args.destination.as_str(), destination);
            }
        }
    }

    #[test]
    fn rejects_invalid_destinations_during_argument_parsing() {
        for destination in [
            "ftp://example.test/api",
            "https://example.test/api?q=value",
            "https://example.test/api#fragment",
            "/relative/path",
        ] {
            let error = Args::try_parse_from(["erm", "--destination", destination]).unwrap_err();

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{destination}");
            assert!(error.to_string().contains("--destination"));
        }
    }
}

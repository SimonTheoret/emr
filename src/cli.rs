use clap::Parser;
use eyre::{Result, bail};
use std::path::PathBuf;
use url::Url;

use crate::ir::UrlIR;

#[derive(Debug, Parser)]
#[command(version="0.1.0", about= "Record and replay proxy", long_about = None)]
pub struct Args {
    #[arg(long , short, value_parser = parse_destination)]
    pub destination: UrlIR,
    #[arg(short, long, default_value = "mocks")]
    pub mock_dir: PathBuf,
    #[arg(short, default_value = "4321")]
    pub port: u16,
}

/// Parse and validate the HTTP(S) base URL used for upstream requests.
fn parse_destination(value: &str) -> Result<UrlIR> {
    let destination = Url::parse(value)?;

    if !matches!(destination.scheme(), "http" | "https") {
        bail!("destination must use the http or https scheme");
    }
    if destination.cannot_be_a_base() {
        bail!("destination cannot be a base");
    }
    if !destination.has_host() {
        bail!("destination must be an absolute URL with a host");
    }
    if destination.query().is_some() {
        bail!("destination must not contain a query");
    }
    if destination.fragment().is_some() {
        bail!("destination must not contain a fragment");
    }

    Ok(UrlIR(destination))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_and_https_destinations_with_base_paths() {
        assert_eq!(
            parse_destination("http://example.test/api")
                .unwrap()
                .as_str(),
            "http://example.test/api"
        );
        assert_eq!(
            parse_destination("https://example.test:8443/api/")
                .unwrap()
                .as_str(),
            "https://example.test:8443/api/"
        );
    }

    #[test]
    fn rejects_invalid_destination_shapes() {
        for destination in [
            "ftp://example.test/api",
            "https://example.test/api?q=value",
            "https://example.test/api#fragment",
            "/relative/path",
        ] {
            assert!(parse_destination(destination).is_err(), "{destination}");
        }
    }
}

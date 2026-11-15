//! Q5: a failed write while recording stops ERM, naming the file and the cause.

use std::{fs, net::Ipv4Addr, process::Stdio, time::Duration};

use axum::Router;
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    time::{sleep, timeout},
};

const FORMAT: &str = if cfg!(feature = "ron") { "ron" } else { "json" };
const TIMEOUT: Duration = Duration::from_secs(20);

#[tokio::test(flavor = "multi_thread")]
async fn q5_deleting_the_directory_while_recording_exits_1_naming_the_file() {
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let upstream_address = upstream.local_addr().unwrap();
    let router = Router::new().fallback(|| async { "ok" });
    tokio::spawn(async move { axum::serve(upstream, router).await.unwrap() });
    let dir = TempDir::new().unwrap();
    let mocks = dir.path().join("mocks");
    let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let erm = Command::new(env!("CARGO_BIN_EXE_erm"))
        .arg("record")
        .args(["--destination", &format!("http://{upstream_address}")])
        .arg("--mock-dir")
        .arg(&mocks)
        .args(["--format", FORMAT, "-p", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    timeout(TIMEOUT, async {
        while TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
        {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("erm accepts connections");

    fs::remove_dir(&mocks).expect("erm created the directory, and it is still empty");
    // The response may or may not reach the caller before erm exits.
    let _ = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://127.0.0.1:{port}/"))
        .header("x-erm-match", "lost")
        .send()
        .await;
    let output = timeout(TIMEOUT, erm.wait_with_output())
        .await
        .expect("erm exits after the failed write")
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "erm exits 1: {stderr}");
    let file = mocks.join(format!("lost.{FORMAT}.tmp"));
    assert!(
        stderr.contains(&file.display().to_string()),
        "stderr names {}: {stderr}",
        file.display()
    );
}

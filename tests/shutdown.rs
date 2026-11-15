//! Q6: Ctrl-C while exchanges are in flight. Every exchange is on disk before ERM exits.
#![cfg(unix)]

use std::{
    net::{Ipv4Addr, SocketAddr},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{Router, extract::Path};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    sync::watch,
    time::{sleep, timeout},
};

const FORMAT: &str = if cfg!(feature = "ron") { "ron" } else { "json" };
const IN_FLIGHT: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(20);

/// Counts each request as it arrives, then holds its response until `release` is sent.
async fn upstream(arrived: Arc<AtomicUsize>, release: watch::Receiver<bool>) -> SocketAddr {
    let router = Router::new().route(
        "/{name}",
        axum::routing::get(move |Path(name): Path<String>| {
            let mut release = release.clone();
            arrived.fetch_add(1, Ordering::SeqCst);
            async move {
                release.wait_for(|released| *released).await.unwrap();
                format!("done {name}")
            }
        }),
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    address
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.local_addr().unwrap().port()
}

async fn eventually(what: &str, mut condition: impl AsyncFnMut() -> bool) {
    timeout(TIMEOUT, async {
        while !condition().await {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting until {what}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn q6_ctrl_c_writes_every_in_flight_exchange_before_exit() {
    let arrived = Arc::new(AtomicUsize::new(0));
    let (release, released) = watch::channel(false);
    let upstream = upstream(arrived.clone(), released).await;
    let dir = TempDir::new().unwrap();
    let port = free_port();
    let erm = Command::new(env!("CARGO_BIN_EXE_erm"))
        .arg("record")
        .args(["--destination", &format!("http://{upstream}")])
        .arg("--mock-dir")
        .arg(dir.path())
        .args(["--format", FORMAT, "-p", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = erm.id().unwrap();
    let erm_address = (Ipv4Addr::LOCALHOST, port);
    eventually("erm accepts connections", async || {
        TcpStream::connect(erm_address).await.is_ok()
    })
    .await;

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let requests: Vec<_> = (0..IN_FLIGHT)
        .map(|i| {
            let request = client
                .get(format!("http://127.0.0.1:{port}/q6-{i}"))
                .header("x-erm-match", format!("q6-{i}"));
            tokio::spawn(request.send())
        })
        .collect();
    eventually("every request reached the upstream", async || {
        arrived.load(Ordering::SeqCst) == IN_FLIGHT
    })
    .await;

    let kill = std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .unwrap();
    assert!(kill.success(), "SIGINT is sent");
    eventually("erm stops accepting connections", async || {
        TcpStream::connect(erm_address).await.is_err()
    })
    .await;
    release.send(true).unwrap();

    for (i, request) in requests.into_iter().enumerate() {
        let response = request
            .await
            .unwrap()
            .expect("an in-flight request finishes");
        assert_eq!(response.status(), 200, "q6-{i}");
        assert_eq!(response.text().await.unwrap(), format!("done q6-{i}"));
    }
    let output = timeout(TIMEOUT, erm.wait_with_output())
        .await
        .expect("erm exits once its exchanges are written")
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "erm exits cleanly: {stderr}");

    let mut files: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    let mut expected: Vec<String> = (0..IN_FLIGHT).map(|i| format!("q6-{i}.{FORMAT}")).collect();
    expected.sort();
    assert_eq!(
        files, expected,
        "every exchange is on disk, with no .tmp file"
    );
}

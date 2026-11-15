//! Q4: corrupted files stop replay mode before it binds its port.

use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

const FORMAT: &str = if cfg!(feature = "ron") { "ron" } else { "json" };

#[test]
fn q4_replay_lists_every_corrupted_file_and_exits_1() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join(format!("login.{FORMAT}"));
    let garbage = dir.path().join(format!("logout.{FORMAT}"));
    fs::write(&empty, "").unwrap();
    fs::write(&garbage, "not an exchange").unwrap();
    // Holding the port makes a bind before the load fail with "address in use" instead of
    // listing the files.
    let held = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let port = held.local_addr().unwrap().port();

    let mut erm = Command::new(env!("CARGO_BIN_EXE_erm"))
        .arg("replay")
        .arg("--mock-dir")
        .arg(dir.path())
        .args(["--format", FORMAT, "-p", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while erm.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            erm.kill().unwrap();
            panic!("erm replay is still running with corrupted files");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = erm.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1), "erm exits 1: {stderr}");
    for path in [&empty, &garbage] {
        assert!(
            stderr.contains(&path.display().to_string()),
            "stderr lists {}: {stderr}",
            path.display()
        );
    }
}

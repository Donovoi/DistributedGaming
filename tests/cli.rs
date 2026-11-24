use std::io::{Read, Write};
use std::process::Command;

#[test]
fn corrupt_pixels_fail_without_creating_report() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let path = std::env::temp_dir().join(format!(
        "dg-corrupt-{}-{}.json",
        std::process::id(),
        address.port()
    ));
    assert!(!path.exists());
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut prefix = [0; 12];
        socket.read_exact(&mut prefix).unwrap();
        let length = u32::from_be_bytes(prefix[8..12].try_into().unwrap()) as usize;
        assert!(length <= 1024);
        let mut header = vec![0; length];
        socket.read_exact(&mut header).unwrap();
        let job: distributed_gaming::render::Job = serde_json::from_slice(&header).unwrap();
        let mut tile = distributed_gaming::render::render(job).unwrap();
        tile.pixels[0] ^= 1;
        socket.write_all(&prefix).unwrap();
        socket.write_all(&header).unwrap();
        socket
            .write_all(&(tile.pixels.len() as u32).to_be_bytes())
            .unwrap();
        socket.write_all(&tile.pixels).unwrap();
        socket.shutdown(std::net::Shutdown::Write).unwrap();
    });
    let output = Command::new(env!("CARGO_BIN_EXE_dg"))
        .args([
            "bench",
            "--worker",
            &address.to_string(),
            "--width",
            "8",
            "--height",
            "8",
            "--frames",
            "2",
            "--warmup",
            "0",
            "--out",
        ])
        .arg(&path)
        .output()
        .unwrap();
    peer.join().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Pixel verification failed"));
    assert!(!path.exists());
}

#[test]
fn bad_arguments_fail_without_a_success_report() {
    for args in [
        vec!["bench", "--frames", "0"],
        vec!["bench", "--width", "4294967295"],
        vec!["bench", "--worker", "0.0.0.0:1"],
        vec!["bench", "--typo", "1"],
        vec!["worker", "--jobs", "0"],
        vec!["bench", "--frames", "2", "--frames", "3"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_dg"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn disconnected_worker_fails_run() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        drop(socket);
    });
    let output = Command::new(env!("CARGO_BIN_EXE_dg"))
        .args([
            "bench",
            "--worker",
            &address.to_string(),
            "--width",
            "8",
            "--height",
            "8",
            "--frames",
            "2",
            "--warmup",
            "0",
        ])
        .output()
        .unwrap();
    peer.join().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn local_report_is_explicit_about_limits() {
    let output = Command::new(env!("CARGO_BIN_EXE_dg"))
        .args([
            "bench", "--width", "8", "--height", "8", "--frames", "2", "--warmup", "0",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["network_scope"], "local_only");
    assert_eq!(report["game_speedup_proven"], false);
    assert_eq!(report["verified_frames"], 2);
}

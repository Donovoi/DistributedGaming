use distributed_gaming::benchmark::{run, Config};
use distributed_gaming::render::Frame;
use distributed_gaming::{invalid, wire, Result};
use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

const HELP: &str = "DistributedGaming test harness (CPU test scene, not a game)\n\n\
  dg worker [--listen 127.0.0.1:47000] [--jobs N] [--timeout-ms 5000]\n\
  dg bench [--worker IP:PORT ...] [--width 320] [--height 180]\n\
           [--samples 4] [--frames 10] [--warmup 2] [--timeout-ms 5000]\n\
           [--out NEW_FILE.json]\n\n\
Worker: serve one bounded job at a time. Stop with Ctrl+C.\n\
Use --jobs for a worker that stops after N connections.\n\
Use only trusted peers. Traffic has no built-in authentication or encryption.\n\
Use an SSH tunnel or a private network you control. Default bind is loopback.\n\
Bench: compare exact pixels and report timings against local parallel work.\n\
No worker means a local self-check. Output paths must not already exist.\n";

fn main() {
    if let Err(error) = execute() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if command == "--help" || command == "help" {
        print!("{HELP}");
        return Ok(());
    }
    if command == "--version" {
        println!("dg {} scene 1", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let allowed = match command.as_str() {
        "worker" => &["--listen", "--jobs", "--timeout-ms"][..],
        "bench" => &[
            "--worker",
            "--width",
            "--height",
            "--samples",
            "--frames",
            "--warmup",
            "--timeout-ms",
            "--out",
        ][..],
        _ => return Err(invalid("Unknown command. Run dg --help.")),
    };
    let mut options = HashMap::new();
    let mut workers = Vec::new();
    while let Some(key) = args.next() {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid(format!("Unknown option: {key}")));
        }
        let value = args
            .next()
            .ok_or_else(|| invalid(format!("Missing value for {key}.")))?;
        if key == "--worker" {
            workers.push(value.parse::<SocketAddr>()?);
        } else if options.insert(key.clone(), value).is_some() {
            return Err(invalid(format!("Repeated option: {key}")));
        }
    }
    let number = |key: &str, fallback: u64| -> Result<u64> {
        options
            .get(key)
            .map(|v| v.parse::<u64>().map_err(Into::into))
            .unwrap_or(Ok(fallback))
    };
    let timeout_ms = number("--timeout-ms", 5000)?;
    if !(10..=120_000).contains(&timeout_ms) {
        return Err(invalid("Timeout must be 10..120000 ms."));
    }
    let timeout = Duration::from_millis(timeout_ms);
    if command == "worker" {
        let address = options
            .get("--listen")
            .map(String::as_str)
            .unwrap_or("127.0.0.1:47000")
            .parse::<SocketAddr>()?;
        let jobs = if options.contains_key("--jobs") {
            let n = number("--jobs", 0)?;
            if !(1..=10000).contains(&n) {
                return Err(invalid("Jobs must be 1..10000."));
            }
            Some(n as usize)
        } else {
            None
        };
        let listener = TcpListener::bind(address)?;
        eprintln!(
            "Worker ready on {}. Fixed CPU scene only.",
            listener.local_addr()?
        );
        if !address.ip().is_loopback() {
            eprintln!("Network bind: use only trusted peers or an encrypted private network.");
        }
        return wire::serve(listener, timeout, jobs);
    }
    let output = options.get("--out");
    if output.is_some_and(|path| std::path::Path::new(path).exists()) {
        return Err(invalid("Output already exists. Use a new report name."));
    }
    let config = Config {
        frame: Frame {
            id: 0,
            width: number("--width", 320)?.try_into()?,
            height: number("--height", 180)?.try_into()?,
            samples: number("--samples", 4)?.try_into()?,
        },
        workers,
        frames: number("--frames", 10)?.try_into()?,
        warmup: number("--warmup", 2)?.try_into()?,
        timeout,
    };
    let report = run(&config)?;
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(json.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        eprintln!(
            "Report saved. All {} frames passed exact pixel checks.",
            report.verified_frames
        );
    } else {
        println!("{json}");
    }
    Ok(())
}

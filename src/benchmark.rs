use crate::render::{assemble, partition, render, Frame, Tile, SCENE};
use crate::wire;
use crate::{invalid, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct Config {
    pub frame: Frame,
    pub workers: Vec<SocketAddr>,
    pub frames: usize,
    pub warmup: usize,
    pub timeout: Duration,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        self.frame.validate()?;
        if self.frames < 2
            || self.frames > 120
            || self.warmup > 10
            || self.workers.len() > 15
            || self.timeout < Duration::from_millis(10)
            || self.timeout > Duration::from_secs(120)
        {
            return Err(invalid(
                "Use 2..120 frames, 0..10 warmups, at most 15 workers, and 10..120000 ms timeout.",
            ));
        }
        if self
            .workers
            .iter()
            .any(|addr| addr.port() == 0 || addr.ip().is_unspecified() || addr.ip().is_multicast())
        {
            return Err(invalid(
                "Workers need a unicast IP address and a nonzero port.",
            ));
        }
        if self.workers.iter().collect::<HashSet<_>>().len() != self.workers.len() {
            return Err(invalid("Worker addresses must be distinct."));
        }
        partition(self.frame, self.workers.len() + 1)?;
        Ok(())
    }
}

pub fn local_parallel(frame: Frame, parts: usize) -> Result<Vec<u8>> {
    let jobs = partition(frame, parts)?;
    let tiles = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|job| scope.spawn(move || render(job)))
            .collect();
        let mut tiles = Vec::new();
        for handle in handles {
            tiles.push(
                handle
                    .join()
                    .map_err(|_| invalid("Local render thread failed."))??,
            );
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(tiles)
    })?;
    assemble(frame, tiles)
}

pub fn distributed(frame: Frame, workers: &[SocketAddr], timeout: Duration) -> Result<Vec<u8>> {
    let jobs = partition(frame, workers.len() + 1)?;
    let tiles = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .enumerate()
            .map(|(i, job)| {
                scope.spawn(move || {
                    if i == 0 {
                        render(job)
                    } else {
                        wire::request(workers[i - 1], job, timeout)
                    }
                })
            })
            .collect();
        let mut tiles: Vec<Tile> = Vec::new();
        for handle in handles {
            tiles.push(
                handle
                    .join()
                    .map_err(|_| invalid("Worker thread failed."))??,
            );
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(tiles)
    })?;
    assemble(frame, tiles)
}

#[derive(Serialize)]
pub struct Summary {
    pub median_ms: f64,
    pub p95_ms: f64,
    pub samples_ms: Vec<f64>,
}

impl Summary {
    pub fn new(samples: Vec<f64>) -> Result<Self> {
        if samples.is_empty() || samples.iter().any(|x| !x.is_finite() || *x <= 0.0) {
            return Err(invalid("Timing samples must be finite and positive."));
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let median = if n.is_multiple_of(2) {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        } else {
            sorted[n / 2]
        };
        let index = (95 * n).div_ceil(100) - 1;
        Ok(Self {
            median_ms: median,
            p95_ms: sorted[index],
            samples_ms: samples,
        })
    }
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub program_version: &'static str,
    pub scene_version: u32,
    pub workload: &'static str,
    pub time_unix_seconds: u64,
    pub coordinator_os: &'static str,
    pub coordinator_arch: &'static str,
    pub coordinator_logical_cpus: usize,
    pub build_debug_assertions: bool,
    pub network_scope: &'static str,
    pub physical_host_count_verified: bool,
    pub game_speedup_proven: bool,
    pub frame: Frame,
    pub worker_addresses: Vec<String>,
    pub warmup_frames: usize,
    pub measured_frames: usize,
    pub timeout_ms: u128,
    pub local_parallel_parts: usize,
    pub remote_pixel_bytes_per_frame: usize,
    pub verified_frames: usize,
    pub verification: &'static str,
    pub timing_definition: &'static str,
    pub percentile_definition: &'static str,
    pub local_parallel: Summary,
    pub distributed: Summary,
    pub local_parallel_over_distributed_median: f64,
}

pub fn run(config: &Config) -> Result<Report> {
    config.validate()?;
    let mut local_times = Vec::new();
    let mut distributed_times = Vec::new();
    for index in 0..config.warmup + config.frames {
        let frame = Frame {
            id: config
                .frame
                .id
                .checked_add(index as u64)
                .ok_or_else(|| invalid("Frame ID overflow."))?,
            ..config.frame
        };
        // Independent full-frame layout checks both split paths. Kept outside timing.
        let reference = render(partition(frame, 1)?[0])?.pixels;
        let local = || -> Result<_> {
            let start = Instant::now();
            let pixels = local_parallel(frame, config.workers.len() + 1)?;
            Ok((pixels, start.elapsed().as_secs_f64() * 1000.0))
        };
        let remote = || -> Result<_> {
            let start = Instant::now();
            let pixels = distributed(frame, &config.workers, config.timeout)?;
            Ok((pixels, start.elapsed().as_secs_f64() * 1000.0))
        };
        let ((local_image, local_ms), (remote_image, remote_ms)) = if index % 2 == 0 {
            (local()?, remote()?)
        } else {
            let r = remote()?;
            (local()?, r)
        };
        if local_image != reference || remote_image != reference {
            return Err(invalid(format!(
                "Pixel verification failed for frame {}.",
                frame.id
            )));
        }
        if index >= config.warmup {
            local_times.push(local_ms);
            distributed_times.push(remote_ms);
        }
    }
    let local_parallel = Summary::new(local_times)?;
    let distributed = Summary::new(distributed_times)?;
    let ratio = local_parallel.median_ms / distributed.median_ms;
    let jobs = partition(config.frame, config.workers.len() + 1)?;
    Ok(Report {
        schema_version: 1,
        program_version: env!("CARGO_PKG_VERSION"),
        scene_version: SCENE,
        workload: "fixed CPU ray tracer; no game or GPU execution",
        time_unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        coordinator_os: std::env::consts::OS,
        coordinator_arch: std::env::consts::ARCH,
        coordinator_logical_cpus: std::thread::available_parallelism()?.get(),
        build_debug_assertions: cfg!(debug_assertions),
        network_scope: if config.workers.is_empty() { "local_only" } else if config.workers.iter().all(|a| a.ip().is_loopback()) { "loopback_only" } else { "non_loopback_addresses; physical placement must be checked separately" },
        physical_host_count_verified: false,
        game_speedup_proven: false,
        frame: config.frame,
        worker_addresses: config.workers.iter().map(ToString::to_string).collect(),
        warmup_frames: config.warmup,
        measured_frames: config.frames,
        timeout_ms: config.timeout.as_millis(),
        local_parallel_parts: config.workers.len() + 1,
        remote_pixel_bytes_per_frame: jobs.iter().skip(1).map(|job| job.byte_len()).collect::<Result<Vec<_>>>()?.iter().sum(),
        verified_frames: config.frames + config.warmup,
        verification: "exact RGBA bytes against a full-frame local render; all frames including warmups",
        timing_definition: "wall time through image assembly; includes threads, per-job TCP connect, transfers and compute; excludes reference rendering, comparison and report writing; alternating mode order; CPU work is bounded but not preempted by socket deadlines",
        percentile_definition: "p95 uses nearest rank; median averages the two middle values for even counts",
        local_parallel,
        distributed,
        local_parallel_over_distributed_median: ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_percentiles() {
        let s = Summary::new(vec![4.0, 1.0, 3.0, 2.0]).unwrap();
        assert_eq!(s.median_ms, 2.5);
        assert_eq!(s.p95_ms, 4.0);
        assert!(Summary::new(vec![]).is_err());
        assert!(Summary::new(vec![f64::NAN]).is_err());
    }
    #[test]
    fn multi_worker_transport_matches_reference() {
        let mut addresses = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..2 {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            addresses.push(listener.local_addr().unwrap());
            handles.push(std::thread::spawn(move || {
                wire::serve(listener, Duration::from_secs(3), Some(3)).unwrap()
            }));
        }
        let config = Config {
            frame: Frame {
                id: 20,
                width: 31,
                height: 17,
                samples: 2,
            },
            workers: addresses,
            frames: 2,
            warmup: 1,
            timeout: Duration::from_secs(3),
        };
        let report = run(&config).unwrap();
        assert_eq!(report.verified_frames, 3);
        assert_eq!(report.network_scope, "loopback_only");
        assert!(!report.game_speedup_proven);
        for h in handles {
            h.join().unwrap();
        }
    }
}

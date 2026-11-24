use crate::render::{render, Job, Tile};
use crate::{invalid, Result};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 8] = b"DGTILE01";
const MAX_HEADER: usize = 1024;

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "Job deadline expired.").into()
        })
}

fn read_until(stream: &mut TcpStream, mut out: &mut [u8], deadline: Instant) -> Result<()> {
    while !out.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(out) {
            Ok(0) => return Err(invalid("Peer closed before the message was complete.")),
            Ok(n) => out = &mut out[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn write_until(stream: &mut TcpStream, mut data: &[u8], deadline: Instant) -> Result<()> {
    while !data.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(data) {
            Ok(0) => return Err(invalid("Peer accepted no data.")),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn write_job(stream: &mut TcpStream, job: Job, deadline: Instant) -> Result<()> {
    job.validate()?;
    let header = serde_json::to_vec(&job)?;
    write_until(stream, MAGIC, deadline)?;
    write_until(stream, &(header.len() as u32).to_be_bytes(), deadline)?;
    write_until(stream, &header, deadline)
}

fn read_job(stream: &mut TcpStream, deadline: Instant) -> Result<Job> {
    let mut prefix = [0; 12];
    read_until(stream, &mut prefix, deadline)?;
    if &prefix[..8] != MAGIC {
        return Err(invalid("Wrong protocol magic or version."));
    }
    let size = u32::from_be_bytes(prefix[8..12].try_into()?) as usize;
    if size == 0 || size > MAX_HEADER {
        return Err(invalid("Job header exceeds limit."));
    }
    let mut header = vec![0; size];
    read_until(stream, &mut header, deadline)?;
    let job: Job = serde_json::from_slice(&header)?;
    job.validate()?;
    Ok(job)
}

// Each connection holds one request and one reply. Half-close marks message end.
fn expect_eof(stream: &mut TcpStream, deadline: Instant) -> Result<()> {
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(invalid("Unexpected bytes after message.")),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn request(address: SocketAddr, job: Job, timeout: Duration) -> Result<Tile> {
    job.validate()?;
    let deadline = Instant::now() + timeout;
    let mut stream = TcpStream::connect_timeout(&address, remaining(deadline)?)?;
    stream.set_nodelay(true)?;
    write_job(&mut stream, job, deadline)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let received = read_job(&mut stream, deadline)?;
    if received != job {
        return Err(invalid("Worker replied with a different job."));
    }
    let mut length = [0; 4];
    read_until(&mut stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length != job.byte_len()? {
        return Err(invalid("Worker returned the wrong pixel count."));
    }
    let mut pixels = vec![0; length];
    read_until(&mut stream, &mut pixels, deadline)?;
    expect_eof(&mut stream, deadline)?;
    Ok(Tile { job, pixels })
}

pub fn serve_connection(mut stream: TcpStream, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    stream.set_nodelay(true)?;
    let job = read_job(&mut stream, deadline)?;
    expect_eof(&mut stream, deadline)?;
    let tile = render(job)?;
    // Rendering is bounded by MAX_RAYS. Socket deadlines cannot preempt CPU work.
    remaining(deadline)?;
    write_job(&mut stream, job, deadline)?;
    write_until(
        &mut stream,
        &(tile.pixels.len() as u32).to_be_bytes(),
        deadline,
    )?;
    write_until(&mut stream, &tile.pixels, deadline)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}

pub fn serve(listener: TcpListener, timeout: Duration, jobs: Option<usize>) -> Result<()> {
    for (accepted, incoming) in listener.incoming().enumerate() {
        let stream = incoming?;
        if let Err(error) = serve_connection(stream, timeout) {
            eprintln!("Job failed: {error}");
        }
        if jobs.is_some_and(|limit| accepted + 1 >= limit) {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{partition, Frame};
    fn job() -> Job {
        partition(
            Frame {
                id: 9,
                width: 16,
                height: 7,
                samples: 2,
            },
            1,
        )
        .unwrap()[0]
    }
    fn peer(
        f: impl FnOnce(TcpStream) + Send + 'static,
    ) -> (SocketAddr, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || f(listener.accept().unwrap().0));
        (address, handle)
    }
    #[test]
    fn real_tcp_round_trip() {
        let (addr, handle) = peer(|s| serve_connection(s, Duration::from_secs(3)).unwrap());
        assert_eq!(
            request(addr, job(), Duration::from_secs(3)).unwrap().pixels,
            render(job()).unwrap().pixels
        );
        handle.join().unwrap();
    }
    #[test]
    fn wrong_job_and_invalid_lengths_are_rejected() {
        for mode in 0..4 {
            let (addr, handle) = peer(move |mut s| {
                let end = Instant::now() + Duration::from_secs(2);
                let mut received = read_job(&mut s, end).unwrap();
                expect_eof(&mut s, end).unwrap();
                if mode == 0 {
                    received.frame.id += 1;
                }
                if mode == 1 {
                    received.tile += 1;
                }
                write_job(&mut s, received, end).unwrap();
                if mode >= 2 {
                    let len = if mode == 2 {
                        u32::MAX
                    } else {
                        received.byte_len().unwrap() as u32
                    };
                    let _ = write_until(&mut s, &len.to_be_bytes(), end);
                    // Mode 3 closes without the declared payload.
                }
            });
            assert!(request(addr, job(), Duration::from_secs(2)).is_err());
            handle.join().unwrap();
        }
    }
    #[test]
    fn malformed_headers_are_rejected() {
        for bytes in [
            b"wrong header".to_vec(),
            [MAGIC.as_slice(), &u32::MAX.to_be_bytes()].concat(),
            b"DGT".to_vec(),
        ] {
            let (addr, handle) =
                peer(|s| assert!(serve_connection(s, Duration::from_secs(2)).is_err()));
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(&bytes).unwrap();
            s.shutdown(std::net::Shutdown::Write).unwrap();
            handle.join().unwrap();
        }
    }
    #[test]
    fn trailing_request_bytes_are_rejected() {
        let (addr, handle) =
            peer(|s| assert!(serve_connection(s, Duration::from_secs(2)).is_err()));
        let mut s = TcpStream::connect(addr).unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        write_job(&mut s, job(), end).unwrap();
        s.write_all(b"extra").unwrap();
        s.shutdown(std::net::Shutdown::Write).unwrap();
        handle.join().unwrap();
    }
    #[test]
    fn slow_drip_cannot_reset_total_deadline() {
        let (addr, handle) = peer(|mut s| {
            let end = Instant::now() + Duration::from_secs(2);
            let _ = read_job(&mut s, end);
            let _ = expect_eof(&mut s, end);
            for byte in MAGIC {
                if s.write_all(&[*byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(70));
            }
        });
        let start = Instant::now();
        assert!(request(addr, job(), Duration::from_millis(180)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        handle.join().unwrap();
    }
    #[test]
    fn silent_peer_times_out() {
        let (addr, handle) = peer(|_s| std::thread::sleep(Duration::from_millis(400)));
        let start = Instant::now();
        assert!(request(addr, job(), Duration::from_millis(100)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        handle.join().unwrap();
    }
}

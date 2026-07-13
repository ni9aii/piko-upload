use anyhow::{Result, bail};
use std::time::{Duration, Instant};

use serialport::SerialPort;

/// Reads whatever is available until `quiet_gap` passes with no new bytes,
/// or `timeout` total elapses. Mirrors Python's `read_available`.
pub fn read_available(port: &mut dyn SerialPort, timeout: Duration, quiet_gap: Duration) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 65536];
    let overall_deadline = Instant::now() + timeout;
    let mut deadline = overall_deadline;

    while Instant::now() < deadline {
        port.set_timeout(Duration::from_millis(50)).ok();
        match port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                data.extend_from_slice(&buf[..n]);
                deadline = Instant::now() + quiet_gap;
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => {}
        }
    }
    data
}

/// Reads exactly `n` bytes or whatever arrived before `timeout` elapses.
/// Mirrors Python's `read_exact`.
pub fn read_exact_upto(port: &mut dyn SerialPort, n: usize, timeout: Duration) -> Vec<u8> {
    let mut data = Vec::with_capacity(n);
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + timeout;

    while data.len() < n && Instant::now() < deadline {
        port.set_timeout(Duration::from_millis(100)).ok();
        let want = (n - data.len()).min(buf.len());
        match port.read(&mut buf[..want]) {
            Ok(0) => {}
            Ok(k) => data.extend_from_slice(&buf[..k]),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => {}
        }
    }
    data
}

/// Sends `data` in `chunk_size`-byte chunks, bailing out if no write
/// progress is made for `stall_timeout`. Mirrors Python's `send_with_watchdog`.
pub fn send_with_watchdog(
    port: &mut dyn SerialPort,
    data: &[u8],
    chunk_size: usize,
    stall_timeout: Duration,
    log_every: usize,
) -> Result<usize> {
    let total = data.len();
    let mut sent = 0usize;
    let mut last_progress = Instant::now();
    let mut chunk_i = 0usize;

    port.set_timeout(Duration::from_millis(1000)).ok();

    while sent < total {
        let end = (sent + chunk_size).min(total);
        let mut view = &data[sent..end];
        while !view.is_empty() {
            match port.write(view) {
                Ok(0) => {
                    if last_progress.elapsed() > stall_timeout {
                        bail!(
                            "stalled: no write progress for {:?} at {}/{} bytes",
                            stall_timeout,
                            sent,
                            total
                        );
                    }
                }
                Ok(n) => {
                    view = &view[n..];
                    sent += n;
                    last_progress = Instant::now();
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if last_progress.elapsed() > stall_timeout {
                        bail!(
                            "stalled: no write progress for {:?} at {}/{} bytes",
                            stall_timeout,
                            sent,
                            total
                        );
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        chunk_i += 1;
        if chunk_i % log_every == 0 || sent == total {
            println!("  sent {sent}/{total} ({:.1}%)", 100.0 * sent as f64 / total as f64);
        }
    }
    Ok(sent)
}

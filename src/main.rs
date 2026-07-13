mod bank;
mod serial_io;
mod wav;

use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const DEV: &str = "/dev/ttyACM0";

fn run() -> Result<ExitCode> {
    let sample_paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if sample_paths.is_empty() {
        println!("usage: piko_upload <wav files...>");
        return Ok(ExitCode::from(1));
    }

    let names: Vec<String> = sample_paths
        .iter()
        .map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        .collect();
    println!("Uploading {} sample(s): {:?}", sample_paths.len(), names);

    let mut port = serialport::new(DEV, 115200)
        .timeout(Duration::from_millis(100))
        .open()
        .with_context(|| format!("opening {DEV}"))?;
    // USB CDC-ACM ignores the requested baud rate; 115200 is a placeholder.

    std::thread::sleep(Duration::from_millis(300));
    port.clear(serialport::ClearBuffer::All).ok();

    port.write_all(b"I").context("sending I command")?;
    let info = serial_io::read_available(port.as_mut(), Duration::from_secs(2), Duration::from_millis(300));
    let text = String::from_utf8_lossy(&info);
    println!("Device info: {text}");

    let parts: Vec<&str> = text.split_whitespace().collect();
    let c_idx = parts
        .iter()
        .position(|&p| p == "C")
        .context("device info missing 'C' capacity field")?;
    let capacity_bytes: u32 = parts
        .get(c_idx + 1)
        .context("device info missing capacity value after 'C'")?
        .parse()
        .context("parsing capacity value")?;

    let built = bank::build_bank(&sample_paths, capacity_bytes)?;
    let total_len = built.blob.len();
    println!(
        "sample_count={} audio_bytes={} total_len={total_len}",
        built.sample_count, built.audio_bytes
    );

    port.write_all(b"W").context("sending W command")?;
    port.write_all(&(total_len as u32).to_le_bytes())
        .context("sending payload length")?;
    let resp = serial_io::read_exact_upto(port.as_mut(), 3, Duration::from_secs(5));
    println!("write-init resp: {resp:?}");
    if resp != b"OK\n" {
        let extra = serial_io::read_available(port.as_mut(), Duration::from_secs(1), Duration::from_millis(300));
        let mut combined = resp;
        combined.extend_from_slice(&extra);
        println!("REJECTED, extra: {combined:?}");
        return Ok(ExitCode::from(1));
    }

    let t0 = std::time::Instant::now();
    match serial_io::send_with_watchdog(port.as_mut(), &built.blob, 256, Duration::from_secs(15), 20) {
        Ok(_) => {}
        Err(e) => {
            println!("UPLOAD STALLED: {e}");
            return Ok(ExitCode::from(2));
        }
    }
    let dt = t0.elapsed().as_secs_f64();
    println!(
        "payload sent in {dt:.1}s ({:.0} B/s), waiting for final ack...",
        total_len as f64 / dt
    );

    let final_resp = serial_io::read_available(port.as_mut(), Duration::from_secs(60), Duration::from_secs(2));
    println!("final resp: {:?}", String::from_utf8_lossy(&final_resp));

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

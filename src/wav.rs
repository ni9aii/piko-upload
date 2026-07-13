use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

/// Minimal RIFF/WAVE parser that extracts the raw `data` chunk bytes for an
/// 8-bit mono PCM file at the bank's sample rate, without any sample-value
/// reinterpretation. WAV always stores 8-bit samples as unsigned bytes, so
/// the returned bytes are exactly what the firmware expects to write to
/// flash as-is (matches the byte-exact behavior of the previously verified
/// Python `wave` module path).
pub fn read_wav_8bit_mono_pcm(path: &Path, expected_sample_rate: u32) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("reading {}", path.display()))?;

    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("{}: not a RIFF/WAVE file", path.display());
    }

    let mut offset = 12usize;
    let mut channels: Option<u16> = None;
    let mut bits_per_sample: Option<u16> = None;
    let mut audio_format: Option<u16> = None;
    let mut sample_rate: Option<u32> = None;
    let mut data: Option<Vec<u8>> = None;

    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body_start = offset + 8;
        let body_end = body_start
            .checked_add(chunk_size)
            .filter(|&e| e <= bytes.len())
            .with_context(|| format!("{}: truncated {:?} chunk", path.display(), String::from_utf8_lossy(chunk_id)))?;
        let body = &bytes[body_start..body_end];

        match chunk_id {
            b"fmt " => {
                if body.len() < 16 {
                    bail!("{}: fmt chunk too short", path.display());
                }
                audio_format = Some(u16::from_le_bytes(body[0..2].try_into().unwrap()));
                channels = Some(u16::from_le_bytes(body[2..4].try_into().unwrap()));
                sample_rate = Some(u32::from_le_bytes(body[4..8].try_into().unwrap()));
                bits_per_sample = Some(u16::from_le_bytes(body[14..16].try_into().unwrap()));
            }
            b"data" => {
                data = Some(body.to_vec());
            }
            _ => {}
        }

        // Chunks are word-aligned: odd-sized chunk bodies have a padding byte.
        offset = body_end + (chunk_size & 1);
    }

    let audio_format = audio_format.with_context(|| format!("{}: missing fmt chunk", path.display()))?;
    let channels = channels.context("unreachable: fmt chunk parsed without channels")?;
    let sample_rate = sample_rate.context("unreachable: fmt chunk parsed without sample_rate")?;
    let bits_per_sample = bits_per_sample.context("unreachable: fmt chunk parsed without bits_per_sample")?;
    let data = data.with_context(|| format!("{}: missing data chunk", path.display()))?;

    if audio_format != 1 {
        bail!(
            "{}: expected PCM (format 1), found format {}",
            path.display(),
            audio_format
        );
    }
    if channels != 1 {
        bail!("{}: expected mono, found {} channel(s)", path.display(), channels);
    }
    if bits_per_sample != 8 {
        bail!(
            "{}: expected 8-bit samples, found {}-bit",
            path.display(),
            bits_per_sample
        );
    }
    if sample_rate != expected_sample_rate {
        bail!(
            "{}: expected {expected_sample_rate} Hz (the fixed bank sample rate), found {sample_rate} Hz",
            path.display()
        );
    }

    Ok(data)
}

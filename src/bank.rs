use anyhow::{Result, bail};
use std::path::Path;

use crate::wav::read_wav_8bit_mono_pcm;

pub const PIKO_BANK_MAGIC: u32 = 0x4f4b4950;
pub const PIKO_BANK_VERSION: u32 = 2;
pub const PIKO_BANK_HEADER_SIZE: u32 = 12288;
pub const PIKO_BANK_SAMPLE_RATE: u32 = 24000;
pub const PIKO_BANK_MAX_SAMPLES: u32 = 128;
pub const RECORD_SIZE: usize = 64;

/// Reference tempo baked into the firmware (BPM_SAMPLED in src/main.cpp,
/// and the default empty_sample in src/PikoAudioBank.cpp). Every uploaded
/// sample must use this value so playback speed stays consistent — a
/// mismatch here (previously hardcoded as 120) throws off the beat timing.
pub const BPM_SAMPLED: u16 = 165;

pub struct Bank {
    pub blob: Vec<u8>,
    pub sample_count: u32,
    pub audio_bytes: u32,
}

/// Builds a PikoBankHeader + audio blob byte-for-byte matching the layout in
/// src/PikoAudioBank.h (PikoBankHeader / PikoBankSampleRecord).
pub fn build_bank(sample_paths: &[impl AsRef<Path>], capacity_bytes: u32) -> Result<Bank> {
    if sample_paths.len() as u32 > PIKO_BANK_MAX_SAMPLES {
        bail!(
            "too many samples: {} (max {})",
            sample_paths.len(),
            PIKO_BANK_MAX_SAMPLES
        );
    }

    let mut records = Vec::with_capacity(sample_paths.len());
    let mut audio_blob = Vec::new();

    for path in sample_paths {
        let path = path.as_ref();
        let pcm = read_wav_8bit_mono_pcm(path, PIKO_BANK_SAMPLE_RATE)?;
        let offset = audio_blob.len() as u32;
        let frame_count = pcm.len() as u32;
        audio_blob.extend_from_slice(&pcm);

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut name_field = [0u8; 48];
        for (i, b) in name.bytes().take(47).enumerate() {
            // Non-ASCII bytes are replaced, mirroring Python's
            // `.encode("ascii", errors="replace")`.
            name_field[i] = if b.is_ascii() { b } else { b'?' };
        }

        records.push(pack_record(offset, frame_count, BPM_SAMPLED, 1, 255, 0, &name_field));
    }

    let sample_count = records.len() as u32;
    let audio_bytes = audio_blob.len() as u32;
    if audio_bytes > capacity_bytes {
        bail!(
            "sample bank too large: {audio_bytes} bytes of audio exceeds device capacity of {capacity_bytes} bytes"
        );
    }

    let mut header = Vec::with_capacity(PIKO_BANK_HEADER_SIZE as usize);
    header.extend_from_slice(&PIKO_BANK_MAGIC.to_le_bytes());
    header.extend_from_slice(&PIKO_BANK_VERSION.to_le_bytes());
    header.extend_from_slice(&PIKO_BANK_HEADER_SIZE.to_le_bytes());
    header.extend_from_slice(&PIKO_BANK_SAMPLE_RATE.to_le_bytes());
    header.extend_from_slice(&sample_count.to_le_bytes());
    header.extend_from_slice(&audio_bytes.to_le_bytes());
    header.extend_from_slice(&capacity_bytes.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // reserved0

    for record in &records {
        header.extend_from_slice(record);
    }
    let unused_records = PIKO_BANK_MAX_SAMPLES - sample_count;
    header.resize(header.len() + unused_records as usize * RECORD_SIZE, 0);
    header.resize(PIKO_BANK_HEADER_SIZE as usize, 0);

    if header.len() != PIKO_BANK_HEADER_SIZE as usize {
        bail!(
            "internal error: header size {} != {}",
            header.len(),
            PIKO_BANK_HEADER_SIZE
        );
    }

    let mut blob = header;
    blob.extend_from_slice(&audio_blob);

    Ok(Bank {
        blob,
        sample_count,
        audio_bytes,
    })
}

/// Packs one PikoBankSampleRecord (64 bytes): u32 offset, u32 frame_count,
/// u16 source_bpm, u16 beat_count, u8 peak, u8 flags, [u8; 48] name,
/// 2 bytes trailing padding (struct is 4-byte aligned due to the u32
/// members: 4+4+2+2+1+1+48 = 62, rounded up to 64).
fn pack_record(
    offset: u32,
    frame_count: u32,
    source_bpm: u16,
    beat_count: u16,
    peak: u8,
    flags: u8,
    name: &[u8; 48],
) -> [u8; RECORD_SIZE] {
    let mut record = [0u8; RECORD_SIZE];
    record[0..4].copy_from_slice(&offset.to_le_bytes());
    record[4..8].copy_from_slice(&frame_count.to_le_bytes());
    record[8..10].copy_from_slice(&source_bpm.to_le_bytes());
    record[10..12].copy_from_slice(&beat_count.to_le_bytes());
    record[12] = peak;
    record[13] = flags;
    record[14..62].copy_from_slice(name);
    // record[62..64] left as zero padding.
    record
}

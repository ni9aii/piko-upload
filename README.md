# Piko Upload

USB-CDC utility for loading audio samples to pikocore. Rust rewrite of the
original Python prototype.

## Protocol

Uses `/dev/ttyACM0` with commands: `I` (device info) and `W` (write bank).
Only these two are implemented — the firmware's `E`/`R`/`S`/`B`/`C`/`U`
commands are not used by this tool.

Builds WAV files into a binary blob (`PikoBankHeader` + audio data) and
uploads it via a watchdog loop with 256-byte chunks.

## Requirements

- 8-bit mono WAV files
- Device connected via USB-CDC at `/dev/ttyACM0`

## Usage

```bash
cargo build --release
./target/release/piko_upload sample0.wav sample1.wav ...
```

## Known Limitations

- Device path is hardcoded to `/dev/ttyACM0`, no CLI flag yet
- No retry/resume on device hang
- All samples get the same `source_bpm` (must match `BPM_SAMPLED` in the
  firmware, currently 165) and `beat_count=1` — not read from sample
  metadata

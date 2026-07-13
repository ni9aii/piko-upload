# Piko Upload

USB-CDC utility for loading audio samples to pikocore.

## Protocol

Uses `/dev/ttyACM0` with commands: `I`/`W`/`E`/`R`/`S`/`B`/`C`/`U`.

Builds WAV files into a binary blob (PikoBankHeader + audio data) and uploads via watchdog loop with 256-byte chunks.

## Requirements

- 8-bit mono WAV files
- Device connected via USB-CDC (appears as `/dev/ttyACM0`)

## Usage

```bash
# Default
python piko_upload.py /path/to/samples/

# Specify device
python piko_upload.py --device /dev/ttyACM1 /path/to/samples/
```

## Known Limitations

- No CLI flags for device path, sample list, BPM/beat_count metadata
- No retry/resume on device hang
- All samples get hardcoded `source_bpm=120, beat_count=1`

See status-2026-07-13.md for details.

import glob
import os
import select
import struct
import sys
import termios
import time
import tty
import wave

DEV = "/dev/ttyACM0"

PIKO_BANK_MAGIC = 0x4F4B4950
PIKO_BANK_VERSION = 2
PIKO_BANK_HEADER_SIZE = 12288
PIKO_BANK_SAMPLE_RATE = 24000
PIKO_BANK_MAX_SAMPLES = 128
RECORD_SIZE = 64


def read_wav_8bit_mono_pcm(path):
    with wave.open(path, "rb") as w:
        assert w.getsampwidth() == 1
        assert w.getnchannels() == 1
        return w.readframes(w.getnframes())


def build_bank(sample_paths, capacity_bytes):
    records = []
    audio_blob = bytearray()
    for path in sample_paths:
        pcm = read_wav_8bit_mono_pcm(path)
        offset = len(audio_blob)
        audio_blob += pcm
        name = os.path.splitext(os.path.basename(path))[0]
        name_bytes = name.encode("ascii", errors="replace")[:47]
        name_field = name_bytes + b"\x00" * (48 - len(name_bytes))
        record = struct.pack("<IIHHBB48s2x", offset, len(pcm), 120, 1, 255, 0, name_field)
        assert len(record) == RECORD_SIZE
        records.append(record)

    sample_count = len(records)
    audio_bytes = len(audio_blob)
    assert audio_bytes <= capacity_bytes

    header = struct.pack("<IIIIIIII", PIKO_BANK_MAGIC, PIKO_BANK_VERSION, PIKO_BANK_HEADER_SIZE,
                          PIKO_BANK_SAMPLE_RATE, sample_count, audio_bytes, capacity_bytes, 0)
    header += b"".join(records)
    header += b"\x00" * ((PIKO_BANK_MAX_SAMPLES - sample_count) * RECORD_SIZE)
    header += b"\x00" * (PIKO_BANK_HEADER_SIZE - len(header))
    assert len(header) == PIKO_BANK_HEADER_SIZE

    return header + bytes(audio_blob), sample_count, audio_bytes


def read_available(fd, timeout, quiet_gap=0.3):
    data = b""
    deadline = time.time() + timeout
    while time.time() < deadline:
        r, _, _ = select.select([fd], [], [], 0.05)
        if fd in r:
            chunk = os.read(fd, 65536)
            if chunk:
                data += chunk
                deadline = time.time() + quiet_gap
    return data


def read_exact(fd, n, timeout):
    data = b""
    deadline = time.time() + timeout
    while len(data) < n and time.time() < deadline:
        r, _, _ = select.select([fd], [], [], 0.1)
        if fd in r:
            chunk = os.read(fd, n - len(data))
            if chunk:
                data += chunk
    return data


def send_with_watchdog(fd, data, chunk_size=256, stall_timeout=15.0, log_every=20):
    total = len(data)
    sent = 0
    last_progress_t = time.time()
    chunk_i = 0
    while sent < total:
        chunk = data[sent:sent + chunk_size]
        # write in a loop handling partial writes, each partial write resets the stall clock
        view = memoryview(chunk)
        while view:
            r, w, _ = select.select([], [fd], [], 1.0)
            if fd in w:
                n = os.write(fd, view)
                view = view[n:]
                sent += n
                last_progress_t = time.time()
            else:
                if time.time() - last_progress_t > stall_timeout:
                    raise TimeoutError(f"stalled: no write progress for {stall_timeout}s at {sent}/{total} bytes")
        chunk_i += 1
        if chunk_i % log_every == 0 or sent == total:
            print(f"  sent {sent}/{total} ({100*sent/total:.1f}%)", flush=True)
    return sent


def main():
    if len(sys.argv) < 2:
        print("usage: piko_upload2.py <wav files...>")
        sys.exit(1)
    sample_paths = sys.argv[1:]
    print(f"Uploading {len(sample_paths)} sample(s): {[os.path.basename(p) for p in sample_paths]}")

    fd = os.open(DEV, os.O_RDWR | os.O_NOCTTY)
    try:
        tty.setraw(fd)
        attrs = termios.tcgetattr(fd)
        attrs[3] &= ~(termios.ECHO | termios.ECHOE | termios.ECHOK | termios.ICANON)
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        termios.tcflush(fd, termios.TCIOFLUSH)
        time.sleep(0.3)
        termios.tcflush(fd, termios.TCIOFLUSH)

        write_view = memoryview(b"I")
        os.write(fd, write_view)
        info = read_available(fd, 2.0)
        print("Device info:", info.decode(errors="replace"))
        text = info.decode(errors="replace")
        parts = text.split()
        capacity_bytes = int(parts[parts.index("C") + 1])

        blob, sample_count, audio_bytes = build_bank(sample_paths, capacity_bytes)
        total_len = len(blob)
        print(f"sample_count={sample_count} audio_bytes={audio_bytes} total_len={total_len}")

        os.write(fd, b"W")
        os.write(fd, struct.pack("<I", total_len))
        resp = read_exact(fd, 3, 5.0)
        print("write-init resp:", resp)
        if resp != b"OK\n":
            extra = read_available(fd, 1.0)
            print("REJECTED, extra:", resp + extra)
            sys.exit(1)

        t0 = time.time()
        try:
            send_with_watchdog(fd, blob, chunk_size=256, stall_timeout=15.0)
        except TimeoutError as e:
            print(f"UPLOAD STALLED: {e}")
            sys.exit(2)
        dt = time.time() - t0
        print(f"payload sent in {dt:.1f}s ({total_len/dt:.0f} B/s), waiting for final ack...")

        final = read_available(fd, 60.0, quiet_gap=2.0)
        print("final resp:", final)
    finally:
        os.close(fd)


if __name__ == "__main__":
    main()

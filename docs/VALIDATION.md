# Validation record

Local Linux x86_64 container, Rust 1.99.1, 2026-10-26. Synthetic input only.

- Rust unit tests: 2 passed; formatting and Clippy with warnings denied passed.
- Real HTTP/process integration tests: 8 passed, covering capability separation,
  roundtrip, persistent replay rejection, bounded retention, concurrent duplicate
  submissions, strict framing, malformed/oversized requests, slow header deadline,
  crash watermark recovery, corruption and config permissions.
- Provider adapter unit tests: 5 passed; fake provider only, including timeout
  without retry and oversized response. No real provider/paid calls.
- Host benchmark: 200 sequential 60,288-byte synthetic JPEG uploads, fsync enabled.
  See `benchmark.json` for measured latency, resident memory, binary size and
  retained bytes. These are container loopback numbers, not camera/TLS performance
  and not a guarantee of durable physical media latency in a virtual filesystem.
- Firmware build: local Docker image unpack failed due to Docker storage space;
  GitHub Actions firmware job provides a separate build check (see Actions).
- Physical ESP32-CAM, firmware flashing, deployment and real footage: not tested
  or performed. TLS on a physical board and real provider calls are untested.

No security audit or independent human code review is claimed. Tests and this
implementation were produced with AI assistance.

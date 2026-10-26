# Validation record

Local Linux x86_64 container, Rust 1.99.1, 2026-10-26. Synthetic input only.

- Rust unit tests: 3 passed (including injected ambiguous storage failure); formatting and Clippy with warnings denied passed.
- Real HTTP/process integration tests: 10 passed, covering capability separation,
  roundtrip, persistent replay rejection, bounded retention, concurrent duplicate
  submissions, strict framing, malformed/oversized requests, slow header deadline,
  crash watermark recovery, corruption and config permissions.
- Provider and Meta policy/budget tests: 18 passed; fake providers only, including prompt-injection containment, strict schemas,
  no model authority over deterministic policy, timeout
  without retry and oversized response. No real provider/paid calls.
- Host benchmark: 200 sequential 70,635-byte synthetic JPEG uploads, fsync enabled.
  See `benchmark.json` for measured latency, resident memory, binary size and
  retained bytes. These are container loopback numbers, not camera/TLS performance
  and not a guarantee of durable physical media latency in a virtual filesystem.
- Chromium browser smoke: passed login, JPEG decoding, mobile layout, no token
  persistence, lock session, and no JavaScript exceptions, and visible simulated AI incident report. See `tests/browser.py`
  (optional Playwright/system Chromium test dependencies) and `dashboard.png`.
- Firmware reference build: passed in GitHub Actions with ESP-IDF 5.4.2 after
  adding the missing `esp_timer` component requirement. Verified run:
  https://github.com/jveliz-08/X-Lens/actions/runs/40372042440
  (`d6f1760`, reference AI-Thinker map only). Explicit board selection also
  compiled in firmware job 121747529442 of run 40372748238 (`7d1c971`). Linker report: 1,000,410-byte image,
  103,834 bytes IRAM and 47,052 bytes static DRAM. These are build-time values,
  not runtime heap/TLS measurements. The ESP-32S/OV3660 target routing remains unverified.
  Local Docker image unpack failed due to Docker storage space; CI supplied
  the actual compiler/build validation instead.
- Physical ESP32-CAM, firmware flashing, deployment and real footage: not tested
  or performed. TLS on a physical board and real provider calls are untested.

No security audit or independent human code review is claimed. Tests and this
implementation were produced with AI assistance.

Publication checks: tracked content contains no runtime secrets, build outputs,
vendor source or private configured email. A targeted private-key/token-pattern
scan had no findings (not a comprehensive secret audit). Original synthetic
fixture/screenshot and MIT project code only. Commits use the verified GitHub
account's public noreply address; they are **unsigned**, not cryptographically
verified signatures. No signing keys or GitHub credentials were created.

An intermediate host CI run exposed a test bug: a single TCP `recv` was assumed
to contain an entire HTTP status line. The timeout test now assembles the line
across segments before asserting its content. The absolute deadline assertion
remains in place. Earlier firmware CI failures for the `esp_timer` dependency
and ESP-IDF's early component-discovery phase were fixed in separate commits;
those failed runs are retained in the public history, not hidden.

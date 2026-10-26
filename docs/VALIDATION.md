# Validation record

Local Linux x86_64 container, Rust 1.99.1, 2026-10-26. Synthetic input only.

- Rust unit tests: 3 passed (including injected ambiguous storage failure); formatting and Clippy with warnings denied passed.
- Real HTTP/process integration tests: 14 passed, covering capability separation,
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

Latest full implementation check: [CI run 40375213040](https://github.com/jveliz-08/X-Lens/actions/runs/40375213040)
passed both host and firmware jobs for `9f03ee4d371a55f592520466fa0bb1b345b8027b`.
This includes explicit pin-map selection, the OV3660 runtime PID expectation,
correlated incident reports and locally computed evidence fingerprints. Later
README-only clarification does not change those executable paths. The standalone
synthetic demo command was also run end to end and produced a `mock` report with
local `review` policy and `actions_executed: false`.

## Local workflows extension

2026-10-26: formatting, Clippy (warnings denied), 3 Rust tests, 14 HTTP/process
integration tests and 18 provider/budget tests passed. Added protocol negotiation,
authenticated rate-limit recovery, device sequence recovery, rule capability
separation, schema rejection, capacity safety and restart persistence checks.
Chromium fake media and stubbed recognition exercise permission rejection,
preview without upload, authenticated snapshots, stopping late permissions,
navigation/logout/pagehide cleanup, rule draft cancellation/edit invalidation,
explicit save/disable, unsupported speech fallback, consent, transcription without
activation, stale/recovered network state, mobile width and no credential storage.
No actual camera, microphone, speech vendor or Meta service was accessed.
Playwright 1.62.0 and Chromium are test-only dependencies; CI now includes this
browser suite. Screenshots `dashboard.png` and `rules.png` show synthetic input.
The benchmark was rerun with 50 ms pacing to respect the new upload budget;
reported requests/second include deliberate pacing and are not peak throughput.
See the current `benchmark.json` for this version's measurements. Firmware CI
continues to compile the reference mapping only, not validate physical routing.

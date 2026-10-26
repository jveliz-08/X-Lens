# X-Lens

**Small cameras. Your infrastructure.** A minimal, self-hosted JPEG snapshot
platform: C firmware for explicitly configured ESP32-CAM boards, a Rust origin server with **zero
external crates**, and a plain HTML/CSS/JS dashboard.

This is an experimental v0, not an audited security product. No H.264, facial
recognition, cloud dependency, or unmeasured FPS promise. Vendor camera/Wi-Fi/TLS
components and a standard TLS reverse proxy are necessary dependencies.

**Board status:** the user’s “regular ESP32-CAM” model is not yet identified.
AI-Thinker is only an explicit build reference, not a compatibility claim.
See [board selection](docs/HARDWARE.md).

## Local synthetic demo

Requires Rust stable and Python 3. No Python packages needed.

```sh
cargo build --release
python3 tools/setup_demo.py
# Terminal 1
./target/release/x-lens runtime/config.secret runtime/frames
# Terminal 2
python3 tools/simulate.py --count 3
```

Open http://127.0.0.1:8080 and enter the `viewer` value from
`runtime/config.secret`. Tokens stay in tab memory. Lock the session when done.
The simulator sends an original synthetic test-pattern JPEG, not camera footage.
Do not put secrets in screenshots, shell history, URLs, or Git.

The server binds only to loopback. HTTPS at a trusted local reverse proxy is
mandatory for cameras or remote dashboard access. See the deployment, protocol,
and hardware documentation as added in the following implementation commits.

## AI-assisted development

This project is being developed with AI assistance, including implementation,
documentation, and test generation. Human maintainers remain responsible for
reviewing changes and deciding whether the system is suitable for their use.
Automated checks do not constitute an independent security audit. Hardware
operation, electrical safety, sustained capture, and real-world reliability have
**not been validated on a physical ESP32-CAM**. Do not treat generated code or
passing host tests as proof of production security.

![Dashboard showing an original synthetic test signal](docs/dashboard.png)

## Architecture

```text
ESP32-CAM (C, JPEG, one framebuffer, device token)
    │ verified HTTPS, bounded POST, persistent sequence
    ▼
Caddy (standard TLS boundary) → 127.0.0.1:8080 Rust origin
                                  ├── private bounded snapshot store
                                  ├── authenticated browser dashboard
                                  └── manual server-side API adapter (off by default)
```

The camera never sees external AI/API keys. The standard-library Python adapter
supports **Meta Model API** image understanding and an advisory cybersecurity
review of bounded numeric telemetry. The concrete Meta wire format follows
[official documentation](https://dev.meta.ai/docs/protocols/chat-completions);
only synthetic/mock tests have run, not real Meta calls. The user's event API,
account entitlement and model ID still need confirmation. A generic JSON image
adapter remains optional; it is not labeled as a Meta integration.

```sh
python3 tools/analyze.py --mode image       # dry-run: no external call
python3 tools/analyze.py --mode security    # local policy and request preview
```

The model can suggest a human review of authentication failures, repeated
sequences or invalid frames. It **cannot generate credentials, authenticate
users, execute commands, or quarantine devices**. Local deterministic rules
remain authoritative; every consequential response requires human review. See
[Meta AI and security review](docs/META_AI.md) and [setup](docs/SETUP.md).

| Component | Deliberate budget |
|---|---|
| Origin binary | Zero third-party Rust crates; four worker threads |
| JPEG / headers | 128 KiB / 4 KiB maximum |
| Devices / retention | 16 devices / 32 snapshots each |
| Camera | One JPEG framebuffer; one HTTPS upload at a time |
| AI requests | Manual, one at a time, bounded input/output, no automatic retries |

## Documentation and validation

- [Setup, TLS and API integration](docs/SETUP.md)
- [Firmware build, provisioning and hardware checklist](docs/HARDWARE.md)
- [Protocol, limits and crash semantics](docs/PROTOCOL.md)
- [Threat model and limitations](docs/SECURITY.md)
- [Measured host benchmark](docs/benchmark.json) and [validation record](docs/VALIDATION.md)
- [CI](https://github.com/jveliz-08/X-Lens/actions)

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
python3 tests/integration.py
python3 -m unittest discover -s tests -p 'test_*.py' -v
python3 tools/benchmark.py
```

Stop other instances on port 8080 before integration tests/benchmark. Tests use
private temporary directories and synthetic images. No deployment, camera
access, firmware flashing, or real external AI calls are required.

Original project code is MIT licensed. ESP-IDF, camera driver, Caddy and toolchain
components retain their upstream licenses; they are not copied into this repo.

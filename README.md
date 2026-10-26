# X-Lens

**Small cameras. Your infrastructure.** A minimal, self-hosted JPEG snapshot
platform: C firmware for AI-Thinker ESP32-CAM, a Rust origin server with **zero
external crates**, and a plain HTML/CSS/JS dashboard.

This is an experimental v0, not an audited security product. No H.264, facial
recognition, cloud dependency, or unmeasured FPS promise. Vendor camera/Wi-Fi/TLS
components and a standard TLS reverse proxy are necessary dependencies.

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
The simulator sends an original tiny synthetic JPEG, not camera footage.
Do not put secrets in screenshots, shell history, URLs, or Git.

The server binds only to loopback. HTTPS at a trusted local reverse proxy is
mandatory for cameras or remote dashboard access. See the deployment, protocol,
and hardware documentation as added in the following implementation commits.

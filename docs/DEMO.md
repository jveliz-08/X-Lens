# X-Lens demo: cheap capture, explainable AI review

## The core idea

A tiny ESP32 camera only captures/compresses/uploads a bounded JPEG. The
self-hosted host handles storage, device security telemetry and a selective
Meta AI review. One review correlates what is visible with what the device is
doing, rather than paying for inference on every frame. The dashboard presents
an incident summary, uncertain visual observations and review suggestions. A
separate deterministic gate keeps the model out of authentication and control.

This is a prototype with a real host pipeline and a compiled reference firmware,
not a claim of tested hardware, validated attack detection, event eligibility,
or live Meta integration. The event is Meta Global AI Developer Hackathon; API entitlement and exact
submission compliance still need operator confirmation.

## Reproducible local demonstration

```sh
cargo build --release
python3 tools/setup_demo.py
./target/release/x-lens runtime/config.secret runtime/frames
```

In a second terminal:

```sh
python3 tools/demo_incident.py
```

Open `http://127.0.0.1:8080`, enter the local viewer token from the private config,
and explain the sequence:

1. A synthetic 320×240 JPEG travels through device authentication into bounded
   storage. There is no real camera or private footage.
2. Five authenticated duplicate sequence submissions are rejected. The telemetry
   records the signal without storing credentials or unbounded attacker logs.
3. Incident mode combines the JPEG and sanitized numeric counters in the same
   request format used by the Meta adapter.
4. The demo injects a **SIMULATED AI** response, then runs the real strict schema
   validation, deterministic policy and private report persistence.
5. The dashboard shows the mock source prominently, a correlated explanation,
   `review` as local policy and **no actions executed**. This is not real model
   reasoning, and replay counters are not proof of intrusion.

The exact pipeline is also covered by an HTTP/process integration test.

## Enabling real model inference later

After participant model availability and account entitlement are confirmed, an
operator can provide a server-side Meta key and run an explicitly authorized
synthetic `--send` review. The implemented official Meta Model API path is
specified in [META_AI.md](META_AI.md); no live call has been performed here.
Changing `--mock` to `--send --model ...` uses the actual provider transport,
shared validation and the same report pipeline. Unknown outcomes are never
retried automatically. Budget slots are reserved before sending.

## What makes AI substantive here

AI consumes **both visual context and cybersecurity signals** and produces a
single explainable incident hypothesis. It can help an operator distinguish
ordinary counter recovery or a confusing scene from something worth reviewing,
while admitting uncertainty. It does not invent cryptographic tokens or become
authoritative merely because it sounds confident. Secure randomness, replay
rejection and deterministic limits work even when the model is offline.

The value of the correlation must still be demonstrated with a real model and
representative consented test cases. A synthetic fixture tests integration, not
accuracy. The ESP-32S/OV3660 target pinout also remains to be confirmed before any
physical run. No cloud deployment or flashing is part of this demo.

## Event alignment, not an eligibility guarantee

The [official Meta event page](https://dev.meta.ai/events/global-hackathon)
lists Muse Spark-series models through Meta Model API and Meta open weights as
building options. X-Lens uses the documented Meta Model API adapter, with a
substantive image-plus-device-signal incident workflow. The [official rules](https://cerebralvalley.ai/e/meta-ai-developer-hackathon/waiver)
should be reviewed by the participant for submission requirements. This project
has not claimed event credits, verified account access, submitted an entry, or
certified eligibility. The next validation is an explicitly authorized synthetic
live call using the participant's verified model/access, then hardware testing.

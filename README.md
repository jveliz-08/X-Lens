<p align="center">
  <img src="docs/assets/banner.svg" alt="X-Lens: a $7 camera that understands" width="100%">
</p>

<p align="center">
  <img alt="Meta Muse Spark" src="https://img.shields.io/badge/Meta-Muse%20Spark%201.3-0866FF?style=for-the-badge&logo=meta&logoColor=white">
  <img alt="SAM 3.1" src="https://img.shields.io/badge/Meta-SAM%203.1-0866FF?style=for-the-badge&logo=meta&logoColor=white">
  <img alt="Muse Voice Transcribe" src="https://img.shields.io/badge/Meta-Muse%20Voice%20Transcribe-0866FF?style=for-the-badge&logo=meta&logoColor=white">
  <br>
  <img alt="ESP32-CAM" src="https://img.shields.io/badge/ESP32--CAM-ESP--IDF%205.4-E7352C?style=flat-square&logo=espressif&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-zero%20crates-000000?style=flat-square&logo=rust&logoColor=white">
  <img alt="Vanilla JS" src="https://img.shields.io/badge/dashboard-vanilla%20HTML%2FJS-F7DF1E?style=flat-square&logo=javascript&logoColor=black">
  <img alt="Tests" src="https://img.shields.io/badge/tests-11%20%2B%2019%20%2B%2037%20%2B%20browser-2EA44F?style=flat-square">
  <img alt="License MIT" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square">
</p>

<p align="center">
  <b>📷 Motion detected on the chip · 🏠 clips that stay home · ✦ one Meta AI call per event</b><br>
  <a href="#demo">Demo</a> ·
  <a href="#features">Features</a> ·
  <a href="#meta-ai">Meta AI</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#safety">Safety</a>
</p>

<p align="center">
  <a href="https://www.youtube.com/watch?v=POObw2y0e_0">
    <img src="https://img.youtube.com/vi/POObw2y0e_0/maxresdefault.jpg" alt="▶ Watch the X-Lens demo on YouTube (3 min)" width="85%">
  </a>
  <br>
  <a href="https://www.youtube.com/watch?v=POObw2y0e_0"><b>▶️ Watch the 3-minute demo on YouTube</b></a>
</p>

---

X-Lens turns a **$7 ESP32-CAM** into a private camera that *understands* what it sees.
The board detects motion **on the chip itself**, streams only when someone is watching
or something moves, and your own host records short clips. **Meta Muse Spark** reads each
event once, describes it, checks the watches you wrote in plain words and **learns from
your corrections**. **SAM 3.1** finds the tree that keeps triggering the camera, **Muse
Voice Transcribe** lets you ask by voice, and **routines** alert you when someone who
lives alone hasn't moved by the time they usually do.

> 💚 **Built for your world:** families who worry about a relative living alone, and anyone
> who wants to know what happens at home without streaming their life to someone else's cloud.

<a id="demo"></a>

## 🎬 Demo

<table>
<tr>
<td width="56%"><img src="docs/assets/walk-in.gif" alt="Someone walks in: the tile turns REC and the motion meter fills" width="100%"></td>
<td>

### ▶️ [Watch the 3-minute video on YouTube](https://www.youtube.com/watch?v=POObw2y0e_0)

Recorded from the real dashboard by
[`tools/record_demo.py`](tools/record_demo.py)
(also in the repo: [`docs/demo.mp4`](docs/demo.mp4)).

1. 🚶 Someone walks in. The **board** detects it.
2. 🎬 A clip with the seconds *before* is saved.
3. ✦ Muse Spark describes the event, once.
4. 🗣️ You ask; the answer cites the clips.
5. 🧓 A routine tells you grandma is up.

**Try it without hardware:**
```sh
runtime/venv/bin/python tools/record_demo.py --serve
```
Press <kbd>Enter</kbd> and someone walks in.

</td>
</tr>
</table>

<a id="features"></a>

## ✨ What it does

<table>
<tr>
<td width="50%" valign="top">

### 📷 Edge motion on a $7 board
The ESP32-CAM decodes its own JPEG at **1/8 scale in 28 ms**, compares it with an
adaptive background (ignoring exposure shifts) and sends the score in one header.
No extra bandwidth, no server-side image processing.

</td>
<td width="50%" valign="top">

### ⚡ Streams only on demand
**~1 fps** when nobody watches and nothing moves; **full rate** the moment someone
opens the feed or something moves. The server tells the board how many viewers it has.

</td>
</tr>
<tr>
<td valign="top">

### 🎬 Clips with pre-roll
Every event is saved with the **seconds before** the motion started and a keyframe at
the motion peak, on *your* host. See how it started, not just a still.

</td>
<td valign="top">

### ✦ One AI call per event
Muse Spark gets three frames (start, peak, latest) and returns a caption, an activity
type and watch checks in a **strict JSON schema**. Never per frame.

</td>
</tr>
<tr>
<td valign="top">

### 👁️ Watches in plain words
*"Alert me if a package is left at the door."* Checked on the same call. A match raises
a banner, a toast, a browser notification and an optional spoken warning 🔊.

</td>
<td valign="top">

### 🧠 Watches that learn
Mark an alert **✓ Right** or **✗ False alarm**, or *"should have alerted"*. That keyframe
travels with the next requests as a reference example. **In-context learning, zero training.**

</td>
</tr>
<tr>
<td valign="top">

### 🗣️ Ask X-Lens, by text or 🎤 voice
*"Did anyone come in while I was out?"* Answered from the live frame and the event log,
with **playable clip citations**. *"Alert me if…"* becomes a watch in one click.

</td>
<td valign="top">

### 📖 The day in one paragraph
One call reads the day's events, a few keyframes and your routines, and writes a short,
factual story with links to every clip.

</td>
</tr>
<tr>
<td valign="top">

### 🗺️ SAM 3.1 ignore zones
Type *"tree"*: SAM 3.1 segments it, X-Lens turns the boxes into an 8×6 grid and the
**microcontroller applies the mask itself**. No more events from a swaying branch.

</td>
<td valign="top">

### 🧓 Routines for loved ones
*"Expect motion between 07:00 and 10:00."* First motion → ✓ done. Nothing by 10:00 →
⚠ alert. **The absence of activity is the signal**, and it needs no AI at all.

</td>
</tr>
</table>

## 🖼️ Screenshots

| 🔴 Live: someone walks in, the board records | 🗣️ Ask, with cited clips and "create watch" |
|---|---|
| ![Live view: the hallway camera is recording event #1](docs/assets/screen-live.png) | ![Ask X-Lens answer with a clip citation and a created watch](docs/assets/screen-ask.png) |
| **🧓 Routines, the day's summary and the timeline** | **🗺️ Painting ignore zones (or letting SAM 3.1 find them)** |
| ![Activity: routines today, daily summary and the event timeline](docs/assets/screen-activity.png) | ![Ignore-zone editor with the window painted out](docs/assets/screen-zones.png) |

> 📸 Captured from the real dashboard with the synthetic hallway camera of the demo recorder.
> AI text in them is the labeled simulation mode used when no key is configured.

<a id="meta-ai"></a>

## ✦ How Meta AI is used

| | Model | What it does in X-Lens | Calls |
|---|---|---|---|
| ✦ | **Muse Spark** `muse-spark-1.3` · Chat Completions · strict `json_schema` | Event captions, watch checks with learned examples, Ask, daily summary, rule drafts, incident review | 1 per event / question / summary |
| ◎ | **SAM 3.1** · Responses API · text prompt | "tree" → boxes → ignore-zone mask for the board | 1 per *Find* click |
| 🎙️ | **Muse Voice Transcribe** `muse-voice-transcribe-1.0` | Push-to-talk questions (16 kHz WAV) | 1 per question |

🔑 **One key, pasted once** in *Watches & rules → Meta AI*. It is stored on your host
(mode 600), never sent back to a browser, and works without a restart. Without a key
every feature still runs in clearly labeled simulation.

<details>
<summary><b>🔍 See the exact request Muse Spark receives for an event</b></summary>
<br>

<img src="docs/assets/request.png" alt="The Muse Spark request: three frames, the watch, a labeled reference frame and a strict JSON schema" width="100%">

Generated by [`tools/vision.py`](tools/vision.py) (images elided). Images and user text are
sent as untrusted data; anything outside the schema is rejected.
</details>

<a id="architecture"></a>

## 🏗️ Architecture

<p align="center"><img src="docs/assets/architecture.svg" alt="ESP32-CAM to Rust host to Meta Model API, with the dashboard" width="100%"></p>

<details>
<summary><b>📏 Budgets and limits</b></summary>

| Budget | Value |
|---|---|
| 🤖 AI | ≤1 automatic analysis per camera per 10 s; 300 vision calls/day ledger, ≥2 s apart; no retries |
| 🎬 Events | 300 events / 512 MiB; clips ≤400 frames or 6 MiB; pre-roll ≤12 frames / 4 s |
| 📷 Devices | 16 devices / 32 snapshots each (≤1 stored per second) |
| 👀 Viewers | 16 concurrent MJPEG streams, 10-minute reconnect |
| 🔌 Camera | QVGA JPEG, two framebuffers, one in-flight upload, ≤15 fps cap, ~1 fps idle |
| 🎙️ Voice | ≤12 s per question, ≤512 KiB WAV, 200 transcriptions/day |
| 🗺️ Zones | 8×6 grid, at most ¾ of the scene ignored |

</details>

<a id="quick-start"></a>

## 🚀 Quick start

Requires Rust stable and Python 3 (no Python packages for the server).

```sh
cargo build --release
python3 tools/setup_demo.py     # 🔐 private tokens
./target/release/x-lens runtime/config.secret runtime/frames
```

1. 🌐 Open http://127.0.0.1:8080 and unlock with the `viewer` token (plus `operator` to use AI).
2. 📷 Add a camera: flash an ESP32-CAM ([firmware](docs/HARDWARE.md)) or use **Sources → webcam**.
3. 🔑 Paste your Meta key in **Watches & rules → Meta AI**. Done.

Remote and camera access go through the TLS proxy ([setup](docs/SETUP.md)).

<a id="safety"></a>

## 🛡️ Safety by design

- 🙅 **The model describes and suggests; it never acts.** A watch match only creates an alert.
- 📐 Every output is parsed against a **strict schema**; unknown fields, IDs and activities are rejected.
- 🧱 Images and user text are **untrusted data**, never instructions; people are never identified.
- 🔑 Spending AI budget, setting the key or editing watches needs the **operator token**.
- 🏠 Footage stays on your host; only the frames of an event go to Meta, only when a key is set.
- 🔁 Replay protection, authentication and storage bounds work with AI disabled.

More in [Meta AI](docs/META_AI.md), [protocol](docs/PROTOCOL.md) and [security](docs/SECURITY.md).

## 🔧 Hardware tested

**ESP32-CAM** (ESP32-D0WD-V3, OV3660, AI-Thinker pin map, 8 MB PSRAM) on a home mesh network:
edge motion, demand-driven rate, events, clips, the zone-mask firmware and the live dashboard
ran on the real board, with strongest-node Wi-Fi selection and self-healing reconnects.

| 📏 Measured | Value |
|---|---|
| On-chip motion decode | ~28 ms per frame |
| Live rate while watched | 5–7 fps (QVGA) |
| Idle rate | ~1 fps |
| Wi-Fi signal after strongest-node fix | −45 to −60 dBm |

Live Meta calls were **not** made during development; the wire formats (Chat Completions,
SAM 3.1 event stream, ASR multipart) follow Meta's documentation and are covered by tests
with fake providers.

## 🧪 Tests

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test                                    # 🦀 11 unit tests
cargo build --release
python3 tests/integration.py                  # 🔌 19 real-socket tests
python3 -m unittest discover -s tests -p 'test_*.py'   # 🐍 37 adapter tests
runtime/venv/bin/python tests/browser.py      # 🌐 full browser workflow
```

Stop other instances on port 8080 first. Tests use private temporary directories and
synthetic images: no camera, flashing or real AI calls needed.

## 📚 Documentation

| | |
|---|---|
| 🏆 [Hackathon criteria and checklist](docs/HACKATHON.md) | 🎤 [Demo script](docs/DEMO.md) · 📝 [Submission kit](docs/SUBMISSION.md) |
| ✦ [Meta AI integration](docs/META_AI.md) | 🔌 [Protocol and limits](docs/PROTOCOL.md) |
| 📷 [Firmware and hardware](docs/HARDWARE.md) | 🌐 [Setup and TLS](docs/SETUP.md) |
| 🛡️ [Threat model](docs/SECURITY.md) | 🧪 [Validation record](docs/VALIDATION.md) · [benchmark](docs/benchmark.json) |

<details>
<summary><b>➕ Also included</b></summary>

- 🚨 **Security incident review** (`tools/analyze.py --mode incident`): one Meta request
  correlates the latest JPEG with authentication/replay telemetry; a deterministic policy
  gate keeps the model out of access control.
- 📏 **Signal rules and voice drafts**: replays, invalid frames, auth failures and stale
  cameras raise local panel events; Meta can draft a rule an operator confirms
  ([local workflows](docs/LOCAL_WORKFLOWS.md)).
- 💻 **Browser webcam source**: any laptop or phone camera becomes an X-Lens source with
  the same authenticated ingest and motion detection.
</details>

## 🤖 AI-assisted development

This project is developed with AI assistance, including implementation, documentation and
tests. Human maintainers remain responsible for reviewing changes and deciding whether the
system suits their use. Automated checks are not an independent security audit. The
firmware ran on one physical board for the measurements in [hardware notes](docs/HARDWARE.md);
electrical safety and long-term reliability are not validated.

---

<p align="center">
  Original project code is MIT licensed. ESP-IDF, camera driver, Caddy and toolchain
  components keep their upstream licenses and are not copied into this repo.<br>
  <b>X-Lens</b> · made with 💚 for the Meta Global AI Developer Hackathon
</p>

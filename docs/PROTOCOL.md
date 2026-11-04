# Protocol version 1

Transport: HTTP/1.1 over verified HTTPS at the proxy. The local Rust origin uses
HTTP only on `127.0.0.1:8080`. No custom cryptography. Each origin connection handles one
request and closes (the TLS proxy keeps device connections alive, so cameras pay
one TLS handshake, not one per frame); chunked requests, duplicate headers, `Expect`, folded
headers, oversized headers, and conflicting framing are rejected.

| Operation | Authorization | Request / result |
|---|---|---|
| `POST /api/frame/{id}` | Device-specific bearer | Raw JPEG, `Content-Type: image/jpeg`, exact `Content-Length`, `X-Sequence: <positive u64>`, optional `X-Motion: 0..1000` (per mille of changed cells, computed by the producer); 201 accepted, 409 stale/replay; response carries `X-Live-Viewers: <n>` so cameras stream at full rate only on demand, and `X-Motion-Mask: <12 hex>` (8×6 ignore grid, bit `row*8+col`) that producers apply when scoring motion |
| `GET /api/review/{id}` | Viewer bearer | Latest bounded AI report, with explicit mock/live source; 404 until reviewed |
| `GET /api/security` | Viewer bearer | Bounded numeric counters since process start; no tokens, IPs, raw logs or footage |
| `GET /api/devices` | Viewer bearer | JSON device ID, latest stored sequence, retained count |
| `GET /api/latest/{id}` | Viewer bearer | Latest JPEG (live frame, else newest stored snapshot) and `X-Sequence` response header; 404 until a frame exists |
| `GET /api/stream/{id}` | Viewer bearer | Live `multipart/x-mixed-replace; boundary=frame` stream: each new frame as an `image/jpeg` part with `Content-Length`, `X-Sequence`, `X-Age-Ms`; empty `text/plain` heartbeat parts after 5 s idle; closed after 10 minutes (clients reconnect); 503 above 16 streams |
| `GET /api/activity` | Viewer bearer | `{events, devices, ai, feedback, zones, routines, notices, digest}`: latest 120 motion events (start/end unix, peak, frames, pending/failed, AI annotation with alerts), per-device motion/active event/viewers, AI mode/model/key configured (boolean only)/calls used today/language |
| `GET /api/clip/{event}` | Viewer bearer | Event clip: `XLC1`, then per frame u32 LE offset ms, u32 LE length, JPEG |
| `GET /api/keyframe/{event}` | Viewer bearer | JPEG at the motion peak; 404 while recording |
| `GET /api/watches` | Viewer bearer | Visual watches: id, device, prompt, cooldown, enabled |
| `POST /api/watches/{id}` | Operator bearer, `text/plain` | Exactly `device`, `prompt` (3–200 chars, one line), `cooldown` (30–86400), `enabled` (0/1); body `delete=1` removes; 204, 400, 404, 422 above 8 watches |
| `POST /api/analyze/{event}` | Operator bearer | Queue (re)analysis of a finished event; 202, or 409 if recording/pending/missing |
| `POST /api/ask/{device}` | Operator bearer, `text/plain`, optional `X-Utc-Offset` minutes | Question (≤300 chars) answered from the current frame and event log: `{source, answer, answerable, events, watch}`; 502 when busy, over budget or outcome unknown |
| `POST /api/ai-key` | Operator bearer, `text/plain` | Store the Meta key (8–4096 printable ASCII) in a mode-600 file; whitespace-only body removes it; never readable back |
| `GET /api/zones` | Viewer bearer | `{device: "<12 hex mask>"}` for every device |
| `POST /api/zones/{device}` | Operator bearer, `text/plain` | `mask=<12 hex>`; at most 36 of 48 cells ignored; `000000000000` clears; 204 or 400 |
| `POST /api/zones-suggest/{device}` | Operator bearer, `text/plain` | A concept (2–60 chars, e.g. `tree`); SAM 3.1 on the latest frame → `{source, concept, mask, boxes}` (boxes normalized 0–1); proposal only, nothing saved; 409 without a frame, 502 on provider failure |
| `POST /api/feedback/{event}` | Operator bearer, `text/plain` | Exactly `watch=<id>` and `verdict=right\|wrong\|clear` for a finished event; up to 64 labels; labeled keyframes become reference examples (≤3 per analysis, ≤2 per watch); 204, 400, 404 |
| `POST /api/routines/{id}` | Operator bearer, `text/plain` | Exactly `device`, `label` (3–80 chars), `from`, `to` (local minutes, `from < to ≤ 1440`), `offset` (UTC offset minutes, −840..840), `enabled`; `delete=1` removes; 204, 400, 404, 422 above 8 |
| `POST /api/digest` | Operator bearer, `text/plain`, required `X-Utc-Offset` | Summary of the local day: `{source, title, summary, highlights[{event, text}], day, at}`; stored and returned by `/api/activity` |
| `POST /api/transcribe` | Operator bearer, `Content-Type: audio/wav`, optional `X-Language` | 16/24 kHz mono 16-bit PCM WAV, ≤512 KiB (the only body allowed above 128 KiB) → `{source, text}` via Muse Voice Transcribe; 503 without a key (the dashboard then uses the browser recognizer) |
| `POST /api/ai-config` | Operator bearer, `text/plain` | `language=<2 letters>` and/or `auto=0|1` (automatic event captions); persisted |
| `GET /`, `/app.js`, `/style.css` | Public | Static login shell; no footage or credentials |

IDs: 1–24 lowercase ASCII letters, digits or hyphens. Tokens: 64 hexadecimal
characters generated from 32 OS-random bytes; each device and viewer must have a
distinct token. No credentials in URLs, firmware source, JS assets, or query
parameters. No CORS grants. Invalid input: 400; incorrect auth: 401; invalid JPEG
markers/type: 415; local storage failure: 500. Responses use `no-store`.

## Bounds

- 16 configured devices; 128 KiB maximum JPEG; 4 KiB total request header.
- Four fixed workers, eight queued sockets; excess connections are closed.
- Three-second absolute header/body read deadline per active request. Queued
  sockets can wait for workers. Write operations have three-second timeouts.
- Motion events (only from frames that carry `X-Motion`): start after two frames
  ≥30‰, stay open while ≥15‰, end after 3 s below or 5 s without frames. Pre-roll
  ≤12 frames, ≤768 KiB and ≤4 s old. Clips ≤400 frames or 6 MiB; at most 300 events
  and 512 MiB, oldest pruned. Clip files are not fsynced (evidence convenience, not
  replay state). Automatic vision analysis: at most one per device every 10 s, one
  job queue of 4, one adapter process at a time, 30 s deadline.
- Live: the newest accepted frame per device is held in memory (≤2 MiB total)
  and pushed to viewer streams. Each stream runs on its own thread (max 16) with
  3-second write timeouts, so streams never occupy the four request workers.
- 32 snapshots/device, at most one stored per second per device (the anti-replay
  watermark is still persisted for every frame): maximum 64 MiB retained JPEG data across 16 devices, plus
  tiny watermarks and filesystem overhead. One serialized pending file adds at
  most 128 KiB during commit. No unbounded image/log/event queue.
- Device list and latest-image reads are bounded by configured devices/retention.
- Local AI reports: one ≤8 KiB report per configured device (≤128 KiB total),
  plus bounded atomic-write temporary files and one tiny operator budget ledger.
- AI adapter: one process lock, at most 128 KiB image, 64 KiB provider response,
  15-second socket timeout and 20-second absolute provider deadline. No retries.

The receiver checks JPEG start/end markers, not full JPEG structure or dimensions.
It never decodes images. A compromised authorized device may submit malformed
images to a viewer's browser. Use current browsers; this is a deliberate v0 limit.

## Replay and crash behavior

The receiver durably writes and fsyncs a per-device high-water mark before
publishing a frame through same-directory atomic rename + directory fsync.
Older frames are pruned before publishing. Success is returned after persistence.
A power loss/storage failure may consume a sequence without saving its image;
retrying that sequence is rejected. Any storage write error stops all further
upload writes until operator recovery and restart, including ambiguous rename/
directory-fsync failure. The in-memory sequence is burned before persistence. Sequence gaps are normal, not an error.
Missing/corrupt watermarks with existing frames fail closed. Never remove/reset
watermarks while retaining the same device identity/key. Backups must include
both frames and watermarks; rolling back storage weakens replay protection.

Firmware reserves 4096 sequence numbers in NVS before use, including each boot.
This avoids writing flash per frame; a reboot skips unused numbers. At 10 fps,
reservation writes happen roughly every 7 minutes of continuous streaming, plus
boots (NVS wear leveling still matters). On 409 the firmware reads
`/api/device-state` and jumps forward to the server's next sequence (never back),
reserving it in NVS first.
NVS corruption never triggers automatic erasure. Reprovision with a new ID/key,
or deliberately recover a counter above the server watermark. A device key
holder can advance the sequence arbitrarily and deny future legitimate uploads;
this is a credential-compromise recovery problem, not solved by counters.

## Security signals

Fixed per-configured-device saturating u64 counters track accepted uploads,
authenticated replay rejections and invalid JPEG envelopes. One aggregate
counter tracks failed bearer authentication, including viewer requests. No
attacker-chosen IDs are inserted into telemetry maps. Counters reset on restart
and are not an audit log; persistent anti-replay watermarks are separate and do
not reset. Requests rejected before authentication/framing are not categorized
as a per-device signal. These counters are cumulative, not rates or proof of
malicious intent. `/api/security` is viewer-authenticated and `no-store`.

## Version negotiation and local producers

Clients send `X-Protocol: 1`; responses advertise version 1. Explicit unsupported
versions on API requests return 426. Legacy upload clients without the header
remain accepted; rule mutations require it. Authenticated upload attempts have a
per-configured-device token bucket: burst 64, refill 20/second. Exhaustion returns
429 with `Retry-After: 1`; 503 also carries that hint. This bounds accepted work,
not volumetric attacks or authentication attempts; proxy limits remain necessary.

`GET /api/device-state/{id}` requires that device's key and returns next_sequence
as a decimal string, maximum frame bytes and protocol version. This enables a
browser producer to resume without resetting replay state. It does not reserve a
sequence; simultaneous producers must use separate identities.

Viewer-authenticated GET endpoints `/api/capabilities`, `/api/rules` and
`/api/events` expose bounded local state. `POST /api/rules/{id}` requires a distinct
optional `operator` config token, protocol 1 and `Content-Type: text/plain`.
Its body is at most 512 bytes with exactly five unique `key=value` lines:
`device`, `signal`, `threshold`, `cooldown`, `enabled` (0 or 1). See local workflows
for enums/bounds. Save succeeds with 204; invalid input 400; full rule table 422.
Ambiguous storage failures stop further writes until recovery/restart. At most
16 persisted rules and 64 in-memory events are allowed. The server validates
schema/capability; explicit human review is enforced by the UI, not independently
provable from an API request by an operator-token holder.

## Advisory rule interpretation

Viewer GET `/api/rule-provider` reports `disabled`, `mock` or `meta`. Operator-only
POST `/api/rule-draft/{device}` accepts protocol 1 and text/plain (960 bytes max).
The fixed optional adapter returns an explanation and validated proposal, never
saves a rule. Unsupported requests return a null rule. Busy, disabled, provider
or adapter failures return 502 with no automatic retry; outcome may be unknown.
One worker can wait up to 25 seconds; remaining workers serve normal traffic.
The device must be configured. See local workflows for consent, budgets and limits.

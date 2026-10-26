# Protocol v0

Transport: HTTP/1.1 over verified HTTPS at the proxy. The local Rust origin uses
HTTP only on `127.0.0.1:8080`. No custom cryptography. Each connection handles one
request and closes; chunked requests, duplicate headers, `Expect`, folded
headers, oversized headers, and conflicting framing are rejected.

| Operation | Authorization | Request / result |
|---|---|---|
| `POST /api/frame/{id}` | Device-specific bearer | Raw JPEG, `Content-Type: image/jpeg`, exact `Content-Length`, `X-Sequence: <positive u64>`; 201 accepted, 409 stale/replay |
| `GET /api/security` | Viewer bearer | Bounded numeric counters since process start; no tokens, IPs, raw logs or footage |
| `GET /api/devices` | Viewer bearer | JSON device ID, latest stored sequence, retained count |
| `GET /api/latest/{id}` | Viewer bearer | Latest JPEG; 404 until a frame exists |
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
- 32 frames/device: maximum 64 MiB retained JPEG data across 16 devices, plus
  tiny watermarks and filesystem overhead. One serialized pending file adds at
  most 128 KiB during commit. No unbounded image/log/event queue.
- Device list and latest-image reads are bounded by configured devices/retention.
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

Firmware reserves 256 sequence numbers in NVS before use, including each boot.
This avoids writing flash per frame; a reboot skips unused numbers. At a five-
second minimum interval, reservation writes happen roughly every 21 minutes of
continuous successful captures, plus boots (NVS wear leveling still matters).
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

# Security boundary and limitations

Experimental prototype, not audited or production-certified. AI-assisted code
requires human review. Run only where you are authorized to collect images;
this repository's tests use synthetic data and no hardware has been flashed.

## Protected against within the stated boundary

- Separate per-device upload and viewer read capabilities; device tokens cannot
  read footage or impersonate another device. Unknown IDs are rejected.
- TLS confidentiality/authentication through a trusted Caddy boundary and ESP-IDF
  mbedTLS CA bundle + hostname checks. Firmware does not follow redirects.
- Bounded messages, queues and retention; monotonically increasing persisted
  sequences reject duplicate or older uploads, including after restart.
- IDs cannot escape the storage directory; UI uses text nodes and object URLs,
  restrictive CSP and no token persistence. Provider output is JSON, not HTML.
- Secrets and storage directories require private POSIX permissions. No image or
  credential logging by the application. Frame files are mode 600 inside mode
  700 directories. Local administrators remain trusted.

## Required deployment boundary

Linux/POSIX is the supported server host. Run as a dedicated unprivileged user.
Keep the loopback port inaccessible to other machines and isolate untrusted local
users/processes. Caddy must terminate TLS on the same trusted host/network
namespace; do not expose origin port 8080 or forward plaintext over a network.
Use a firewall/VPN and add proxy connection/rate limits appropriate to the site.
This small origin parser supports only the documented HTTP subset; it is not a
replacement for an Internet-facing hardened HTTP stack. Slow clients can occupy
all workers until deadlines; application limits do not prevent network DoS.

## Not solved in v0

Physical capture of a device can expose its Wi-Fi password and token: NVS is not
encrypted and secure boot/flash encryption are not configured. Provisioning
binaries contain secrets and must remain private. TLS does not protect a rooted
server, browser, camera, or proxy. Browser-held viewer tokens are bearer secrets;
there is no user account system, MFA, per-camera viewer ACL, session revocation,
or at-rest encryption. Rotate tokens by editing private config and restarting;
retain sequence state. Remove abandoned device directories manually to reclaim
space; the retention bound applies to configured devices, not archived IDs.

SNTP is unauthenticated, so hostile network time can cause certificate failures
or affect validity checks; manage trusted time on the camera network. No OTA,
secure update pipeline, biometric identification or continuous recording is
implemented; motion events record bounded clips only. The edge motion score is
reported by an authenticated device and is trusted only to start recordings and
AI analysis, which a compromised device could trigger within the stated budgets. JPEG decoders may have vulnerabilities. Only
JPEG envelope checks run on the server. External AI may hallucinate and must
never be used as sole evidence or a safety-critical decision system.

With a Meta key configured, motion events are sent to Meta automatically (at most
one analysis per camera every 10 s, 300 calls per day): this changes the privacy
boundary for those frames, and an operator enables it deliberately by saving the
key. Spending AI budget and changing AI settings require the operator token; the
key is write-only through the API and stored mode 600. Model output is validated
against strict schemas and only ever produces text and alerts. Timeout or invalid
or oversized responses mean the outcome is unknown, not that the provider did no
work; nothing is retried automatically. The CLI incident review remains manual.

## Reporting

Do not put tokens, private footage, or exploitable installation details in public
issues. Coordinate a private report with the repository owner through GitHub's
private reporting feature if enabled. Rotate exposed credentials promptly.

The Meta security reviewer is an **advisory** layer over bounded counters, not an
authentication authority. Model output is schema-validated and cannot execute
code, alter secrets, suppress deterministic signals or quarantine a device.
Prompt injection can still influence model judgments; every consequential
response remains a human decision. Tokens use OS randomness, never generated
text. See [Meta AI](META_AI.md) for the explicit trust boundary and tests.

## Browser sources and operator rules

Webcam uploads use existing device credentials and durable sequence checks.
Preview and upload are separate explicit actions. Browser speech is optional,
consented, and may use a vendor internet service; no audio is sent by X-Lens to
Meta. Typed rule templates work locally. Imported model output remains untrusted.
A separate operator token permits only bounded rule writes. Viewer/device tokens
cannot write rules. Rules produce local UI events only; the server does not
execute model code, rotate credentials or quarantine devices. Operator-token
possession itself authorizes API writes; UI confirmation is not a cryptographic
proof of human presence. Counters/events/cooldowns reset at restart, unlike rules
and device watermarks. Per-device upload buckets reduce authenticated load but
are not a global DoS defense; retain trusted proxy limits and local access control.

## Voice, zones, labels, routines and summaries

- **Voice**: audio is captured only while the push-to-talk button is active
  (≤12 s), sent to your host, then to Meta only when a key is configured; it is not
  stored. The 512 KiB body limit applies to `/api/transcribe` alone. Without a key,
  the browser recognizer is used only after a consent prompt.
- **SAM 3.1 zones**: the latest frame goes to Meta only on an explicit "Find" click;
  the response is a proposal that changes nothing until saved. Masks cannot ignore
  more than three quarters of the scene, so motion detection cannot be silently
  switched off from the API.
- **Labels**: an operator's ✓/✗ only selects which stored keyframes are sent as
  examples; examples are bounded (≤3 per call) and still pass through the strict
  schema. A wrong label can make a watch less sensitive: labels are visible on
  each event and can be undone.
- **Routines** run locally from motion events. A routine is a safety net, not a
  medical or emergency device: it detects motion, not wellbeing, and depends on
  the camera, power and network being up.
- **Summaries** are generated text over your own event log; they can be wrong and
  are labeled with their source.

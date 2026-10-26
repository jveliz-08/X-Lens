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
secure update pipeline, motion detector, biometric identification, or continuous
video recording is implemented. JPEG decoders may have vulnerabilities. Only
JPEG envelope checks run on the server. External AI may hallucinate and must
never be used as sole evidence or a safety-critical decision system.

AI calls are explicit and manual, disabled by default. The operator chooses a
trusted provider URL locally, never through camera/user HTTP input. An enabled
adapter sends a JPEG to that provider, which changes the privacy boundary and
may cost money. No paid/live provider was called during development. Timeout or
invalid/oversized response means outcome unknown/rejected, not proof that the
provider did no work. Inspect provider records before manually retrying.

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

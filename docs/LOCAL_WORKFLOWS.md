# Local camera and reviewed rules

Capture, storage, dashboard and rule execution work locally without Meta or an
internet connection. Meta incident interpretation still requires cloud inference;
there is no local model in this release. LOCAL TEMPLATE phrases use a small deterministic parser. The separately invoked
Meta proposal path uses cloud AI. Both produce untrusted drafts, never actions.

## Start and connect

Follow the README synthetic demo first. New `tools/setup_demo.py` configurations
include separate `viewer`, `operator`, `device.demo` and `device.webcam` secrets.
The script refuses to overwrite existing configuration. Existing installations
remain compatible; add distinct OS-random 32-byte hex tokens locally for an
operator and a webcam device, preserving existing device keys and watermarks.
Keep the configuration private (mode 0600); restart after changing it. Without an
operator token, rule viewing works but rule writes are unavailable.

Open the dashboard on localhost, enter the viewer token and choose Sources.
Select the configured webcam identity, then Open preview. Browser permission is
requested only after this click; microphone access is not part of webcam capture.
Preview alone uploads nothing. Enter that device's upload token and choose Start
snapshots. The browser sends a 320×240 JPEG at most every five seconds through the
same device-authenticated, bounded endpoint as ESP32 firmware. No separate webcam
upload bypass exists. The device-state endpoint returns the next sequence as a
decimal string, preserving 64-bit precision. Use one producer per device identity;
concurrent producers can collide and must reconnect after rejection.

Stop, changing dashboard sections, hiding the tab, locking, and leaving the page
release camera tracks and stop uploads. Late permission results are also stopped.
An upload already accepted by the server cannot be undone by stopping. Network,
replay or rate-limit failures stop the producer; reconnect explicitly rather than
retrying an ambiguous upload. No camera or credential is stored in browser storage.
Real hardware permissions and physical cameras have not been tested here.

Remote browsers require HTTPS and the documented trusted proxy boundary. Browser
camera access requires a secure context (localhost is permitted):
https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia

## Draft, review, confirm

In Rules, type a supported phrase such as:

- `When demo has 5 replays, notify me`
- `When demo has 3 invalid frames, notify me`
- `When demo has 10 authentication failures, notify me`
- `When demo is stale for 60 seconds, notify me`

Unsupported free-form requests fail visibly. Edit the resulting structured fields,
review the exact rule, enter the separate operator token, check the confirmation
box, then save. Cancel does not write. Editing invalidates the previous review.
Editing or disabling a saved rule uses the same confirmation flow. An uncertain
save result requires checking the saved list before trying again.

Optional browser speech recognition only fills the text box. It needs explicit
consent and a separate Dictate click; it never activates a rule. Browser/vendor
speech services may transmit audio and require internet access. This is **not an
offline transcription guarantee**, nor Meta speech integration. Unsupported
browsers use the same typed input. Recognition ends after 20 seconds or on Stop,
navigation or locking. See the browser service caveat:
https://developer.mozilla.org/en-US/docs/Web/API/SpeechRecognition

AI draft import accepts only a strict JSON object with `id`, `device`, `signal`,
`threshold`, `cooldown`, `enabled`, `action`; action must be `ui_event`. Unknown or
duplicate keys, escapes, unsupported signals/actions and invalid bounds fail.
This does not call a model. Model-generated code, shell commands, URLs, token
rotation and autonomous device blocking are not supported.

## Local execution semantics

At most 16 rules persist atomically and survive restart. The server checks them
once per second even without an open dashboard. Actions only create local UI
events (64 retained in memory); there are no OS push notifications or external
messages. Thresholds are 1–3600; cooldown is 10–86400 seconds. A condition that
remains true emits again after cooldown. Replay/invalid counters are cumulative
since restart, not rates. Authentication failures are global, not attributable
to the selected device. Staleness measures time since server startup or the last
accepted upload. Events, cooldowns, counters and staleness clocks reset on restart;
anti-replay watermarks and rules do not. These are operational hints, not proof
of intrusion. Changing a rule resets its cooldown.

## Meta natural-language proposals

The Rules panel also offers **Ask for an AI proposal** for typed or browser-
transcribed text. This invokes the server-side Meta adapter, not the local
phrase matcher. The provider status is explicit: disabled, SIMULATED or META LIVE.
The user selects a device, enters the operator token and approves sending text.
Only that text and selected device ID go to Meta: no microphone audio, image,
credentials or telemetry. Browser speech remains a separate vendor service.
Do not put secrets or sensitive personal information into rule text.

For the no-network fixture, launch from the repository root:

```sh
XLENS_RULE_MODE=mock XLENS_AI_STATE_DIR=runtime \
  ./target/release/x-lens runtime/config.secret runtime/frames
```

The simulated response is a fixed, clearly labeled replay rule, **not an actual
interpretation of arbitrary input**. Production mode is opt-in operator setup:
set `XLENS_RULE_MODE=meta`, `XLENS_META_MODEL` to an authorized supported model,
`XLENS_META_KEY_FILE` to an existing mode-0600 key file, and
`XLENS_AI_STATE_DIR` to the same private configuration directory used by
`tools/analyze.py`. No key provisioning or live calls were performed here.
The directory must be mode 0700. Python 3 and the project's `tools/` files must
be present; the Rust server invokes a fixed script without a shell. Start from
the repository root. Invalid/missing settings fail without activating rules.

The adapter uses the documented [Meta Chat Completions contract](https://dev.meta.ai/docs/protocols/chat-completions),
Bearer authentication and JSON schema output. Local validation rejects unknown
fields, tools, truncated replies, duplicate keys and unsupported types/ranges.
The model must decline compound conditions, time-window rates, schedules,
image conditions or unsupported actions rather than approximate them. Schema
validation cannot prove semantic fidelity: compare the model's explanation and
exact fields with your intent before confirming. Model output never writes rules.
An unsupported proposal contains no rule; the LOCAL TEMPLATE fallback is still
available without cloud access.

Input is at most 240 characters/960 UTF-8 bytes, output content 2048 characters,
provider response 64 KiB, and adapter output 4 KiB. One proposal runs at a time.
Live calls share the existing private budget/lock with incident review when using
the same state directory: 10/day and a 60-second cooldown. Budget is reserved
before sending, including failed/unknown calls. The network deadline is 20 seconds;
the Rust subprocess watchdog is 25 seconds. There are no automatic retries.
Cancelling or navigating away discards the browser result, but cannot undo an
already submitted provider call or its cost. Check provider records after an
unknown outcome. The panel never silently activates or retries a proposal.

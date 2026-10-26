# Local camera and reviewed rules

Capture, storage, dashboard and rule execution work locally without Meta or an
internet connection. Meta incident interpretation still requires cloud inference;
there is no local model in this release. Rule phrases use a small deterministic
parser, not an LLM. Imported AI JSON is an untrusted draft, never an action.

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

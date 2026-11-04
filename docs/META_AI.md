# Meta AI: image review and defensive signal review

## Event vision, watches and Ask (dashboard workflow)

`tools/vision.py` is a fixed adapter the server runs with no shell and no
model-chosen arguments. It uses the same documented endpoint
(`https://api.meta.ai/v1/chat/completions`), `developer` + `user` messages,
inline `image_url` JPEG data URIs, `response_format` `json_schema` with
`strict: true`, `max_completion_tokens` and `reasoning_effort: "low"`.

- **event**: up to three frames of one motion event (start, peak, latest) plus the
  camera's enabled watches in one request. Schema: `caption`, `activity` (fixed
  enum) and, only when watches exist, `watches[]` with `id` restricted to the sent
  IDs, `match`, `confidence` and `reason`. Only `match: true` becomes an alert.
- **ask**: the question, the current frame (if any) and the latest 40 events as
  text with local times. Schema: `answer`, `answerable`, `events[]`, `watch`.
  Cited IDs the model was not shown are dropped; `watch` is only a suggestion the
  user confirms in the UI.

- **learned examples**: operators label alerts ✓ right / ✗ false alarm (or "should
  have alerted"). Up to three labeled keyframes (≤2 per watch, newest first, never
  the event being analyzed) follow the watch conditions in the same request, each
  introduced as "this frame DID / did NOT match", with a standing instruction to
  avoid repeating rejected false alarms. Few-shot in context: nothing is trained or
  uploaded besides those JPEGs.
- **digest**: the operator's local day: up to 120 events as text, up to four
  keyframes (alerted events first, then highest motion) and routine outcomes.
  Schema: `title`, `summary`, `highlights[{event, text}]`; highlights citing events
  the model was not shown are dropped. Simulation writes local statistics only.

### SAM 3.1 ignore zones (`tools/segment.py`)

`POST https://api.meta.ai/v1/responses`, model `sam-3.1`, one user message with an
`input_text` concept and an `input_image` data URL, `stream: true`,
`metadata.mask_encoding: "one_bit"`. The adapter concatenates
`response.output_text.delta` events, parses the documented
`<|box;x1=..;y1=..;x2=..;y2=..;w=..;h=..|>` tokens (pixel coordinates, validated
against `w`/`h`, ≤16 boxes) and marks every 8×6 grid cell at least 30 % covered. The
result is a proposal drawn in the editor; the operator saves it explicitly.
Shares the vision ledger and lock; mock mode proposes nothing.

### Muse Voice Transcribe (`tools/voice.py`)

The dashboard records push-to-talk audio (≤12 s) with Web Audio at 16 kHz and
encodes mono 16-bit PCM WAV, the documented format. The host checks the RIFF
header and sends one multipart request to `https://api.meta.ai/v1/asr/transcribe`
(`request` part: `muse-voice-transcribe-1.0`, `audioEncoding: WAV`,
`mode: PUSH_TO_TALK`, `languageBias` from the AI language setting; `audio` part).
Only `transcript` is kept (≤300 chars); audio is never stored. Separate
`voice-budget.json` ledger (200/day). Without a key the dashboard offers the
browser's recognizer after an explicit consent prompt; answers and alerts are
spoken by the browser's local speech synthesis.

**Enabling it:** an operator pastes the key in **Watches & rules → Meta AI**
(`POST /api/ai-key`). The server stores it as `meta.secret` next to the config
(or `XLENS_META_KEY_FILE`), mode 600, and passes only the file path to the
adapter. No endpoint returns it; the dashboard sees `key: true/false`. Mode is
`meta` when the key exists, otherwise `mock`; `XLENS_RULE_MODE=meta|mock|disabled`
overrides. Default model `muse-spark-1.3` (Meta's documented example), overridable
with `XLENS_META_MODEL`. The same key also enables Meta rule drafting.

**Cost bounds:** inference runs per motion event, never per frame: automatic
analysis at most once per camera every 10 s (later events can be analyzed
manually), a queue of 4 jobs, one adapter process at a time, and a private
`vision-budget.json` ledger (300 calls per UTC day, ≥2 s apart) reserved before
sending. Unknown outcomes consume their slot and are never retried. Simulation
mode makes no network call, spends nothing, and labels every output SIMULATED;
it never claims to have seen an image.

**Validation:** `tests/test_vision.py` checks the exact wire shapes: Chat
Completions (roles, image parts, strict schema rules, labeled examples, digest),
the SAM 3.1 request and event-stream parsing with the box-to-grid mapping, and the
ASR multipart body and WAV checks. It also covers rejection of extra keys, unknown
activities, unknown watch IDs, tool calls, refusals, truncation and duplicate JSON
keys, citation filtering, and runs each live path end to end against a fake
provider (one call, fixed endpoint, budget reserved). No real Meta call was made.

## Security incident review (CLI)

**Incident mode:** one bounded model call jointly reviews
the latest JPEG and sanitized device telemetry, producing visual observations,
security advice and a correlated incident summary. A separate deterministic
policy gate supplies evidence, and a bounded local report appears in the
dashboard. Nothing in the review automatically changes device access.

`tools/meta_provider.py` implements Meta Model API's documented HTTPS Chat
Completions wire format at `https://api.meta.ai/v1/chat/completions`. It uses the
standard-library HTTP/TLS adapter, no SDK. Image mode sends inline JPEG bytes;
security mode sends only sanitized numeric counters with numbered camera slots.
No device names, Wi-Fi secrets, bearer tokens, private keys, IPs or raw logs are
included in security prompts. The provider key is only an Authorization header
to the fixed Meta endpoint and remains server-side.

The operator must choose `--model` for any real call. Current official examples
use `muse-spark-1.3`, but no default model is silently assumed for paid calls.
The event is Meta Global AI Developer Hackathon, whose public page lists Meta
Model API. The participant's account entitlement and available model IDs
**remain unconfirmed**. This adapter follows verified public Meta API docs but
has not made a live request. No key provisioning or paid action was performed.

Official sources checked 2026-10-26:
- [Meta Chat Completions](https://dev.meta.ai/docs/protocols/chat-completions)
- [Meta image understanding](https://dev.meta.ai/docs/image-understanding)
- [Meta structured output](https://dev.meta.ai/docs/structured-output)

## Local preview and deliberate external sending

```sh
python3 tools/analyze.py --mode image --device demo
python3 tools/analyze.py --mode security
python3 tools/analyze.py --mode incident
python3 tools/demo_incident.py  # complete synthetic, clearly labeled simulation
```

The analysis commands are dry-runs. The first reports bounded JPEG/request sizes; the second also
prints a deterministic local signal review without an AI call. For a real call,
authorized operators supply a mode-600 API key file and a verified supported
model ID, for example:

```sh
export XLENS_META_KEY_FILE=/private/path/meta.secret
python3 tools/analyze.py --mode incident --device demo --model YOUR_VERIFIED_MODEL_ID \
  --report-dir runtime/frames --send
```

`--mode image --send` sends the latest JPEG to Meta and changes the privacy
boundary. This requires the operator's informed authorization. The code refuses
redirects, verifies system CA/hostname, allows one call per config directory,
caps input at 128 KiB and response at 64 KiB, and bounds generated output to
1,024 tokens. A 20-second absolute deadline and 15-second socket timeout apply.
No automatic retries, tool definitions, remote image URLs, or provider tools are
available. Provider timeout/incomplete output can still incur cost; inspect the
provider's records before trying again.

## An advisory reviewer, never an authentication authority

Local thresholds (cumulative since process start) trigger `review` at 10 failed
authentications, 5 authenticated replay rejections for one device, or 3 invalid
frames for one device. These are demonstrative rules requiring site-specific
calibration, not validated detection rates. Credential guessing by outsiders,
legitimate retries or network problems can all produce signals. Restart clears
telemetry, not durable anti-replay state.

The model returns constrained risk/recommendation/reason enums. Local validation
rejects extra keys, duplicate JSON keys, unknown values, tool calls, refusals,
truncation and oversized output. A deterministic gate retains its own evidence
regardless of the model's risk rating. `step_up_review` and `quarantine_review`
are suggestions only; no automatic blocker or auth mutation exists. Every
result states `human_approval_required: true` and `automatic_action: false`.
An operator must separately verify facts and approve any future consequential
action. AI cannot waive a local rule, execute shell commands, fetch URLs, mint
keys, or rotate credentials.

Image mode returns bounded object categories, an untrusted plain-text summary
and an uncertainty flag. No identity or biometric fields exist. Images may
contain prompt injection, and a model may still infer or repeat inappropriate
text; prompts alone do not prevent that. The output remains data, is not
executed, does not enter an auth decision, and is printed as escaped JSON.

Tokens/nonces must come from OS/hardware cryptographic randomness, not an LLM.
AI-generated security questions are not strong authenticators. A future
step-up workflow should use established authentication standards and separate
human-reviewed policy; it is intentionally not improvised in this prototype.

## Tests and limitations

Mock tests cover wire shape, numeric-only input, secret-field rejection,
prompt-injection strings, schema errors, attempted tool output, local-rule
non-bypass, output bounds, and no retry after timeout. These tests validate the
application boundary, not model accuracy or adversarial robustness. No real
Meta account, endpoint response or model behavior has been validated. The
remaining integration step is confirmation of the participant's available Meta model/account access
and a separately authorized synthetic live test.

## Cost and report bounds

Inference is manual and combines image plus telemetry into **one** request,
never a per-frame AI loop. Default maximum: 10 requests per UTC day and at least
60 seconds between attempts. Operator flags `--daily-budget` (1..100) and
`--cooldown` (60..86400 seconds) make the limit explicit. A private ledger reserves
a slot before sending; timeout/unknown outcomes still consume that slot. The
process lock spans budget reservation and provider I/O. Corrupt ledgers or a
backward clock fail closed. Use one config directory per installation; deleting
or splitting ledgers defeats the operator-side budget and is not supported.

`--report-dir` writes one atomically replaced, mode-600 report per configured
device, at most 8 KiB each. The viewer-authenticated `/api/review/{id}` endpoint
serves it to the dashboard. Reports show `mock` versus `meta`, review time, an
untrusted incident summary and deterministic policy. The model is not run by a
browser request; no inference/API keys are exposed through the dashboard.

Image and counters are sampled sequentially and counters cover the process
lifetime; this is contextual correlation, not a synchronized event timeline or
proof of causality. A report can be older than the latest displayed snapshot.
The dashboard states that limitation. Synthetic demo mode uses the exact output
validator and report pipeline, but cannot establish real model quality.

Each incident report also carries the exact consumed image sequence from the
origin response, its SHA-256 fingerprint, byte count, sample time and sanitized
telemetry snapshot. These fields are computed locally, not invented by the
model. They link the explanation to an input even if a later frame is displayed.
The fingerprint identifies content; it is not a device signature or proof that
the physical scene was genuine. Retention can eventually remove that JPEG.

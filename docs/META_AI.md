# Meta AI: image review and defensive signal review

## What is implemented

**Incident mode is the core workflow:** one bounded model call jointly reviews
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

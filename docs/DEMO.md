# X-Lens demo: a $7 camera that understands

## The pitch (30 seconds)

Smart cameras today either stream everything to someone else's cloud, or send
every frame to an AI and burn money. X-Lens does neither. A $7 ESP32-CAM detects
motion **on the chip**, the self-hosted server records a short clip with the
seconds *before* the motion, and **Meta Muse Spark** looks at that event once:
it says what happened, checks the conditions you wrote in plain words, **learns
from your corrections**, and at night writes the story of your day. You can ask it
questions **by voice** (Muse Voice Transcribe), let **SAM 3.1** find the tree that
keeps triggering the camera, and set **routines** that alert you when someone who
lives alone has not moved by 10:00. One model call per event, never per frame.
Footage stays on your host.

## Recorded video

```sh
runtime/venv/bin/python tools/record_demo.py              # docs/demo.mp4, English, simulation draft
runtime/venv/bin/python tools/record_demo.py --lang es --out docs/demo-es.mp4
runtime/venv/bin/python tools/record_demo.py --live       # real Meta output (key in runtime/meta.secret)
runtime/venv/bin/python tools/record_demo.py --serve      # present live without hardware: Enter = someone walks in
```

Narration uses offline Piper neural voices when `runtime/voices/en_US-ryan-high.onnx`
and `es_MX-ald-medium.onnx` exist (`pip install piper-tts pillow` in the venv), else
espeak-ng. The video includes a card with the exact Muse Spark request built by
`tools/vision.py` for the recorded event (images elided). Length is paced to `--seconds` (default 180 s, exactly the 3-minute limit).

The recorder starts a private server on port 8099 (your instance and data are not
touched), feeds it a synthetic hallway camera (`tools/demo_scene.py`: a person
carries a box to the door and leaves), drives the real dashboard with Playwright
and adds captions and espeak-ng narration. Without `--live` every AI output is
labeled SIMULATED and the video carries a "simulation draft" mark. **Record the
submission with `--live`** once the key is saved: it costs about ten calls (two
event captions, one Ask, one summary, one SAM call). For the final cut, consider
replacing the synthetic narration with your own voice over the same footage.

## Setup before presenting live

```sh
cargo build --release
python3 tools/setup_demo.py            # once: private tokens in runtime/config.secret
./target/release/x-lens runtime/config.secret runtime/frames   # plus Caddy for cameras (SETUP.md)
```

1. Power the ESP32-CAM (or open **Sources** on a laptop/phone and start the webcam).
2. Open the dashboard, unlock with the `viewer` and `operator` tokens.
3. **Watches & rules → Meta AI**: paste the Meta API key. The status line turns
   to `META · muse-spark-1.3 · n/300 calls today`. No restart.
4. Add one watch, for example *"A person holding a phone up to the camera"*, and one
   routine covering the next hour (*"Grandma gets up"*).
5. Tick **Speak alerts & answers** and click **Enable alerts** on the Live tab.

## Live script (3 minutes)

1. **Live view.** `LIVE`, fps and the motion meter. Nobody moving and nobody
   watching: the board idles at ~1 fps; opening the feed makes it go to full rate.
2. **Edge motion.** Walk into view. The meter jumps, the tile shows `REC #n`: the
   board decoded its own JPEG at 1/8 scale and reported the score in a header.
3. **Event understanding.** **Activity**: within seconds Muse Spark describes the
   event from three frames. Click the thumbnail: the clip includes the pre-roll.
4. **Plain-language watch.** Hold a phone up to the camera: alert banner, toast,
   notification and a spoken warning. Nobody trained a detector.
5. **It learns.** Mark an alert ✗ *False alarm* (or ✓ *Right*). Trigger a similar
   event: that keyframe now travels with the request as a reference example.
6. **Ask by voice.** Press 🎤 and say *"¿Alguien dejó algo en la puerta?"*. Muse
   Voice Transcribe turns it into text; the answer cites clips and is read aloud.
   Say *"Avísame si la puerta queda abierta"* → **Create watch**.
7. **Ignore zones.** On the tile, **Zones** → type *"window"* → **Find with SAM 3.1**
   → the proposed cells light up → **Save**. Wave in front of the window: no event.
8. **Routines and the day.** Activity shows *✓ Grandma gets up · first motion
   10:42*. Click **Summarize today**: one call writes the day with clip links.
9. **Close with the boundary.** The model only describes and suggests. It cannot
   unlock, block or run anything; every output is schema-checked; the key never
   leaves the host; viewers cannot spend AI budget.

## Without a key or a camera

Everything above runs in clearly labeled **SIMULATED** mode without a key. Captions
and answers say so and never pretend to have seen the image; the daily summary
uses local statistics; SAM proposes nothing (paint zones by hand); voice uses the
browser recognizer after a consent prompt. Without hardware, use the browser
webcam source, the video recorder above, or the synthetic incident:

```sh
python3 tools/demo_incident.py
```

## What makes the AI substantive

- **Temporal**: the model sees a short sequence, so it says what *changed*.
- **Learns in context**: user labels become few-shot reference frames; no
  training, no data pipeline, undo at any time.
- **Grounded memory**: answers and summaries are built from the event log the
  server recorded, with citations the UI turns into playable clips; IDs the model
  invents are dropped.
- **Three Meta models, one key**: Muse Spark for understanding, SAM 3.1 to set up
  the camera, Muse Voice Transcribe for hands-free questions.
- **Cheap by construction**: edge motion and on-device ignore zones gate
  inference; one call per event; per-camera gap and daily ledgers; nothing retries.
- **Knows when not to use AI**: routines detect the *absence* of motion locally.

See [HACKATHON.md](HACKATHON.md) for the judging criteria and checklist, and
[SUBMISSION.md](SUBMISSION.md) for the form text and the X post.

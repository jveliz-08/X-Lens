# Submission kit

Copy-paste material for the Cerebral Valley form and the X post. Replace the
`[brackets]` before submitting.

## Project name
X-Lens

## Tagline (one line)
A $7 camera that understands: on-chip motion, clips that stay home, and Meta AI that
describes, learns from you and watches over the people you love, one call per event.

## Track and theme
Indie developer · **Build for Your World** (home and elderly-care monitoring).

## Short description (≈80 words)
X-Lens turns a $7 ESP32-CAM into a private, intelligent camera. The board detects
motion on the chip and streams only when needed; a zero-dependency Rust host records
clips with the seconds before the motion. Muse Spark reads each event once, checks
watches written in plain words and learns from your ✓/✗ labels; SAM 3.1 finds the tree
or TV the camera should ignore; Muse Voice Transcribe lets you ask by voice. Routines
alert you when someone living alone hasn't moved.

## Long description
**Problem.** Families worry about relatives who live alone, and people want to know what
happens at home. Today's options stream everything to a third-party cloud or send every
frame to an AI, which is expensive and invasive.

**What X-Lens does.**
- **Edge motion on a $7 board**: the ESP32-CAM decodes its own JPEG at 1/8 scale (28 ms),
  scores motion against an adaptive background and sends it in a header. Nobody watching
  and nothing moving: ~1 fps. Someone opens the feed or moves: full rate.
- **Clips with pre-roll** recorded on your own host, never in someone else's cloud.
- **Muse Spark, one call per event**: three frames (start, peak, latest) → caption,
  activity type and plain-language watch checks, all in a strict JSON schema.
- **Watches that learn**: mark an alert right or a false alarm; that keyframe travels with
  the next requests as a labeled reference example. In-context learning, zero training.
- **Ask X-Lens**, by text or voice (**Muse Voice Transcribe**): answers grounded in the
  event log with playable clip citations; "alert me if…" becomes a watch with one click.
- **Daily summary**: one call turns the day's events, keyframes and routines into a story.
- **SAM 3.1 ignore zones**: type "tree"; SAM 3.1 segments it, X-Lens turns the boxes into an
  8×6 mask, and the **microcontroller applies the mask itself**.
- **Routines**: "expect motion between 07:00 and 10:00". No motion → alert, banner,
  notification and spoken warning. Absence of activity is the signal; no AI needed.

**Safety.** The model describes and suggests; it never acts. Every output is parsed against
a strict schema; images and user text are treated as untrusted data; people are never
identified; viewers cannot spend AI budget; the key is write-only, stored on the host.

**Tech.** C firmware (ESP-IDF 5.4), Rust server with zero third-party crates, vanilla
HTML/JS dashboard, Python adapters for the Meta Model API. 11 Rust unit tests,
19 integration tests, 37 adapter tests with fake providers, and a full Playwright browser
workflow. Tested on a real ESP32-CAM (OV3660) on a home mesh network.

## Meta models used
- **Muse Spark** (`muse-spark-1.3`, Chat Completions, strict `json_schema`): event
  understanding, learned watches, Ask, daily summary.
- **SAM 3.1** (Responses API, text prompt): ignore-zone proposals.
- **Muse Voice Transcribe** (`muse-voice-transcribe-1.0`): voice questions.

## Built during the event
[Fill in honestly. The rules allow showing only what was built during the event, and an
unclear contribution means disqualification. Example, if a base existed before Oct 26:
"The authenticated frame ingest and replay protection existed before the event. Built
during the hackathon (Oct 26 – Nov 4): edge motion detection and demand-driven streaming in
the firmware, event clips, every Meta AI feature (event vision, watches, learning from
labels, Ask, voice, daily summary, SAM 3.1 zones), routines, the new dashboard and the demo
recorder." If everything is new, say so.]

## Links
- Repo: [https://github.com/…]
- Video (≤3 min): [YouTube/Vimeo link]; file: `docs/demo.mp4`
- X post: [link]

## X post (≤280 characters)
I turned a $7 ESP32-CAM into a camera that understands 🧠📷
• motion detected on the chip
• clips stay at home
• @Meta Muse Spark describes each event once, learns from my ✓/✗
• SAM 3.1 masks the tree that keeps triggering it
• alerts if Grandma hasn't moved by 10am
#MetaHackathon [video]

## For judges who want to run it
```sh
cargo build --release
runtime/venv/bin/python tools/record_demo.py --serve   # synthetic camera, prints URL and tokens
```
Open the URL, unlock with the printed tokens, press Enter in the terminal to make
someone walk in. Paste a Meta key under Watches & rules → Meta AI to switch from labeled
simulation to live Meta models.

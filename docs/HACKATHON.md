# Meta Global AI Developer Hackathon: criteria and checklist

Researched 2026-10-29. Dates and rules can change; check the official pages before
submitting.

## Key dates (ET)

| Date | What |
|---|---|
| Oct 26 | Kickoff (building starts) |
| Oct 27 – Nov 4 | Workshops and Discord office hours |
| **Nov 4, 9:00 PM** | **Submission deadline** (late submissions are not considered) |
| Nov 4 – 8 | Judging |
| Nov 9 | Closing ceremony, top finalists present live |

## Judging

**Official event page** ([dev.meta.ai](https://dev.meta.ai/events/global-hackathon),
[Cerebral Valley](https://cerebralvalley.ai/e/meta-ai-developer-hackathon)):

- Indie track: *"creativity, technical ambition, and whether it works."*
- Startup track: *"technical execution and whether it holds up in production."*

**Detailed rubric** (from the community event brief at
[github.com/meta-ai-hackathon/meta-hack](https://github.com/meta-ai-hackathon/meta-hack);
not an official Meta page, so verify it on the submission platform):

1. **Pass/fail gate**: fits a problem statement (*Build for Your World* or *Push the
   Frontier*) **and** genuinely uses the Meta Model API or open-weight models.
2. **Scored 0–10**: Impact 30 %, Meta AI tooling 25 %, Demo 25 %, Technical depth 20 %.
3. Top 5 per track present live, same rubric.

Submission: through the Cerebral Valley platform; a **3-minute demo video is the
primary judging artifact**; public repo; working video link; all contributors
listed. **New-work rule: the demo may show only what was built during the event;
unclear contribution means disqualification.**

Bonus prizes listed in the same brief: Most Viral (X engagement on the demo post
through Nov 8), Best Build on Open Weight Models (Muse Glimmer / Muse Spark Open),
Community Vote (public gallery), Best Agentic Use-Case, Best Creative Build (Muse
Image / Muse Wonder).

## How X-Lens maps to the rubric

| Criterion | Evidence |
|---|---|
| Gate: theme | *Build for Your World*: home and elderly-care monitoring on $7 hardware |
| Gate: Meta models | Muse Spark (vision, Ask, summaries, learned watches), SAM 3.1 (zones), Muse Voice Transcribe (voice), all through the Meta Model API with one key |
| Impact (30 %) | Routines for people living alone; private by design (self-hosted, clips stay home); cheap enough to deploy per room |
| Meta AI tooling (25 %) | Multi-frame temporal captions; watches evaluated in the same call; **in-context learning from user labels**; grounded Q&A with citations; SAM 3.1 output turned into a **mask the microcontroller applies**; voice in, speech out |
| Demo (25 %) | Real ESP32-CAM; reproducible recorder (`tools/record_demo.py --live`) for the 3-minute video |
| Technical depth (20 %) | On-chip JPEG DC decode motion (28 ms), demand-driven streaming, zero-crate Rust server, strict schemas, budget ledgers, 11 Rust + 19 integration + 37 adapter tests + full browser workflow |

## Checklist before Nov 4, 9:00 PM ET

- [ ] **New-work rule.** Make clear what was built during the event. Initialize
      git and commit now so the history shows it. If X-Lens existed before Oct 26,
      list what predates the event (for example the base ingest/replay protection)
      and what was added during it (edge motion, clips, all Meta features, routines,
      zones, voice, recorder) in the README and the submission form.
- [ ] Public GitHub repo; make sure `runtime/` (tokens, Wi-Fi, keys) is **not** pushed
      (it is gitignored; check with `git status --ignored`).
- [ ] Save the Meta key in the dashboard and do one real check of each call: an
      event caption, an Ask, **Summarize today**, **Find with SAM 3.1**, a voice question.
- [ ] Record the final video with `--live` (≤ 3 min), ideally with your own voice
      and real footage from the ESP32-CAM.
- [ ] Upload the video (YouTube/Vimeo unlisted) and check that the link works logged out.
- [ ] Post the demo on X (Most Viral bonus) and submit that link too.
- [ ] Fill in the submission form; choose the Indie or Startup track.

## Ideas not built yet

- **Open-weight mode** (Best Build on Open Weight Models): point the adapter at a
  local or partner-hosted **Muse Spark Open** OpenAI-compatible endpoint (for
  Baseten the brief names `meta-models/muse-spark-open-1026`) so footage never
  leaves the house at all. Needs a configurable base URL restricted by the operator.
- **Agentic Ask**: let the model call read-only tools (search events by time, fetch
  a clip's frames) over several steps before answering.
- **WhatsApp alerts** with the keyframe and two-way questions.

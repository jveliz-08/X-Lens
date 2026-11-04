//! Motion events from device-reported edge scores: pre-roll, bounded clips, AI annotations.
//! Clip format: "XLC1" then per frame u32 LE offset_ms, u32 LE length, JPEG bytes.
use super::*;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// Per-mille changed cells that starts an event (two frames in a row) and keeps it alive.
pub const ON: u16 = 30;
const OFF: u16 = 15;
const QUIET: Duration = Duration::from_secs(3);
const STALE: Duration = Duration::from_secs(5);
const PREROLL_FRAMES: usize = 12;
const PREROLL_BYTES: usize = 768 * 1024;
/// Pre-roll older than this is not "just before" the event (e.g. the device was offline).
const PREROLL_AGE: Duration = Duration::from_secs(4);
const MAX_CLIP_FRAMES: u32 = 400;
const MAX_CLIP_BYTES: usize = 6 * 1024 * 1024;
const MAX_EVENTS: usize = 300;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// Describe an event once it has run this long, so alerts do not wait for it to end.
const ANALYZE_AFTER: Duration = Duration::from_secs(6);
/// Minimum gap between automatic analyses per device: AI runs per event, never per frame.
const ANALYZE_GAP: Duration = Duration::from_secs(10);

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
pub struct Alert {
    pub watch: String,
    pub confidence: String,
    pub reason: String,
}
pub struct Annotation {
    pub source: String,
    pub activity: String,
    pub caption: String,
    pub alerts: Vec<Alert>,
    pub at: u64,
}
impl Annotation {
    /// Line protocol written by tools/vision.py (already validated there; checked again here).
    pub fn parse(text: &str, watches: &[String]) -> io::Result<Self> {
        let mut a = Annotation {
            source: String::new(),
            activity: String::new(),
            caption: String::new(),
            alerts: Vec::new(),
            at: unix_now(),
        };
        let clean = |v: &str, max: usize| {
            if v.chars().count() > max || v.chars().any(char::is_control) {
                Err(invalid())
            } else {
                Ok(v.to_owned())
            }
        };
        for line in text.lines() {
            let (k, v) = line.split_once('=').ok_or_else(invalid)?;
            match k {
                "source" if matches!(v, "meta" | "mock") => a.source = v.into(),
                "activity" => a.activity = clean(v, 24)?,
                "caption" => a.caption = clean(v, 400)?,
                "alert" => {
                    let mut parts = v.splitn(3, '\t');
                    let (Some(watch), Some(confidence), Some(reason)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        return Err(invalid());
                    };
                    if !watches.iter().any(|w| w == watch)
                        || !matches!(confidence, "low" | "medium" | "high")
                        || a.alerts.len() >= 8
                    {
                        return Err(invalid());
                    }
                    a.alerts.push(Alert {
                        watch: watch.into(),
                        confidence: confidence.into(),
                        reason: clean(reason, 240)?,
                    });
                }
                _ => return Err(invalid()),
            }
        }
        if a.source.is_empty() || a.caption.is_empty() {
            return Err(invalid());
        }
        Ok(a)
    }
    fn wire(&self) -> String {
        let mut out = format!(
            "source={}\nactivity={}\ncaption={}\nat={}\n",
            self.source, self.activity, self.caption, self.at
        );
        for a in &self.alerts {
            out.push_str(&format!(
                "alert={}\t{}\t{}\n",
                a.watch, a.confidence, a.reason
            ));
        }
        out
    }
}
pub struct Event {
    pub id: u64,
    pub device: String,
    pub start: u64,
    pub end: Option<u64>,
    pub peak: u16,
    pub frames: u32,
    pub bytes: u64,
    pub ai: Option<Annotation>,
    pub pending: bool,
    pub failed: bool,
}
impl Event {
    pub fn json(&self) -> String {
        let ai = self.ai.as_ref().map_or("null".into(), |a| {
            let alerts = a
                .alerts
                .iter()
                .map(|x| {
                    format!(
                        "{{\"watch\":{},\"confidence\":{},\"reason\":{}}}",
                        json_str(&x.watch),
                        json_str(&x.confidence),
                        json_str(&x.reason)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"source\":{},\"activity\":{},\"caption\":{},\"alerts\":[{alerts}],\"at\":{}}}",
                json_str(&a.source),
                json_str(&a.activity),
                json_str(&a.caption),
                a.at
            )
        });
        format!(
            "{{\"id\":{},\"device\":{},\"start\":{},\"end\":{},\"peak\":{},\"frames\":{},\"pending\":{},\"failed\":{},\"ai\":{ai}}}",
            self.id,
            json_str(&self.device),
            self.start,
            self.end.map_or("null".into(), |e| e.to_string()),
            self.peak,
            self.frames,
            self.pending,
            self.failed
        )
    }
}
struct Recording {
    id: u64,
    file: File,
    origin: Instant,
    last_frame: Instant,
    quiet: Option<Instant>,
    peak: u16,
    first: Arc<Vec<u8>>,
    key: Arc<Vec<u8>>,
    latest: Arc<Vec<u8>>,
    frames: u32,
    bytes: usize,
    analyzed: bool,
}
#[derive(Default)]
struct Tracker {
    preroll: VecDeque<(Instant, Arc<Vec<u8>>)>,
    preroll_bytes: usize,
    hot: u8,
    motion: u16,
    rec: Option<Recording>,
}
/// Frames sent to the vision model for one event, in time order.
pub struct Job {
    pub event: u64,
    pub device: String,
    pub frames: Vec<Arc<Vec<u8>>>,
    pub peak: u16,
    pub seconds: u64,
}
pub struct Activity {
    dir: PathBuf,
    next: u64,
    pub events: BTreeMap<u64, Event>,
    trackers: BTreeMap<String, Tracker>,
    last_job: BTreeMap<String, Instant>,
    pub auto: bool,
}
fn write_frame(f: &mut File, offset: Duration, jpeg: &[u8]) -> io::Result<()> {
    let mut head = [0u8; 8];
    head[..4].copy_from_slice(&(offset.as_millis().min(u32::MAX as u128) as u32).to_le_bytes());
    head[4..].copy_from_slice(&(jpeg.len() as u32).to_le_bytes());
    f.write_all(&head)?;
    f.write_all(jpeg)
}
fn private_file(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}
/// Frames stored in a clip file; bounded by the clip limits when it was written.
pub fn clip_frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    if !bytes.starts_with(b"XLC1") {
        return out;
    }
    let mut at = 4;
    while at + 8 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let Some(frame) = bytes.get(at + 8..at + 8 + len) else {
            break;
        };
        out.push(frame);
        at += 8 + len;
    }
    out
}
impl Activity {
    pub fn open(root: &Path) -> io::Result<Self> {
        let dir = root.join("_events");
        private_directory(&dir)?;
        let mut events = BTreeMap::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let Ok(id) = entry.file_name().to_string_lossy().parse::<u64>() else {
                continue;
            };
            if let Some(event) = Self::load(&entry.path(), id) {
                events.insert(id, event);
            } else {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
        let next = events.keys().next_back().map_or(1, |id| id + 1);
        let mut activity = Self {
            dir,
            next,
            events,
            trackers: BTreeMap::new(),
            last_job: BTreeMap::new(),
            auto: true,
        };
        activity.prune();
        Ok(activity)
    }
    fn load(path: &Path, id: u64) -> Option<Event> {
        let meta = fs::read_to_string(path.join("meta")).ok()?;
        let fields = meta
            .lines()
            .filter_map(|l| l.split_once('='))
            .collect::<BTreeMap<_, _>>();
        let num = |k: &str| fields.get(k)?.parse::<u64>().ok();
        let start = num("start")?;
        let device = fields.get("device").filter(|d| valid_id(d))?.to_string();
        let ai = fs::read_to_string(path.join("ai")).ok().and_then(|text| {
            let at = text
                .lines()
                .find_map(|l| l.strip_prefix("at="))
                .and_then(|v| v.parse().ok());
            let body = text
                .lines()
                .filter(|l| !l.starts_with("at="))
                .collect::<Vec<_>>()
                .join("\n");
            let watches = body
                .lines()
                .filter_map(|l| l.strip_prefix("alert="))
                .filter_map(|v| v.split('\t').next().map(String::from))
                .collect::<Vec<_>>();
            let mut a = Annotation::parse(&body, &watches).ok()?;
            a.at = at.unwrap_or(start);
            Some(a)
        });
        Some(Event {
            id,
            device,
            start,
            // An event interrupted by a restart ends where its metadata stopped.
            end: Some(num("end").filter(|e| *e >= start).unwrap_or(start)),
            peak: num("peak")?.min(1000) as u16,
            frames: num("frames")? as u32,
            bytes: num("bytes").unwrap_or(0),
            ai,
            pending: false,
            failed: false,
        })
    }
    fn event_dir(&self, id: u64) -> PathBuf {
        self.dir.join(id.to_string())
    }
    pub fn clip_path(&self, id: u64) -> Option<PathBuf> {
        self.events
            .contains_key(&id)
            .then(|| self.event_dir(id).join("clip"))
    }
    pub fn key_path(&self, id: u64) -> Option<PathBuf> {
        self.events
            .get(&id)
            .filter(|e| e.end.is_some())
            .map(|_| self.event_dir(id).join("key.jpg"))
    }
    fn save_meta(&self, e: &Event) -> io::Result<()> {
        let text = format!(
            "device={}\nstart={}\nend={}\npeak={}\nframes={}\nbytes={}\n",
            e.device,
            e.start,
            e.end.unwrap_or(0),
            e.peak,
            e.frames,
            e.bytes
        );
        private_file(&self.event_dir(e.id).join("meta"))?.write_all(text.as_bytes())
    }
    /// Current motion level and recording event for a device.
    pub fn status(&self, device: &str) -> (u16, u64) {
        self.trackers
            .get(device)
            .map_or((0, 0), |t| (t.motion, t.rec.as_ref().map_or(0, |r| r.id)))
    }
    /// Record one accepted frame. Returns the active event ID (0 if none) and a vision job.
    pub fn frame(
        &mut self,
        device: &str,
        jpeg: &Arc<Vec<u8>>,
        motion: Option<u16>,
    ) -> (u64, Option<Job>) {
        let Some(motion) = motion else {
            return (0, None);
        };
        let now = Instant::now();
        let mut tracker = self.trackers.remove(device).unwrap_or_default();
        tracker.motion = motion;
        tracker.hot = if motion >= ON {
            tracker.hot.saturating_add(1)
        } else {
            0
        };
        while tracker
            .preroll
            .front()
            .is_some_and(|(at, _)| at.elapsed() > PREROLL_AGE)
        {
            if let Some((_, old)) = tracker.preroll.pop_front() {
                tracker.preroll_bytes -= old.len();
            }
        }
        if tracker.rec.is_none() && tracker.hot >= 2 {
            tracker.rec = self.start(device, &tracker).ok();
        }
        let mut job = None;
        if let Some(mut rec) = tracker.rec.take() {
            let room = rec.frames < MAX_CLIP_FRAMES && rec.bytes + jpeg.len() <= MAX_CLIP_BYTES;
            let ok = !room || write_frame(&mut rec.file, rec.origin.elapsed(), jpeg).is_ok();
            if room && ok {
                rec.frames += 1;
                rec.bytes += jpeg.len();
                rec.latest = Arc::clone(jpeg);
                if motion > rec.peak {
                    rec.peak = motion;
                    rec.key = Arc::clone(jpeg);
                }
            }
            rec.last_frame = now;
            rec.quiet = if motion >= OFF {
                None
            } else {
                rec.quiet.or(Some(now))
            };
            if !rec.analyzed && rec.origin.elapsed() >= ANALYZE_AFTER {
                job = self.auto_job(device, &mut rec);
            }
            let done = !ok || !room || rec.quiet.is_some_and(|q| q.elapsed() >= QUIET);
            if done {
                job = job.or(self.finish(device, rec));
            } else {
                tracker.rec = Some(rec);
            }
        }
        tracker.preroll_bytes += jpeg.len();
        tracker.preroll.push_back((now, Arc::clone(jpeg)));
        while tracker.preroll.len() > PREROLL_FRAMES
            || tracker.preroll_bytes > PREROLL_BYTES
            || tracker
                .preroll
                .front()
                .is_some_and(|(at, _)| at.elapsed() > PREROLL_AGE)
        {
            if let Some((_, old)) = tracker.preroll.pop_front() {
                tracker.preroll_bytes -= old.len();
            }
        }
        let id = tracker.rec.as_ref().map_or(0, |r| r.id);
        self.trackers.insert(device.into(), tracker);
        (id, job)
    }
    fn start(&mut self, device: &str, tracker: &Tracker) -> io::Result<Recording> {
        let id = self.next;
        self.next += 1;
        let dir = self.event_dir(id);
        private_directory(&dir)?;
        let origin = tracker
            .preroll
            .front()
            .map_or_else(Instant::now, |(at, _)| *at);
        let mut file = private_file(&dir.join("clip"))?;
        file.write_all(b"XLC1")?;
        let (mut frames, mut bytes) = (0, 0);
        for (at, jpeg) in &tracker.preroll {
            write_frame(&mut file, at.duration_since(origin), jpeg)?;
            frames += 1;
            bytes += jpeg.len();
        }
        let first = tracker
            .preroll
            .back()
            .map(|(_, j)| Arc::clone(j))
            .unwrap_or_default();
        let event = Event {
            id,
            device: device.into(),
            start: unix_now().saturating_sub(origin.elapsed().as_secs()),
            end: None,
            peak: tracker.motion,
            frames,
            bytes: bytes as u64,
            ai: None,
            pending: false,
            failed: false,
        };
        self.save_meta(&event)?;
        self.events.insert(id, event);
        self.prune();
        Ok(Recording {
            id,
            file,
            origin,
            last_frame: Instant::now(),
            quiet: None,
            // The triggering frame is written next and becomes the keyframe.
            peak: 0,
            key: Arc::clone(&first),
            latest: Arc::clone(&first),
            first,
            frames,
            bytes,
            analyzed: false,
        })
    }
    fn auto_job(&mut self, device: &str, rec: &mut Recording) -> Option<Job> {
        rec.analyzed = true;
        let due = self
            .last_job
            .get(device)
            .is_none_or(|at| at.elapsed() >= ANALYZE_GAP);
        if !self.auto || !due {
            return None;
        }
        self.last_job.insert(device.into(), Instant::now());
        if let Some(e) = self.events.get_mut(&rec.id) {
            e.pending = true;
        }
        let mut frames = vec![Arc::clone(&rec.first)];
        for f in [&rec.key, &rec.latest] {
            if !frames.iter().any(|x| Arc::ptr_eq(x, f)) {
                frames.push(Arc::clone(f));
            }
        }
        Some(Job {
            event: rec.id,
            device: device.into(),
            frames,
            peak: rec.peak,
            seconds: rec.origin.elapsed().as_secs(),
        })
    }
    fn finish(&mut self, device: &str, mut rec: Recording) -> Option<Job> {
        let job = if rec.analyzed {
            None
        } else {
            self.auto_job(device, &mut rec)
        };
        let _ = private_file(&self.event_dir(rec.id).join("key.jpg"))
            .and_then(|mut f| f.write_all(&rec.key));
        if let Some(mut e) = self.events.remove(&rec.id) {
            e.end = Some(unix_now());
            e.peak = rec.peak;
            e.frames = rec.frames;
            e.bytes = rec.bytes as u64;
            let _ = self.save_meta(&e);
            self.events.insert(rec.id, e);
        }
        job
    }
    /// Close recordings whose device stopped sending frames.
    pub fn tick(&mut self) -> Vec<Job> {
        let stale = self
            .trackers
            .iter()
            .filter(|(_, t)| {
                t.rec
                    .as_ref()
                    .is_some_and(|r| r.last_frame.elapsed() >= STALE)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let mut jobs = Vec::new();
        for device in stale {
            let rec = self.trackers.get_mut(&device).and_then(|t| t.rec.take());
            if let Some(job) = rec.and_then(|rec| self.finish(&device, rec)) {
                jobs.push(job);
            }
        }
        jobs
    }
    /// A manual re-analysis job from a finished clip: first, middle and last frames.
    pub fn job_for(&mut self, id: u64) -> Option<Job> {
        let e = self
            .events
            .get(&id)
            .filter(|e| e.end.is_some() && !e.pending)?;
        let bytes = fs::read(self.event_dir(id).join("clip")).ok()?;
        let frames = clip_frames(&bytes);
        if frames.is_empty() {
            return None;
        }
        let picks = [0, frames.len() / 2, frames.len() - 1];
        let mut chosen: Vec<usize> = picks.to_vec();
        chosen.dedup();
        let job = Job {
            event: id,
            device: e.device.clone(),
            frames: chosen
                .into_iter()
                .map(|i| Arc::new(frames[i].to_vec()))
                .collect(),
            peak: e.peak,
            seconds: e.end.unwrap_or(e.start).saturating_sub(e.start),
        };
        if let Some(e) = self.events.get_mut(&id) {
            e.pending = true;
            e.failed = false;
        }
        Some(job)
    }
    pub fn annotate(&mut self, id: u64, result: Option<Annotation>) {
        let path = self.event_dir(id).join("ai");
        let Some(e) = self.events.get_mut(&id) else {
            return;
        };
        e.pending = false;
        e.failed = result.is_none();
        if let Some(a) = result {
            let _ = private_file(&path).and_then(|mut f| f.write_all(a.wire().as_bytes()));
            e.ai = Some(a);
        }
    }
    pub fn cancel(&mut self, id: u64) {
        if let Some(e) = self.events.get_mut(&id) {
            e.pending = false;
        }
    }
    fn prune(&mut self) {
        let mut total: u64 = self.events.values().map(|e| e.bytes).sum();
        while self.events.len() > MAX_EVENTS || (total > MAX_TOTAL_BYTES && self.events.len() > 1) {
            let Some((&id, _)) = self.events.iter().next() else {
                break;
            };
            if self
                .trackers
                .values()
                .any(|t| t.rec.as_ref().is_some_and(|r| r.id == id))
            {
                break;
            }
            if let Some(e) = self.events.remove(&id) {
                total = total.saturating_sub(e.bytes);
            }
            let _ = fs::remove_dir_all(self.event_dir(id));
        }
    }
    /// Earliest event start for a device within [from, to] (UTC seconds).
    pub fn first_motion(&self, device: &str, from: u64, to: u64) -> Option<u64> {
        self.events
            .values()
            .filter(|e| e.device == device && (from..=to).contains(&e.start))
            .map(|e| e.start)
            .min()
    }
    /// Events that started at or after `since`, oldest first.
    pub fn since(&self, since: u64) -> Vec<&Event> {
        self.events.values().filter(|e| e.start >= since).collect()
    }
    pub fn json(&self, limit: usize) -> String {
        let rows = self
            .events
            .values()
            .rev()
            .take(limit)
            .map(Event::json)
            .collect::<Vec<_>>();
        format!("[{}]", rows.join(","))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root(tag: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("xlens-activity-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        private_directory(&root).unwrap();
        root
    }
    fn jpeg(n: u8) -> Arc<Vec<u8>> {
        Arc::new(vec![0xff, 0xd8, n, 0xff, 0xd9])
    }
    #[test]
    fn event_needs_two_hot_frames_records_preroll_and_persists() {
        let root = root("lifecycle");
        let mut a = Activity::open(&root).unwrap();
        let (id, job) = a.frame("cam", &jpeg(1), None); // No motion data: no events.
        assert!(id == 0 && job.is_none());
        a.frame("cam", &jpeg(2), Some(0));
        assert_eq!(a.frame("cam", &jpeg(3), Some(ON)).0, 0); // One hot frame is noise.
        assert_eq!(a.frame("cam", &jpeg(4), Some(5)).0, 0);
        a.frame("cam", &jpeg(5), Some(ON));
        let (id, job) = a.frame("cam", &jpeg(6), Some(400));
        assert_eq!((id, job.is_none()), (1, true));
        assert_eq!(a.status("cam"), (400, 1));
        a.frame("cam", &jpeg(7), Some(OFF)); // Hysteresis keeps it alive.
                                             // Device goes silent: tick closes it and asks for exactly one analysis.
        a.trackers
            .get_mut("cam")
            .unwrap()
            .rec
            .as_mut()
            .unwrap()
            .last_frame -= STALE;
        let jobs = a.tick();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!((job.event, job.peak, job.frames.len()), (1, 400, 3));
        assert_eq!(job.frames[1].as_slice(), jpeg(6).as_slice()); // Peak frame in the middle.
        let clip = fs::read(a.clip_path(1).unwrap()).unwrap();
        let frames = clip_frames(&clip);
        assert_eq!(frames.len(), 6); // 4 pre-roll frames (frame 1 had no motion data), trigger, one more.
        assert_eq!(frames[0], jpeg(2).as_slice());
        assert_eq!(fs::read(a.key_path(1).unwrap()).unwrap(), *jpeg(6));
        let watches = vec!["door".to_string()];
        let text = "source=meta\nactivity=person\ncaption=A person waves.\nalert=door\thigh\tWaving at the door.\n";
        a.annotate(1, Some(Annotation::parse(text, &watches).unwrap()));
        let reopened = Activity::open(&root).unwrap();
        let e = &reopened.events[&1];
        assert_eq!((e.frames, e.peak, e.device.as_str()), (6, 400, "cam"));
        let ai = e.ai.as_ref().unwrap();
        assert_eq!(
            (ai.caption.as_str(), ai.alerts[0].watch.as_str()),
            ("A person waves.", "door")
        );
        assert!(e.json().contains("\"reason\":\"Waving at the door.\""));
        assert_eq!(reopened.next, 2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn analysis_is_rate_limited_per_device_and_annotations_are_validated() {
        let root = root("limits");
        let mut a = Activity::open(&root).unwrap();
        let mut jobs = 0;
        for round in 0..2u8 {
            for m in [ON, ON, 0] {
                a.frame("cam", &jpeg(round), Some(m));
            }
            a.trackers
                .get_mut("cam")
                .unwrap()
                .rec
                .as_mut()
                .unwrap()
                .quiet = Some(Instant::now() - QUIET);
            jobs += usize::from(a.frame("cam", &jpeg(9), Some(0)).1.is_some());
        }
        assert_eq!((a.events.len(), jobs), (2, 1)); // Second event within ANALYZE_GAP is not auto-analyzed.
        assert!(a.job_for(2).is_some() && a.job_for(2).is_none()); // Manual analysis; no double queueing.
        let bad = [
            "activity=person\ncaption=x\n",                     // No source.
            "source=meta\ncaption=x\nalert=other\thigh\tr\n",   // Unknown watch.
            "source=meta\ncaption=x\nalert=door\tcertain\tr\n", // Unknown confidence.
            "source=meta\ncaption=line\u{7}\n",                 // Control character.
            "source=meta\ncaption=x\ncommand=unlock\n",         // Unknown field.
        ];
        for text in bad {
            assert!(
                Annotation::parse(text, &["door".to_string()]).is_err(),
                "{text}"
            );
        }
        assert_eq!(json_str("a\"<b>\n"), "\"a\\\"\\u003cb>\\u000a\"");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn preroll_skips_frames_from_before_an_outage() {
        let root = root("outage");
        let mut a = Activity::open(&root).unwrap();
        for n in 0..5 {
            a.frame("cam", &jpeg(n), Some(0));
        }
        for (at, _) in &mut a.trackers.get_mut("cam").unwrap().preroll {
            *at -= PREROLL_AGE * 2; // Device went offline after these frames.
        }
        a.frame("cam", &jpeg(10), Some(ON));
        assert_eq!(a.frame("cam", &jpeg(11), Some(ON)).0, 1);
        let clip = fs::read(a.clip_path(1).unwrap()).unwrap();
        assert_eq!(
            clip_frames(&clip),
            vec![jpeg(10).as_slice(), jpeg(11).as_slice()]
        );
        fs::remove_dir_all(root).unwrap();
    }
}

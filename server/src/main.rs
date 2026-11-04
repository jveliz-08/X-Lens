//! Small, deliberately restricted HTTP/1.1 origin behind a trusted TLS proxy.
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

mod activity;
mod api;
mod care;
mod drafts;
mod rules;
mod vision;
mod watches;
mod zones;
const MAX_FRAME: usize = 128 * 1024;
/// Push-to-talk recordings: 16 s of 16 kHz mono 16-bit WAV.
const MAX_AUDIO: usize = 512 * 1024;
const MAX_HEAD: usize = 4096;
const RETAIN: usize = 32;
const WORKERS: usize = 4;
const DEADLINE: Duration = Duration::from_secs(3);
// Live frames stay in memory; disk keeps at most one evidence snapshot per interval.
const SNAPSHOT_EVERY: Duration = Duration::from_secs(1);
const MAX_STREAMS: usize = 16;
const STREAM_IDLE: Duration = Duration::from_secs(5);
const STREAM_LIFETIME: Duration = Duration::from_secs(600);
const HTML: &[u8] = include_bytes!("../../web/index.html");
const JS: &[u8] = include_bytes!("../../web/app.js");
const CSS: &[u8] = include_bytes!("../../web/style.css");
struct Config {
    /// Directory holding the config; also the default home of meta.secret and AI ledgers.
    dir: PathBuf,
    operator: Option<String>,
    viewer: String,
    devices: BTreeMap<String, String>,
}
struct State {
    engine: rules::Engine,
    last_seen: BTreeMap<String, Instant>,
    buckets: BTreeMap<String, (Instant, f64)>,
    started: Instant,
    auth_failures: u64,
    stats: BTreeMap<String, [u64; 3]>,
    writable: bool,
    high: BTreeMap<String, u64>,
    snapshots: BTreeMap<String, Instant>,
    snapshot_every: Duration,
    activity: activity::Activity,
    watches: watches::Watches,
    care: care::Care,
    zones: zones::Zones,
    /// Language for automatic event captions (questions are answered in their own language).
    language: String,
    root: PathBuf,
}
/// Sequence, JPEG bytes and age of the newest accepted frame.
type Snapshot = (u64, Arc<Vec<u8>>, Duration);
struct Frame {
    seq: u64,
    jpeg: Arc<Vec<u8>>,
    at: Instant,
    motion: Option<u16>,
    event: u64,
}
/// Latest accepted frame per device, shared with long-lived viewer streams.
struct Live {
    frames: Mutex<BTreeMap<String, Frame>>,
    changed: Condvar,
    streams: AtomicUsize,
    /// Open viewer streams per device; reported to cameras so they stream on demand.
    viewers: Mutex<BTreeMap<String, usize>>,
}
impl Live {
    fn new() -> Self {
        Self {
            frames: Mutex::new(BTreeMap::new()),
            changed: Condvar::new(),
            streams: AtomicUsize::new(0),
            viewers: Mutex::new(BTreeMap::new()),
        }
    }
    fn watch(&self, id: &str, delta: isize) {
        if let Ok(mut v) = self.viewers.lock() {
            let n = v.entry(id.into()).or_default();
            *n = n.saturating_add_signed(delta);
        }
    }
    fn viewers(&self, id: &str) -> usize {
        self.viewers
            .lock()
            .map_or(0, |v| v.get(id).copied().unwrap_or(0))
    }
    fn publish(
        &self,
        id: &str,
        seq: u64,
        jpeg: Arc<Vec<u8>>,
        motion: Option<u16>,
        event: u64,
    ) -> io::Result<()> {
        let mut frames = self
            .frames
            .lock()
            .map_err(|_| io::Error::other("live state unavailable"))?;
        let frame = Frame {
            seq,
            jpeg,
            at: Instant::now(),
            motion,
            event,
        };
        frames.insert(id.into(), frame);
        self.changed.notify_all();
        Ok(())
    }
    fn latest(&self, id: &str) -> io::Result<Option<Snapshot>> {
        let frames = self
            .frames
            .lock()
            .map_err(|_| io::Error::other("live state unavailable"))?;
        Ok(frames
            .get(id)
            .map(|f| (f.seq, Arc::clone(&f.jpeg), f.at.elapsed())))
    }
    fn tags(&self, id: &str) -> (Option<u16>, u64) {
        self.frames
            .lock()
            .ok()
            .and_then(|f| f.get(id).map(|f| (f.motion, f.event)))
            .unwrap_or((None, 0))
    }
    /// Wait for a frame newer than `after`; None means the idle timeout elapsed.
    fn wait_newer(&self, id: &str, after: u64, timeout: Duration) -> io::Result<Option<Snapshot>> {
        let deadline = Instant::now() + timeout;
        let mut frames = self
            .frames
            .lock()
            .map_err(|_| io::Error::other("live state unavailable"))?;
        loop {
            if let Some(f) = frames.get(id).filter(|f| f.seq > after) {
                return Ok(Some((f.seq, Arc::clone(&f.jpeg), f.at.elapsed())));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            frames = self
                .changed
                .wait_timeout(frames, left)
                .map_err(|_| io::Error::other("live state unavailable"))?
                .0;
        }
    }
}
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid input")
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 24
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn token(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn config(path: &Path) -> io::Result<Config> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other("config must be mode 600"));
        }
    }
    let text = fs::read_to_string(path)?;
    if text.len() > 4096 {
        return Err(invalid());
    }
    let mut viewer = String::new();
    let mut operator = None;
    let mut devices = BTreeMap::new();
    for line in text
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let (key, value) = line.split_once('=').ok_or_else(invalid)?;
        if !token(value) {
            return Err(invalid());
        }
        if key == "viewer" && viewer.is_empty() {
            viewer = value.into();
        } else if key == "operator" && operator.is_none() {
            operator = Some(value.to_owned());
        } else if let Some(id) = key.strip_prefix("device.") {
            if !valid_id(id) || devices.insert(id.into(), value.into()).is_some() {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
    }
    let mut all = devices.values().collect::<Vec<_>>();
    all.push(&viewer);
    if let Some(key) = &operator {
        all.push(key);
    }
    all.sort();
    if !token(&viewer)
        || devices.is_empty()
        || devices.len() > 16
        || all.windows(2).any(|p| p[0] == p[1])
    {
        return Err(invalid());
    }
    Ok(Config {
        dir: path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        viewer,
        devices,
        operator,
    })
}
fn private_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(path)?;
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "data directory must be private (mode 700), not a symlink",
            ));
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)?;
    }
    Ok(())
}
fn files(dir: &Path) -> io::Result<Vec<u64>> {
    let mut seqs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if let Some(s) = name.strip_suffix(".jpg") {
            seqs.push(s.parse().map_err(|_| invalid())?);
        }
    }
    seqs.sort_unstable();
    Ok(seqs)
}
fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let seqs = files(dir)?;
    for seq in seqs.iter().take(seqs.len().saturating_sub(keep)) {
        fs::remove_file(dir.join(format!("{seq}.jpg")))?;
    }
    Ok(())
}
fn atomic(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let tmp = dir.join("pending");
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(tmp, dir.join(name))?;
    File::open(dir)?.sync_all()
}
impl State {
    fn open(root: PathBuf, cfg: &Config) -> io::Result<Self> {
        private_directory(&root)?;
        let mut high = BTreeMap::new();
        for id in cfg.devices.keys() {
            let dir = root.join(id);
            private_directory(&dir)?;
            let watermark = dir.join("sequence");
            let seq = match fs::read_to_string(&watermark) {
                Ok(v) => v.parse().map_err(|_| invalid())?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if !files(&dir)?.is_empty() {
                        return Err(invalid());
                    }
                    0
                }
                Err(e) => return Err(e),
            };
            if files(&dir)?.iter().any(|n| *n > seq) {
                return Err(invalid());
            }
            prune(&dir, RETAIN)?;
            match fs::remove_file(dir.join("pending")) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
            high.insert(id.clone(), seq);
        }
        let mut state = Self {
            engine: rules::Engine::open(&root, cfg)?,
            last_seen: cfg
                .devices
                .keys()
                .map(|id| (id.clone(), Instant::now()))
                .collect(),
            buckets: cfg
                .devices
                .keys()
                .map(|id| (id.clone(), (Instant::now(), 64.0)))
                .collect(),
            high,
            snapshots: BTreeMap::new(),
            snapshot_every: SNAPSHOT_EVERY,
            activity: activity::Activity::open(&root)?,
            watches: watches::Watches::open(&root, cfg)?,
            care: care::Care::open(&root, cfg)?,
            zones: zones::Zones::open(&root, cfg)?,
            language: "en".into(),
            root,
            writable: true,
            started: Instant::now(),
            auth_failures: 0,
            stats: cfg.devices.keys().map(|id| (id.clone(), [0; 3])).collect(),
        };
        if let Ok(text) = fs::read_to_string(state.root.join("_ai")) {
            state.set_ai(text.as_bytes())?;
        }
        Ok(state)
    }
    /// AI settings body: `language=<2 lowercase letters>` and `auto=0|1` (automatic event captions).
    fn set_ai(&mut self, body: &[u8]) -> io::Result<()> {
        let text = std::str::from_utf8(body).map_err(|_| invalid())?;
        let (mut language, mut auto) = (self.language.clone(), self.activity.auto);
        for line in text.lines().filter(|l| !l.is_empty()) {
            match line.split_once('=') {
                Some(("language", v))
                    if v.len() == 2 && v.bytes().all(|b| b.is_ascii_lowercase()) =>
                {
                    language = v.into()
                }
                Some(("auto", "1")) => auto = true,
                Some(("auto", "0")) => auto = false,
                _ => return Err(invalid()),
            }
        }
        (self.language, self.activity.auto) = (language, auto);
        Ok(())
    }
    fn ai_json(&self, cfg: &Config) -> String {
        format!(
            "{{\"mode\":\"{}\",\"model\":{},\"key\":{},\"used\":{},\"daily\":{},\"language\":\"{}\",\"auto\":{}}}",
            vision::mode(&cfg.dir),
            activity::json_str(&vision::model()),
            vision::key_file(&cfg.dir).is_file(),
            vision::used_today(&cfg.dir),
            vision::DAILY_BUDGET,
            self.language,
            self.activity.auto
        )
    }
    fn save(&mut self, id: &str, seq: u64, body: &[u8]) -> io::Result<bool> {
        self.save_using(id, seq, body, atomic)
    }
    fn save_using<F>(&mut self, id: &str, seq: u64, body: &[u8], persist: F) -> io::Result<bool>
    where
        F: Fn(&Path, &str, &[u8]) -> io::Result<()>,
    {
        if !self.writable {
            return Err(io::Error::other("storage requires operator recovery"));
        }
        if seq <= self.high[id] {
            self.event(id, 1);
            return Ok(false);
        }
        let dir = self.root.join(id);
        // Burn the in-memory sequence first. A rename followed by failed fsync has
        // ambiguous durability; stop all writes until recovery rather than reuse it.
        self.high.insert(id.into(), seq);
        let snapshot = self
            .snapshots
            .get(id)
            .is_none_or(|at| at.elapsed() >= self.snapshot_every);
        let result = (|| {
            persist(&dir, "sequence", seq.to_string().as_bytes())?;
            if snapshot {
                prune(&dir, RETAIN - 1)?;
                persist(&dir, &format!("{seq}.jpg"), body)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.writable = false;
            return Err(e);
        }
        if snapshot {
            self.snapshots.insert(id.into(), Instant::now());
        }
        self.last_seen.insert(id.into(), Instant::now());
        self.event(id, 0);
        Ok(true)
    }
    fn admit(&mut self, id: &str) -> bool {
        let (at, tokens) = self.buckets.get_mut(id).unwrap();
        *tokens = (*tokens + at.elapsed().as_secs_f64() * 20.0).min(64.0);
        *at = Instant::now();
        if *tokens < 1.0 {
            false
        } else {
            *tokens -= 1.0;
            true
        }
    }
    fn evaluate(&mut self) {
        self.engine
            .evaluate(&self.stats, self.auth_failures, &self.last_seen);
    }
    fn event(&mut self, id: &str, index: usize) {
        if let Some(counts) = self.stats.get_mut(id) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
}
fn remaining(s: &TcpStream, start: Instant) -> io::Result<()> {
    let left = DEADLINE
        .checked_sub(start.elapsed())
        .filter(|v| !v.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "deadline"))?;
    s.set_read_timeout(Some(left))
}
fn parse(s: &mut TcpStream) -> io::Result<Request> {
    let start = Instant::now();
    let mut head = Vec::with_capacity(MAX_HEAD);
    let mut b = [0u8; 1];
    // No read-ahead: never allocate an attacker-selected body before checking the length.
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HEAD {
            return Err(invalid());
        }
        remaining(s, start)?;
        s.read_exact(&mut b)?;
        head.push(b[0]);
    }
    let h = std::str::from_utf8(&head).map_err(|_| invalid())?;
    let mut lines = h[..h.len() - 4].split("\r\n");
    let parts = lines
        .next()
        .ok_or_else(invalid)?
        .split(' ')
        .collect::<Vec<_>>();
    if parts.len() != 3
        || parts[2] != "HTTP/1.1"
        || !matches!(parts[0], "GET" | "POST")
        || !parts[1].starts_with('/')
    {
        return Err(invalid());
    }
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for line in lines {
        let (key, val) = line.split_once(':').ok_or_else(invalid)?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || val.bytes().any(|b| b < 32 || b == 127)
        {
            return Err(invalid());
        }
        if headers
            .insert(key.to_ascii_lowercase(), val.trim().into())
            .is_some()
        {
            return Err(invalid());
        }
    }
    if !headers.contains_key("host")
        || headers.contains_key("transfer-encoding")
        || headers.contains_key("expect")
    {
        return Err(invalid());
    }
    let len = match headers.get("content-length") {
        Some(v) if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) => {
            v.parse::<usize>().map_err(|_| invalid())?
        }
        None => 0,
        _ => return Err(invalid()),
    };
    let limit = if parts[0] == "POST" && parts[1] == "/api/transcribe" {
        MAX_AUDIO
    } else {
        MAX_FRAME
    };
    if len > limit || (parts[0] == "GET" && len != 0) {
        return Err(invalid());
    }
    let mut body = vec![0; len];
    let mut read = 0;
    while read < len {
        remaining(s, start)?;
        let n = s.read(&mut body[read..])?;
        if n == 0 {
            return Err(invalid());
        }
        read += n;
    }
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].into(),
        headers,
        body,
    })
}
fn authorized(r: &Request, key: &str) -> bool {
    r.headers
        .get("authorization")
        .is_some_and(|v| v.strip_prefix("Bearer ") == Some(key))
}
fn reply(s: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> io::Result<()> {
    reply_with_sequence(s, code, kind, body, None)
}
fn reply_with_sequence(
    s: &mut TcpStream,
    code: u16,
    kind: &str,
    body: &[u8],
    sequence: Option<u64>,
) -> io::Result<()> {
    let sequence_header = sequence
        .map(|n| format!("X-Sequence: {n}\r\n"))
        .unwrap_or_default();
    reply_full(s, code, kind, body, &sequence_header)
}
/// `extra` is zero or more complete `Name: value\r\n` header lines built by this server.
fn reply_full(
    s: &mut TcpStream,
    code: u16,
    kind: &str,
    body: &[u8],
    extra: &str,
) -> io::Result<()> {
    let retry_header = if matches!(code, 429 | 503) {
        "Retry-After: 1\r\n"
    } else {
        ""
    };
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(s, "HTTP/1.1 {code} {}\r\nContent-Length: {}\r\nX-Protocol: 1\r\n{retry_header}Content-Type: {kind}\r\n{extra}Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src blob:; media-src blob:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\nConnection: close\r\n\r\n", if code < 400 { "OK" } else { "Error" }, body.len())?;
    s.write_all(body)
}
fn denied(state: &Mutex<State>) -> io::Result<()> {
    let mut state = state
        .lock()
        .map_err(|_| io::Error::other("state unavailable"))?;
    state.auth_failures = state.auth_failures.saturating_add(1);
    Ok(())
}
/// Hand an authorized viewer socket to a dedicated MJPEG thread; workers stay free.
fn start_stream(s: &mut TcpStream, live: &Arc<Live>, id: &str) -> io::Result<()> {
    if live.streams.fetch_add(1, Ordering::SeqCst) >= MAX_STREAMS {
        live.streams.fetch_sub(1, Ordering::SeqCst);
        return reply(s, 503, "text/plain", b"Too many live streams");
    }
    live.watch(id, 1);
    let spawned = s.try_clone().and_then(|mut out| {
        let (live, id) = (Arc::clone(live), id.to_owned());
        thread::Builder::new().spawn(move || {
            let _ = stream(&mut out, &live, &id);
            live.watch(&id, -1);
            live.streams.fetch_sub(1, Ordering::SeqCst);
        })
    });
    if let Err(e) = spawned {
        live.watch(id, -1);
        live.streams.fetch_sub(1, Ordering::SeqCst);
        return Err(e);
    }
    Ok(())
}
fn stream(s: &mut TcpStream, live: &Live, id: &str) -> io::Result<()> {
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    s.write_all(b"HTTP/1.1 200 OK\r\nX-Protocol: 1\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n")?;
    let start = Instant::now();
    let mut sent = 0;
    while start.elapsed() < STREAM_LIFETIME {
        let part = match live.wait_newer(id, sent, STREAM_IDLE)? {
            Some((seq, jpeg, age)) => {
                sent = seq;
                let (motion, event) = live.tags(id);
                let motion = motion.map_or(String::new(), |m| format!("X-Motion: {m}\r\n"));
                let mut part = format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nX-Sequence: {seq}\r\nX-Age-Ms: {}\r\n{motion}X-Event: {event}\r\n\r\n",
                    jpeg.len(),
                    age.as_millis()
                )
                .into_bytes();
                part.extend_from_slice(&jpeg);
                part.extend_from_slice(b"\r\n");
                part
            }
            // Heartbeat: detects closed viewers and keeps proxies from idling out.
            None => {
                b"--frame\r\nContent-Type: text/plain\r\nContent-Length: 0\r\n\r\n\r\n".to_vec()
            }
        };
        s.write_all(&part)?;
    }
    Ok(())
}
fn handle(
    s: &mut TcpStream,
    cfg: &Config,
    state: &Mutex<State>,
    live: &Arc<Live>,
    jobs: &api::Jobs,
) -> io::Result<()> {
    let r = match parse(s) {
        Ok(r) => r,
        Err(_) => return reply(s, 400, "text/plain", b"Invalid request"),
    };
    let status = |s: &mut TcpStream, code| reply(s, code, "text/plain", b"");
    if r.path.starts_with("/api/") && r.headers.get("x-protocol").is_some_and(|v| v != "1") {
        return status(s, 426);
    }
    if let Some(result) = api::route(s, &r, cfg, state, live, jobs) {
        return result;
    }
    if let Some(id) = r.path.strip_prefix("/api/rule-draft/") {
        if r.method != "POST" || !cfg.operator.as_ref().is_some_and(|key| authorized(&r, key)) {
            denied(state)?;
            return status(s, 401);
        }
        if !cfg.devices.contains_key(id)
            || r.headers.get("x-protocol").map(String::as_str) != Some("1")
            || r.headers.get("content-type").map(String::as_str) != Some("text/plain")
            || r.body.is_empty()
            || r.body.len() > 960
        {
            return status(s, 400);
        }
        return match drafts::call(&cfg.dir, id, &r.body) {
            Ok(data) => reply(s, 200, "application/json", &data),
            Err(_) => reply(
                s,
                502,
                "text/plain",
                b"Proposal unavailable or outcome unknown; no automatic retry. No rule activated.",
            ),
        };
    }
    if let Some(id) = r.path.strip_prefix("/api/device-state/") {
        if r.method != "GET" || !cfg.devices.get(id).is_some_and(|key| authorized(&r, key)) {
            denied(state)?;
            return status(s, 401);
        }
        let state = state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?;
        let Some(next) = state.high[id].checked_add(1) else {
            return status(s, 503);
        };
        return reply(
            s,
            200,
            "application/json",
            format!(
                "{{\"next_sequence\":\"{next}\",\"protocol\":1,\"max_frame_bytes\":{MAX_FRAME}}}"
            )
            .as_bytes(),
        );
    }
    if r.method == "POST" && r.path.starts_with("/api/rules/") {
        if !cfg.operator.as_ref().is_some_and(|key| authorized(&r, key)) {
            denied(state)?;
            return status(s, 401);
        }
        if r.headers.get("x-protocol").map(String::as_str) != Some("1")
            || r.headers.get("content-type").map(String::as_str) != Some("text/plain")
        {
            return status(s, 400);
        }
        let rule = match rules::Rule::parse(&r.path[11..], &r.body, cfg) {
            Ok(r) => r,
            Err(_) => return status(s, 400),
        };
        let mut st = state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?;
        if !st.writable {
            return status(s, 503);
        }
        if !st.engine.rules.contains_key(&rule.id) && st.engine.rules.len() >= 16 {
            return status(s, 422);
        }
        let root = st.root.clone();
        if st.engine.put(&root, rule).is_err() {
            st.writable = false;
            return status(s, 500);
        }
        return status(s, 204);
    }
    if r.method == "GET" {
        match r.path.as_str() {
            "/" => return reply(s, 200, "text/html; charset=utf-8", HTML),
            "/app.js" => return reply(s, 200, "text/javascript", JS),
            "/style.css" => return reply(s, 200, "text/css", CSS),
            _ => (),
        }
        // Browsers probe /favicon.ico and similar; only API paths count toward auth failures.
        if !r.path.starts_with("/api/") {
            return status(s, 404);
        }
        if !authorized(&r, &cfg.viewer) {
            denied(state)?;
            return status(s, 401);
        }
        if let Some(id) = r.path.strip_prefix("/api/stream/") {
            if !cfg.devices.contains_key(id) {
                return status(s, 404);
            }
            return start_stream(s, live, id);
        }
        let mut state = state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?;
        if r.path == "/api/rules" {
            return reply(
                s,
                200,
                "application/json",
                format!(
                    "[{}]",
                    state
                        .engine
                        .rules
                        .values()
                        .map(|r| r.json())
                        .collect::<Vec<_>>()
                        .join(",")
                )
                .as_bytes(),
            );
        }
        if r.path == "/api/events" {
            state.evaluate();
            return reply(
                s,
                200,
                "application/json",
                format!(
                    "[{}]",
                    state
                        .engine
                        .events
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(",")
                )
                .as_bytes(),
            );
        }
        if r.path == "/api/rule-provider" {
            return reply(
                s,
                200,
                "application/json",
                format!("{{\"source\":\"{}\"}}", vision::mode(&cfg.dir)).as_bytes(),
            );
        }
        if r.path == "/api/capabilities" {
            return reply(s,200,"application/json",br#"{"protocol":1,"max_frame_bytes":131072,"upload_rate":20,"burst":64,"max_rules":16,"rule_action":"ui_event"}"#);
        }
        if r.path == "/api/security" {
            let rows = state
                .stats
                .iter()
                .map(|(id, c)| {
                    format!(
                        "{{\"id\":\"{id}\",\"accepted\":{},\"replays\":{},\"invalid_frames\":{}}}",
                        c[0], c[1], c[2]
                    )
                })
                .collect::<Vec<_>>();
            let json = format!(
                "{{\"uptime_seconds\":{},\"auth_failures\":{},\"devices\":[{}]}}",
                state.started.elapsed().as_secs(),
                state.auth_failures,
                rows.join(",")
            );
            return reply(s, 200, "application/json", json.as_bytes());
        }
        if r.path == "/api/devices" {
            let mut rows = Vec::new();
            for id in cfg.devices.keys() {
                let seqs = files(&state.root.join(id))?;
                let stored = seqs.last().copied().unwrap_or(0);
                let last = live.latest(id)?.map_or(stored, |(seq, ..)| seq.max(stored));
                rows.push(format!(
                    "{{\"id\":\"{id}\",\"sequence\":{last},\"retained\":{}}}",
                    seqs.len()
                ));
            }
            return reply(
                s,
                200,
                "application/json",
                format!("[{}]", rows.join(",")).as_bytes(),
            );
        }
        if let Some(id) = r.path.strip_prefix("/api/review/") {
            if !cfg.devices.contains_key(id) {
                return status(s, 404);
            }
            let path = state.root.join(id).join("review.json");
            match fs::metadata(&path) {
                Ok(meta) if meta.len() <= 8192 => {
                    return reply(s, 200, "application/json", &fs::read(path)?)
                }
                Ok(_) => return status(s, 500),
                Err(e) if e.kind() == io::ErrorKind::NotFound => return status(s, 404),
                Err(e) => return Err(e),
            }
        }
        if let Some(id) = r.path.strip_prefix("/api/latest/") {
            if !cfg.devices.contains_key(id) {
                return status(s, 404);
            }
            if let Some((seq, jpeg, _)) = live.latest(id)? {
                return reply_with_sequence(s, 200, "image/jpeg", &jpeg, Some(seq));
            }
            if let Some(seq) = files(&state.root.join(id))?.last() {
                let path = state.root.join(id).join(format!("{seq}.jpg"));
                if fs::metadata(&path)?.len() > MAX_FRAME as u64 {
                    return status(s, 500);
                }
                return reply_with_sequence(s, 200, "image/jpeg", &fs::read(path)?, Some(*seq));
            }
        }
        return status(s, 404);
    }
    if let Some(id) = r.path.strip_prefix("/api/frame/") {
        let Some(key) = cfg.devices.get(id) else {
            denied(state)?;
            return status(s, 401);
        };
        if !authorized(&r, key) {
            denied(state)?;
            return status(s, 401);
        }
        if !state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?
            .admit(id)
        {
            return status(s, 429);
        }
        if r.headers.get("content-type").map(String::as_str) != Some("image/jpeg")
            || r.body.len() < 4
            || !r.body.starts_with(&[0xff, 0xd8])
            || !r.body.ends_with(&[0xff, 0xd9])
        {
            state
                .lock()
                .map_err(|_| io::Error::other("state unavailable"))?
                .event(id, 2);
            return status(s, 415);
        }
        let seq = r
            .headers
            .get("x-sequence")
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u64>().ok());
        let Some(seq) = seq.filter(|v| *v > 0) else {
            return status(s, 400);
        };
        // Optional edge motion score (per mille of changed cells), computed by the producer.
        let motion = r
            .headers
            .get("x-motion")
            .filter(|v| (1..=4).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|m| *m <= 1000);
        let jpeg = Arc::new(r.body);
        let (saved, event, job) = {
            let mut st = state
                .lock()
                .map_err(|_| io::Error::other("state unavailable"))?;
            let saved = st.save(id, seq, &jpeg)?;
            let (event, job) = if saved {
                st.activity.frame(id, &jpeg, motion)
            } else {
                (0, None)
            };
            (saved, event, job)
        };
        if let Some(job) = job {
            api::queue(state, jobs, job);
        }
        if saved {
            live.publish(id, seq, jpeg, motion, event)?;
        }
        // Cameras stream at full rate only while someone watches (or the scene moves),
        // and skip the operator's ignore zones when they score motion.
        let mask = state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?
            .zones
            .header(id);
        let viewers = format!("X-Live-Viewers: {}\r\n{mask}", live.viewers(id));
        return reply_full(
            s,
            if saved { 201 } else { 409 },
            "text/plain",
            b"",
            &viewers,
        );
    }
    status(s, 404)
}
fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: x-lens CONFIG_FILE DATA_DIRECTORY (loopback :8080)");
        std::process::exit(2);
    }
    let cfg = Arc::new(config(Path::new(&args[1]))?);
    let state = Arc::new(Mutex::new(State::open(args[2].clone().into(), &cfg)?));
    let live = Arc::new(Live::new());
    // Loopback only; XLENS_PORT lets a second instance (e.g. the demo recorder) run beside it.
    let port = env::var("XLENS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p >= 1024)
        .unwrap_or(8080);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (jobs, pending) = mpsc::sync_channel::<activity::Job>(4);
    {
        let (cfg, state) = (Arc::clone(&cfg), Arc::clone(&state));
        thread::spawn(move || api::worker(cfg, state, pending));
    }
    {
        let state = Arc::clone(&state);
        let jobs = jobs.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            let finished = match state.lock() {
                Ok(mut s) => {
                    s.evaluate();
                    let jobs = s.activity.tick();
                    let st = &mut *s;
                    let activity = &st.activity;
                    let _ = st.care.check(activity::unix_now(), |d, from, to| {
                        activity.first_motion(d, from, to)
                    });
                    jobs
                }
                Err(_) => Vec::new(),
            };
            for job in finished {
                api::queue(&state, &jobs, job);
            }
        });
    }

    let (tx, rx) = mpsc::sync_channel::<TcpStream>(8);
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..WORKERS {
        let rx = Arc::clone(&rx);
        let cfg = Arc::clone(&cfg);
        let state = Arc::clone(&state);
        let live = Arc::clone(&live);
        let jobs = jobs.clone();
        thread::spawn(move || loop {
            let stream = { rx.lock().unwrap().recv() };
            let Ok(mut stream) = stream else { break };
            if handle(&mut stream, &cfg, &state, &live, &jobs).is_err() {
                let _ = reply(&mut stream, 500, "text/plain", b"Request failed");
            }
        });
    }
    eprintln!(
        "X-Lens on 127.0.0.1:{port}; 4 workers; 128 KiB/frame; 32 snapshots/device (1/s); {MAX_STREAMS} live streams; TLS proxy required"
    );
    for s in listener.incoming().flatten() {
        if let Err(mpsc::TrySendError::Full(s)) = tx.try_send(s) {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_storage_error_stops_writes() {
        let root = env::temp_dir().join(format!(
            "xlens-fault-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cfg = Config {
            dir: PathBuf::new(),
            operator: None,
            viewer: "a".repeat(64),
            devices: BTreeMap::from([("demo".into(), "b".repeat(64))]),
        };
        let mut state = State::open(root.clone(), &cfg).unwrap();
        let result = state.save_using("demo", 10, b"synthetic", |dir, name, body| {
            atomic(dir, name, body)?; // Simulate successful rename followed by a reported sync failure.
            Err(io::Error::other("injected directory sync failure"))
        });
        assert!(result.is_err());
        assert_eq!(state.high["demo"], 10);
        assert!(state.save("demo", 10, b"synthetic").is_err());
        assert!(state.save("demo", 11, b"synthetic").is_err());
        let mut recovered = State::open(root.clone(), &cfg).unwrap();
        assert!(!recovered.save("demo", 10, b"synthetic").unwrap());
        assert!(recovered.save("demo", 11, b"synthetic").unwrap());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshots_are_throttled_but_watermark_is_not() {
        let root = env::temp_dir().join(format!("xlens-snap-{}", std::process::id()));
        let cfg = Config {
            dir: PathBuf::new(),
            operator: None,
            viewer: "a".repeat(64),
            devices: BTreeMap::from([("demo".into(), "b".repeat(64))]),
        };
        let mut state = State::open(root.clone(), &cfg).unwrap();
        for seq in 1..=5 {
            assert!(state.save("demo", seq, b"synthetic").unwrap());
        }
        let dir = root.join("demo");
        assert_eq!(files(&dir).unwrap(), vec![1]);
        assert_eq!(fs::read_to_string(dir.join("sequence")).unwrap(), "5");
        assert!(!state.save("demo", 5, b"synthetic").unwrap());
        state.snapshot_every = Duration::ZERO;
        for seq in 6..=45 {
            assert!(state.save("demo", seq, b"synthetic").unwrap());
        }
        assert_eq!(files(&dir).unwrap(), (14..=45).collect::<Vec<_>>());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn live_waits_for_newer_frames() {
        let live = Live::new();
        assert!(live
            .wait_newer("demo", 0, Duration::from_millis(10))
            .unwrap()
            .is_none());
        live.publish("demo", 7, Arc::new(b"jpeg".to_vec()), Some(40), 3)
            .unwrap();
        assert_eq!(live.tags("demo"), (Some(40), 3));
        assert_eq!(
            live.wait_newer("demo", 0, Duration::ZERO)
                .unwrap()
                .unwrap()
                .0,
            7
        );
        assert!(live
            .wait_newer("demo", 7, Duration::from_millis(10))
            .unwrap()
            .is_none());
    }
    #[test]
    fn identifiers() {
        for v in ["../x", "a/b", "A", "", "a\"b"] {
            assert!(!valid_id(v));
        }
        assert!(valid_id("front-door-1"));
    }
    #[test]
    fn secrets() {
        assert!(token(&"a".repeat(64)));
        assert!(!token("short"));
        assert!(!token(&"z".repeat(64)));
    }
}

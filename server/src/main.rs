//! Small, deliberately restricted HTTP/1.1 origin behind a trusted TLS proxy.
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

mod rules;
const MAX_FRAME: usize = 128 * 1024;
const MAX_HEAD: usize = 4096;
const RETAIN: usize = 32;
const WORKERS: usize = 4;
const DEADLINE: Duration = Duration::from_secs(3);
const HTML: &[u8] = include_bytes!("../../web/index.html");
const JS: &[u8] = include_bytes!("../../web/app.js");
const CSS: &[u8] = include_bytes!("../../web/style.css");
struct Config {
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
    root: PathBuf,
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
        Ok(Self {
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
            root,
            writable: true,
            started: Instant::now(),
            auth_failures: 0,
            stats: cfg.devices.keys().map(|id| (id.clone(), [0; 3])).collect(),
        })
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
        let result = (|| {
            persist(&dir, "sequence", seq.to_string().as_bytes())?;
            prune(&dir, RETAIN - 1)?;
            persist(&dir, &format!("{seq}.jpg"), body)
        })();
        if let Err(e) = result {
            self.writable = false;
            return Err(e);
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
    if len > MAX_FRAME || (parts[0] == "GET" && len != 0) {
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
    let retry_header = if matches!(code, 429 | 503) {
        "Retry-After: 1\r\n"
    } else {
        ""
    };
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(s, "HTTP/1.1 {code} {}\r\nContent-Length: {}\r\nX-Protocol: 1\r\n{retry_header}Content-Type: {kind}\r\n{sequence_header}Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src blob:; media-src blob:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\nConnection: close\r\n\r\n", if code < 400 { "OK" } else { "Error" }, body.len())?;
    s.write_all(body)
}
fn denied(state: &Mutex<State>) -> io::Result<()> {
    let mut state = state
        .lock()
        .map_err(|_| io::Error::other("state unavailable"))?;
    state.auth_failures = state.auth_failures.saturating_add(1);
    Ok(())
}
fn handle(s: &mut TcpStream, cfg: &Config, state: &Mutex<State>) -> io::Result<()> {
    let r = match parse(s) {
        Ok(r) => r,
        Err(_) => return reply(s, 400, "text/plain", b"Invalid request"),
    };
    let status = |s: &mut TcpStream, code| reply(s, code, "text/plain", b"");
    if r.path.starts_with("/api/") && r.headers.get("x-protocol").is_some_and(|v| v != "1") {
        return status(s, 426);
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
        if !authorized(&r, &cfg.viewer) {
            denied(state)?;
            return status(s, 401);
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
                let last = seqs.last().copied().unwrap_or(0);
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
        let saved = state
            .lock()
            .map_err(|_| io::Error::other("state unavailable"))?
            .save(id, seq, &r.body)?;
        return status(s, if saved { 201 } else { 409 });
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
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    {
        let state = Arc::clone(&state);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            if let Ok(mut s) = state.lock() {
                s.evaluate();
            }
        });
    }

    let (tx, rx) = mpsc::sync_channel::<TcpStream>(8);
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..WORKERS {
        let rx = Arc::clone(&rx);
        let cfg = Arc::clone(&cfg);
        let state = Arc::clone(&state);
        thread::spawn(move || loop {
            let stream = { rx.lock().unwrap().recv() };
            let Ok(mut stream) = stream else { break };
            if handle(&mut stream, &cfg, &state).is_err() {
                let _ = reply(&mut stream, 500, "text/plain", b"Request failed");
            }
        });
    }
    eprintln!(
        "X-Lens on 127.0.0.1:8080; 4 workers; 128 KiB/frame; 32 frames/device; TLS proxy required"
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

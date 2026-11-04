//! Activity, visual watch and Meta vision endpoints.
//! Viewers read; only the operator token can spend AI budget or change settings.
use super::*;

pub type Jobs = mpsc::SyncSender<activity::Job>;

fn lock(state: &Mutex<State>) -> io::Result<std::sync::MutexGuard<'_, State>> {
    state
        .lock()
        .map_err(|_| io::Error::other("state unavailable"))
}
fn status(s: &mut TcpStream, code: u16) -> io::Result<()> {
    reply(s, code, "text/plain", b"")
}
fn text_body(r: &Request) -> bool {
    r.headers.get("x-protocol").map(String::as_str) == Some("1")
        && r.headers.get("content-type").map(String::as_str) == Some("text/plain")
}
/// Hand a job to the vision worker; a full queue leaves the event unanalyzed, never blocks ingest.
pub fn queue(state: &Mutex<State>, jobs: &Jobs, job: activity::Job) {
    let event = job.event;
    if jobs.try_send(job).is_err() {
        if let Ok(mut st) = state.lock() {
            st.activity.cancel(event);
        }
    }
}
/// Describe one motion event and evaluate the device's active watches in a single model call.
pub fn analyze(
    cfg: &Config,
    state: &Mutex<State>,
    job: &activity::Job,
) -> Option<activity::Annotation> {
    let (watches, examples, language) = {
        let st = state.lock().ok()?;
        let watches = st.watches.active(&job.device);
        // Keyframes the operator labeled earlier: the model learns from them per call.
        let examples = st
            .watches
            .examples(&watches, job.event)
            .into_iter()
            .filter_map(|(event, watch, right)| {
                let jpeg = fs::read(st.activity.key_path(event)?).ok()?;
                (jpeg.len() <= MAX_FRAME).then_some((watch, right, jpeg))
            })
            .collect::<Vec<_>>();
        (watches, examples, st.language.clone())
    };
    let mode = vision::mode(&cfg.dir);
    if mode == "disabled" {
        return None;
    }
    let dir = vision::state_dir(&cfg.dir);
    let input = vision::event_input(job, &watches, &examples, &language);
    let out = vision::run(
        &cfg.dir,
        "tools/vision.py",
        &[mode, "event", dir.to_str()?],
        &input,
        8192,
        true,
    )
    .ok()?;
    let ids = watches.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
    activity::Annotation::parse(std::str::from_utf8(&out).ok()?, &ids).ok()
}
pub fn worker(cfg: Arc<Config>, state: Arc<Mutex<State>>, jobs: mpsc::Receiver<activity::Job>) {
    for job in jobs {
        let result = analyze(&cfg, &state, &job);
        if let Ok(mut st) = state.lock() {
            for alert in result.iter().flat_map(|a| &a.alerts) {
                st.watches.fire(&alert.watch);
            }
            st.activity.annotate(job.event, result);
        }
    }
}
fn ask(
    s: &mut TcpStream,
    r: &Request,
    cfg: &Config,
    state: &Mutex<State>,
    live: &Live,
    device: &str,
) -> io::Result<()> {
    let Ok(question) = std::str::from_utf8(&r.body) else {
        return status(s, 400);
    };
    let question = question.trim();
    let offset = utc_offset(r).unwrap_or(0);
    if !cfg.devices.contains_key(device)
        || !text_body(r)
        || !(1..=300).contains(&question.chars().count())
        || question.chars().any(char::is_control)
    {
        return status(s, 400);
    }
    let mode = vision::mode(&cfg.dir);
    if mode == "disabled" {
        return status(s, 503);
    }
    let (frame, age) = match live.latest(device)? {
        Some((_, jpeg, age)) => (format!("\"{}\"", vision::base64(&jpeg)), age.as_secs()),
        None => ("null".into(), 0),
    };
    let (events, watches) = {
        let st = lock(state)?;
        (st.activity.json(40), st.watches.json())
    };
    let input = format!(
        "{{\"device\":{},\"question\":{},\"offset\":{offset},\"now\":{},\"frame\":{frame},\"frame_age\":{age},\"events\":{events},\"watches\":{watches}}}",
        activity::json_str(device),
        activity::json_str(question),
        activity::unix_now()
    );
    let dir = vision::state_dir(&cfg.dir);
    let dir = dir.to_str().ok_or_else(invalid)?;
    match vision::run(
        &cfg.dir,
        "tools/vision.py",
        &[mode, "ask", dir],
        input.as_bytes(),
        8192,
        true,
    ) {
        Ok(out) => reply(s, 200, "application/json", &out),
        Err(_) => reply(
            s,
            502,
            "text/plain",
            b"Answer unavailable: provider busy, over budget, or outcome unknown. No automatic retry.",
        ),
    }
}
fn utc_offset(r: &Request) -> Option<i32> {
    r.headers
        .get("x-utc-offset")
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| (-840..=840).contains(v))
}
/// Run one adapter task and map failures to a plain 502 (never retried).
fn adapter(
    s: &mut TcpStream,
    cfg: &Config,
    script: &str,
    args: &[&str],
    input: &[u8],
) -> io::Result<Option<Vec<u8>>> {
    match vision::run(&cfg.dir, script, args, input, 16384, true) {
        Ok(out) => Ok(Some(out)),
        Err(_) => reply(
            s,
            502,
            "text/plain",
            b"AI unavailable: provider busy, over budget, or outcome unknown. No automatic retry.",
        )
        .map(|()| None),
    }
}
/// Daily summary of the operator's local day: event log, a few keyframes, routine outcomes.
fn digest(s: &mut TcpStream, r: &Request, cfg: &Config, state: &Mutex<State>) -> io::Result<()> {
    let Some(offset) = utc_offset(r) else {
        return status(s, 400);
    };
    let mode = vision::mode(&cfg.dir);
    if mode == "disabled" {
        return status(s, 503);
    }
    let now = activity::unix_now();
    let local_day = (now as i64 + i64::from(offset) * 60).div_euclid(86400);
    let since = (local_day * 86400 - i64::from(offset) * 60).max(0) as u64;
    let input = {
        let st = lock(state)?;
        let mut today = st.activity.since(since);
        let events = today
            .iter()
            .rev()
            .take(120)
            .map(|e| e.json())
            .collect::<Vec<_>>()
            .join(",");
        // Up to four keyframes: events with watch alerts first, then the most motion.
        today.retain(|e| e.end.is_some());
        today.sort_by_key(|e| {
            (
                std::cmp::Reverse(e.ai.as_ref().map_or(0, |a| a.alerts.len())),
                std::cmp::Reverse(e.peak),
            )
        });
        let frames = today
            .iter()
            .filter_map(|e| {
                let jpeg = fs::read(st.activity.key_path(e.id)?).ok()?;
                (jpeg.len() <= MAX_FRAME).then(|| {
                    format!(
                        "{{\"event\":{},\"frame\":\"{}\"}}",
                        e.id,
                        vision::base64(&jpeg)
                    )
                })
            })
            .take(4)
            .collect::<Vec<_>>()
            .join(",");
        let routines = st
            .care
            .routines
            .values()
            .map(|r| r.json(now))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"offset\":{offset},\"now\":{now},\"language\":{},\"events\":[{events}],\"frames\":[{frames}],\"routines\":[{routines}]}}",
            activity::json_str(&st.language)
        )
    };
    let dir = vision::state_dir(&cfg.dir);
    let dir = dir.to_str().ok_or_else(invalid)?;
    let Some(out) = adapter(
        s,
        cfg,
        "tools/vision.py",
        &[mode, "digest", dir],
        input.as_bytes(),
    )?
    else {
        return Ok(());
    };
    lock(state)?.care.set_digest(&out)?;
    reply(s, 200, "application/json", &out)
}
/// Push-to-talk: a short WAV goes to Meta Muse Voice Transcribe; the text comes back.
fn transcribe(
    s: &mut TcpStream,
    r: &Request,
    cfg: &Config,
    state: &Mutex<State>,
) -> io::Result<()> {
    if r.headers.get("content-type").map(String::as_str) != Some("audio/wav") || r.body.len() < 44 {
        return status(s, 400);
    }
    if vision::mode(&cfg.dir) != "meta" {
        // Without a key the browser's own recognizer is used; nothing to simulate here.
        return status(s, 503);
    }
    let language = match r.headers.get("x-language") {
        Some(l) if l.len() == 2 && l.bytes().all(|b| b.is_ascii_lowercase()) => l.clone(),
        Some(_) => return status(s, 400),
        None => lock(state)?.language.clone(),
    };
    let dir = vision::state_dir(&cfg.dir);
    let dir = dir.to_str().ok_or_else(invalid)?;
    match adapter(s, cfg, "tools/voice.py", &[dir, &language], &r.body)? {
        Some(out) => reply(s, 200, "application/json", &out),
        None => Ok(()),
    }
}
/// SAM 3.1 finds a named thing ("tree", "television") in the latest frame and proposes the
/// grid cells it covers as an ignore zone. The operator reviews and saves it separately.
fn suggest_zones(
    s: &mut TcpStream,
    r: &Request,
    cfg: &Config,
    live: &Live,
    device: &str,
) -> io::Result<()> {
    let Ok(concept) = std::str::from_utf8(&r.body) else {
        return status(s, 400);
    };
    let concept = concept.trim();
    if !cfg.devices.contains_key(device)
        || !(2..=60).contains(&concept.chars().count())
        || concept.chars().any(char::is_control)
    {
        return status(s, 400);
    }
    let mode = vision::mode(&cfg.dir);
    if mode == "disabled" {
        return status(s, 503);
    }
    let Some((_, jpeg, _)) = live.latest(device)? else {
        return status(s, 409);
    };
    let input = format!(
        "{{\"concept\":{},\"frame\":\"{}\"}}",
        activity::json_str(concept),
        vision::base64(&jpeg)
    );
    let dir = vision::state_dir(&cfg.dir);
    let dir = dir.to_str().ok_or_else(invalid)?;
    match adapter(s, cfg, "tools/segment.py", &[mode, dir], input.as_bytes())? {
        Some(out) => reply(s, 200, "application/json", &out),
        None => Ok(()),
    }
}
/// Returns None when the request is not one of these routes.
pub fn route(
    s: &mut TcpStream,
    r: &Request,
    cfg: &Config,
    state: &Mutex<State>,
    live: &Live,
    jobs: &Jobs,
) -> Option<io::Result<()>> {
    let path = r.path.as_str();
    let viewer_route = r.method == "GET"
        && (path == "/api/activity"
            || path == "/api/watches"
            || path == "/api/zones"
            || path.starts_with("/api/clip/")
            || path.starts_with("/api/keyframe/"));
    let operator_route = r.method == "POST"
        && (path.starts_with("/api/watches/")
            || path.starts_with("/api/analyze/")
            || path.starts_with("/api/ask/")
            || path.starts_with("/api/zones/")
            || path.starts_with("/api/zones-suggest/")
            || path.starts_with("/api/feedback/")
            || path.starts_with("/api/routines/")
            || path == "/api/digest"
            || path == "/api/transcribe"
            || path == "/api/ai-key"
            || path == "/api/ai-config");
    if !viewer_route && !operator_route {
        return None;
    }
    let key = if viewer_route {
        Some(&cfg.viewer)
    } else {
        cfg.operator.as_ref()
    };
    if !key.is_some_and(|k| authorized(r, k)) {
        return Some(denied(state).and_then(|()| status(s, 401)));
    }
    Some((|| {
        if path == "/api/activity" {
            let st = lock(state)?;
            let devices = cfg
                .devices
                .keys()
                .map(|id| {
                    let (motion, event) = st.activity.status(id);
                    format!(
                        "{{\"id\":{},\"motion\":{motion},\"event\":{event},\"viewers\":{}}}",
                        activity::json_str(id),
                        live.viewers(id)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let body = format!(
                "{{\"events\":{},\"devices\":[{devices}],\"ai\":{},\"feedback\":{},\"zones\":{},{}}}",
                st.activity.json(120),
                st.ai_json(cfg),
                st.watches.feedback_json(),
                st.zones.json(cfg),
                st.care.json(activity::unix_now())
            );
            return reply(s, 200, "application/json", body.as_bytes());
        }
        if path == "/api/watches" {
            let body = lock(state)?.watches.json();
            return reply(s, 200, "application/json", body.as_bytes());
        }
        if path == "/api/zones" {
            let body = lock(state)?.zones.json(cfg);
            return reply(s, 200, "application/json", body.as_bytes());
        }
        let event_id = |prefix: &str| {
            path.strip_prefix(prefix)
                .and_then(|v| v.parse::<u64>().ok())
        };
        if let Some(id) = event_id("/api/clip/") {
            let Some(file) = lock(state)?.activity.clip_path(id) else {
                return status(s, 404);
            };
            return match fs::read(file) {
                Ok(bytes) => reply(s, 200, "application/octet-stream", &bytes),
                Err(_) => status(s, 404),
            };
        }
        if let Some(id) = event_id("/api/keyframe/") {
            let Some(file) = lock(state)?.activity.key_path(id) else {
                return status(s, 404);
            };
            return match fs::read(file) {
                Ok(bytes) if bytes.len() <= MAX_FRAME => reply(s, 200, "image/jpeg", &bytes),
                _ => status(s, 404),
            };
        }
        if let Some(id) = event_id("/api/analyze/") {
            let job = lock(state)?.activity.job_for(id);
            return match job {
                Some(job) => {
                    queue(state, jobs, job);
                    status(s, 202)
                }
                None => status(s, 409),
            };
        }
        if let Some(device) = path.strip_prefix("/api/ask/") {
            return ask(s, r, cfg, state, live, device);
        }
        if path == "/api/transcribe" {
            return if r.headers.get("x-protocol").map(String::as_str) == Some("1") {
                transcribe(s, r, cfg, state)
            } else {
                status(s, 400)
            };
        }
        if !text_body(r) {
            return status(s, 400);
        }
        if let Some(device) = path.strip_prefix("/api/zones-suggest/") {
            return suggest_zones(s, r, cfg, live, device);
        }
        if path == "/api/digest" {
            return digest(s, r, cfg, state);
        }
        if let Some(device) = path.strip_prefix("/api/zones/") {
            let Some(mask) = zones::parse(&r.body).filter(|_| cfg.devices.contains_key(device))
            else {
                return status(s, 400);
            };
            lock(state)?.zones.set(device, mask)?;
            return status(s, 204);
        }
        if let Some(id) = event_id("/api/feedback/") {
            let text = std::str::from_utf8(&r.body).unwrap_or("");
            let mut fields = text.lines().filter_map(|l| l.split_once('='));
            let (Some(("watch", watch)), Some(("verdict", verdict)), None) =
                (fields.next(), fields.next(), fields.next())
            else {
                return status(s, 400);
            };
            let verdict = match verdict {
                "right" => Some(true),
                "wrong" => Some(false),
                "clear" => None,
                _ => return status(s, 400),
            };
            let mut st = lock(state)?;
            if st.activity.key_path(id).is_none() {
                return status(s, 404);
            }
            return status(
                s,
                if st.watches.label(id, watch, verdict)? {
                    204
                } else {
                    404
                },
            );
        }
        if let Some(id) = path.strip_prefix("/api/routines/") {
            let mut st = lock(state)?;
            if r.body == b"delete=1" {
                return status(s, if st.care.remove(id)? { 204 } else { 404 });
            }
            let Ok(routine) = care::Routine::parse(id, &r.body, cfg) else {
                return status(s, 400);
            };
            return status(s, if st.care.put(routine)? { 204 } else { 422 });
        }
        if let Some(id) = path.strip_prefix("/api/watches/") {
            let mut st = lock(state)?;
            if r.body == b"delete=1" {
                return status(s, if st.watches.remove(id)? { 204 } else { 404 });
            }
            let Ok(watch) = watches::Watch::parse(id, &r.body, cfg) else {
                return status(s, 400);
            };
            return status(s, if st.watches.put(watch)? { 204 } else { 422 });
        }
        if path == "/api/ai-key" {
            return match vision::set_key(&cfg.dir, r.body.trim_ascii()) {
                Ok(()) => status(s, 204),
                Err(e) if e.kind() == io::ErrorKind::InvalidData => status(s, 400),
                Err(e) => Err(e),
            };
        }
        // /api/ai-config
        let mut st = lock(state)?;
        if st.set_ai(&r.body).is_err() {
            return status(s, 400);
        }
        let body = format!(
            "language={}\nauto={}\n",
            st.language,
            u8::from(st.activity.auto)
        );
        let root = st.root.clone();
        atomic(&root, "_ai", body.as_bytes())?;
        status(s, 204)
    })())
}

//! Routines: "expect motion on this camera between 07:00 and 10:00". Checked on the host
//! every second, with or without a dashboard open. The first motion in the window marks the
//! routine done for the day; no motion by the end of the window raises a notice. Absence of
//! activity is the signal, so this needs no AI at all.
use super::*;

pub const MAX_ROUTINES: usize = 8;
const MAX_NOTICES: usize = 100;

pub struct Routine {
    pub id: String,
    pub device: String,
    pub label: String,
    /// Window in local minutes after midnight; `from < to` (no window crosses midnight).
    pub from: u16,
    pub to: u16,
    /// Local time = UTC + offset minutes (from the operator's browser when saved).
    pub offset: i16,
    pub enabled: bool,
    /// Local day number and outcome: Some(unix) = first motion, None = missed.
    pub last: Option<(i64, Option<u64>)>,
}
pub struct Notice {
    pub id: u64,
    /// "seen" (routine done) or "missed" (no motion in the window).
    pub kind: &'static str,
    pub routine: String,
    pub device: String,
    pub label: String,
    pub at: u64,
}
pub struct Care {
    dir: PathBuf,
    pub routines: BTreeMap<String, Routine>,
    pub notices: Vec<Notice>,
    next: u64,
    /// Latest daily summary as validated JSON written by tools/vision.py.
    pub digest: Option<String>,
}
fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}
fn valid_label(l: &str) -> bool {
    (3..=80).contains(&l.chars().count())
        && !l.chars().any(char::is_control)
        && !l.trim().is_empty()
}
impl Routine {
    /// Body: key=value lines `device`, `label`, `from`, `to` (minutes 0..1440), `offset`
    /// (-840..840), `enabled` (0/1).
    pub fn parse(id: &str, body: &[u8], cfg: &Config) -> io::Result<Self> {
        if !valid_id(id) || body.len() > 1024 {
            return Err(invalid());
        }
        let text = std::str::from_utf8(body).map_err(|_| invalid())?;
        let mut fields = BTreeMap::new();
        for line in text.lines() {
            let (k, v) = line.split_once('=').ok_or_else(invalid)?;
            if fields.insert(k, v).is_some() {
                return Err(invalid());
            }
        }
        let get = |k| fields.get(k).copied().ok_or_else(invalid);
        let num = |k| -> io::Result<i64> { get(k)?.parse().map_err(|_| invalid()) };
        let (from, to, offset) = (num("from")?, num("to")?, num("offset")?);
        let r = Self {
            id: id.into(),
            device: get("device")?.into(),
            label: get("label")?.trim().into(),
            from: u16::try_from(from).map_err(|_| invalid())?,
            to: u16::try_from(to).map_err(|_| invalid())?,
            offset: i16::try_from(offset).map_err(|_| invalid())?,
            enabled: match get("enabled")? {
                "1" => true,
                "0" => false,
                _ => return Err(invalid()),
            },
            last: None,
        };
        if fields.len() != 6
            || !cfg.devices.contains_key(&r.device)
            || !valid_label(&r.label)
            || r.from >= r.to
            || r.to > 1440
            || !(-840..=840).contains(&r.offset)
        {
            return Err(invalid());
        }
        Ok(r)
    }
    fn local(&self, unix: u64) -> (i64, u16) {
        let local = unix as i64 + i64::from(self.offset) * 60;
        (
            local.div_euclid(86400),
            (local.rem_euclid(86400) / 60) as u16,
        )
    }
    /// UTC unix time of a local minute on a local day.
    fn utc(&self, day: i64, minute: u16) -> u64 {
        (day * 86400 + i64::from(minute) * 60 - i64::from(self.offset) * 60).max(0) as u64
    }
    pub fn json(&self, now: u64) -> String {
        let (today, _) = self.local(now);
        let status = match self.last {
            Some((day, Some(at))) if day == today => format!("\"seen\",\"at\":{at}"),
            Some((day, None)) if day == today => "\"missed\",\"at\":null".into(),
            _ => "\"waiting\",\"at\":null".into(),
        };
        format!(
            "{{\"id\":{},\"device\":{},\"label\":{},\"from\":{},\"to\":{},\"offset\":{},\"enabled\":{},\"status\":{status}}}",
            activity::json_str(&self.id),
            activity::json_str(&self.device),
            activity::json_str(&self.label),
            self.from,
            self.to,
            self.offset,
            self.enabled
        )
    }
}
impl Notice {
    pub fn json(&self) -> String {
        format!(
            "{{\"id\":{},\"kind\":\"{}\",\"routine\":{},\"device\":{},\"label\":{},\"at\":{}}}",
            self.id,
            self.kind,
            activity::json_str(&self.routine),
            activity::json_str(&self.device),
            activity::json_str(&self.label),
            self.at
        )
    }
}
impl Care {
    pub fn open(root: &Path, cfg: &Config) -> io::Result<Self> {
        let mut care = Self {
            dir: root.to_path_buf(),
            routines: BTreeMap::new(),
            notices: Vec::new(),
            next: 1,
            digest: fs::read_to_string(root.join("_digest"))
                .ok()
                .filter(|d| d.len() <= 16384 && d.starts_with('{') && d.is_ascii()),
        };
        if let Ok(text) = fs::read_to_string(root.join("_routines")) {
            for line in text.lines() {
                let f = line.split('\t').collect::<Vec<_>>();
                let [id, device, from, to, offset, enabled, label, day, seen] = f[..] else {
                    return Err(invalid());
                };
                let label = unhex(label).ok_or_else(invalid)?;
                let body = format!("device={device}\nlabel={label}\nfrom={from}\nto={to}\noffset={offset}\nenabled={enabled}");
                // Routines for devices removed from the config are dropped, not fatal.
                if let Ok(mut r) = Routine::parse(id, body.as_bytes(), cfg) {
                    r.last = day
                        .parse::<i64>()
                        .ok()
                        .filter(|d| *d >= 0)
                        .map(|d| (d, seen.parse().ok().filter(|s| *s > 0)));
                    care.routines.insert(r.id.clone(), r);
                }
            }
        }
        if let Ok(text) = fs::read_to_string(root.join("_notices")) {
            for line in text.lines() {
                let f = line.split('\t').collect::<Vec<_>>();
                let [id, kind, routine, device, label, at] = f[..] else {
                    continue;
                };
                let (Ok(id), Ok(at), Some(label)) = (id.parse(), at.parse(), unhex(label)) else {
                    continue;
                };
                let kind = if kind == "seen" { "seen" } else { "missed" };
                care.notices.push(Notice {
                    id,
                    kind,
                    routine: routine.into(),
                    device: device.into(),
                    label,
                    at,
                });
            }
            care.next = care.notices.iter().map(|n| n.id + 1).max().unwrap_or(1);
        }
        Ok(care)
    }
    fn persist(&self) -> io::Result<()> {
        let routines = self
            .routines
            .values()
            .map(|r| {
                let (day, seen) = r.last.map_or((-1, 0), |(d, s)| (d, s.unwrap_or(0)));
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{day}\t{seen}\n",
                    r.id,
                    r.device,
                    r.from,
                    r.to,
                    r.offset,
                    u8::from(r.enabled),
                    hex(&r.label)
                )
            })
            .collect::<String>();
        atomic(&self.dir, "_routines", routines.as_bytes())?;
        let notices = self
            .notices
            .iter()
            .map(|n| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\n",
                    n.id,
                    n.kind,
                    n.routine,
                    n.device,
                    hex(&n.label),
                    n.at
                )
            })
            .collect::<String>();
        atomic(&self.dir, "_notices", notices.as_bytes())
    }
    pub fn put(&mut self, r: Routine) -> io::Result<bool> {
        if !self.routines.contains_key(&r.id) && self.routines.len() >= MAX_ROUTINES {
            return Ok(false);
        }
        self.routines.insert(r.id.clone(), r);
        self.persist()?;
        Ok(true)
    }
    pub fn remove(&mut self, id: &str) -> io::Result<bool> {
        if self.routines.remove(id).is_none() {
            return Ok(false);
        }
        self.persist()?;
        Ok(true)
    }
    /// Decide today's outcome for each routine. `first_motion(device, from, to)` returns the
    /// earliest motion event start in that UTC range.
    pub fn check<F>(&mut self, now: u64, first_motion: F) -> io::Result<()>
    where
        F: Fn(&str, u64, u64) -> Option<u64>,
    {
        let mut changed = false;
        for r in self.routines.values_mut().filter(|r| r.enabled) {
            let (day, minute) = r.local(now);
            if r.last.is_some_and(|(d, _)| d == day) || minute < r.from {
                continue;
            }
            let start = r.utc(day, r.from);
            let end = r.utc(day, r.to).min(now);
            let kind = match first_motion(&r.device, start, end) {
                Some(at) => {
                    r.last = Some((day, Some(at)));
                    "seen"
                }
                None if minute >= r.to => {
                    r.last = Some((day, None));
                    "missed"
                }
                None => continue,
            };
            self.notices.push(Notice {
                id: self.next,
                kind,
                routine: r.id.clone(),
                device: r.device.clone(),
                label: r.label.clone(),
                at: now,
            });
            self.next += 1;
            changed = true;
        }
        if self.notices.len() > MAX_NOTICES {
            self.notices.drain(..self.notices.len() - MAX_NOTICES);
        }
        if changed {
            self.persist()?;
        }
        Ok(())
    }
    pub fn set_digest(&mut self, json: &[u8]) -> io::Result<()> {
        let text = std::str::from_utf8(json).map_err(|_| invalid())?.trim();
        if text.len() > 16384 || !text.starts_with('{') || !text.ends_with('}') || !text.is_ascii()
        {
            return Err(invalid());
        }
        atomic(&self.dir, "_digest", text.as_bytes())?;
        self.digest = Some(text.into());
        Ok(())
    }
    pub fn json(&self, now: u64) -> String {
        let routines = self
            .routines
            .values()
            .map(|r| r.json(now))
            .collect::<Vec<_>>()
            .join(",");
        let notices = self
            .notices
            .iter()
            .rev()
            .take(30)
            .map(Notice::json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "\"routines\":[{routines}],\"notices\":[{notices}],\"digest\":{}",
            self.digest.as_deref().unwrap_or("null")
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> Config {
        Config {
            dir: env::temp_dir(),
            operator: None,
            viewer: "v".repeat(64),
            devices: [("cam".to_string(), "d".repeat(64))].into(),
        }
    }
    #[test]
    fn routine_is_seen_once_or_missed_at_window_end() {
        let root = env::temp_dir().join(format!("xlens-care-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        private_directory(&root).unwrap();
        let cfg = cfg();
        let mut care = Care::open(&root, &cfg).unwrap();
        // 07:00-10:00 at UTC-3.
        let body = b"device=cam\nlabel=Mom gets up\nfrom=420\nto=600\noffset=-180\nenabled=1";
        assert!(care
            .put(Routine::parse("wake", body, &cfg).unwrap())
            .unwrap());
        for bad in [
            &b"device=cam\nlabel=x\nfrom=420\nto=600\noffset=-180\nenabled=1"[..],
            b"device=cam\nlabel=Late\nfrom=600\nto=420\noffset=-180\nenabled=1",
            b"device=other\nlabel=Nope\nfrom=1\nto=2\noffset=0\nenabled=1",
            b"device=cam\nlabel=Nope\nfrom=1\nto=2\noffset=0\nenabled=1\nextra=1",
        ] {
            assert!(Routine::parse("bad", bad, &cfg).is_err());
        }
        let day = 20_000i64;
        let local = |h: i64, m: i64| (day * 86400 + h * 3600 + m * 60 + 180 * 60) as u64;
        let none = |_: &str, _: u64, _: u64| None;
        care.check(local(6, 59), none).unwrap(); // Before the window: undecided.
        care.check(local(8, 0), none).unwrap(); // In the window, nothing yet: still waiting.
        assert!(care.notices.is_empty());
        let woke = local(8, 12);
        care.check(local(8, 13), |d, from, to| {
            assert_eq!((d, from), ("cam", local(7, 0)));
            (woke <= to).then_some(woke)
        })
        .unwrap();
        care.check(local(11, 0), none).unwrap(); // Decided: no second notice.
        assert_eq!(care.notices.len(), 1);
        assert_eq!(
            (care.notices[0].kind, care.routines["wake"].last),
            ("seen", Some((day, Some(woke))))
        );
        assert!(care.routines["wake"]
            .json(local(12, 0))
            .contains("\"status\":\"seen\""));
        // Next day nobody moves: one "missed" notice after 10:00, persisted across restarts.
        let next = |h: i64| local(h, 0) + 86400;
        care.check(next(9), none).unwrap();
        assert_eq!(care.notices.len(), 1);
        care.check(next(10), none).unwrap();
        assert_eq!(care.notices.last().unwrap().kind, "missed");
        let reopened = Care::open(&root, &cfg).unwrap();
        assert_eq!(reopened.notices.len(), 2);
        assert_eq!(reopened.routines["wake"].last, Some((day + 1, None)));
        fs::remove_dir_all(root).unwrap();
    }
}

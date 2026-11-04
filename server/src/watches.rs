//! Natural-language visual watches. Evaluated by the vision model only on motion events;
//! a match creates a dashboard alert, never an action. Operators label alerts as right or
//! wrong; those keyframes go back to the model as reference examples (few-shot, no training).
use super::*;

pub const MAX_WATCHES: usize = 8;
const MAX_FEEDBACK: usize = 64;
/// Reference keyframes sent with one event analysis, across all watches.
pub const MAX_EXAMPLES: usize = 3;
pub struct Watch {
    pub id: String,
    pub device: String,
    pub prompt: String,
    pub cooldown: u64,
    pub enabled: bool,
}
pub struct Watches {
    path: PathBuf,
    pub list: BTreeMap<String, Watch>,
    fired: BTreeMap<String, Instant>,
    /// (event, watch) -> true if the operator confirmed a match, false if it was a false alarm.
    feedback: BTreeMap<(u64, String), bool>,
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
fn valid_prompt(p: &str) -> bool {
    let n = p.chars().count();
    (3..=200).contains(&n) && !p.chars().any(char::is_control) && !p.trim().is_empty()
}
impl Watch {
    /// Body: key=value lines `device`, `prompt`, `cooldown` (30..86400), `enabled` (0/1).
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
        let watch = Self {
            id: id.into(),
            device: get("device")?.into(),
            prompt: get("prompt")?.trim().into(),
            cooldown: get("cooldown")?.parse().map_err(|_| invalid())?,
            enabled: match get("enabled")? {
                "1" => true,
                "0" => false,
                _ => return Err(invalid()),
            },
        };
        if fields.len() != 4
            || !cfg.devices.contains_key(&watch.device)
            || !valid_prompt(&watch.prompt)
            || !(30..=86400).contains(&watch.cooldown)
        {
            return Err(invalid());
        }
        Ok(watch)
    }
    pub fn json(&self, learned: usize) -> String {
        format!(
            "{{\"id\":{},\"device\":{},\"prompt\":{},\"cooldown\":{},\"enabled\":{},\"learned\":{learned}}}",
            activity::json_str(&self.id),
            activity::json_str(&self.device),
            activity::json_str(&self.prompt),
            self.cooldown,
            self.enabled
        )
    }
}
impl Watches {
    pub fn open(root: &Path, cfg: &Config) -> io::Result<Self> {
        let path = root.join("_watches");
        let mut list = BTreeMap::new();
        match fs::read_to_string(&path) {
            Ok(text) => {
                for line in text.lines() {
                    let f = line.split('\t').collect::<Vec<_>>();
                    let [id, device, cooldown, enabled, prompt] = f[..] else {
                        return Err(invalid());
                    };
                    let prompt = unhex(prompt).ok_or_else(invalid)?;
                    let body = format!(
                        "device={device}\nprompt={prompt}\ncooldown={cooldown}\nenabled={enabled}"
                    );
                    // Watches for devices removed from the config are dropped, not fatal.
                    if let Ok(w) = Watch::parse(id, body.as_bytes(), cfg) {
                        list.insert(w.id.clone(), w);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        let mut feedback = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(root.join("_feedback")) {
            for line in text.lines() {
                if let [event, watch, verdict] = line.split('\t').collect::<Vec<_>>()[..] {
                    if let (Ok(event), true) = (event.parse(), list.contains_key(watch)) {
                        feedback.insert((event, watch.to_string()), verdict == "1");
                    }
                }
            }
        }
        Ok(Self {
            path,
            list,
            fired: BTreeMap::new(),
            feedback,
        })
    }
    fn persist(&self) -> io::Result<()> {
        let text = self
            .list
            .values()
            .map(|w| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    w.id,
                    w.device,
                    w.cooldown,
                    u8::from(w.enabled),
                    hex(&w.prompt)
                )
            })
            .collect::<String>();
        let dir = self.path.parent().ok_or_else(invalid)?;
        atomic(dir, "_watches", text.as_bytes())?;
        let labels = self
            .feedback
            .iter()
            .map(|((e, w), v)| format!("{e}\t{w}\t{}\n", u8::from(*v)))
            .collect::<String>();
        atomic(dir, "_feedback", labels.as_bytes())
    }
    pub fn put(&mut self, w: Watch) -> io::Result<bool> {
        if !self.list.contains_key(&w.id) && self.list.len() >= MAX_WATCHES {
            return Ok(false);
        }
        self.fired.remove(&w.id);
        self.list.insert(w.id.clone(), w);
        self.persist()?;
        Ok(true)
    }
    pub fn remove(&mut self, id: &str) -> io::Result<bool> {
        if self.list.remove(id).is_none() {
            return Ok(false);
        }
        self.fired.remove(id);
        self.feedback.retain(|(_, w), _| w != id);
        self.persist()?;
        Ok(true)
    }
    /// Record (or with `None`, clear) the operator's verdict on one event for one watch.
    pub fn label(&mut self, event: u64, watch: &str, verdict: Option<bool>) -> io::Result<bool> {
        if !self.list.contains_key(watch) {
            return Ok(false);
        }
        let key = (event, watch.to_string());
        match verdict {
            Some(v) => {
                self.feedback.insert(key, v);
            }
            None => {
                self.feedback.remove(&key);
            }
        }
        while self.feedback.len() > MAX_FEEDBACK {
            let oldest = self.feedback.keys().next().cloned();
            if let Some(k) = oldest {
                self.feedback.remove(&k);
            }
        }
        self.persist()?;
        Ok(true)
    }
    /// Newest labeled events for these watches (at most two per watch), never `exclude`.
    pub fn examples(&self, watches: &[(String, String)], exclude: u64) -> Vec<(u64, String, bool)> {
        let mut out: Vec<(u64, String, bool)> = Vec::new();
        for ((event, watch), right) in self.feedback.iter().rev() {
            if out.len() >= MAX_EXAMPLES {
                break;
            }
            if *event != exclude
                && watches.iter().any(|(id, _)| id == watch)
                && out.iter().filter(|(_, w, _)| w == watch).count() < 2
            {
                out.push((*event, watch.clone(), *right));
            }
        }
        out
    }
    pub fn feedback_json(&self) -> String {
        let rows = self
            .feedback
            .iter()
            .map(|((e, w), v)| {
                format!(
                    "{{\"event\":{e},\"watch\":{},\"right\":{v}}}",
                    activity::json_str(w)
                )
            })
            .collect::<Vec<_>>();
        format!("[{}]", rows.join(","))
    }
    /// Enabled watches for a device that are not cooling down after a recent alert.
    pub fn active(&self, device: &str) -> Vec<(String, String)> {
        self.list
            .values()
            .filter(|w| w.enabled && w.device == device)
            .filter(|w| {
                self.fired
                    .get(&w.id)
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(w.cooldown))
            })
            .map(|w| (w.id.clone(), w.prompt.clone()))
            .collect()
    }
    pub fn fire(&mut self, id: &str) {
        self.fired.insert(id.into(), Instant::now());
    }
    pub fn json(&self) -> String {
        format!(
            "[{}]",
            self.list
                .values()
                .map(|w| w.json(self.feedback.keys().filter(|(_, id)| *id == w.id).count()))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

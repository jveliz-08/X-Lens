//! Fixed local rule language. No code, network actions, or model authority.
use super::*;
#[derive(Clone)]
pub struct Rule {
    pub id: String,
    pub device: String,
    pub signal: String,
    pub threshold: u64,
    pub cooldown: u64,
    pub enabled: bool,
}
impl Rule {
    pub fn parse(id: &str, body: &[u8], cfg: &Config) -> io::Result<Self> {
        if !valid_id(id) || body.len() > 512 {
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
        if fields.len() != 5 {
            return Err(invalid());
        }
        let get = |k| fields.get(k).copied().ok_or_else(invalid);
        let device = get("device")?;
        let signal = get("signal")?;
        let threshold = get("threshold")?.parse::<u64>().map_err(|_| invalid())?;
        let cooldown = get("cooldown")?.parse::<u64>().map_err(|_| invalid())?;
        let enabled = match get("enabled")? {
            "1" => true,
            "0" => false,
            _ => return Err(invalid()),
        };
        if !cfg.devices.contains_key(device)
            || !matches!(
                signal,
                "replays" | "invalid_frames" | "auth_failures" | "stale_seconds"
            )
            || !(1..=3600).contains(&threshold)
            || !(10..=86400).contains(&cooldown)
        {
            return Err(invalid());
        }
        Ok(Self {
            id: id.into(),
            device: device.into(),
            signal: signal.into(),
            threshold,
            cooldown,
            enabled,
        })
    }
    pub fn wire(&self) -> String {
        format!(
            "device={}\nsignal={}\nthreshold={}\ncooldown={}\nenabled={}\n",
            self.device,
            self.signal,
            self.threshold,
            self.cooldown,
            u8::from(self.enabled)
        )
    }
    pub fn json(&self) -> String {
        format!("{{\"id\":\"{}\",\"device\":\"{}\",\"signal\":\"{}\",\"threshold\":{},\"cooldown\":{},\"enabled\":{},\"action\":\"ui_event\"}}",self.id,self.device,self.signal,self.threshold,self.cooldown,self.enabled)
    }
}
pub struct Engine {
    pub rules: BTreeMap<String, Rule>,
    fired: BTreeMap<String, Instant>,
    pub events: std::collections::VecDeque<String>,
    serial: u64,
}
impl Engine {
    pub fn open(root: &Path, cfg: &Config) -> io::Result<Self> {
        let mut rules = BTreeMap::new();
        let dir = root.join("rules");
        private_directory(&dir)?;
        for e in fs::read_dir(dir)? {
            let e = e?;
            let name = e.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".rule") {
                if rules.len() >= 16 || e.metadata()?.len() > 512 {
                    return Err(invalid());
                }
                let r = Rule::parse(id, &fs::read(e.path())?, cfg)?;
                rules.insert(id.into(), r);
            }
        }
        Ok(Self {
            rules,
            fired: BTreeMap::new(),
            events: std::collections::VecDeque::new(),
            serial: 0,
        })
    }
    pub fn put(&mut self, root: &Path, rule: Rule) -> io::Result<()> {
        if !self.rules.contains_key(&rule.id) && self.rules.len() >= 16 {
            return Err(invalid());
        }
        atomic(
            &root.join("rules"),
            &format!("{}.rule", rule.id),
            rule.wire().as_bytes(),
        )?;
        self.fired.remove(&rule.id);
        self.rules.insert(rule.id.clone(), rule);
        Ok(())
    }
    pub fn evaluate(
        &mut self,
        stats: &BTreeMap<String, [u64; 3]>,
        auth: u64,
        last: &BTreeMap<String, Instant>,
    ) {
        for rule in self.rules.values().filter(|r| r.enabled) {
            let value = match rule.signal.as_str() {
                "replays" => stats[&rule.device][1],
                "invalid_frames" => stats[&rule.device][2],
                "auth_failures" => auth,
                _ => last[&rule.device].elapsed().as_secs(),
            };
            if value < rule.threshold
                || self
                    .fired
                    .get(&rule.id)
                    .is_some_and(|t| t.elapsed().as_secs() < rule.cooldown)
            {
                continue;
            }
            self.serial = self.serial.saturating_add(1);
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.events.push_front(format!("{{\"id\":{},\"rule\":\"{}\",\"device\":\"{}\",\"signal\":\"{}\",\"value\":{},\"at\":{},\"action\":\"ui_event\"}}",self.serial,rule.id,rule.device,rule.signal,value,at));
            self.events.truncate(64);
            self.fired.insert(rule.id.clone(), Instant::now());
        }
    }
}

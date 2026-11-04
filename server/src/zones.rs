//! Motion-ignore zones: an 8x6 grid per camera (bit = row * 8 + col, set = ignore).
//! The mask rides on every upload reply (`X-Motion-Mask`), so the camera itself skips
//! those cells when it scores motion: a swaying tree or a TV no longer starts events.
use super::*;

pub const COLS: u32 = 8;
pub const ROWS: u32 = 6;
const CELLS: u32 = COLS * ROWS;
/// At least a quarter of the scene must stay watched.
const MAX_IGNORED: u32 = CELLS * 3 / 4;

pub struct Zones {
    path: PathBuf,
    masks: BTreeMap<String, u64>,
}
/// Body `mask=<12 hex digits>`.
pub fn parse(body: &[u8]) -> Option<u64> {
    let hex = std::str::from_utf8(body)
        .ok()?
        .trim()
        .strip_prefix("mask=")?;
    let mask = (hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u64::from_str_radix(hex, 16).ok())??;
    (mask >> CELLS == 0 && mask.count_ones() <= MAX_IGNORED).then_some(mask)
}
impl Zones {
    pub fn open(root: &Path, cfg: &Config) -> io::Result<Self> {
        let path = root.join("_zones");
        let mut masks = BTreeMap::new();
        match fs::read_to_string(&path) {
            Ok(text) => {
                for line in text.lines() {
                    let (device, hex) = line.split_once('\t').ok_or_else(invalid)?;
                    let mask = parse(format!("mask={hex}").as_bytes()).ok_or_else(invalid)?;
                    if cfg.devices.contains_key(device) {
                        masks.insert(device.to_string(), mask);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        Ok(Self { path, masks })
    }
    pub fn get(&self, device: &str) -> u64 {
        self.masks.get(device).copied().unwrap_or(0)
    }
    pub fn set(&mut self, device: &str, mask: u64) -> io::Result<()> {
        if mask == 0 {
            self.masks.remove(device);
        } else {
            self.masks.insert(device.into(), mask);
        }
        let text = self
            .masks
            .iter()
            .map(|(d, m)| format!("{d}\t{m:012x}\n"))
            .collect::<String>();
        let dir = self.path.parent().ok_or_else(invalid)?;
        atomic(dir, "_zones", text.as_bytes())
    }
    pub fn header(&self, device: &str) -> String {
        format!("X-Motion-Mask: {:012x}\r\n", self.get(device))
    }
    pub fn json(&self, cfg: &Config) -> String {
        let rows = cfg
            .devices
            .keys()
            .map(|d| format!("{}:\"{:012x}\"", activity::json_str(d), self.get(d)))
            .collect::<Vec<_>>();
        format!("{{{}}}", rows.join(","))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_are_bounded_hex() {
        assert_eq!(parse(b"mask=000000000003"), Some(3));
        assert_eq!(parse(b"mask=00000000ff00\n"), Some(0xff00));
        for bad in [
            &b"mask=3"[..],
            b"mask=0000000000zz",
            b"mask=ffffffffffff", // Everything ignored.
            b"mask=1000000000000",
            b"zones=000000000003",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }
}

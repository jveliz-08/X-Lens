//! Fixed Python adapters (no shell, no model-selected executable or arguments).
//! The Meta key lives in a mode-600 file on this host: an operator can set or remove it
//! from the dashboard, but no endpoint ever returns it.
use super::*;
use std::process::{Command, Stdio};
static GATE: Mutex<()> = Mutex::new(());
pub const DEFAULT_MODEL: &str = "muse-spark-1.3";
pub const DAILY_BUDGET: u64 = 300;

pub fn key_file(dir: &Path) -> PathBuf {
    env::var_os("XLENS_META_KEY_FILE").map_or_else(|| dir.join("meta.secret"), PathBuf::from)
}
pub fn model() -> String {
    env::var("XLENS_META_MODEL")
        .ok()
        .filter(|m| {
            (1..=80).contains(&m.len())
                && m.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        })
        .unwrap_or_else(|| DEFAULT_MODEL.into())
}
pub fn state_dir(dir: &Path) -> PathBuf {
    env::var_os("XLENS_AI_STATE_DIR").map_or_else(|| dir.to_path_buf(), PathBuf::from)
}
/// `XLENS_RULE_MODE` overrides; otherwise a configured key means live Meta, else simulation.
pub fn mode(dir: &Path) -> &'static str {
    match env::var("XLENS_RULE_MODE").as_deref() {
        Ok("meta") => "meta",
        Ok("mock") => "mock",
        Ok("disabled") => "disabled",
        _ if key_file(dir).is_file() => "meta",
        _ => "mock",
    }
}
pub fn set_key(dir: &Path, key: &[u8]) -> io::Result<()> {
    let path = key_file(dir);
    if key.is_empty() {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if !(8..=4096).contains(&key.len()) || !key.iter().all(|b| (33..=126).contains(b)) {
        return Err(invalid());
    }
    let parent = path.parent().ok_or_else(invalid)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(invalid)?;
    atomic(parent, name, key)
}
/// Calls used today from the adapter's private ledger (written by tools/vision.py).
pub fn used_today(dir: &Path) -> u64 {
    let Ok(text) = fs::read_to_string(state_dir(dir).join("vision-budget.json")) else {
        return 0;
    };
    let field = |k: &str| {
        let rest = text.split(&format!("\"{k}\":")).nth(1)?;
        rest.split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse::<u64>()
            .ok()
    };
    match (field("day"), field("count")) {
        (Some(day), Some(count)) if day == activity::unix_now() / 86400 => count,
        _ => 0,
    }
}
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
/// Run a fixed adapter script. `wait` serializes behind other calls; otherwise busy fails fast.
pub fn run(
    dir: &Path,
    script: &str,
    args: &[&str],
    input: &[u8],
    limit: usize,
    wait: bool,
) -> io::Result<Vec<u8>> {
    let _gate = if wait {
        GATE.lock().map_err(|_| invalid())?
    } else {
        GATE.try_lock().map_err(|_| invalid())?
    };
    let mut child = Command::new("python3")
        .arg(script)
        .args(args)
        .env("XLENS_META_KEY_FILE", key_file(dir))
        .env("XLENS_META_MODEL", model())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or_else(invalid)?;
    let input = input.to_vec();
    // Feed stdin from a thread so a large input can never deadlock against stdout.
    let feeder = thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child.stdout.take().ok_or_else(invalid)?;
    let reader = thread::spawn(move || {
        let mut out = Vec::new();
        (&mut stdout)
            .take(limit as u64 + 1)
            .read_to_end(&mut out)
            .map(|_| out)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if start.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(25))
            }
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(invalid());
            }
        }
    };
    let fed = feeder.join().map_err(|_| invalid())?;
    let out = reader.join().map_err(|_| invalid())??;
    if fed.is_err() || !status.success() || out.is_empty() || out.len() > limit {
        return Err(invalid());
    }
    Ok(out)
}
/// Vision adapter input for one motion event. `examples` are operator-labeled keyframes
/// (watch, confirmed match, JPEG) the model uses as references.
pub fn event_input(
    job: &activity::Job,
    watches: &[(String, String)],
    examples: &[(String, bool, Vec<u8>)],
    language: &str,
) -> Vec<u8> {
    let frames = job
        .frames
        .iter()
        .map(|f| format!("\"{}\"", base64(f)))
        .collect::<Vec<_>>()
        .join(",");
    let watches = watches
        .iter()
        .map(|(id, prompt)| {
            format!(
                "{{\"id\":{},\"prompt\":{}}}",
                activity::json_str(id),
                activity::json_str(prompt)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let examples = examples
        .iter()
        .map(|(watch, right, jpeg)| {
            format!(
                "{{\"watch\":{},\"match\":{right},\"frame\":\"{}\"}}",
                activity::json_str(watch),
                base64(jpeg)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"device\":{},\"seconds\":{},\"peak\":{},\"language\":{},\"frames\":[{frames}],\"watches\":[{watches}],\"examples\":[{examples}]}}",
        activity::json_str(&job.device),
        job.seconds,
        job.peak,
        activity::json_str(language)
    )
    .into_bytes()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_matches_rfc4648() {
        for (raw, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(raw.as_bytes()), enc);
        }
    }
}

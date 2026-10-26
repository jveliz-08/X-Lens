//! Optional fixed Python adapter: no shell, no model-selected executable or arguments.
use super::*;
use std::process::{Command, Stdio};
static GATE: Mutex<()> = Mutex::new(());
pub fn mode() -> String {
    match env::var("XLENS_RULE_MODE").as_deref() {
        Ok("meta") => "meta",
        Ok("mock") => "mock",
        _ => "disabled",
    }
    .into()
}
pub fn call(id: &str, body: &[u8]) -> io::Result<Vec<u8>> {
    let _gate = GATE.try_lock().map_err(|_| invalid())?;
    let mode = mode();
    if mode == "disabled" || body.len() > 960 || std::str::from_utf8(body).is_err() {
        return Err(invalid());
    }
    let mut child = Command::new("python3")
        .arg("tools/rule_draft.py")
        .arg(mode)
        .arg(id)
        .arg(env::var("XLENS_AI_STATE_DIR").map_err(|_| invalid())?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    if child
        .stdin
        .take()
        .ok_or_else(invalid)?
        .write_all(body)
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(invalid());
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(invalid());
                }
                let mut out = Vec::new();
                child
                    .stdout
                    .take()
                    .ok_or_else(invalid)?
                    .take(4097)
                    .read_to_end(&mut out)?;
                if out.is_empty() || out.len() > 4096 {
                    return Err(invalid());
                }
                return Ok(out);
            }
            Ok(None) if start.elapsed() < Duration::from_secs(25) => {
                thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(invalid());
            }
        }
    }
}

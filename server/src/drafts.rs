//! Optional fixed Python adapter: no shell, no model-selected executable or arguments.
use super::*;
pub fn call(dir: &Path, id: &str, body: &[u8]) -> io::Result<Vec<u8>> {
    let mode = vision::mode(dir);
    if mode == "disabled" || body.len() > 960 || std::str::from_utf8(body).is_err() {
        return Err(invalid());
    }
    let state = vision::state_dir(dir);
    let state = state.to_str().ok_or_else(invalid)?;
    vision::run(
        dir,
        "tools/rule_draft.py",
        &[mode, id, state],
        body,
        4096,
        false,
    )
}

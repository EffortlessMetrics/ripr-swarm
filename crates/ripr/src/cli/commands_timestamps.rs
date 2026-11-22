use std::time::{SystemTime, UNIX_EPOCH};

/// The `unix_ms:<millis>` stamp a report command writes: the
/// `RIPR_GENERATED_AT` pin when set, else the wall clock (#14230).
pub(crate) fn generated_at_unix_ms() -> Result<String, String> {
    if let Some(pinned) = crate::agent::artifact::pinned_generated_at_unix_ms()? {
        return Ok(format!("unix_ms:{pinned}"));
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock before unix epoch: {err}; set the system clock to the current time, then rerun"))?
        .as_millis();
    Ok(format!("unix_ms:{millis}"))
}

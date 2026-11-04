//! Shared syntax of durable repair-attempt identifiers.

pub(crate) const REPAIR_ATTEMPT_ID_PREFIX: &str = "repair-attempt-";
pub(crate) const REPAIR_ATTEMPT_ID_HEX_LEN: usize = 24;

pub(crate) fn validate_repair_attempt_id(value: &str) -> Result<(), String> {
    let Some(suffix) = value.strip_prefix(REPAIR_ATTEMPT_ID_PREFIX) else {
        return Err(format!(
            "repair attempt ID must start with `{REPAIR_ATTEMPT_ID_PREFIX}`; copy the ID that `ripr agent repair` printed or `ripr agent status` lists"
        ));
    };
    if suffix.len() != REPAIR_ATTEMPT_ID_HEX_LEN
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(format!(
            "repair attempt ID suffix must contain {REPAIR_ATTEMPT_ID_HEX_LEN} lowercase hexadecimal characters; copy the ID that `ripr agent repair` printed or `ripr agent status` lists"
        ));
    }
    Ok(())
}

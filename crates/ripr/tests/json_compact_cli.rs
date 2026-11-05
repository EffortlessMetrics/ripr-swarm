//! Public-process regression for #13103. Compact presentation must preserve
//! refusal documents as well as successful reports. Do not ignore this test:
//! a format with empty refusal stdout is not ready for machine consumers.

use std::process::{Command, Output};

fn refused_check(root: &std::path::Path, format: &str) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--root"])
        .arg(root)
        .args(["--format", format])
        .env_remove("RIPR_CHECK_FINDINGS_BYTES")
        .env_remove("RIPR_CHECK_TIMEOUT")
        .env_remove("RIPR_GIT_TIMEOUT")
        .output()
        .map_err(|error| format!("run check --format {format}: {error}"))
}

#[test]
fn json_compact_cli_preserves_structured_refusal_exit_and_recovery() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-json-compact-missing-{}-{stamp}",
        std::process::id()
    ));
    if root.exists() {
        return Err("refusal fixture must name a nonexistent directory".to_string());
    }
    let pretty = refused_check(&root, "json")?;
    let compact = refused_check(&root, "json-compact")?;
    if pretty.status.success() || pretty.status.code() != compact.status.code() {
        return Err(format!(
            "the two formats must retain the same refusal exit: {:?} vs {:?}",
            pretty.status.code(),
            compact.status.code()
        ));
    }
    let expected: serde_json::Value = serde_json::from_slice(&pretty.stdout)
        .map_err(|error| format!("legacy JSON refusal must parse: {error}"))?;
    let actual: serde_json::Value = serde_json::from_slice(&compact.stdout)
        .map_err(|error| format!("compact JSON refusal must not be empty or malformed: {error}"))?;
    if expected != actual {
        return Err("compact refusal changed its identity, scope or recovery route".to_string());
    }
    if pretty.stderr != compact.stderr {
        return Err("compact presentation changed refusal stderr".to_string());
    }
    let text = std::str::from_utf8(&compact.stdout).map_err(|error| error.to_string())?;
    if !text.ends_with('\n') || text.lines().count() != 1 {
        return Err("compact refusal must be one JSON line with a final newline".to_string());
    }
    if compact.stdout.len() >= pretty.stdout.len() {
        return Err("pretty refusal must become smaller without losing its document".to_string());
    }
    Ok(())
}

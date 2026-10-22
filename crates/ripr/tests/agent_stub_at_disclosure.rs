//! #8444 item 3: the CLI adapter's nearest-gap note is the first disclosure
//! warning, including a `predicate` family (the #8364 test is `return_value`
//! through `resolve_test_stub` only) and an off-line `--at` that then refuses.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create(label: &str, source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-agent-stub-at-disclosure-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{label}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stub(root: &Path, at: &str, kind: &str, json: bool) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at, "--kind", kind]);
    if json {
        command.arg("--json");
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr agent stub: {error}"))
}

fn split_refusal_envelope<'a>(
    stdout: &str,
    stderr: &'a str,
) -> Result<(serde_json::Value, &'a str), String> {
    if !stderr.starts_with("ripr: agent stub: ") {
        return Err(format!("the refusal prose must be on stderr: {stderr}"));
    }
    let envelope: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|error| format!("the refusal envelope did not parse: {error}: {stdout}"))?;
    Ok((envelope, stderr))
}

fn note_names_substitution(note: &str, requested: &str, stubbed: &str) -> Result<(), String> {
    if note.contains(requested) && note.contains(stubbed) && note.contains("nearest") {
        Ok(())
    } else {
        Err(format!(
            "substitution note must name {requested} -> {stubbed} and nearest: {note}"
        ))
    }
}

/// Same gripped-predicate fixture as the use-case test in `app/test_stub.rs`.
/// Line 2 is the comparison; line 3 is the closing brace.
const GRIPPED_PREDICATE: &str = "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn at_boundary_clamps() {\n        assert_eq!(clamp_to(100), 100);\n        assert_eq!(clamp_to(99), 99);\n        assert_eq!(clamp_to(500), 100);\n    }\n}\n";

const ASYNC_PREDICATE: &str =
    "pub async fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";

/// Dropping `disclosure.warnings.insert(0, note)` would put the grip-class
/// omission first (or leave the note out). The #8364 test never sees this
/// vector.
#[test]
fn off_line_predicate_note_precedes_the_grip_class_warning() -> Result<(), String> {
    let scratch = Scratch::create("gripped-predicate", GRIPPED_PREDICATE)?;
    let json = stub(&scratch.0, "src/lib.rs:3", "predicate", true)?;
    let stdout = String::from_utf8_lossy(&json.stdout).to_string();
    let stderr = String::from_utf8_lossy(&json.stderr).to_string();
    if !json.status.success() {
        return Err(format!(
            "off-line predicate --at must stay ready: status={:?} stdout={stdout} stderr={stderr}",
            json.status.code()
        ));
    }
    let document: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|error| format!("ready stub JSON did not parse: {error}: {stdout}"))?;
    if document["state"] != "ready" {
        return Err(format!("expected state ready, got {document}"));
    }
    if document["grip_class"] != "strongly_gripped" {
        return Err(format!(
            "fixture must disclose the gripped class so both warnings exist: {document}"
        ));
    }
    let warnings = document["warnings"]
        .as_array()
        .ok_or_else(|| format!("warnings must be an array: {document}"))?;
    if warnings.len() < 2 {
        return Err(format!(
            "need the nearest-gap note and the grip omission, got {warnings:?}"
        ));
    }
    let note = warnings[0]
        .as_str()
        .ok_or_else(|| format!("warnings[0] must be the note string: {warnings:?}"))?;
    note_names_substitution(note, "src/lib.rs:3", "src/lib.rs:2")?;
    let grip = warnings[1]
        .as_str()
        .ok_or_else(|| format!("warnings[1] must be the grip omission: {warnings:?}"))?;
    if !grip.contains("strongly_gripped") || !grip.contains("configured off") {
        return Err(format!("grip-class warning must follow the note: {grip}"));
    }

    let human = stub(&scratch.0, "src/lib.rs:3", "predicate", false)?;
    let human_out = String::from_utf8_lossy(&human.stdout).to_string();
    if !human.status.success() {
        return Err(format!(
            "human off-line --at must stay ready: {}",
            String::from_utf8_lossy(&human.stderr)
        ));
    }
    let first = human_out
        .lines()
        .next()
        .ok_or_else(|| format!("human stub printed nothing: {human_out:?}"))?;
    note_names_substitution(first, "src/lib.rs:3", "src/lib.rs:2")
}

/// An off-line `--at` whose nearest gap then refuses still carries the
/// substitution note in the `--json` envelope on stdout (exit 3, prose on
/// stderr); `insert(0)` runs before the outcome match. A selector miss would leave `seam_id` null and `warnings` empty.
#[test]
fn off_line_at_that_refuses_keeps_the_note_first_in_json_warnings() -> Result<(), String> {
    let scratch = Scratch::create("async-predicate", ASYNC_PREDICATE)?;
    let json = stub(&scratch.0, "src/lib.rs:3", "predicate", true)?;
    let stdout = String::from_utf8_lossy(&json.stdout).to_string();
    let stderr = String::from_utf8_lossy(&json.stderr).to_string();
    if json.status.code() != Some(3) {
        return Err(format!(
            "substituted async owner must refuse with exit 3: status={:?} stdout={stdout} stderr={stderr}",
            json.status.code()
        ));
    }
    let (envelope, prose) = split_refusal_envelope(&stdout, &stderr)?;
    if envelope["state"] != "refused" {
        return Err(format!("expected refused envelope, got {envelope}"));
    }
    if envelope["refusal"]["kind"] != "owner_async" {
        return Err(format!(
            "nearest gap must refuse as owner_async, got {envelope}"
        ));
    }
    if envelope["seam_id"].is_null() {
        return Err(format!(
            "post-resolution refusal must keep the nearest seam id: {envelope}"
        ));
    }
    let warnings = envelope["warnings"]
        .as_array()
        .ok_or_else(|| format!("warnings must be an array: {envelope}"))?;
    let note = warnings
        .first()
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("warnings[0] must be the substitution note: {warnings:?}"))?;
    note_names_substitution(note, "src/lib.rs:3", "src/lib.rs:2")?;
    if !prose.contains("owner_async") {
        return Err(format!("human tail must still name the refusal: {prose}"));
    }
    Ok(())
}

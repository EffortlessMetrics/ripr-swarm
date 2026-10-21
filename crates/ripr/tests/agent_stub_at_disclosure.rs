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
    stub_options(root, at, Some(kind), json, false)
}

fn stub_options(
    root: &Path,
    at: &str,
    kind: Option<&str>,
    json: bool,
    write: bool,
) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at]);
    if let Some(kind) = kind {
        command.args(["--kind", kind]);
    }
    if write {
        command.arg("--write");
    }
    if json {
        command.arg("--json");
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr agent stub: {error}"))
}

fn stub_write(root: &Path, at: &str, kind: &str, json: bool) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at, "--kind", kind, "--write"]);
    if json {
        command.arg("--json");
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr agent stub --write: {error}"))
}

fn split_refusal_envelope(stderr: &str) -> Result<(serde_json::Value, &str), String> {
    let prose_at = stderr
        .find("ripr: agent stub: ")
        .ok_or_else(|| format!("the refusal prose must follow the envelope: {stderr}"))?;
    let envelope: serde_json::Value = serde_json::from_str(&stderr[..prose_at])
        .map_err(|error| format!("the refusal envelope did not parse: {error}: {stderr}"))?;
    Ok((envelope, &stderr[prose_at..]))
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

const DIVERGENT_METHOD: &str = "pub struct Clamp;\nimpl Clamp {\n    pub fn clamp_to(&self, x: i32) -> i32 {\n        if x >= 100 { 100 } else { x }\n    }\n}\n";

fn repair_fixture(label: &str, source: &str, observer: &str) -> Result<Scratch, String> {
    let scratch = Scratch::create(label, source)?;
    std::fs::create_dir_all(scratch.0.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/clamp_tests.rs"), observer)
        .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("sentinel"), "untracked user data\n")
        .map_err(|error| error.to_string())?;
    Ok(scratch)
}

#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    bytes: Vec<u8>,
    modified: std::time::SystemTime,
    permissions: std::fs::Permissions,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

/// Source no-touch excludes only the normal analysis cache. Warm snapshots
/// include that cache too; directory access times are not a write oracle.
fn file_snapshot(
    root: &Path,
    include_cache: bool,
) -> Result<std::collections::BTreeMap<PathBuf, FileSnapshot>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = std::collections::BTreeMap::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            if !include_cache && relative == Path::new("target/ripr/cache") {
                continue;
            }
            let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            #[cfg(unix)]
            let identity = {
                use std::os::unix::fs::MetadataExt;
                (
                    metadata.dev(),
                    metadata.ino(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                )
            };
            files.insert(
                relative.to_path_buf(),
                FileSnapshot {
                    bytes: std::fs::read(&path).map_err(|error| error.to_string())?,
                    modified: metadata.modified().map_err(|error| error.to_string())?,
                    permissions: metadata.permissions(),
                    #[cfg(unix)]
                    identity,
                },
            );
        }
    }
    Ok(files)
}

/// Real inventory selects an inline impl-method stub while the packet admits
/// its existing integration observer. No synthetic placement is injected.
#[test]
fn divergent_write_refuses_with_json_and_preserves_source_and_warm_cache() -> Result<(), String> {
    let observer = "use divergent_write::Clamp;\n#[test]\nfn below_boundary() { assert_eq!(Clamp.clamp_to(80), 80); }\n";
    let dry = repair_fixture("divergent_write", DIVERGENT_METHOD, observer)?;
    let output = stub_options(&dry.0, "src/lib.rs:4", None, true, false)?;
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("dry projection: {error}: {output:?}"))?;
    if !output.status.success()
        || document["state"] != "ready"
        || document["next_repair"]["placement_divergence"]["stub_file"] != "src/lib.rs"
        || document["next_repair"]["placement_divergence"]["packet_file"] != "tests/clamp_tests.rs"
    {
        return Err(format!(
            "fixture must establish the real divergent route: {document}"
        ));
    }
    for json in [true, false] {
        // Each mode starts cold independently of the dry projection above.
        let scratch = repair_fixture("divergent_write", DIVERGENT_METHOD, observer)?;
        let before = file_snapshot(&scratch.0, false)?;
        if before.len() != 4 || scratch.0.join("target").exists() {
            return Err(format!(
                "fixture must start cold with four user files: {before:?}"
            ));
        }
        let mut warm = None;
        for _ in 0..2 {
            let output = stub_options(&scratch.0, "src/lib.rs:4", None, json, true)?;
            if file_snapshot(&scratch.0, false)? != before {
                return Err("refused write changed user paths, bytes or metadata".to_owned());
            }
            if output.status.code() != Some(2) || !output.stdout.is_empty() {
                return Err(format!(
                    "write refusal must exit 2 with empty stdout: {output:?}"
                ));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            let prose = if json {
                let (envelope, prose) = split_refusal_envelope(&stderr)?;
                if envelope["schema_version"] != "0.3"
                    || envelope["kind"] != "rust_test_stub"
                    || envelope["state"] != "refused"
                    || envelope["refusal"]["kind"] != "placement_divergence"
                    || envelope["seam_id"] != document["seam_id"]
                    || envelope["owner"] != document["owner"]
                    || envelope["grip_class"] != document["grip_class"]
                    || envelope["classification"] != document["classification"]
                    || envelope["warnings"] != document["warnings"]
                    || envelope["refusal"]["reason"].as_str()
                        != Some(prose.trim().trim_start_matches("ripr: "))
                    || envelope.get("written").is_some()
                    || envelope.get("next_repair").is_some()
                {
                    return Err(format!(
                        "versioned refusal must retain resolved identity: {envelope}"
                    ));
                }
                prose.to_owned()
            } else {
                stderr.to_string()
            };
            if !prose.contains("src/lib.rs")
                || !prose.contains("tests/clamp_tests.rs")
                || !prose.contains("Rerun without --write")
                || !prose.contains("No source or test file was written")
            {
                return Err(format!(
                    "refusal must name both files and recovery: {prose}"
                ));
            }
            let after = file_snapshot(&scratch.0, true)?;
            if let Some(warm) = &warm
                && &after != warm
            {
                return Err(
                    "warm refusal changed file bytes or metadata, including cache".to_owned(),
                );
            }
            warm = Some(after);
        }
    }
    Ok(())
}

/// An agreeing placement still writes through the real CLI: the refusal
/// regression cannot pass by disabling all writes or all repair projections.
#[test]
fn agreeing_repair_placement_still_writes_through_cli() -> Result<(), String> {
    let scratch = repair_fixture(
        "agreeing_write",
        "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n",
        "use agreeing_write::clamp_to;\n#[test]\nfn below_boundary() { assert_eq!(clamp_to(80), 80); }\n",
    )?;
    let before =
        std::fs::read(scratch.0.join("tests/clamp_tests.rs")).map_err(|error| error.to_string())?;
    let output = stub_options(&scratch.0, "src/lib.rs:2", None, true, true)?;
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("agreeing write JSON: {error}: {output:?}"))?;
    let after =
        std::fs::read(scratch.0.join("tests/clamp_tests.rs")).map_err(|error| error.to_string())?;
    if !output.status.success()
        || !output.stderr.is_empty()
        || document["state"] != "ready"
        || document["written"] != true
        || document["next_repair"]["allowed_edit_surface"] != "tests/clamp_tests.rs"
        || document["next_repair"]
            .get("placement_divergence")
            .is_some()
        || before == after
    {
        return Err(format!(
            "agreeing placement must really write: {output:?}: {document}"
        ));
    }
    Ok(())
}

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
    if !grip.contains("strongly_gripped")
        || !grip.contains("existing tests already discriminate it")
        || grip.contains("configured off")
    {
        return Err(format!(
            "grip-class warning must name the real reason, not 'configured off': {grip}"
        ));
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
/// substitution note in the `--json` envelope (`insert(0)` runs before the
/// outcome match). A selector miss would leave `seam_id` null and `warnings` empty.
#[test]
fn off_line_at_that_refuses_keeps_the_note_first_in_json_warnings() -> Result<(), String> {
    let scratch = Scratch::create("async-predicate", ASYNC_PREDICATE)?;
    let json = stub(&scratch.0, "src/lib.rs:3", "predicate", true)?;
    let stdout = String::from_utf8_lossy(&json.stdout).to_string();
    let stderr = String::from_utf8_lossy(&json.stderr).to_string();
    if json.status.success() || !stdout.is_empty() {
        return Err(format!(
            "substituted async owner must refuse with empty stdout: status={:?} stdout={stdout} stderr={stderr}",
            json.status.code()
        ));
    }
    let (envelope, prose) = split_refusal_envelope(&stderr)?;
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

/// An open-gap seam owns the stub ripr writes; the stub's own seam is then
/// strongly gripped. #8891: the second `--write` must report the existing
/// stub ("already written", with the run command) instead of suffixing a
/// `_2` duplicate, and its disclosure must name the real reason (the seam is
/// exposed) rather than claiming it is "configured off".
#[test]
fn second_write_reports_already_written_instead_of_a_duplicate() -> Result<(), String> {
    const OPEN_GAP: &str =
        "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
    let scratch = Scratch::create("rewrite-clarity", OPEN_GAP)?;
    let root = &scratch.0;

    let first = stub_write(root, "src/lib.rs:2", "predicate", false)?;
    if !first.status.success() {
        return Err(format!(
            "first --write must succeed: {}",
            String::from_utf8_lossy(&first.stderr)
        ));
    }

    let second = stub_write(root, "src/lib.rs:2", "predicate", false)?;
    let stdout = String::from_utf8_lossy(&second.stdout).to_string();
    let stderr = String::from_utf8_lossy(&second.stderr).to_string();
    if !second.status.success() {
        return Err(format!("second --write must stay exit 0: {stderr}"));
    }
    if !stdout.contains("Stub already written at") {
        return Err(format!(
            "second --write must report the existing stub: {stdout}"
        ));
    }
    if stdout.contains("Wrote test stub") || stdout.contains("_2") {
        return Err(format!(
            "second --write must not write or name a `_2` duplicate: {stdout}"
        ));
    }
    if !stdout.contains("Run it:") {
        return Err(format!(
            "the already-written report must still carry the run command: {stdout}"
        ));
    }
    if stdout.contains("configured off") {
        return Err(format!(
            "nothing was configured off; the disclosure must name the real reason: {stdout}"
        ));
    }
    if !stdout.contains("strongly_gripped") {
        return Err(format!(
            "the disclosure must still name the grip class: {stdout}"
        ));
    }

    let written = std::fs::read_to_string(root.join("src/lib.rs"))
        .map_err(|error| format!("read back the written file: {error}"))?;
    let occurrences = written
        .matches("fn clamp_to_boundary_discriminator(")
        .count();
    if occurrences != 1 || written.contains("_2(") {
        return Err(format!(
            "the file must hold exactly one base-named stub, no `_2` duplicate:\n{written}"
        ));
    }

    // The JSON document reports the same state for machine consumers.
    let json = stub_write(root, "src/lib.rs:2", "predicate", true)?;
    let document: serde_json::Value = serde_json::from_slice(&json.stdout)
        .map_err(|error| format!("ready stub JSON did not parse: {error}"))?;
    if document["state"] != "ready"
        || document["already_present"] != true
        || document["written"] != false
    {
        return Err(format!(
            "the second write document must be ready, already present, not rewritten: {document}"
        ));
    }
    let warnings = document["warnings"]
        .as_array()
        .ok_or_else(|| format!("warnings must be an array: {document}"))?;
    let warning = warnings
        .first()
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the disclosure warning must be present: {document}"))?;
    if !warning.contains("strongly_gripped")
        || !warning.contains("existing tests already discriminate it")
        || warning.contains("configured off")
    {
        return Err(format!(
            "the disclosure must name the real reason, not 'configured off': {warning}"
        ));
    }
    Ok(())
}

/// The first write must change the source; repeated writes must leave every
/// byte alone, including when a different test owns the base name. Comparing
/// metadata also catches an unnecessary rewrite of identical bytes. Analysis
/// may populate target caches; this contract covers the source file.
#[test]
fn repeated_writes_preserve_source_bytes_and_metadata_with_name_collisions() -> Result<(), String> {
    const OPEN_GAP: &str =
        "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
    const COLLISION: &str = "\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn clamp_to_boundary_discriminator() {\n        assert_eq!(super::clamp_to(80), 80);\n    }\n}\n";
    for collision in [false, true] {
        for crlf in [false, true] {
            let mut source = OPEN_GAP.to_string();
            if collision {
                source.push_str(COLLISION);
            }
            if crlf {
                source = source.replace('\n', "\r\n");
            }
            let scratch = Scratch::create("rewrite-collision", &source)?;
            let path = scratch.0.join("src/lib.rs");
            let expected_name = if collision {
                "clamp_to_boundary_discriminator_2"
            } else {
                "clamp_to_boundary_discriminator"
            };
            let first = stub_write(&scratch.0, "src/lib.rs:2", "predicate", true)?;
            assert!(first.status.success(), "first write: {first:?}");
            let document: serde_json::Value = serde_json::from_slice(&first.stdout)
                .map_err(|error| format!("first write JSON: {error}: {first:?}"))?;
            assert_eq!(document["state"], "ready");
            assert_eq!(document["test_name"], expected_name);
            assert_eq!(document["already_present"], false);
            assert_eq!(document["written"], true);
            let written = std::fs::read(&path).map_err(|error| error.to_string())?;
            assert_ne!(
                written,
                source.as_bytes(),
                "first write must reach the file"
            );
            let text = std::str::from_utf8(&written).map_err(|error| error.to_string())?;
            assert_eq!(text.matches(&format!("fn {expected_name}(")).count(), 1);
            assert!(!text.contains("fn clamp_to_boundary_discriminator_3("));
            if crlf {
                assert!(!text.replace("\r\n", "").contains('\n'));
            }
            let before = std::fs::metadata(&path).map_err(|error| error.to_string())?;
            for json in [false, true] {
                let repeat = stub_write(&scratch.0, "src/lib.rs:2", "predicate", json)?;
                assert!(repeat.status.success(), "repeat write: {repeat:?}");
                if json {
                    let document: serde_json::Value = serde_json::from_slice(&repeat.stdout)
                        .map_err(|error| format!("repeat JSON: {error}: {repeat:?}"))?;
                    assert_eq!(document["state"], "ready");
                    assert_eq!(document["test_name"], expected_name);
                    assert_eq!(document["already_present"], true);
                    assert_eq!(document["written"], false);
                } else {
                    let stdout = String::from_utf8_lossy(&repeat.stdout);
                    assert!(stdout.contains("Stub already written at"), "{stdout}");
                    assert!(stdout.contains("Run it:"), "{stdout}");
                    assert!(stdout.contains(expected_name), "{stdout}");
                    assert!(!stdout.contains("Wrote test stub"), "{stdout}");
                }
                assert_eq!(
                    std::fs::read(&path).map_err(|error| error.to_string())?,
                    written
                );
                let after = std::fs::metadata(&path).map_err(|error| error.to_string())?;
                assert_eq!(after.len(), before.len());
                assert_eq!(
                    after.modified().map_err(|error| error.to_string())?,
                    before.modified().map_err(|error| error.to_string())?
                );
                assert_eq!(after.permissions(), before.permissions());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    assert_eq!((after.dev(), after.ino()), (before.dev(), before.ino()));
                    assert_eq!(
                        (after.mtime(), after.mtime_nsec()),
                        (before.mtime(), before.mtime_nsec())
                    );
                    assert_eq!(
                        (after.ctime(), after.ctime_nsec()),
                        (before.ctime(), before.ctime_nsec())
                    );
                }
            }
        }
    }
    Ok(())
}

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

fn ripr_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
}

fn stub(root: &Path, at: &str, kind: Option<&str>, json: bool) -> Result<Output, String> {
    let mut command = ripr_command();
    command
        .args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at]);
    if let Some(kind) = kind {
        command.args(["--kind", kind]);
    }
    if json {
        command.arg("--json");
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr agent stub: {error}"))
}

fn stub_write(root: &Path, at: &str, kind: Option<&str>, json: bool) -> Result<Output, String> {
    let mut command = ripr_command();
    command
        .args(["agent", "stub", "--root"])
        .arg(root)
        .args(["--at", at, "--write"]);
    if let Some(kind) = kind {
        command.args(["--kind", kind]);
    }
    if json {
        command.arg("--json");
    }
    command
        .output()
        .map_err(|error| format!("spawn ripr agent stub --write: {error}"))
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

/// #10716: after root/config admission, capability is decided from the selected
/// path before Rust seam analysis, independent of --kind, JSON, or a write.
/// Rust-looking contents under a non-Rust suffix must not bypass the gate.
#[test]
fn non_rust_paths_refuse_before_rust_analysis_and_never_write() -> Result<(), String> {
    const RUST_SOURCE: &str = "pub fn value(n: u32) -> bool {\n    n > 3\n}\n";
    let cases = [
        ("pricing.py", "Python", "def fee(n):\n    return n * 4\n"),
        (
            "src/pricing.ts",
            "TypeScript",
            "export function fee(n: number) { return n * 4; }\n",
        ),
        (
            "src/pricing.mjs",
            "JavaScript",
            "export function fee(n) { return n * 4; }\n",
        ),
        ("pricing.pm", "Perl", "sub fee { return $_[0] * 4; }\n1;\n"),
        ("src/pricing.rs.py", "Python", RUST_SOURCE),
        ("src/pricing.txt", "non-Rust", RUST_SOURCE),
    ];
    for (file, language, source) in cases {
        // An unrelated malformed Rust file must not turn a language refusal
        // into a Rust parse failure or require a successful Rust inventory.
        let unrelated = "pub fn unrelated(\n";
        let scratch = Scratch::create("non_rust_language", unrelated)?;
        let root = &scratch.0;
        // Configuration admission precedes stub resolution. Pin Rust so a
        // Python-shaped path cannot select an unavailable preview adapter in
        // the Rust-only build; the path still must receive a language refusal.
        let config = "[languages]\nenabled = [\"rust\"]\n";
        std::fs::write(root.join("ripr.toml"), config).map_err(|error| error.to_string())?;
        std::fs::write(root.join(file), source).map_err(|error| error.to_string())?;
        let at = format!("{file}:2");
        for kind in [None, Some("predicate")] {
            for write in [false, true] {
                for json in [true, false] {
                    let output = if write {
                        stub_write(root, &at, kind, json)?
                    } else {
                        stub(root, &at, kind, json)?
                    };
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert_eq!(output.status.code(), Some(3), "{file}: {stdout}{stderr}");
                    if json {
                        let document: serde_json::Value = serde_json::from_str(&stdout)
                            .map_err(|error| format!("{file}: refusal JSON: {error}: {stdout}"))?;
                        // This assertion is the old-behavior discriminator;
                        // schema movement alone cannot satisfy the test.
                        assert_eq!(
                            document["refusal"]["kind"], "unsupported_language",
                            "{file}: {document}"
                        );
                        assert_eq!(document["schema_version"], "0.4");
                        assert_eq!(document["kind"], "rust_test_stub");
                        assert_eq!(document["state"], "refused");
                        for field in ["seam_id", "owner", "grip_class", "classification"] {
                            assert!(document[field].is_null(), "{file}: {field}: {document}");
                        }
                        assert_eq!(document["warnings"], serde_json::json!([]));
                        let reason = document["refusal"]["reason"].as_str().ok_or_else(|| {
                            format!("{file}: refusal reason is absent: {document}")
                        })?;
                        assert!(
                            stderr.contains(reason),
                            "{file}: JSON and human reasons differ: {stderr}"
                        );
                    } else {
                        assert!(
                            stdout.is_empty(),
                            "{file}: human refusal wrote stdout: {stdout}"
                        );
                    }
                    for expected in [
                        file,
                        language,
                        "Rust files only",
                        "test file/node",
                        "verify command when available",
                        "ripr check --root",
                    ] {
                        assert!(
                            stderr.contains(expected),
                            "{file}: missing {expected:?}: {stderr}"
                        );
                    }
                    for false_cause in [
                        "no reported gap",
                        "did not parse cleanly",
                        "lexical fallback",
                        "ripr pilot",
                    ] {
                        assert!(
                            !stderr.contains(false_cause),
                            "{file}: false recovery/cause {false_cause:?}: {stderr}"
                        );
                    }
                    let manual = stderr
                        .find("Add or strengthen the test by hand")
                        .ok_or_else(|| format!("{file}: missing manual-test route: {stderr}"))?;
                    let native = stderr
                        .find("run the verify command or your repository's test command")
                        .ok_or_else(|| format!("{file}: missing native-test route: {stderr}"))?;
                    let refresh = stderr
                        .find("then rerun ")
                        .ok_or_else(|| format!("{file}: missing post-test refresh: {stderr}"))?;
                    assert!(
                        manual < native && native < refresh,
                        "{file}: recovery order: {stderr}"
                    );
                    assert_eq!(
                        std::fs::read_to_string(root.join(file))
                            .map_err(|error| error.to_string())?,
                        source
                    );
                    assert_eq!(
                        std::fs::read_to_string(root.join("src/lib.rs"))
                            .map_err(|error| error.to_string())?,
                        unrelated
                    );
                    assert_eq!(
                        std::fs::read_to_string(root.join("ripr.toml"))
                            .map_err(|error| error.to_string())?,
                        config
                    );
                    assert!(
                        !root.join("tests").exists(),
                        "{file}: unsupported write created a tests target"
                    );
                }
            }
        }
    }
    Ok(())
}

/// An unconfigured Python marker retains the existing adapter admission check.
/// Both feature sets execute this control; only their expected result differs.
#[test]
fn python_marker_without_config_keeps_feature_admission() -> Result<(), String> {
    let source = "pub fn value(n: u32) -> bool {\n    n > 3\n}\n";
    let scratch = Scratch::create("python_config_admission", source)?;
    let root = &scratch.0;
    // Make this fixture a config-discovery boundary, including when TMPDIR is
    // inside another repository, without creating or inheriting ripr.toml.
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        format!("{manifest}\n[workspace]\n"),
    )
    .map_err(|error| error.to_string())?;
    let file = "src/pricing.rs.py";
    std::fs::write(root.join(file), source).map_err(|error| error.to_string())?;
    assert!(!root.join("ripr.toml").exists());

    let output = stub_write(root, "src/pricing.rs.py:2", None, true)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if cfg!(feature = "lang-python") {
        assert_eq!(output.status.code(), Some(3), "{stdout}{stderr}");
        let document: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|error| format!("Python capability refusal JSON: {error}: {stdout}"))?;
        assert_eq!(document["refusal"]["kind"], "unsupported_language");
    } else {
        assert_eq!(output.status.code(), Some(2), "{stdout}{stderr}");
        assert!(
            stdout.is_empty(),
            "configuration failure wrote stdout: {stdout}"
        );
        for expected in [
            "Python project markers were detected",
            "without Cargo feature `lang-python`",
            "[languages] enabled = [\"rust\"]",
        ] {
            assert!(stderr.contains(expected), "missing {expected:?}: {stderr}");
        }
    }
    for path in [file, "src/lib.rs"] {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).map_err(|error| error.to_string())?,
            source
        );
    }
    assert!(!root.join("ripr.toml").exists());
    assert!(!root.join("tests").exists());
    Ok(())
}

/// A capability refusal must not mask malformed repository configuration.
#[test]
fn malformed_config_precedes_non_rust_stub_refusal() -> Result<(), String> {
    let rust_source = "pub fn value(n: u32) -> bool {\n    n > 3\n}\n";
    let python_source = "def fee(n):\n    return n * 4\n";
    let malformed_config = "[analysis\n";
    let scratch = Scratch::create("malformed_config_admission", rust_source)?;
    let root = &scratch.0;
    std::fs::write(root.join("pricing.py"), python_source).map_err(|error| error.to_string())?;
    std::fs::write(root.join("ripr.toml"), malformed_config).map_err(|error| error.to_string())?;

    let output = stub_write(root, "pricing.py:2", None, true)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stdout}{stderr}");
    assert!(
        stdout.is_empty(),
        "configuration failure wrote stdout: {stdout}"
    );
    assert!(stderr.contains("ripr.toml"), "{stderr}");
    assert!(stderr.contains("Fix the error above"), "{stderr}");
    assert!(!stderr.contains("unsupported_language"), "{stderr}");
    for (path, expected) in [
        ("pricing.py", python_source),
        ("src/lib.rs", rust_source),
        ("ripr.toml", malformed_config),
    ] {
        assert_eq!(
            std::fs::read_to_string(root.join(path)).map_err(|error| error.to_string())?,
            expected
        );
    }
    assert!(!root.join("tests").exists());
    Ok(())
}

/// A blanket refusal or a check against source contents instead of path
/// capability must not erase the retained Rust success and failure routes.
#[test]
fn rust_paths_keep_ready_missing_and_malformed_results() -> Result<(), String> {
    let source = "pub fn value(n: u32) -> bool {\n    n > 3\n}\n";
    let scratch = Scratch::create("rust_language_control", source)?;
    let root = &scratch.0;
    for kind in [None, Some("predicate")] {
        let ready = stub(root, "src/lib.rs:2", kind, true)?;
        assert_eq!(ready.status.code(), Some(0), "{ready:?}");
        let document: serde_json::Value = serde_json::from_slice(&ready.stdout)
            .map_err(|error| format!("Rust ready document: {error}"))?;
        assert_eq!(document["state"], "ready");
        assert_eq!(document["written"], false);

        let missing = stub(root, "src/missing.rs:2", kind, true)?;
        assert_eq!(missing.status.code(), Some(3), "{missing:?}");
        let document: serde_json::Value = serde_json::from_slice(&missing.stdout)
            .map_err(|error| format!("Rust missing-selector document: {error}"))?;
        assert_eq!(document["refusal"]["kind"], "seam_not_found");
    }
    std::fs::write(root.join("src/broken.rs"), "pub fn broken(\n")
        .map_err(|error| error.to_string())?;
    for kind in [None, Some("predicate")] {
        let malformed = stub(root, "src/broken.rs:1", kind, true)?;
        assert_eq!(malformed.status.code(), Some(3), "{malformed:?}");
        let document: serde_json::Value = serde_json::from_slice(&malformed.stdout)
            .map_err(|error| format!("Rust malformed-source document: {error}"))?;
        assert_eq!(document["refusal"]["kind"], "seam_not_found");
        let stderr = String::from_utf8_lossy(&malformed.stderr);
        assert!(
            stderr.contains("did not parse cleanly as current-edition Rust"),
            "{stderr}"
        );
        assert!(!stderr.contains("unsupported_language"), "{stderr}");
    }
    assert_eq!(
        std::fs::read_to_string(root.join("src/lib.rs")).map_err(|error| error.to_string())?,
        source
    );
    Ok(())
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
    let json = stub(&scratch.0, "src/lib.rs:3", Some("predicate"), true)?;
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

    let human = stub(&scratch.0, "src/lib.rs:3", Some("predicate"), false)?;
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
    let json = stub(&scratch.0, "src/lib.rs:3", Some("predicate"), true)?;
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

    let first = stub_write(root, "src/lib.rs:2", Some("predicate"), false)?;
    if !first.status.success() {
        return Err(format!(
            "first --write must succeed: {}",
            String::from_utf8_lossy(&first.stderr)
        ));
    }

    let second = stub_write(root, "src/lib.rs:2", Some("predicate"), false)?;
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
    let json = stub_write(root, "src/lib.rs:2", Some("predicate"), true)?;
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

/// #10683: a related test already reaches the changed boundary and its
/// assertion is a weak check. `agent stub` must not write a second skeleton
/// test with `todo!()` placeholders and `repair` must not ask for a new
/// boundary test or print producer-internal evidence names; both reuse
/// `check`'s wording and point at the existing test. The control (same crate
/// without the test) keeps the skeleton.
#[test]
fn weak_oracle_boundary_points_at_the_existing_test_instead_of_a_new_one() -> Result<(), String> {
    const LIB: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty > 10 { price * 10 / 100 } else { 0 }\n}\n";
    const WEAK_TEST: &str =
        "use weakoracle::discount;\n#[test]\nfn t() {\n    assert!(discount(10, 10) > 0);\n}\n";
    let scratch = Scratch::create("weakoracle", LIB)?;
    let control = Scratch::create("weakoracle", LIB)?;
    std::fs::create_dir_all(scratch.0.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/t.rs"), WEAK_TEST).map_err(|error| error.to_string())?;

    let wording = "write it as an exact-value assertion ripr can credit, for example assert_eq!(result, expected)";
    let pointer = "Related test: tests/t.rs:3 t";

    let human = stub(&scratch.0, "src/lib.rs:2", Some("predicate"), false)?;
    let stdout = String::from_utf8_lossy(&human.stdout).to_string();
    let stderr = String::from_utf8_lossy(&human.stderr).to_string();
    if human.status.code() != Some(3)
        || !stderr.contains(wording)
        || !stderr.contains(pointer)
        || stdout.contains("todo!")
    {
        return Err(format!(
            "weak-oracle stub must refuse with check's wording and the test, no skeleton: status={:?} stdout={stdout} stderr={stderr}",
            human.status.code()
        ));
    }

    let json = stub(&scratch.0, "src/lib.rs:2", Some("predicate"), true)?;
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout)
        .map_err(|error| format!("refusal envelope did not parse: {error}"))?;
    if envelope["state"] != "refused" || envelope["refusal"]["kind"] != "existing_test_weak_oracle"
    {
        return Err(format!("expected the weak-oracle refusal: {envelope}"));
    }

    let repair = ripr_command()
        .args(["repair", "--root"])
        .arg(&scratch.0)
        .output()
        .map_err(|error| format!("spawn ripr repair: {error}"))?;
    let repair_err = String::from_utf8_lossy(&repair.stderr).to_string();
    if !repair_err.contains(wording)
        || !repair_err.contains(pointer)
        || repair_err.contains("producer-owned")
        || repair_err.contains("write the boundary test")
    {
        return Err(format!(
            "repair must reuse check's wording and name the test: {repair_err}"
        ));
    }
    if repair.status.success() {
        return Err("repair eligibility must stay fail-closed for this seam".to_string());
    }

    // Control: no related test, so the boundary test is the right ask.
    let ready = stub(&control.0, "src/lib.rs:2", Some("predicate"), false)?;
    if !ready.status.success() || !String::from_utf8_lossy(&ready.stdout).contains("todo!") {
        return Err("the no-test control must keep the skeleton".to_string());
    }
    Ok(())
}

/// #10683 / activation aggregates boundary values across related tests: when
/// one test supplies the boundary input without an assertion and another
/// carries only a weak relational check, the weak test is not provably the
/// boundary observer, so neither `agent stub` nor `repair` may name it.
#[test]
fn weak_oracle_pointer_is_withheld_when_the_boundary_input_may_come_from_another_test()
-> Result<(), String> {
    const LIB: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty > 10 { price * 10 / 100 } else { 0 }\n}\n";
    const TESTS: &str = "use twotests::discount;\n#[test]\nfn near() {\n    assert!(discount(10, 9) < 1);\n}\n#[test]\nfn at_boundary() {\n    discount(10, 10);\n}\n";
    let scratch = Scratch::create("twotests", LIB)?;
    std::fs::create_dir_all(scratch.0.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("tests/t.rs"), TESTS).map_err(|error| error.to_string())?;

    let json = stub(&scratch.0, "src/lib.rs:2", Some("predicate"), true)?;
    let envelope: serde_json::Value = serde_json::from_slice(&json.stdout)
        .map_err(|error| format!("stub stdout did not parse: {error}"))?;
    if envelope["refusal"]["kind"] == "existing_test_weak_oracle"
        || envelope.to_string().contains("Related test:")
    {
        return Err(format!("the weak test must not be named: {envelope}"));
    }
    let repair = ripr_command()
        .args(["repair", "--root"])
        .arg(&scratch.0)
        .output()
        .map_err(|error| format!("spawn ripr repair: {error}"))?;
    let repair_err = String::from_utf8_lossy(&repair.stderr).to_string();
    if repair_err.contains("Related test:") {
        return Err(format!("repair must not name a test: {repair_err}"));
    }
    Ok(())
}

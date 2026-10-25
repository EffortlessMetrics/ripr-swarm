//! Public-API controls for a path-qualified scanner state compared against an
//! enum variant (RIPR-SPEC-0163 example 2, #10108). Each case writes a small
//! crate, changes the compared variant, and reads the predicate finding
//! through `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-scanner-path-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation, then the guard owns the root before anything else
        // is written, so a failed setup step cleans up after itself.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"scanner_path\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

fn source(comparison: &str, tests: &str, extra: &str) -> String {
    let base = format!(
        "#[derive(Clone, Copy, PartialEq, Eq)]\npub enum Scan {{\n    Text,\n    Word,\n}}\n\npub fn scan_path(input: &str) -> Scan {{\n    let mut state = Scan::Text;\n    for symbol in input.chars() {{\n        state = match (state, symbol) {{\n            (Scan::Text, ' ') => Scan::Text,\n            (Scan::Text, _) => Scan::Word,\n            (Scan::Word, ' ') => Scan::Text,\n            (Scan::Word, _) => Scan::Word,\n        }};\n    }}\n    state\n}}\n\npub fn classify_path(input: &str) -> &'static str {{\n    let final_state = scan_path(input);\n    if final_state {comparison} {{\n        \"word\"\n    }} else {{\n        \"blank\"\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn classify_reads_the_last_state() {{\n{tests}\n    }}\n}}\n"
    );
    // Declarations go after the test module so the compared line stays put.
    format!("{base}{extra}")
}

/// The compared line is always line 22; the diff changes `Scan::Word` to the
/// compared operand.
fn diff(compared: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -20,6 +20,6 @@\n pub fn classify_path(input: &str) -> &'static str {{\n     let final_state = scan_path(input);\n-    if final_state == Scan::Word {{\n+    if final_state {compared} {{\n         \"word\"\n     }} else {{\n         \"blank\"\n"
    )
}

const BOTH_ROWS: &str = "        assert_eq!(classify_path(\"ab\"), \"word\");\n        assert_eq!(classify_path(\"ab \"), \"blank\");";
const WORD_ROW_ONLY: &str = "        assert_eq!(classify_path(\"ab\"), \"word\");";

fn predicate_finding(comparison: &str, tests: &str) -> Result<Value, String> {
    predicate_finding_with(comparison, tests, "")
}

/// `extra` declares items after the test module; the fixture must compile.
fn predicate_finding_with(comparison: &str, tests: &str, extra: &str) -> Result<Value, String> {
    finding_for(&source(comparison, tests, extra), &diff(comparison))
}

/// The predicate finding of a compiled fixture crate under a diff.
fn finding_for(source: &str, diff: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    assert_compiles(&scratch.0.join("src/lib.rs"), &scratch.0)?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))
}

/// The fixture is real Rust: a control that does not compile proves nothing
/// about how a declared path reads.
fn assert_compiles(lib: &std::path::Path, out_dir: &std::path::Path) -> Result<(), String> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = std::process::Command::new(rustc)
        .args(["--edition", "2021", "--crate-type", "lib", "--test"])
        .arg("--out-dir")
        .arg(out_dir)
        .arg(lib)
        .output()
        .map_err(|error| format!("rustc did not run: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "the fixture must compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    ))
}

// Both states are pinned by a row, so the changed comparison is observed.
#[test]
fn variant_operand_beside_a_path_state_scanner_exposes() -> Result<(), String> {
    let finding = predicate_finding("== Scan::Text", BOTH_ROWS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// Control: with only the word row the text state is never reached, so the
// variant operand must not manufacture an observed equality.
#[test]
fn variant_operand_does_not_credit_a_row_that_misses_the_changed_state() -> Result<(), String> {
    let finding = predicate_finding("== Scan::Text", WORD_ROW_ONLY)?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// `!=` takes the same exact-operand path as `==`.
#[test]
fn variant_operand_under_not_equal_reads_the_same_way() -> Result<(), String> {
    let finding = predicate_finding("!= Scan::Text", BOTH_ROWS)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// A declared enum of the same variant name. `PartialEq<Other>` makes the
// comparison compile, so this is a real wrong-enum read, not an unresolved path.
const OTHER_ENUM: &str = "pub enum Other {\n    Text,\n}\n\nimpl PartialEq<Other> for Scan {\n    fn eq(&self, _: &Other) -> bool {\n        false\n    }\n}\n";

#[test]
fn a_declared_enum_with_the_same_variant_name_is_not_credited() -> Result<(), String> {
    let finding = predicate_finding_with("== Other::Text", BOTH_ROWS, OTHER_ENUM)?;
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

// Aliases are not resolved (RIPR-SPEC-0163 has no enum-type semantics): the
// operand keeps its written path, which is not the scanner's `Scan::Text`
// token, so an alias is a missed credit and never an observed equality.
#[test]
fn an_alias_of_the_scanner_enum_keeps_its_written_path() -> Result<(), String> {
    for (comparison, extra) in [
        ("== S::Text", "use Scan as S;\n"),
        ("== Alias::Text", "type Alias = Scan;\n"),
    ] {
        let finding = predicate_finding_with(comparison, BOTH_ROWS, extra)?;
        eprintln!("ALIAS {comparison}: {}", finding["classification"]);
        assert_ne!(
            finding["classification"], "exposed",
            "{comparison}: {finding}"
        );
    }
    Ok(())
}

// A crate-rooted spelling and a bare constant are not the scanner's token.
#[test]
fn a_qualified_prefix_or_a_constant_is_not_a_scanner_state_operand() -> Result<(), String> {
    for (comparison, extra) in [
        ("== crate::Scan::Text", ""),
        ("== TEXT_STATE", "const TEXT_STATE: Scan = Scan::Text;\n"),
    ] {
        let finding = predicate_finding_with(comparison, BOTH_ROWS, extra)?;
        eprintln!("PREFIX {comparison}: {}", finding["classification"]);
        assert_ne!(
            finding["classification"], "exposed",
            "{comparison}: {finding}"
        );
    }
    Ok(())
}

// #10308 review: a path-shaped string a test passes as an argument is not a
// scanner state. `classify("Scan::Word")` is false, because the associated
// constant holds "other", so the comparison must not gain an equality witness.
const PARAMETER_FIXTURE: &str = "pub struct Scan;\n\nimpl Scan {\n    #[allow(non_upper_case_globals)]\n    pub const Word: &'static str = \"other\";\n}\n\npub fn classify(input: &str) -> bool {\n    if input != Scan::Word {\n        return false;\n    }\n    true\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn the_path_text_is_not_the_constant() {\n        assert!(!classify(\"Scan::Word\"));\n    }\n}\n";

#[test]
fn a_path_shaped_string_argument_is_not_a_scanner_state() -> Result<(), String> {
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -8,5 +8,5 @@\n pub fn classify(input: &str) -> bool {\n-    if input == Scan::Word {\n+    if input != Scan::Word {\n         return false;\n     }\n     true\n";
    let finding = finding_for(PARAMETER_FIXTURE, diff)?;
    eprintln!("PARAM {}", finding["classification"]);
    assert_ne!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

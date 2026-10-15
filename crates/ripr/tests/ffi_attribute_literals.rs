#![cfg(feature = "lang-rust")]
//! Literal/comment contents cannot create or hide a Rust FFI binding attribute.

use ripr::domain::StaticLimitKind;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("../../../fixtures/rust_ffi_attribute_literals/input/src/lib.rs");
const MANIFEST: &str =
    include_str!("../../../fixtures/rust_ffi_attribute_literals/input/Cargo.toml");
const DIFF: &str = include_str!("../../../fixtures/rust_ffi_attribute_literals/diff.patch");

struct Fixture(PathBuf);

impl Fixture {
    fn create() -> Result<Self, String> {
        Self::with_input(SOURCE, DIFF)
    }

    fn with_input(source: &str, diff: &str) -> Result<Self, String> {
        Self::with_root(Self::next_root(), source, diff)
    }

    fn next_root() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "ripr-ffi-attribute-literals-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn with_root(root: PathBuf, source: &str, diff: &str) -> Result<Self, String> {
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        // Only successful exclusive creation grants the cleanup guard ownership.
        std::fs::create_dir(&root)
            .map_err(|error| format!("create fixture root {}: {error}", root.display()))?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        Ok(fixture)
    }

    fn finding(&self, line: usize) -> Result<Finding, String> {
        self.finding_among(line, 3)
    }

    fn finding_among(&self, line: usize, count: usize) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        assert_eq!(output.findings.len(), count, "changed predicate count");
        let finding = output
            .findings
            .into_iter()
            .find(|finding| {
                finding.probe.location.line == line
                    && finding.probe.location.file.ends_with("src/lib.rs")
            })
            .ok_or_else(|| format!("missing predicate at src/lib.rs:{line}"))?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath);
        assert!(finding.related_tests.is_empty());
        Ok(finding)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn collision_preserves_preexisting_root_and_sentinel() -> Result<(), String> {
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
    let root = Fixture::next_root();
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let owner = Fixture(root);
    let sentinel = owner.0.join("sentinel");
    std::fs::write(&sentinel, b"preexisting owner").map_err(|error| error.to_string())?;
    std::fs::write(owner.0.join("src"), b"preexisting file").map_err(|error| error.to_string())?;

    let attempt = Fixture::with_root(owner.0.clone(), SOURCE, DIFF);
    assert!(
        attempt.is_err(),
        "a collision must refuse the existing root"
    );
    assert!(sentinel.exists(), "failed setup deleted a preexisting root");
    assert_eq!(
        std::fs::read(&sentinel).map_err(|error| error.to_string())?,
        b"preexisting owner"
    );
    assert_eq!(
        std::fs::read(owner.0.join("src")).map_err(|error| error.to_string())?,
        b"preexisting file"
    );
    Ok(())
}

#[test]
fn raw_doc_string_cannot_invent_a_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(3)?;
    assert_eq!(finding.static_limit_kind, None);
    assert!(
        !finding
            .recommended_next_step
            .as_deref()
            .unwrap_or_default()
            .contains("language boundary")
    );
    Ok(())
}

#[test]
fn raw_doc_string_cannot_hide_a_real_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(8)?;
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved)
    );
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .unwrap_or_default()
            .contains("language boundary")
    );
    Ok(())
}

#[test]
fn comment_cannot_invent_a_binding_attribute() -> Result<(), String> {
    let finding = Fixture::create()?.finding(13)?;
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}

#[derive(serde::Deserialize)]
struct AttributeCase {
    name: String,
    attribute: String,
    ffi: bool,
}

#[test]
fn attribute_paths_ignore_trivia_and_normalize_raw_identifiers() -> Result<(), String> {
    let cases: Vec<AttributeCase> = serde_json::from_str(include_str!(
        "../../../fixtures/rust_ffi_attribute_literals/attribute-cases.json"
    ))
    .map_err(|error| error.to_string())?;
    assert_eq!(cases.len(), 33, "nonempty retained category matrix");
    let mut failures = Vec::new();
    for case in cases {
        let source = format!(
            "{}\npub fn analyzed(amount: u64) -> u64 {{\n    if amount >= 100 {{ 5 }} else {{ 0 }}\n}}\n",
            case.attribute
        );
        let parsed = ra_ap_syntax::SourceFile::parse(&source, ra_ap_syntax::Edition::CURRENT);
        assert!(
            parsed.errors().is_empty(),
            "{}: {:?}",
            case.name,
            parsed.errors()
        );
        let lines = source.lines().count();
        let line = case.attribute.lines().count() + 2;
        let mut diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{lines} +1,{lines} @@\n"
        );
        for text in source.lines() {
            if text.contains("if amount >= 100") {
                diff.push_str(&format!("-{}\n+{text}\n", text.replace(">=", ">")));
            } else {
                diff.push_str(&format!(" {text}\n"));
            }
        }
        let finding = Fixture::with_input(&source, &diff)?.finding_among(line, 1)?;
        let expected = case
            .ffi
            .then_some(StaticLimitKind::CrossLanguageOracleVisibilityUnresolved);
        println!(
            "{}: expected {expected:?}, actual {:?}",
            case.name, finding.static_limit_kind
        );
        if finding.static_limit_kind != expected {
            failures.push(format!(
                "{}: expected {expected:?}, got {:?}",
                case.name, finding.static_limit_kind
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

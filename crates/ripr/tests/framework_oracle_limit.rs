//! Public-API controls for framework oracles ripr does not evaluate
//! (RIPR-SPEC-0240).
//!
//! A related test that checks the owner through an environment variable it
//! sets, a mockall closure that reads its arguments, an expect-test debug
//! literal, or an insta snapshot of a field no redaction names reads as
//! `static_unknown` under `rust_framework_oracle_unresolved`. Each control
//! below changes one thing about the shape and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-framework-oracle-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"framework-oracle\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A one-line edit of `line` in `src/lib.rs`.
fn diff(line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{before}
+{after}
"
    )
}

/// Every finding on `line`: one edit can carry several probes.
fn findings_on(lib: &str, diff: &str, line: usize) -> Result<Vec<Finding>, String> {
    let findings = TempRepo::create(lib, diff)?.findings()?;
    let on_line: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if on_line.is_empty() {
        return Err(format!("premise: a finding on line {line}"));
    }
    Ok(on_line)
}

const LIMIT: &str = "rust_framework_oracle_unresolved";

fn is_gap(finding: &Finding) -> bool {
    matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    )
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

/// Some finding on the line carries the limit and none is still a gap.
fn withheld(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(is_gap) || !findings.iter().any(limited) {
        return Err(format!(
            "{label}: expected the framework limit, got {:?}",
            findings
                .iter()
                .map(|f| (
                    f.probe.family.as_str(),
                    f.class.as_str(),
                    f.static_limit_kind
                ))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// No finding carries the limit and some finding is still a gap.
fn keeps_gap(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap kept, got {:?}",
            findings
                .iter()
                .map(|f| (
                    f.probe.family.as_str(),
                    f.class.as_str(),
                    f.static_limit_kind
                ))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn env_source(owner_read: &str, test_body: &str) -> String {
    format!(
        "pub fn retries() -> u32 {{
    match {owner_read} {{
        Ok(text) => text.parse().unwrap_or(3),
        Err(_) => 1 + 2,
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn retries_read_the_environment() {{
        {test_body}
    }}
}}
"
    )
}

const ENV_EXACT: &str = "std::env::set_var(\"APP_RETRIES\", \"7\");
        assert_eq!(retries(), 7);
        std::env::remove_var(\"APP_RETRIES\");
        assert_eq!(retries(), 3);";

#[test]
fn an_arm_chosen_by_the_environment_is_withheld() -> Result<(), String> {
    let edit = diff(4, "        Err(_) => 3,", "        Err(_) => 1 + 2,");
    withheld(
        &env_source("std::env::var(\"APP_RETRIES\")", ENV_EXACT),
        &edit,
        4,
        "environment arm",
    )
}

#[test]
fn an_environment_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    let edit = diff(4, "        Err(_) => 3,", "        Err(_) => 1 + 2,");
    // The owner reads no environment variable.
    keeps_gap(
        &env_source("\"7\".parse::<u32>().map_err(|_| ())", ENV_EXACT),
        &edit,
        4,
        "owner without env",
    )?;
    // The test never changes the environment.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            "assert_eq!(retries(), 3);",
        ),
        &edit,
        4,
        "test without set_var",
    )?;
    // The only assertion is relational.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            "std::env::remove_var(\"APP_RETRIES\");\n        assert!(retries() > 0);",
        ),
        &edit,
        4,
        "relational oracle",
    )
}

fn expect_source(assertion: &str) -> String {
    format!(
        "pub fn tokenize(src: &str) -> Vec<u32> {{
    src.split_whitespace()
        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))
        .collect()
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use expect_test::expect;

    #[test]
    fn tokens_debug_inline() {{
        let tokens = tokenize(\"7\");
        {assertion}
    }}
}}
"
    )
}

#[test]
fn an_expect_debug_literal_is_withheld() -> Result<(), String> {
    let edit = diff(
        3,
        "        .map(|w| w.parse::<u32>().map_or(0, |n| n * 10))",
        "        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))",
    );
    withheld(
        &expect_source(
            "expect![[r#\"\n            [\n                70,\n            ]\n        \"#]]\n        .assert_debug_eq(&tokens);",
        ),
        &edit,
        3,
        "expect debug",
    )
}

#[test]
fn an_expect_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    let edit = diff(
        3,
        "        .map(|w| w.parse::<u32>().map_or(0, |n| n * 10))",
        "        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))",
    );
    // An empty expectation is another rule's case.
    keeps_gap(
        &expect_source("expect![[\"\"]].assert_debug_eq(&tokens);"),
        &edit,
        3,
        "empty expectation",
    )?;
    // A smoke assertion is not a snapshot.
    keeps_gap(
        &expect_source("assert!(!tokens.is_empty());"),
        &edit,
        3,
        "no expectation",
    )
}

fn insta_source(assertion: &str) -> String {
    format!(
        "pub struct Record {{
    pub id: String,
    pub items: usize,
}}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {{
    Record {{
        id: format!(\"{{}}-1\", prefix),
        items: items.iter().count(),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn record_snapshot() {{
        let record = make_record(\"ord\", &[1, 2]);
        {assertion}
    }}
}}
"
    )
}

#[test]
fn an_unredacted_insta_field_is_withheld() -> Result<(), String> {
    let edit = diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    );
    for assertion in [
        "insta::assert_json_snapshot!(record, { \".id\" => \"[id]\" });",
        "insta::assert_debug_snapshot!(record);",
    ] {
        withheld(&insta_source(assertion), &edit, 9, assertion)?;
    }
    Ok(())
}

#[test]
fn an_insta_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    // The redaction names the changed field.
    let items = diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    );
    keeps_gap(
        &insta_source("insta::assert_json_snapshot!(record, { \".items\" => \"[n]\" });"),
        &items,
        9,
        "redacted field",
    )?;
    // The redaction names the changed id.
    let id = diff(
        8,
        "        id: format!(\"{}-0\", prefix),",
        "        id: format!(\"{}-1\", prefix),",
    );
    keeps_gap(
        &insta_source("insta::assert_json_snapshot!(record, { \".id\" => \"[id]\" });"),
        &id,
        8,
        "redacted id",
    )?;
    // An inline snapshot is the other insta rules' case.
    keeps_gap(
        &insta_source("insta::assert_debug_snapshot!(record, @\"Record\");"),
        &items,
        9,
        "inline snapshot",
    )
}

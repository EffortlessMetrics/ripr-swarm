//! Public-API controls for an edit to an inline branch value.
//!
//! `if n > 5 { 1 } else { 2 }` becoming `if n > 5 { 3 } else { 2 }` changes
//! the returned value, not the condition. The line's finding names the value,
//! and a test pinning the returned value exposes it; no boundary pairing for
//! `n > 5` is asked for. An edit to the condition on the same line still
//! reads as the predicate.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-inline-branch-value-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"inline\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn diff(before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,1 @@\n-{before}\n+{after}\n"
    )
}

fn lib(owner: &str, assertion: &str) -> String {
    format!(
        "{owner}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn pins_the_value() {{\n        {assertion}\n    }}\n}}\n"
    )
}

fn line_one(
    owner_before: &str,
    owner_after: &str,
    assertion: &str,
) -> Result<Vec<Finding>, String> {
    let found: Vec<Finding> = TempRepo::create(
        &lib(owner_after, assertion),
        &diff(owner_before, owner_after),
    )?
    .findings()?
    .into_iter()
    .filter(|finding| finding.probe.location.line == 1)
    .collect();
    if found.is_empty() {
        return Err("premise: a finding on the changed line".to_string());
    }
    Ok(found)
}

fn summary(found: &[Finding]) -> Vec<(ProbeFamily, String, ExposureClass)> {
    found
        .iter()
        .map(|finding| {
            (
                finding.probe.family.clone(),
                finding.probe.expression.clone(),
                finding.class.clone(),
            )
        })
        .collect()
}

#[test]
fn an_inline_branch_value_edit_is_a_return_value_not_a_boundary() -> Result<(), String> {
    for (before, after, assertion) in [
        (
            "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 2 } }",
            "pub fn f(n: u32) -> u32 { if n > 5 { 3 } else { 2 } }",
            "assert_eq!(f(9), 1);",
        ),
        (
            "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 2 } }",
            "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 4 } }",
            "assert_eq!(f(0), 2);",
        ),
    ] {
        let found = line_one(before, after, assertion)?;
        let rows = summary(&found);
        if rows
            .iter()
            .any(|(family, _, _)| *family == ProbeFamily::Predicate)
        {
            return Err(format!(
                "{after:?}: unchanged condition read as a predicate: {rows:?}"
            ));
        }
        if !rows.iter().any(|(family, _, class)| {
            *family == ProbeFamily::ReturnValue && *class == ExposureClass::Exposed
        }) {
            return Err(format!(
                "{after:?}: the pinned value must be exposed: {rows:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_threshold_assertion_misses_the_changed_branch_value() -> Result<(), String> {
    let found = line_one(
        "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 2 } }",
        "pub fn f(n: u32) -> u32 { if n > 5 { 3 } else { 2 } }",
        "assert_eq!(f(5), 2);",
    )?;
    let rows = summary(&found);
    assert!(
        matches!(rows.as_slice(), [(ProbeFamily::ReturnValue, value, class)]
            if value == "3" && *class != ExposureClass::Exposed),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn an_inline_condition_edit_stays_the_predicate() -> Result<(), String> {
    let found = line_one(
        "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 2 } }",
        "pub fn f(n: u32) -> u32 { if n >= 5 { 1 } else { 2 } }",
        "assert_eq!(f(9), 1);",
    )?;
    let rows = summary(&found);
    if !rows
        .iter()
        .any(|(family, _, _)| *family == ProbeFamily::Predicate)
    {
        return Err(format!(
            "the changed condition must stay a predicate: {rows:?}"
        ));
    }
    if rows
        .iter()
        .any(|(family, _, _)| *family == ProbeFamily::ReturnValue)
    {
        return Err(format!(
            "unchanged branch values must not get a row: {rows:?}"
        ));
    }
    Ok(())
}

#[test]
fn an_added_inline_if_keeps_its_single_predicate() -> Result<(), String> {
    let owner = "pub fn f(n: u32) -> u32 { if n > 5 { 1 } else { 2 } }";
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nnew file mode 100644\n--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1,1 @@\n+{owner}\n"
    );
    let found: Vec<Finding> = TempRepo::create(&lib(owner, "assert_eq!(f(9), 1);"), &diff)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 1)
        .collect();
    let rows = summary(&found);
    let families = rows
        .iter()
        .map(|(family, _, _)| family.clone())
        .collect::<Vec<_>>();
    if families != [ProbeFamily::Predicate] {
        return Err(format!(
            "an added inline if must keep one predicate row: {rows:?}"
        ));
    }
    Ok(())
}

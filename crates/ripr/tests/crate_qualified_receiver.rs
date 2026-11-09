//! A crate-qualified constructor receiver (`krate::Rng::new()`) keeps its call
//! path to the owner's own method, and only for the owner's own crate (#14036).

use ripr::domain::{RelationConfidence, RelationReason};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const OWNER: &str = "pub struct Rng(u32);\nimpl Rng {\n    pub fn new() -> Rng {\n        Rng(0)\n    }\n    pub fn digit(&mut self, base: u32) -> u32 {\n        if base > 9 { 2 } else { 1 }\n    }\n}\n";

/// The predicate finding of `digit` with `test` as the only integration test.
fn finding(test: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-crate-qualified-receiver-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"rg\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", OWNER)?;
    write("tests/t.rs", test)?;
    let lines: Vec<&str> = OWNER.lines().collect();
    let changed_line = lines
        .iter()
        .position(|line| line.contains("base > 9"))
        .ok_or("owner has no `base > 9` line")?
        + 1;
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("base > 9", "base >= 10"))
            } else {
                format!(" {line}\n")
            }
        })
        .collect();
    write(
        "diff.patch",
        &format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n{hunk}",
            n = lines.len()
        ),
    )?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == changed_line
                && finding.probe.family == ripr::domain::ProbeFamily::Predicate
        })
        .ok_or_else(|| "no predicate finding on the changed line".to_string())
}

fn related_reason(
    found: &Finding,
    name: &str,
) -> Result<(Option<RelationReason>, Option<RelationConfidence>), String> {
    found
        .related_tests
        .iter()
        .find(|test| test.name == name)
        .map(|test| (test.relation_reason, test.relation_confidence))
        .ok_or_else(|| format!("missing related test {name}: {found:#?}"))
}

#[test]
fn own_crate_qualified_constructor_receiver_is_a_direct_owner_call() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n    assert_eq!(rng.digit(9), 1);\n}\n";
    let found = finding(test)?;
    assert_eq!(
        related_reason(&found, "rng_digit")?,
        (
            Some(RelationReason::DirectOwnerCall),
            Some(RelationConfidence::High)
        ),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn imported_constructor_receiver_is_the_unchanged_control() -> Result<(), String> {
    let test = "use rg::Rng;\n#[test]\nfn rng_digit() {\n    let mut rng = Rng::new();\n    assert_eq!(rng.digit(10), 2);\n    assert_eq!(rng.digit(9), 1);\n}\n";
    let found = finding(test)?;
    assert_eq!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn foreign_crate_qualified_receiver_keeps_no_direct_call() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = other_crate::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "a foreign crate's same-named type is not the owner: {found:#?}"
    );
    Ok(())
}

#[test]
fn local_module_shadowing_the_crate_name_keeps_no_direct_call() -> Result<(), String> {
    let test = "mod rg {\n    pub struct Rng;\n    impl Rng {\n        pub fn new() -> Rng { Rng }\n        pub fn digit(&mut self, _base: u32) -> u32 { 0 }\n    }\n}\n#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 0);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "a local `mod rg` is not the owner's crate: {found:#?}"
    );
    Ok(())
}

#[test]
fn enum_variant_spelling_is_not_a_constructor_receiver() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Mode::Rng(1);\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    Ok(())
}

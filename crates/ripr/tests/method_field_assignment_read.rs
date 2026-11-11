//! A `&mut self` method that assigns a field (`self.n = 1`) is exposed by a
//! test that calls it on a receiver binding and then reads that binding's
//! field exactly, including through a crate-qualified struct literal receiver
//! (`let mut c = krate::Counter { n: 5 }`) and a field name too short to be a
//! token match (#14407). Every weaker shape keeps the gap open.

use ripr::domain::RelationReason;
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

const OWNER: &str = "pub struct Counter {\n    pub n: u32,\n    pub m: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n";

/// The `field_construction` finding of the changed `self.n = 1` (base
/// `self.n = 0`) with `test` written at `test_path`.
fn finding_at(owner_source: &str, test_path: &str, test: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-method-field-assignment-{}-{sequence}",
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
        "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", owner_source)?;
    if test_path != "src/lib.rs" {
        write(test_path, test)?;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_line = lines
        .iter()
        .position(|line| line.contains("self.n = 1;"))
        .ok_or("owner has no `self.n = 1;` line")?
        + 1;
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("self.n = 1;", "self.n = 0;"))
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
                && finding.probe.family == ripr::domain::ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| "no field_construction finding on the changed line".to_string())
}

fn finding(test: &str) -> Result<Finding, String> {
    finding_at(OWNER, "tests/t.rs", test)
}

fn relation(found: &Finding) -> Option<RelationReason> {
    found
        .related_tests
        .iter()
        .find(|test| test.name == "reference")
        .and_then(|test| test.relation_reason)
}

fn test_body(lines: &str) -> String {
    format!("#[test]\nfn reference() {{\n{lines}\n}}\n")
}

#[test]
fn crate_qualified_literal_receiver_field_read_is_exposed() -> Result<(), String> {
    let found = finding(&test_body(
        "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
    ))?;
    assert_eq!(
        relation(&found),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn imported_literal_receiver_short_field_read_is_exposed() -> Result<(), String> {
    // The unqualified spelling already related the test; only the short field
    // name kept the exact read from confirming observation.
    let found = finding(&format!(
        "use probe::Counter;\n{}",
        test_body(
            "    let mut c = Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);"
        )
    ))?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn in_crate_unit_test_short_field_read_is_exposed() -> Result<(), String> {
    let owner = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5, m: 0 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&owner, "src/lib.rs", "")?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn weaker_reads_keep_the_field_write_gap_open() -> Result<(), String> {
    for (case, lines) in [
        (
            "never calls the owner",
            "    let c = probe::Counter { n: 1, m: 0 };\n    assert_eq!(c.n, 1);",
        ),
        (
            "reads another binding",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    let d = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(d.n, 1);",
        ),
        (
            "reads another field",
            "    let mut c = probe::Counter { n: 5, m: 1 };\n    c.reset();\n    assert_eq!(c.m, 1);",
        ),
        (
            "rewrites the field before the read",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    c.n = 1;\n    assert_eq!(c.n, 1);",
        ),
        (
            "rebinds the receiver before the read",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    let c = probe::Counter { n: 1, m: 0 };\n    assert_eq!(c.n, 1);",
        ),
        (
            "builds a foreign crate's namesake",
            "    let mut c = other::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    ] {
        let found = finding(&test_body(lines))?;
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "{case} must not expose the write: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn local_module_shadowing_the_crate_name_gets_no_credit() -> Result<(), String> {
    let test = "mod probe {\n    pub struct Counter {\n        pub n: u32,\n    }\n    impl Counter {\n        pub fn reset(&mut self) {\n            self.n = 1;\n        }\n    }\n}\n#[test]\nfn reference() {\n    let mut c = probe::Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        relation(&found),
        Some(RelationReason::DirectOwnerCall),
        "a local `mod probe` is not the owner's crate: {found:#?}"
    );
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

//! Direct collection reads must not credit `exposed` when the comparison
//! cannot discriminate the changed push (#7135, causes 2 and 3).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn non_discriminating_collection_reads_never_credit_exposed() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("collection_read_nd");
    let production = "pub struct Bag { pub items: Vec<u32> }\nimpl Bag {\n    pub fn new() -> Self { Bag { items: Vec::new() } }\n    pub fn add(&mut self, x: u32) {\n        self.items.push(x);\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,3 +4,3 @@ impl Bag {\n     pub fn add(&mut self, x: u32) {\n-        self.items.push(x + 1);\n+        self.items.push(x);\n     }\n";
    // (name, test body, credited)
    let cases = [
        (
            "control_exact_items",
            "let mut c = Bag::new();\nc.add(20);\nassert_eq!(c.items, vec![20]);",
            true,
        ),
        (
            "snapshot_of_same_state",
            "let mut c = Bag::new();\nc.add(20);\nlet snap = c.items.clone();\nassert_eq!(c.items, snap);",
            false,
        ),
        (
            "twin_instance_ran_owner",
            "let mut c = Bag::new();\nc.add(20);\nlet mut d = Bag::new();\nd.add(20);\nassert_eq!(c.items, d.items);",
            false,
        ),
        (
            "independent_seeded_twin_keeps_credit",
            "let mut c = Bag::new();\nc.add(20);\nlet d = Bag { items: vec![20] };\nassert_eq!(c.items, d.items);",
            true,
        ),
        (
            "asymmetric_twin_histories_keep_credit",
            "let mut c = Bag::new();\nc.add(20);\nc.add(20);\nlet mut d = Bag::new();\nd.items.push(20);\nd.add(20);\nassert_eq!(c.items, d.items);",
            true,
        ),
        (
            "owner_on_rhs_of_or",
            "let mut c = Bag::new();\nlet _ = true || { c.add(20); true };\nassert_eq!(c.items, vec![]);",
            false,
        ),
    ];
    let mut wrong = Vec::new();
    for (name, body, credited) in cases {
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn bag_observed() {{\n{body}\n}}\n}}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"bag_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let classes: Vec<String> = report
            .findings
            .iter()
            .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
            .collect();
        let any_exposed = report
            .findings
            .iter()
            .any(|f| f.class.as_str() == "exposed");
        if any_exposed != credited {
            wrong.push(format!("{name}: {classes:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// A local `Vec` filled through the owner never credits a self-comparison or
/// a twin either (#7135); the control form is not credited at all today, so
/// this pins only the negative direction.
#[test]
fn local_collection_twins_never_credit_exposed() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("collection_read_nd_direct");
    let production = "pub fn fill(v: &mut Vec<u32>, x: u32) {\n    v.push(x);\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn fill(v: &mut Vec<u32>, x: u32) {\n-    v.push(x + 1);\n+    v.push(x);\n }\n";
    let cases = [
        (
            "snapshot_of_same_state",
            "let mut a = Vec::new();\nfill(&mut a, 20);\nlet snap = a.clone();\nassert_eq!(a, snap);",
            false,
        ),
        (
            "twin_local_ran_owner",
            "let mut a = Vec::new();\nfill(&mut a, 20);\nlet mut b = Vec::new();\nfill(&mut b, 20);\nassert_eq!(a, b);",
            false,
        ),
    ];
    let mut wrong = Vec::new();
    for (name, body, credited) in cases {
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn fill_observed() {{\n{body}\n}}\n}}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"fill_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let classes: Vec<String> = report
            .findings
            .iter()
            .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
            .collect();
        let any_exposed = report
            .findings
            .iter()
            .any(|f| f.class.as_str() == "exposed");
        if report.findings.is_empty() || any_exposed != credited {
            wrong.push(format!("{name}: {classes:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

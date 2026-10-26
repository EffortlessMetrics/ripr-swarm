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
            "readonly_use_does_not_make_twins_independent",
            "let mut c = Bag::new();\nc.add(20);\nlet mut d = Bag::new();\nd.add(20);\nlet _ = c.items.len();\nassert_eq!(c.items, d.items);",
            false,
        ),
        (
            "readonly_use_of_a_distinct_binding_is_not_history",
            "let mut c = Bag::new();\nc.add(20);\nlet mut cc = Bag::new();\nlet mut d = Bag::new();\nd.add(20);\nlet _ = cc.items.len();\nassert_eq!(c.items, d.items);",
            false,
        ),
        (
            "identically_seeded_twins",
            "let mut c = Bag::new();\nc.items.push(1);\nc.add(20);\nlet mut d = Bag::new();\nd.items.push(1);\nd.add(20);\nassert_eq!(c.items, d.items);",
            false,
        ),
        (
            "twin_rewritten_after_owner_keeps_credit",
            "let mut c = Bag::new();\nc.add(20);\nlet mut d = Bag::new();\nd.add(20);\nd.items.clear();\nd.items.push(20);\nassert_eq!(c.items, d.items);",
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
        if report.findings.is_empty() || any_exposed != credited {
            wrong.push(format!("{name}: {classes:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}

/// Two owner-fed bindings driven with different arguments diverge when the
/// owner changes, so the comparison keeps its credit (#7135).
#[test]
fn argument_sensitive_twin_comparison_keeps_credit() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("collection_read_nd_args");
    let production = "pub struct Bag { pub items: Vec<u32> }\nimpl Bag {\n    pub fn new() -> Self { Bag { items: Vec::new() } }\n    pub fn add(&mut self, x: u32) {\n        self.items.push(x % 2);\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,3 +4,3 @@ impl Bag {\n     pub fn add(&mut self, x: u32) {\n-        self.items.push(x % 3);\n+        self.items.push(x % 2);\n     }\n";
    let body = "let mut c = Bag::new();\nc.add(2);\nlet mut d = Bag::new();\nd.add(4);\nassert_eq!(c.items, d.items);";
    let source = format!(
        "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn bag_observed() {{\n{body}\n}}\n}}\n"
    );
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"bag_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
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
    assert!(!report.findings.is_empty());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.class.as_str() == "exposed"),
        "{:?}",
        report
            .findings
            .iter()
            .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// A conditional owner (`push` only when empty) with one side seeded: equal
/// call counts, but the seed makes the sides diverge. The seed spelling must
/// not matter (#7135).
#[test]
fn seeded_conditional_owner_twin_keeps_credit_for_any_spelling() -> Result<(), String> {
    let production = "pub struct Bag { pub items: Vec<u32> }\nimpl Bag {\n    pub fn new() -> Self { Bag { items: Vec::new() } }\n    pub fn add(&mut self, x: u32) {\n        if self.items.is_empty() {\n            self.items.push(x);\n        }\n    }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,5 +5,5 @@ impl Bag {\n         if self.items.is_empty() {\n-            self.items.push(x + 1);\n+            self.items.push(x);\n         }\n     }\n";
    let mut wrong = Vec::new();
    for (name, seed) in [
        ("tight", "c.items.push(20);"),
        ("spaced", "c . items.push(20);"),
    ] {
        let body = format!(
            "let mut c = Bag::new();\nlet mut d = Bag::new();\n{seed}\nc.add(20);\nd.add(20);\nassert_eq!(c.items, d.items);"
        );
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn bag_observed() {{\n{body}\n}}\n}}\n"
        );
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("collection_read_nd_cond")
            .join(name);
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
        if report.findings.is_empty()
            || !report
                .findings
                .iter()
                .any(|f| f.class.as_str() == "exposed")
        {
            wrong.push(format!(
                "{name}: {:?}",
                report
                    .findings
                    .iter()
                    .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
                    .collect::<Vec<_>>()
            ));
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

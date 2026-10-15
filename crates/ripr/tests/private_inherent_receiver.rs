//! Production-path relations for a private two-target/type-collision owner (#7415).
//! The same fixture's five native subjects compile and run independently.

use ripr::domain::{RelationConfidence, RelationReason};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const FILES: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        r###"[package]
name = "private-target-attribution"
version = "0.0.0"
edition = "2024"

[workspace]

[dependencies]
serde_json = "=1.0.150"

[[bin]]
name = "xtask"
path = "xtask/main.rs"
"###,
    ),
    (
        "src/main.rs",
        r###"fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn unrelated_main_json_object() {
        let serialized = serde_json::json!({"key": 1});
        assert_eq!(serialized.as_object().unwrap().len(), 1);
    }
}
"###,
    ),
    (
        "src/reader.rs",
        r###"use std::collections::BTreeMap;

pub(crate) enum JsonValue {
    Object(BTreeMap<String, i32>),
    Null,
}

impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<&BTreeMap<String, i32>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_target_actual_object() {
        let value = JsonValue::Object(BTreeMap::from([("key".to_owned(), 1)]));
        assert_eq!(value.as_object().unwrap().len(), 1);
    }

    #[test]
    fn same_target_actual_null() {
        let value = JsonValue::Null;
        assert_eq!(value.as_object(), None);
    }

    #[test]
    fn same_target_wrong_json_type() {
        let value = serde_json::json!({"key": 1});
        assert_eq!(value.as_object().unwrap().len(), 1);
    }
}
"###,
    ),
    (
        "xtask/main.rs",
        r###"#[path = "../src/reader.rs"]
mod reader;

fn main() {
    let _ = reader::JsonValue::Null.as_object();
    let _ = reader::JsonValue::Object(std::collections::BTreeMap::new()).as_object();
}
"###,
    ),
    (
        "src/lib.rs",
        r###"pub fn public_value(input: u32) -> u32 {
    input + 1
}
"###,
    ),
    (
        "tests/public_library.rs",
        r###"#[test]
fn legitimate_cross_target_public_call() {
    assert_eq!(private_target_attribution::public_value(1), 2);
}
"###,
    ),
    (
        "diff.patch",
        r###"diff --git a/src/reader.rs b/src/reader.rs
--- a/src/reader.rs
+++ b/src/reader.rs
@@ -8,7 +8,7 @@
 impl JsonValue {
     pub(crate) fn as_object(&self) -> Option<&BTreeMap<String, i32>> {
         match self {
-            Self::Object(_) => None,
+            Self::Object(value) => Some(value),
             _ => None,
         }
     }
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn public_value(input: u32) -> u32 {
-    input + 0
+    input + 1
 }
"###,
    ),
];

struct Fixture(PathBuf);

impl Fixture {
    fn create() -> Result<Self, String> {
        Self::with_files(FILES)
    }

    fn with_files(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-private-inherent-receiver-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir(&fixture.0).map_err(|error| error.to_string())?;
        for (relative, text) in files {
            let path = fixture.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(path, text).map_err(|error| error.to_string())?;
        }
        Ok(fixture)
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(
            output.summary.probes, 3,
            "nonempty complete probe inventory"
        );
        assert_eq!(output.findings.len(), 3, "all original subjects retained");
        Ok(output.findings)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unique_private_inherent_receiver_keeps_real_calls_and_refuses_serde_decoys() -> Result<(), String>
{
    let fixture = Fixture::create()?;
    let findings = fixture.findings()?;
    let reader: Vec<_> = findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("src/reader.rs"))
        .collect();
    assert_eq!(reader.len(), 2, "both private-owner probes must remain");
    for finding in reader {
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        for name in ["same_target_actual_object", "same_target_actual_null"] {
            let related = finding
                .related_tests
                .iter()
                .find(|test| test.name == name)
                .ok_or_else(|| format!("missing actual owner test {name}: {finding:?}"))?;
            assert_eq!(related.file, PathBuf::from("src/reader.rs"));
            assert_eq!(
                related.relation_reason,
                Some(RelationReason::DirectOwnerCall)
            );
            assert_eq!(related.relation_confidence, Some(RelationConfidence::High));
        }
        for name in ["unrelated_main_json_object", "same_target_wrong_json_type"] {
            let related = finding
                .related_tests
                .iter()
                .find(|test| test.name == name)
                .ok_or_else(|| format!("missing retained name-only decoy {name}: {finding:?}"))?;
            assert_eq!(
                related.relation_reason,
                Some(RelationReason::WeakTokenSubstring),
                "a serde_json::Value call cannot reach private JsonValue::as_object"
            );
            assert_eq!(related.relation_confidence, Some(RelationConfidence::Low));
        }
    }
    let public = findings
        .iter()
        .find(|finding| {
            finding.probe.location.file.ends_with("src/lib.rs")
                && finding.probe.family == ProbeFamily::ReturnValue
        })
        .ok_or("missing public-library production probe")?;
    assert_eq!(public.class, ExposureClass::Exposed);
    let test = public
        .related_tests
        .iter()
        .find(|test| test.name == "legitimate_cross_target_public_call")
        .ok_or("missing legitimate cross-target library call")?;
    assert_eq!(test.file, PathBuf::from("tests/public_library.rs"));
    assert_eq!(test.relation_reason, Some(RelationReason::DirectOwnerCall));
    assert_eq!(test.relation_confidence, Some(RelationConfidence::High));
    Ok(())
}

#[test]
fn public_inherent_method_keeps_cross_target_receiver_attribution() -> Result<(), String> {
    let fixture = Fixture::create()?;
    let production = "pub struct PublicReader { pub size: u32 }\nimpl PublicReader {\n    pub fn object_size(&self) -> u32 { self.size + 1 }\n}\n";
    let test = "use private_target_attribution::PublicReader;\n#[test]\nfn public_inherent_owner_call() {\n    let value = PublicReader { size: 1 };\n    assert_eq!(value.object_size(), 2);\n}\n";
    std::fs::write(fixture.0.join("src/lib.rs"), production).map_err(|e| e.to_string())?;
    std::fs::write(fixture.0.join("tests/public_library.rs"), test).map_err(|e| e.to_string())?;
    std::fs::write(fixture.0.join("diff.patch"), "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub struct PublicReader { pub size: u32 }\n impl PublicReader {\n-    pub fn object_size(&self) -> u32 { self.size + 0 }\n+    pub fn object_size(&self) -> u32 { self.size + 1 }\n }\n").map_err(|e| e.to_string())?;
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<_> = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.location.file.ends_with("src/lib.rs")
                && finding.probe.family == ProbeFamily::ReturnValue
        })
        .collect();
    assert_eq!(
        findings.len(),
        1,
        "the changed public inherent method exists"
    );
    let related = findings[0]
        .related_tests
        .iter()
        .find(|test| test.name == "public_inherent_owner_call")
        .ok_or("missing genuine cross-target public inherent call")?;
    assert_eq!(related.file, PathBuf::from("tests/public_library.rs"));
    assert_eq!(
        related.relation_reason,
        Some(RelationReason::DirectOwnerCall)
    );
    assert_eq!(related.relation_confidence, Some(RelationConfidence::High));
    Ok(())
}

fn decoy_relations_are_name_only(reader_source: &str) -> Result<(), String> {
    let fixture = Fixture::create()?;
    std::fs::write(fixture.0.join("src/reader.rs"), reader_source).map_err(|e| e.to_string())?;
    let findings = fixture.findings()?;
    let reader: Vec<_> = findings
        .iter()
        .filter(|f| f.probe.location.file.ends_with("src/reader.rs"))
        .collect();
    assert_eq!(reader.len(), 2);
    for finding in reader {
        let decoys: Vec<_> = finding
            .related_tests
            .iter()
            .filter(|test| test.name == "same_target_wrong_json_type")
            .collect();
        assert!(!decoys.is_empty(), "name-only rows remain visible");
        for decoy in decoys {
            assert_eq!(
                decoy.relation_reason,
                Some(RelationReason::WeakTokenSubstring)
            );
            assert_eq!(decoy.relation_confidence, Some(RelationConfidence::Low));
        }
        for name in ["same_target_actual_object", "same_target_actual_null"] {
            assert!(finding.related_tests.iter().any(|test| test.name == name
                && test.relation_reason == Some(RelationReason::DirectOwnerCall)));
        }
    }
    Ok(())
}

#[test]
fn raw_call_line_comment_cannot_supply_inherent_receiver() -> Result<(), String> {
    let source = FILES
        .iter()
        .find(|(path, _)| *path == "src/reader.rs")
        .ok_or("missing reader fixture")?
        .1;
    decoy_relations_are_name_only(&source.replace(
        "assert_eq!(value.as_object().unwrap().len(), 1);\n    }\n}\n",
        "assert_eq!(value.as_object().unwrap().len(), 1); // JsonValue::as_object()\n    }\n}\n",
    ))
}

#[test]
fn expired_inner_binding_cannot_type_outer_inherent_receiver() -> Result<(), String> {
    let source = FILES
        .iter()
        .find(|(path, _)| *path == "src/reader.rs")
        .ok_or("missing reader fixture")?
        .1;
    decoy_relations_are_name_only(&source.replace(
        "let value = serde_json::json!({\"key\": 1});",
        "let value = serde_json::json!({\"key\": 1});\n        { let value = JsonValue::Null; assert!(matches!(value, JsonValue::Null)); }",
    ))
}

#[test]
fn expired_foreign_shadow_cannot_hide_actual_outer_receiver() -> Result<(), String> {
    let fixture = Fixture::create()?;
    let source = FILES
        .iter()
        .find(|(path, _)| *path == "src/reader.rs")
        .ok_or("missing reader fixture")?
        .1;
    let source = source.replace(
        "let value = JsonValue::Null;",
        "let value = JsonValue::Null;\n        { let value = serde_json::Value::Null; let _ = value; }",
    );
    std::fs::write(fixture.0.join("src/reader.rs"), source).map_err(|error| error.to_string())?;
    let findings = fixture.findings()?;
    let reader: Vec<_> = findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with("src/reader.rs"))
        .collect();
    assert_eq!(reader.len(), 2, "both private-owner subjects retained");
    for finding in reader {
        let actual = finding
            .related_tests
            .iter()
            .find(|test| test.name == "same_target_actual_null")
            .ok_or("missing actual outer receiver test")?;
        assert_eq!(
            actual.relation_reason,
            Some(RelationReason::DirectOwnerCall)
        );
        assert_eq!(actual.relation_confidence, Some(RelationConfidence::High));
    }
    Ok(())
}

#[test]
fn while_let_pattern_cannot_borrow_outer_inherent_receiver_type() -> Result<(), String> {
    let source = FILES
        .iter()
        .find(|(path, _)| *path == "src/reader.rs")
        .ok_or("missing reader fixture")?
        .1;
    decoy_relations_are_name_only(&source.replace(
        "let value = serde_json::json!({\"key\": 1});\n        assert_eq!(value.as_object().unwrap().len(), 1);",
        "let value = JsonValue::Null;\n        let mut values = Some(serde_json::json!({\"key\": 1}));\n        while let Some(value) = values.take() {\n            assert_eq!(value.as_object().unwrap().len(), 1);\n        }",
    ))
}

#[test]
fn scope_items_and_assertion_operand_items_cannot_borrow_outer_receiver_type() -> Result<(), String>
{
    let source = FILES
        .iter()
        .find(|(path, _)| *path == "src/reader.rs")
        .ok_or("missing reader fixture")?
        .1;
    let source = source.replace(
        "#[cfg(test)]\nmod tests",
        "trait ForeignView { fn as_object(&self) -> Option<()> { None } }\nimpl<T> ForeignView for T {}\n\n#[cfg(test)]\nmod tests",
    );
    for decoy in [
        "let value = JsonValue::Null;\n        assert_eq!({ struct value; value.as_object() }, None);",
        "let value = JsonValue::Null;\n        assert_eq!({ struct value(); value.as_object() }, None);",
        "let value = JsonValue::Null;\n        assert_eq!({ enum JsonValue { Null } JsonValue::Null.as_object() }, None);",
        "let value = JsonValue::Null;\n        assert_eq!({ type JsonValue = serde_json::Value; JsonValue::Null.as_object() }, None);",
        "let value = JsonValue::Null;\n        assert_eq!({ fn ripr_receiver_operand(value: serde_json::Value) -> Option<()> { value.as_object(); None } None::<()> }, None);",
        "let value = JsonValue::Null;\n        assert_eq!({ struct value; impl value { fn as_object(&self) -> Option<()> { None } } value.as_object() }, None);",
        "let value = JsonValue::Null;\n        { const value: serde_json::Value = serde_json::Value::Null; assert_eq!(value.as_object(), None); }",
        "let value = JsonValue::Null;\n        mod foreign { pub static value: serde_json::Value = serde_json::Value::Null; }\n        { use foreign::value; assert_eq!(value.as_object(), None); }",
        "let value = JsonValue::Null;\n        assert_eq!({ const value: serde_json::Value = serde_json::Value::Null; value.as_object() }, None);",
    ] {
        decoy_relations_are_name_only(&source.replace(
            "let value = serde_json::json!({\"key\": 1});\n        assert_eq!(value.as_object().unwrap().len(), 1);", decoy,
        ))?;
    }
    Ok(())
}

#[test]
fn module_assertion_shadow_and_standard_positive_have_consistent_diff_relations()
-> Result<(), String> {
    let discarded =
        include_str!("../../../fixtures/inherent_receiver_macro_discard/input/src/lib.rs");
    let shadow = "macro_rules! assert_eq { ($($tokens:tt)*) => {}; }";
    assert!(
        discarded.contains(shadow),
        "the control must contain the discarded macro"
    );
    for (source, expected) in [
        (discarded.to_owned(), RelationReason::WeakTokenSubstring),
        (
            discarded.replace(shadow, ""),
            RelationReason::DirectOwnerCall,
        ),
    ] {
        let fixture = Fixture::create()?;
        std::fs::write(fixture.0.join("src/reader.rs"), source).map_err(|e| e.to_string())?;
        let diff = include_str!("../../../fixtures/inherent_receiver_macro_discard/diff.patch")
            .replace("src/lib.rs", "src/reader.rs");
        std::fs::write(fixture.0.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let output = check_workspace(CheckInput {
            root: fixture.0.clone(),
            diff_file: Some(fixture.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert!(
            !output.findings.is_empty(),
            "changed owner findings must exist"
        );
        for finding in &output.findings {
            let related = finding
                .related_tests
                .iter()
                .find(|test| test.name == "ignored_operand")
                .ok_or("missing retained macro control")?;
            assert_eq!(related.relation_reason, Some(expected));
        }
    }
    Ok(())
}

/// Exercise the actual diff producer on the same native subjects as the
/// full/compact repository matrix. Helper-only reach remains repository-specific.
#[test]
fn executable_owner_identity_boundaries_agree_in_diff() -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Case {
        path: String,
        name: String,
        relation: Option<RelationReason>,
    }
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../fixtures/inherent_receiver_free_helper_identity/cases.json"
    ))
    .map_err(|error| error.to_string())?;
    assert_eq!(cases.len(), 19);
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for case in cases {
        let source = std::fs::read_to_string(fixtures.join(&case.path))
            .map_err(|error| error.to_string())?;
        let fixture = Fixture::with_files(&[
            (
                "Cargo.toml",
                include_str!(
                    "../../../fixtures/inherent_receiver_free_helper_identity/input/Cargo.toml"
                ),
            ),
            ("src/lib.rs", &source),
            (
                "diff.patch",
                include_str!("../../../fixtures/inherent_receiver_free_helper_identity/diff.patch"),
            ),
        ])?;
        let output = check_workspace(CheckInput {
            root: fixture.0.clone(),
            diff_file: Some(fixture.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(
            output.findings.len(),
            2,
            "{}: retain both changed-owner probes",
            case.path
        );
        for finding in &output.findings {
            let rows: Vec<_> = finding
                .related_tests
                .iter()
                .filter(|test| test.name == case.name)
                .collect();
            // Diff reports retain oracle rows: one test may have both an
            // inner assertion and an outer assertion. Check every row rather
            // than silently collapsing them by test name.
            assert!(!rows.is_empty(), "{}: retain the named subject", case.path);
            assert_eq!(
                rows.len(),
                finding.related_tests.len(),
                "{}: no substitute subjects",
                case.path
            );
            for row in rows {
                if case.relation == Some(RelationReason::DirectOwnerCall) {
                    assert_eq!(
                        row.relation_reason,
                        Some(RelationReason::DirectOwnerCall),
                        "{}",
                        case.path
                    );
                    assert_eq!(
                        row.relation_confidence,
                        Some(RelationConfidence::High),
                        "{}",
                        case.path
                    );
                } else {
                    assert!(
                        !matches!(
                            row.relation_reason,
                            Some(RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall)
                        ),
                        "{}: {:?}",
                        case.path,
                        row
                    );
                    assert_ne!(finding.class, ExposureClass::Exposed, "{}", case.path);
                }
            }
        }
    }
    Ok(())
}

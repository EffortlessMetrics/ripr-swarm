//! Public renderer regression for #4681: parent output flow owns contradiction.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, OutputFormat, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug)]
enum Placement {
    Parent,
    InlineChild,
    FileChild,
}
const PLACEMENTS: [Placement; 3] = [
    Placement::Parent,
    Placement::InlineChild,
    Placement::FileChild,
];
struct Fixture {
    root: PathBuf,
    owner: &'static str,
}
impl Fixture {
    fn new(
        placement: Placement,
        helper_value: i32,
        parent_body: &str,
        expected: i32,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-parent-output-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        let (extra, helper, owner) = match placement {
            Placement::Parent => (
                format!("fn compute_value() -> i32 {{ {helper_value} }}"),
                "compute_value()",
                "src/lib.rs::compute_value",
            ),
            Placement::InlineChild => (
                format!(
                    "mod child {{ pub(super) fn compute_value() -> i32 {{ {helper_value} }} }}"
                ),
                "child::compute_value()",
                "src/lib.rs::child::compute_value",
            ),
            Placement::FileChild => {
                std::fs::write(
                    root.join("src/child.rs"),
                    format!("pub(super) fn compute_value() -> i32 {{ {helper_value} }}"),
                )
                .map_err(|e| e.to_string())?;
                (
                    "mod child;".to_string(),
                    "child::compute_value()",
                    "src/child.rs::compute_value",
                )
            }
        };
        let fixture = Self { root, owner };
        std::fs::write(
            fixture.root.join("Cargo.toml"),
            "[package]\nname='parent-output-flow'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        let source = format!(
            "{extra}\npub fn entry() -> i32 {{ {} }}\n#[cfg(test)] mod tests {{ #[test] fn observes_public_parent() {{ assert_eq!(super::entry(), {expected}); }} }}\n",
            parent_body.replace("HELPER", helper)
        );
        std::fs::write(fixture.root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        Ok(fixture)
    }
    fn use_integration_observer(&self) -> Result<(), String> {
        let path = self.root.join("src/lib.rs");
        let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let (library, _) = source
            .split_once("#[cfg(test)] mod tests")
            .ok_or("missing fixture unit-test boundary")?;
        std::fs::write(&path, library).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(self.root.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(self.root.join("tests/integration.rs"),
            "#[test] fn observes_public_parent() { assert_eq!(parent_output_flow::entry(), 10); }\n")
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    fn install_eraser(&self, replacement: &str) -> Result<(), String> {
        let manifest = self.root.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|error| error.to_string())?;
        std::fs::write(
            &manifest,
            format!("{text}\n[dependencies]\neraser={{path='eraser'}}\n"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(self.root.join("eraser/src")).map_err(|error| error.to_string())?;
        std::fs::write(
            self.root.join("eraser/Cargo.toml"),
            "[package]\nname='eraser'\nversion='0.0.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(self.root.join("eraser/src/lib.rs"), format!(
            "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro_attribute] pub fn erase(_:TokenStream,_:TokenStream)->TokenStream {{ {replacement:?}.parse().unwrap() }}\n"))
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    fn seam(&self) -> Result<Value, String> {
        let output = ripr::app::repo_seam_inventory_input(CheckInput {
            root: self.root.clone(),
            ..CheckInput::default()
        });
        let rendered = render_check(&output, &OutputFormat::RepoExposureJson)?;
        let report: Value = serde_json::from_str(&rendered).map_err(|e| e.to_string())?;
        assert_eq!(report["run_status"], "complete", "{report}");
        let seams = report["seams"].as_array().ok_or("missing seams")?;
        let matching = seams
            .iter()
            .filter(|seam| seam["kind"] == "return_value" && seam["owner"] == self.owner)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "actual helper return seam: {report}");
        assert!(
            matching[0]["evidence_record"].is_object(),
            "missing evidence record"
        );
        Ok(matching[0].clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn contradictions(seam: &Value) -> Result<u64, String> {
    let total = seam
        .get("related_tests_total")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or untyped related test count: {seam}"))?;
    let tests = seam
        .get("related_tests")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing related test observations: {seam}"))?;
    assert_eq!(total, 1, "must observe the fixture test: {seam}");
    assert_eq!(tests.len(), 1, "must retain the fixture test: {seam}");
    let name = tests[0]
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or untyped observed test identity: {seam}"))?;
    assert_eq!(
        name.rsplit("::").next(),
        Some("observes_public_parent"),
        "{seam}"
    );
    assert_eq!(tests[0]["relation_reason"], "helper_owner_call", "{seam}");
    assert_eq!(seam["evidence"]["reach"], "yes", "{seam}");
    let record = seam
        .get("evidence_record")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("missing evidence record: {seam}"))?;
    // The producer deliberately omits this field at zero. A present value,
    // including null, must still satisfy its unsigned-count schema.
    match record.get("statically_contradicted_related_tests") {
        None => Ok(0),
        Some(value) => value
            .as_u64()
            .ok_or_else(|| format!("untyped contradiction count: {seam}")),
    }
}

#[test]
fn direct_parent_returns_preserve_agreement_and_actual_contradiction() -> Result<(), String> {
    for placement in PLACEMENTS {
        for body in ["HELPER", "return HELPER;"] {
            let positive = Fixture::new(placement, 9, body, 9)?.seam()?;
            assert_eq!(
                contradictions(&positive)?,
                0,
                "{placement:?} {body}: {positive}"
            );
            assert_eq!(positive["evidence"]["discriminate"], "yes", "{positive}");
            let changed = Fixture::new(placement, 10, body, 9)?.seam()?;
            assert_eq!(
                contradictions(&changed)?,
                1,
                "{placement:?} {body}: {changed}"
            );
            assert_ne!(changed["evidence"]["discriminate"], "yes", "{changed}");
        }
    }
    Ok(())
}

#[test]
fn transformed_or_discarded_helper_outputs_do_not_create_false_contradictions() -> Result<(), String>
{
    for placement in PLACEMENTS {
        for body in [
            "let value = HELPER; value + 1",
            "HELPER + 1",
            "HELPER; 10",
            "({ HELPER }) + 1",
        ] {
            let seam = Fixture::new(placement, 9, body, 10)?.seam()?;
            assert_eq!(contradictions(&seam)?, 0, "{placement:?} {body}: {seam}");
        }
    }
    Ok(())
}

#[test]
fn uncalled_child_delegations_do_not_establish_helper_reach() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for body in [
            "let _f = || HELPER; 10",
            "let _f = move || HELPER; 10",
            "let _f = || { let value = HELPER; value }; 10",
            "let _f = async { let value = HELPER; value }; 10",
            "fn hidden() -> i32 { HELPER } 10",
            "macro_rules! deferred { ($e:expr) => { || $e } } let _f = deferred!(HELPER); 10",
        ] {
            let seam = Fixture::new(placement, 9, body, 10)?.seam()?;
            assert_no_helper_reach(&seam)?;
        }
    }
    Ok(())
}

fn assert_no_helper_reach(seam: &Value) -> Result<(), String> {
    let reach = seam
        .get("evidence")
        .and_then(|evidence| evidence.get("reach"))
        .and_then(Value::as_str)
        .filter(|reach| !reach.is_empty())
        .ok_or_else(|| format!("missing or untyped reach: {seam}"))?;
    assert_ne!(reach, "yes", "{seam}");
    let total = seam
        .get("related_tests_total")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or untyped related count: {seam}"))?;
    let tests = seam
        .get("related_tests")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing related test array: {seam}"))?;
    let retained = u64::try_from(tests.len()).map_err(|error| error.to_string())?;
    assert_eq!(total, retained, "related count/list disagree: {seam}");
    for test in tests {
        let relation = test
            .get("relation_reason")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing or untyped relation: {seam}"))?;
        assert_ne!(relation, "helper_owner_call", "{seam}");
    }
    Ok(())
}

#[test]
fn compile_time_child_calls_do_not_establish_runtime_helper_reach() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for body in [
            "const UNUSED: i32 = HELPER; 10",
            "static UNUSED: i32 = HELPER; 10",
            "let _value = const { HELPER }; 10",
            "let _a = [0u8; HELPER as usize]; 10",
            "let _a: [u8; HELPER as usize] = [0u8; 9]; 10",
            "fn ignore<const N: usize>() {} ignore::<{ HELPER as usize }>(); 10",
        ] {
            let fixture = Fixture::new(placement, 9, body, 10)?;
            let file = if matches!(placement, Placement::FileChild) {
                fixture.root.join("src/child.rs")
            } else {
                fixture.root.join("src/lib.rs")
            };
            let text = std::fs::read_to_string(&file).map_err(|error| error.to_string())?;
            std::fs::write(
                &file,
                text.replace("fn compute_value", "const fn compute_value"),
            )
            .map_err(|error| error.to_string())?;
            assert_no_helper_reach(&fixture.seam()?)?;
        }
    }
    Ok(())
}

#[test]
fn runtime_array_elements_keep_child_helper_reach() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for body in ["let _a = [HELPER; 1]; 10", "let _a = [HELPER, 0]; 10"] {
            let seam = Fixture::new(placement, 9, body, 10)?.seam()?;
            assert_eq!(contradictions(&seam)?, 0, "{placement:?} {body}: {seam}");
        }
    }
    Ok(())
}

#[test]
fn conditional_child_calls_need_build_independent_availability() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for body in [
            "#[cfg(any())] { HELPER; } 10",
            "#[cfg(any())] HELPER; 10",
            "#[cfg(any())] let _unused = HELPER; 10",
            "#[cfg(any())] return HELPER; 10",
            "#[cfg_attr(all(), cfg(any()))] { HELPER; } 10",
            "#[cfg_attr(all(), cfg(any()))] let _unused = HELPER; 10",
            "#[cfg(feature = \"not_established\")] { HELPER; } 10",
            "#[cfg_attr(feature = \"not_established\", cfg(any()))] { HELPER; } 10",
            "#[cfg(test)] { HELPER; } 10",
            "#[cfg(not(test))] { HELPER; } 10",
        ] {
            assert_no_helper_reach(&Fixture::new(placement, 9, body, 10)?.seam()?)?;
        }
        for body in [
            "#[cfg(all())] { HELPER; } 10",
            "#[cfg(all())] HELPER; 10",
            "#[cfg_attr(any(), cfg(any()))] { HELPER; } 10",
            "#[cfg(any())] { HELPER; } HELPER; 10",
        ] {
            let seam = Fixture::new(placement, 9, body, 10)?.seam()?;
            assert_eq!(contradictions(&seam)?, 0, "{placement:?} {body}: {seam}");
        }
    }
    Ok(())
}

#[test]
fn integration_call_does_not_make_library_cfg_test_code_available() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        let fixture = Fixture::new(placement, 9, "#[cfg(test)] { HELPER; } 10", 10)?;
        fixture.use_integration_observer()?;
        assert_no_helper_reach(&fixture.seam()?)?;
    }
    Ok(())
}

#[test]
fn proc_attributes_cannot_manufacture_child_helper_reach() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for (attribute, preserves_body) in [
            ("#[eraser::erase]", false),
            ("#[cfg_attr(all(), eraser::erase)]", false),
            ("#[cfg_attr(not(test), eraser::erase)]", false),
            ("#[cfg_attr(any(), eraser::erase)]", true),
        ] {
            let fixture = Fixture::new(placement, 9, "HELPER; 10", 10)?;
            fixture.use_integration_observer()?;
            let path = fixture.root.join("src/lib.rs");
            let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            std::fs::write(
                &path,
                source.replace("pub fn entry", &format!("{attribute}\npub fn entry")),
            )
            .map_err(|error| error.to_string())?;
            fixture.install_eraser("pub fn entry()->i32 { 10 }")?;
            let seam = fixture.seam()?;
            if preserves_body {
                assert_eq!(
                    contradictions(&seam)?,
                    0,
                    "{placement:?} {attribute}: {seam}"
                );
            } else {
                assert_no_helper_reach(&seam)?;
            }
        }
    }
    Ok(())
}

#[test]
fn enclosing_module_proc_attributes_cannot_manufacture_child_helper_reach() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for (attribute, preserves_body) in [
            ("#[eraser::erase]", false),
            ("#[cfg_attr(any(), eraser::erase)]", true),
        ] {
            let fixture = Fixture::new(placement, 9, "HELPER; 10", 10)?;
            fixture.use_integration_observer()?;
            let path = fixture.root.join("src/lib.rs");
            let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let (child, entry) = source.split_once("pub fn entry").ok_or("missing entry")?;
            std::fs::write(
                &path,
                format!(
                    "{child}{attribute} pub mod parent {{ pub fn entry{} }}\n",
                    entry.replace("child::compute_value()", "crate::child::compute_value()")
                ),
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(fixture.root.join("tests/integration.rs"),
                "#[test] fn observes_public_parent() { assert_eq!(parent_output_flow::parent::entry(), 10); }\n")
                .map_err(|error| error.to_string())?;
            fixture.install_eraser("pub mod parent { pub fn entry()->i32 { 10 } }")?;
            let seam = fixture.seam()?;
            if preserves_body {
                assert_eq!(
                    contradictions(&seam)?,
                    0,
                    "{placement:?} {attribute}: {seam}"
                );
            } else {
                assert_no_helper_reach(&seam)?;
            }
        }
    }
    Ok(())
}

#[test]
fn same_named_inline_rival_does_not_reach_a_different_child_owner() -> Result<(), String> {
    for placement in [Placement::InlineChild, Placement::FileChild] {
        for calls_target in [false, true] {
            let body = if calls_target {
                "HELPER"
            } else {
                "rival::compute_value()"
            };
            let fixture = Fixture::new(placement, 9, body, 9)?;
            let path = fixture.root.join("src/lib.rs");
            let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            std::fs::write(
                &path,
                format!("mod rival {{ pub(super) fn compute_value() -> i32 {{ 9 }} }}\n{source}"),
            )
            .map_err(|error| error.to_string())?;
            let seam = fixture.seam()?;
            if calls_target {
                assert_eq!(contradictions(&seam)?, 0, "{placement:?}: {seam}");
            } else {
                assert_no_helper_reach(&seam)?;
            }
        }
    }
    Ok(())
}

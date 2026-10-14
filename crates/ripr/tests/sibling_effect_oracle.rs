//! #7794: sibling parser assertions cannot observe an aggregation effect.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;

const SOURCE: &str =
    include_str!("../../../fixtures/rust_sibling_effect_oracle/input/src/sensors/ripr_limits.rs");
const DIFF: &str = include_str!("../../../fixtures/rust_sibling_effect_oracle/diff.patch");

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn effect(source: &str, diff: &str, expression: &str) -> Result<Value, String> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-sibling-effect-{}-{}",
        std::process::id(),
        expression.len()
    )));
    std::fs::create_dir_all(scratch.0.join("src/sensors")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname=\"sibling_effect\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), "mod sensors;\n").map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/sensors/mod.rs"), "mod ripr_limits;\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("src/sensors/ripr_limits.rs"), source)
        .map_err(|e| e.to_string())?;
    let patch = scratch.0.join("change.diff");
    std::fs::write(&patch, diff).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(patch),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|e| e.to_string())?;
    json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|f| f["probe"]["expression"] == expression && f["probe"]["family"] == "side_effect")
        .cloned()
        .ok_or_else(|| format!("missing effect: {json}"))
}

#[test]
fn parser_only_decoys_cannot_supply_aggregation_discrimination() -> Result<(), String> {
    let with = effect(SOURCE, DIFF, ".or_default()")?;
    let without = SOURCE.replace("assert_eq!(parse_note(&oriented), expected);", "let _ = parse_note(&oriented);").replace("assert_eq!(\n            parse_note(&format!(\"{oriented} {MODULE_AMBIGUOUS_PARENT_NEXT_STEP}\")),\n            expected\n        );", "let _ = parse_note(&format!(\"{oriented} {MODULE_AMBIGUOUS_PARENT_NEXT_STEP}\"));");
    assert_ne!(without, SOURCE);
    assert!(!without.contains("assert_eq!(parse_note(&oriented), expected);"));
    assert!(!without.contains("parse_note(&format!(\"{oriented} {MODULE_AMBIGUOUS_PARENT_NEXT_STEP}\")),\n            expected"));
    assert!(without.contains("parse_note(&oriented)"));
    assert!(
        without
            .contains("parse_note(&format!(\"{oriented} {MODULE_AMBIGUOUS_PARENT_NEXT_STEP}\"))")
    );
    let control = effect(&without, DIFF, ".or_default()")?;
    let rows = with["related_tests"]
        .as_array()
        .ok_or("missing related tests")?;
    let decoys: Vec<_> = rows
        .iter()
        .filter(|r| r["name"] == "module_note_orientation_sentence_accepts_optional_next_step")
        .collect();
    assert_eq!(decoys.len(), 2, "{with}");
    for row in decoys {
        assert_eq!(row["relation_reason"], "same_test_file", "{row}");
        assert_eq!(row["miss"], "observation_unconfirmed", "{row}");
    }
    for caller in [
        "module_note_line_byte_boundary_excludes_line_terminators",
        "module_evidence_retains_single_pass_duplicates_benign_and_unavailable",
    ] {
        assert!(
            rows.iter()
                .any(|r| r["name"] == caller && r["relation_reason"] == "direct_owner_call"),
            "missing real caller: {with}"
        );
        assert!(
            control["related_tests"]
                .as_array()
                .ok_or("missing control rows")?
                .iter()
                .any(|r| r["name"] == caller && r["relation_reason"] == "direct_owner_call"),
            "missing retained control caller: {control}"
        );
    }
    assert_eq!(with["classification"], "weakly_exposed", "{with}");
    assert_eq!(with["ripr"]["discriminate"]["state"], "weak", "{with}");
    assert_eq!(with["classification"], control["classification"]);
    assert_eq!(
        with["ripr"]["discriminate"]["state"],
        control["ripr"]["discriminate"]["state"]
    );
    Ok(())
}

#[test]
fn direct_effect_observer_retains_strong_credit() -> Result<(), String> {
    let source = "use std::cell::RefCell;\nstruct Recorder { sent: RefCell<Vec<String>> }\nfn record(recorder: &Recorder, payload: &str) {\n    recorder.sent.borrow_mut().push(payload.to_owned());\n}\n#[test]\nfn observes() {\n    let recorder = Recorder { sent: RefCell::new(Vec::new()) };\n    record(&recorder, \"note\");\n    assert_eq!(*recorder.sent.borrow(), vec![\"note\".to_owned()]);\n}\n";
    let diff = "diff --git a/src/sensors/ripr_limits.rs b/src/sensors/ripr_limits.rs\n--- a/src/sensors/ripr_limits.rs\n+++ b/src/sensors/ripr_limits.rs\n@@ -3,3 +3,3 @@\n fn record(recorder: &Recorder, payload: &str) {\n-    recorder.sent.borrow_mut().push(\"old\".to_owned());\n+    recorder.sent.borrow_mut().push(payload.to_owned());\n }\n";
    let finding = effect(
        source,
        diff,
        "recorder.sent.borrow_mut().push(payload.to_owned())",
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    Ok(())
}

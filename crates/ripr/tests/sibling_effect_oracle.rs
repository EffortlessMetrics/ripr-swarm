//! #7794: sibling parser assertions cannot observe an aggregation effect.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

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
    finding(source, diff, expression, "side_effect")
}

fn finding(source: &str, diff: &str, expression: &str, family: &str) -> Result<Value, String> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-sibling-effect-{}-{}",
        std::process::id(),
        NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
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
        .find(|f| f["probe"]["expression"] == expression && f["probe"]["family"] == family)
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
    let without = without.replace(
        "let expected =",
        "let expected: Option<Vec<(String, String)>> =",
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
    for source in [
        source.to_string(),
        source.replace("#[test]", "fn through_helper(recorder: &Recorder, payload: &str) { record(recorder, payload); }\n#[test]").replace("record(&recorder, \"note\")", "through_helper(&recorder, \"note\")"),
    ] {
        let finding = effect(&source, diff, "recorder.sent.borrow_mut().push(payload.to_owned())")?;
        assert_eq!(finding["classification"], "exposed", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    }
    Ok(())
}

const ITERATOR_OBSERVER: &str = r#"use std::{cell::Cell, rc::Rc};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl Iterator for Counter {
    type Item = ();
    fn next(&mut self) -> Option<()> {
        if self.finished { return None; }
        self.finished = true;
        self.sink.set(1);
        Some(())
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn direct_smoke() {
    let mut counter = Counter { sink: Rc::new(Cell::new(0)), finished: false };
    let _ = counter.next();
}
#[test]
fn implicit_observer() {
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    for _ in counter {}
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
"#;
const ADD_OBSERVER: &str = r#"use std::{cell::Cell, rc::Rc, ops::Add};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl std::ops::Add<()> for Counter {
    type Output = ();
    fn add(self, _: ()) {
        self.sink.set(1);
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn direct_smoke() {
    let counter = Counter { sink: Rc::new(Cell::new(0)), finished: false };
    let _ = counter.add(());
}
#[test]
fn implicit_observer() {
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    let _ = counter + ();
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
"#;

fn changed_effect(source: &str, expression: &str) -> String {
    let line = source
        .lines()
        .position(|line| line.contains(expression))
        .unwrap_or(0)
        + 1;
    let previous = expression.replace("(1)", "(0)");
    format!(
        "--- a/src/sensors/ripr_limits.rs\n+++ b/src/sensors/ripr_limits.rs\n@@ -{line},1 +{line},1 @@\n-        {previous};\n+        {expression};\n"
    )
}

#[test]
fn implicit_trait_effect_observers_retain_strong_credit() -> Result<(), String> {
    for source in [
        ITERATOR_OBSERVER.to_string(),
        ITERATOR_OBSERVER.replace("for _ in counter {}", "counter.for_each(|_| {});"),
        ITERATOR_OBSERVER
            .replace(
                "let counter = Counter { sink: state.clone()",
                "let mut counter = Counter { sink: state.clone()",
            )
            .replace("for _ in counter {}", "while counter.next().is_some() {}"),
        ADD_OBSERVER.to_string(),
    ] {
        let finding = finding(
            &source,
            &changed_effect(&source, "self.sink.set(1)"),
            "self.sink.set(1)",
            "call_deletion",
        )?;
        assert_eq!(finding["classification"], "exposed", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
        let rows = finding["related_tests"]
            .as_array()
            .ok_or("missing observer rows")?;
        assert!(rows.iter().any(|r| r["name"] == "direct_smoke" && r["relation_reason"] == "direct_owner_call"), "{finding}");
        assert!(
            rows.iter()
                .any(|r| r["name"] == "implicit_observer" && r["miss"].is_null()),
            "{finding}"
        );
    }
    Ok(())
}

#[test]
fn free_and_inherent_trait_method_names_do_not_admit_decoys() -> Result<(), String> {
    for source in [
        ITERATOR_OBSERVER.replace("impl Iterator for Counter {\n    type Item = ();", "impl Counter {"),
        ITERATOR_OBSERVER.replace("impl Iterator for Counter {\n    type Item = ();\n    fn next(&mut self) -> Option<()> {", "fn next(counter: &mut Counter) -> Option<()> {").replace("self.", "counter.").replace("    }\n}\n#[derive", "}\n#[derive").replace("counter.next()", "next(&mut counter)"),
    ] {
        for source in [source.clone(), format!("{source}\nstruct Other;\nimpl Iterator for Other {{ type Item = (); fn next(&mut self) -> Option<()> {{ None }} }}\n")] {
        let source = source.replace("for _ in counter {}", "let _ = counter;").replace("Cell::new(1)", "Cell::new(0)");
        let expression = if source.contains("self.sink.set(1)") { "self.sink.set(1)" } else { "counter.sink.set(1)" };
        let finding = finding(&source, &changed_effect(&source, expression), expression, "call_deletion")?;
        assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "weak", "{finding}");
        assert!(finding["related_tests"].as_array().ok_or("missing decoy rows")?.iter().any(|r| r["name"] == "implicit_observer" && r["miss"] == "observation_unconfirmed"), "{finding}");
    }
    }
    Ok(())
}

#[test]
fn lone_proximity_retains_discrimination_with_weak_reach() -> Result<(), String> {
    let source = ITERATOR_OBSERVER.replace("#[test]\nfn direct_smoke() {\n    let mut counter = Counter { sink: Rc::new(Cell::new(0)), finished: false };\n    let _ = counter.next();\n}\n", "");
    assert!(!source.contains("fn direct_smoke"));
    let finding = finding(
        &source,
        &changed_effect(&source, "self.sink.set(1)"),
        "self.sink.set(1)",
        "call_deletion",
    )?;
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert_eq!(finding["ripr"]["reach"]["state"], "weak", "{finding}");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    Ok(())
}

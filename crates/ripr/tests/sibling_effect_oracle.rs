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
    finding_with_files(source, diff, expression, family, &[])
}

fn finding_with_files(
    source: &str,
    diff: &str,
    expression: &str,
    family: &str,
    extra_files: &[(&str, &str)],
) -> Result<Value, String> {
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
    for (relative, contents) in extra_files {
        let path = scratch.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, contents).map_err(|e| e.to_string())?;
    }
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
        for family in ["side_effect", "call_deletion"] {
        let finding = finding(&source, diff, "recorder.sent.borrow_mut().push(payload.to_owned())", family)?;
        assert_eq!(finding["classification"], "exposed", "{finding}");
        assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
        }
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
const ADD_OBSERVER: &str = r#"use std::ops::Add;
use std::{cell::Cell, rc::Rc};
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

const UNUSED_TRAIT: &str =
    include_str!("../../../fixtures/rust_unused_trait_effect_oracle/input/src/lib.rs");

#[test]
fn unused_reverse_trait_does_not_authorize_parser_oracle() -> Result<(), String> {
    for source in [UNUSED_TRAIT.to_string(), UNUSED_TRAIT.replace("let input=", "let _counter=Counter { sink:Rc::new(RefCell::new(Vec::new())), finished:false };\n    let input=")] {
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(&source, &changed_effect(&source, expression), expression, family)?;
            assert_eq!(row["classification"], "weakly_exposed", "{row}");
            assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
            assert!(row["related_tests"].as_array().ok_or("missing rows")?.iter().any(|r| r["name"] == "parser_only" && r["miss"] == "observation_unconfirmed"), "{row}");
        }
    }
    Ok(())
}

const REVERSE_OBSERVER: &str = r#"#[test]
fn implicit_observer() {
    let sink=Rc::new(RefCell::new(Vec::new()));
    let counter=Counter { sink:sink.clone(), finished:false };
    for _ in counter {}
    assert_eq!(Observation { sink }, Observation { sink:Rc::new(RefCell::new(vec![1])) });
}
"#;

#[test]
fn paired_implicit_dispatch_matrix_binds_both_effect_families() -> Result<(), String> {
    let iterator = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}");
    let add = iterator.replace("impl Iterator for Counter {\n    type Item=();\n    fn next(&mut self)->Option<()> {\n        if self.finished { return None; }\n        self.finished=true;\n        record(&self.sink);\n        Some(())\n    }\n}", "impl std::ops::Add<()> for Counter {\n    type Output=();\n    fn add(self, _: ()) {\n        record(&self.sink);\n    }\n}").replace("for _ in counter {}", "let _=counter + ();");
    assert!(add.contains("impl std::ops::Add"));
    let paths = [
        iterator.clone(),
        iterator.replace("for _ in counter {}", "counter.for_each(|_| {});"),
        iterator
            .replace("let counter=", "let mut counter=")
            .replace("for _ in counter {}", "while counter.next().is_some() {}"),
        add,
    ];
    for reverse in paths {
        let direct = reverse
            .replace("    output.borrow_mut().push(1);", "")
            .replace("record(&self.sink);", "self.sink.borrow_mut().push(1);");
        let direct = if direct.contains("impl std::ops::Add") {
            format!(
                "use std::ops::Add;\n{}",
                direct.replace(
                    "record(&sink);",
                    "let counter=Counter { sink,finished:false }; let _=counter.add(());",
                )
            )
        } else {
            direct.replace(
                "record(&sink);",
                "let _=Counter { sink,finished:false }.next();",
            )
        };
        for (source, expression) in [
            (reverse.clone(), "output.borrow_mut().push(1)"),
            (direct, "self.sink.borrow_mut().push(1)"),
        ] {
            for family in ["side_effect", "call_deletion"] {
                let row = finding(
                    &source,
                    &changed_effect(&source, expression),
                    expression,
                    family,
                )?;
                assert_eq!(row["classification"], "exposed", "{row}");
                assert_eq!(row["ripr"]["discriminate"]["state"], "yes", "{row}");
                let rows = row["related_tests"].as_array().ok_or("missing rows")?;
                assert!(
                    rows.iter()
                        .any(|r| r["name"] == "implicit_observer" && r["miss"].is_null()),
                    "{row}"
                );
                assert!(
                    rows.iter()
                        .any(|r| r["name"] == "parser_only"
                            && r["miss"] == "observation_unconfirmed"),
                    "{row}"
                );
            }
        }
        // Same owner and indexed trait, but this test never dispatches it.
        let negative = reverse
            .split("#[test]\nfn implicit_observer")
            .next()
            .ok_or("missing observer")?;
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(
                negative,
                &changed_effect(negative, expression),
                expression,
                family,
            )?;
            assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
        }
    }
    // A reverse path cannot choose between an indexed trait and inherent name.
    let ambiguous = format!("{iterator}\nstruct Other; impl Other {{ fn next(&self) {{}} }}\n");
    for family in ["side_effect", "call_deletion"] {
        let expression = "output.borrow_mut().push(1)";
        let row = finding(
            &ambiguous,
            &changed_effect(&ambiguous, expression),
            expression,
            family,
        )?;
        assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
    }
    Ok(())
}

#[test]
fn free_inherent_and_wrong_receiver_push_matrix_stays_weak() -> Result<(), String> {
    let iterator = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}");
    let inherent = iterator
        .replace(
            "impl Iterator for Counter {\n    type Item=();",
            "impl Counter {",
        )
        .replace("for _ in counter {}", "let _=counter;")
        .replace("vec![1]", "vec![]");
    let free = inherent
        .replace(
            "impl Counter {\n    fn next(&mut self)->Option<()> {",
            "fn next(counter: &mut Counter)->Option<()> {",
        )
        .replace("self.", "counter.")
        .replace("    }\n}\n#[derive", "}\n#[derive");
    let wrong = iterator
        .replace("for _ in counter {}", "let _=counter; for _ in [()] {}")
        .replace("vec![1]", "vec![]");
    for source in [inherent, free, wrong] {
        for source in [
            source.clone(),
            format!(
                "{source}\nstruct Other; impl Iterator for Other {{ type Item=(); fn next(&mut self)->Option<()> {{ None }} }}\n"
            ),
        ] {
            for family in ["side_effect", "call_deletion"] {
                let expression = "output.borrow_mut().push(1)";
                let row = finding(
                    &source,
                    &changed_effect(&source, expression),
                    expression,
                    family,
                )?;
                assert_eq!(row["classification"], "weakly_exposed", "{row}");
                assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
            }
        }
    }
    Ok(())
}

#[test]
fn same_test_dispatch_cannot_credit_disconnected_assertions() -> Result<(), String> {
    let dispatch = "let sink=Rc::new(RefCell::new(Vec::new())); let counter=Counter { sink:sink.clone(), finished:false }; for _ in counter {}";
    let parser = "assert_eq!(parse_note(&format!(\"{input}\")), expected);";
    let cases = [
        UNUSED_TRAIT.replace("    let input=", &format!("    {dispatch}\n    let input=")),
        UNUSED_TRAIT.replace(parser, &format!("{parser}\n    {dispatch}")),
        format!(
            "{UNUSED_TRAIT}\n#[test]\nfn disconnected() {{\n {dispatch}\n let fresh=Rc::new(RefCell::new(vec![7]));\n assert_eq!(Observation {{ sink:fresh }}, Observation {{ sink:Rc::new(RefCell::new(vec![7])) }});\n}}\n"
        ),
        format!(
            "{UNUSED_TRAIT}{}",
            REVERSE_OBSERVER
                .replace(
                    "Observation { sink },",
                    "Observation { sink:Rc::new(RefCell::new(Vec::new())) },"
                )
                .replace("vec![1]", "vec![]")
        ),
        format!(
            "{UNUSED_TRAIT}{}",
            REVERSE_OBSERVER
                .replace("    for _ in counter {}\n", "")
                .replace("    assert_eq!", "    for _ in [()] {}\n    assert_eq!")
        ),
    ];
    for source in cases {
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(
                &source,
                &changed_effect(&source, expression),
                expression,
                family,
            )?;
            assert_eq!(row["classification"], "weakly_exposed", "{row}");
            assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
            let rows = row["related_tests"].as_array().ok_or("missing rows")?;
            assert!(
                rows.iter()
                    .any(|r| r["oracle"].is_string() && r["miss"] == "observation_unconfirmed"),
                "{row}"
            );
        }
    }
    Ok(())
}

#[test]
fn implicit_credit_belongs_to_the_ordered_shared_sink_assertion() -> Result<(), String> {
    let assertion =
        "assert_eq!(Observation { sink }, Observation { sink:Rc::new(RefCell::new(vec![1])) });";
    let decoy = "let expected=Some(vec![7]); assert_eq!(parse_note(\"x\"), expected);";
    for observer in [
        REVERSE_OBSERVER.to_string(),
        REVERSE_OBSERVER.replace("for _ in counter {}", "counter.for_each(|_| {});"),
        REVERSE_OBSERVER
            .replace("let counter=", "let mut counter=")
            .replace("for _ in counter {}", "while counter.next().is_some() {}"),
    ] {
        for source in [
            format!(
                "{UNUSED_TRAIT}{}",
                observer.replace(assertion, &format!("{decoy}\n    {assertion}"))
            ),
            format!(
                "{UNUSED_TRAIT}{}",
                observer.replace(assertion, &format!("{assertion}\n    {decoy}"))
            ),
        ] {
            for family in ["side_effect", "call_deletion"] {
                let expression = "output.borrow_mut().push(1)";
                let row = finding(
                    &source,
                    &changed_effect(&source, expression),
                    expression,
                    family,
                )?;
                assert_eq!(row["classification"], "exposed", "{row}");
                assert_eq!(row["ripr"]["discriminate"]["state"], "yes", "{row}");
                let rows = row["related_tests"].as_array().ok_or("missing rows")?;
                let own: Vec<_> = rows
                    .iter()
                    .filter(|r| r["name"] == "implicit_observer")
                    .collect();
                assert_eq!(own.len(), 2, "{row}");
                assert!(
                    own.iter().any(|r| r["oracle"]
                        .as_str()
                        .is_some_and(|s| s.contains("parse_note"))
                        && r["miss"] == "observation_unconfirmed"),
                    "{row}"
                );
                assert!(
                    own.iter().any(|r| r["oracle"]
                        .as_str()
                        .is_some_and(|s| s.contains("Observation"))
                        && r["miss"].is_null()),
                    "{row}"
                );
            }
        }
    }
    for source in [
        format!(
            "{UNUSED_TRAIT}{}",
            REVERSE_OBSERVER
                .replace("    for _ in counter {}\n", "")
                .replace(
                    assertion,
                    &format!("{assertion}\n    for _ in counter {{}}")
                )
        ),
        format!(
            "{UNUSED_TRAIT}{}",
            REVERSE_OBSERVER.replace(
                assertion,
                &format!("let sink=Rc::new(RefCell::new(vec![1]));\n    {assertion}")
            )
        ),
        format!(
            "{UNUSED_TRAIT}{}",
            REVERSE_OBSERVER.replace(
                assertion,
                &format!("sink.borrow_mut().clear();\n    {assertion}")
            )
        ),
    ] {
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(
                &source,
                &changed_effect(&source, expression),
                expression,
                family,
            )?;
            assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
        }
    }
    Ok(())
}

#[test]
fn implicit_binding_refuses_self_comparison_and_opaque_observers() -> Result<(), String> {
    let iterator = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}");
    let actual = "Observation { sink }, Observation { sink:Rc::new(RefCell::new(vec![1])) }";
    for source in [
        iterator.replace(
            actual,
            "Observation { sink:sink.clone() }, Observation { sink:sink.clone() }",
        ),
        iterator
            .replace("assert_eq!", "assert_ne!")
            .replace("vec![1]", "vec![42]"),
        iterator.replace("#[derive(Debug,PartialEq)]", "#[derive(Debug)]")
            + "\nimpl PartialEq for Observation { fn eq(&self, _: &Self) -> bool { true } }\n",
        iterator.replace(
            "let sink=Rc::new(RefCell::new(Vec::new()));\n    let counter=",
            "let sink=Rc::new(RefCell::new(Vec::new())).clone();\n    let counter=",
        ),
        iterator.replace(
            "    assert_eq!(Observation",
            "    let never = || assert_eq!(Observation",
        ),
        iterator.replace("record(&self.sink);", "let never = || record(&self.sink);"),
    ] {
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(
                &source,
                &changed_effect(&source, expression),
                expression,
                family,
            )?;
            assert_ne!(row["classification"], "exposed", "{row}");
            assert_ne!(row["ripr"]["discriminate"]["state"], "yes", "{row}");
            assert!(
                !row["related_tests"]
                    .as_array()
                    .ok_or("missing rows")?
                    .is_empty(),
                "{row}"
            );
        }
    }
    Ok(())
}

#[test]
fn receiver_construction_cannot_seed_the_observed_effect() -> Result<(), String> {
    let source = ITERATOR_OBSERVER.replace(
        "Counter { sink: state.clone(), finished: false };",
        "Counter { sink: state.clone(), finished: { state.set(1); false } };",
    );
    assert_ne!(source, ITERATOR_OBSERVER);
    let expression = "self.sink.set(1)";
    let row = finding(
        &source,
        &changed_effect(&source, expression),
        expression,
        "call_deletion",
    )?;
    assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
    Ok(())
}

#[test]
fn competing_production_writes_cannot_supply_implicit_discrimination() -> Result<(), String> {
    let reverse = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}").replace(
        "record(&self.sink);",
        "record(&self.sink);\n        self.sink.borrow_mut().clear();\n        self.sink.borrow_mut().push(1);",
    );
    let macro_definition = "macro_rules! reset_sink { ($sink:expr) => {{ $sink.borrow_mut().clear(); $sink.borrow_mut().push(1); }}; }\n";
    let mut hidden_dispatch_write = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}").replace(
        "record(&self.sink);",
        "record(&self.sink);\n        reset_sink!(&self.sink);",
    );
    hidden_dispatch_write.insert_str(0, macro_definition);
    let mut hidden_owner_write = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}").replace(
        "output.borrow_mut().push(1);",
        "output.borrow_mut().push(1);\n    reset_sink!(output);",
    );
    hidden_owner_write.insert_str(0, macro_definition);
    let destructor_write = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        + "\nimpl Drop for crate::Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }\n";
    let aliased_destructor = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        + "\nuse std::ops::Drop as Finalizer;\nimpl Finalizer for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }\n";
    let aliased_dispatch_trait = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}").replace(
        "impl Iterator for Counter",
        "impl std::iter::Iterator for Counter",
    ) + "\nuse hostile_crate::Iterator;\nimpl Iterator for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }\n";
    let shadowed_core_trait = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        + "\nmod core { pub use hostile_crate::iter; }\nimpl core::iter::Iterator for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }\n";
    let imported_core_trait = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        + "\nuse hostile_crate::core;\nimpl core::iter::Iterator for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }\n";
    let macro_destructor_write = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        + "\nmacro_rules! install_drop { () => { impl Drop for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } } }; }\ninstall_drop!();\n";
    let custom_derive = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        .replace("struct Counter", "#[derive(CustomDrop)]\nstruct Counter");
    let imported_builtin_derive = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        .replace("use std::", "use hostile_macros::{Debug};\nuse std::");
    let qualified_builtin_spelling =
        format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}") + "\nother_macros::vec!();\n";
    let grouped_macro_import = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}")
        .replace("use std::", "use hostile_macros::{vec};\nuse std::")
        + "\nvec!();\n";
    for source in [
        reverse,
        format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}").replace(
            "output.borrow_mut().push(1);",
            "output.borrow_mut().push(1);\n    output.borrow_mut().clear();\n    output.borrow_mut().push(1);",
        ),
        hidden_dispatch_write,
        hidden_owner_write,
        destructor_write,
        aliased_destructor,
        aliased_dispatch_trait,
        shadowed_core_trait,
        imported_core_trait,
        macro_destructor_write,
        custom_derive,
        imported_builtin_derive,
        qualified_builtin_spelling,
        grouped_macro_import,
    ] {
        for family in ["side_effect", "call_deletion"] {
            let expression = "output.borrow_mut().push(1)";
            let row = finding(&source, &changed_effect(&source, expression), expression, family)?;
            assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "source={source}\n{row}");
            assert!(row["related_tests"].as_array().ok_or("missing rows")?.iter().any(|r| r["name"] == "implicit_observer" && r["miss"] == "observation_unconfirmed"), "{row}");
        }
    }
    let cross_file_drop = format!("{UNUSED_TRAIT}{REVERSE_OBSERVER}") + "\nmod drop_impl;\n";
    let expression = "output.borrow_mut().push(1)";
    for family in ["side_effect", "call_deletion"] {
        let row = finding_with_files(
            &cross_file_drop,
            &changed_effect(&cross_file_drop, expression),
            expression,
            family,
            &[(
                "src/sensors/ripr_limits/drop_impl.rs",
                "impl Drop for super::Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }",
            )],
        )?;
        assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
    }
    let source = ITERATOR_OBSERVER.replace(
        "self.sink.set(1);",
        "self.sink.set(1);\n        self.sink.set(1);",
    );
    let expression = "self.sink.set(1)";
    let row = finding(
        &source,
        &changed_effect(&source, expression),
        expression,
        "call_deletion",
    )?;
    assert_eq!(row["ripr"]["discriminate"]["state"], "weak", "{row}");
    Ok(())
}

#[test]
fn independent_token_observer_survives_implicit_refusal() -> Result<(), String> {
    for original in [ITERATOR_OBSERVER, ADD_OBSERVER] {
        let source = original.replace("state", "sink").replace(
            "assert_eq!(Observation { cell: sink }, Observation { cell: Rc::new(Cell::new(1)) });",
            "assert_eq!(sink.get(), 1);",
        );
        assert_ne!(source, original);
        assert!(source.contains("assert_eq!(sink.get(), 1);"));
        {
            let family = "call_deletion";
            let expression = "self.sink.set(1)";
            let row = finding(
                &source,
                &changed_effect(&source, expression),
                expression,
                family,
            )?;
            assert_eq!(row["ripr"]["discriminate"]["state"], "yes", "{row}");
            assert!(
                row["related_tests"]
                    .as_array()
                    .ok_or("missing rows")?
                    .iter()
                    .any(|r| r["name"] == "implicit_observer" && r["miss"].is_null()),
                "{row}"
            );
        }
    }
    Ok(())
}

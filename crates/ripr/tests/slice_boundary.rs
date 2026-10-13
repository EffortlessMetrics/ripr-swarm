//! Public-path controls for the retained #7553 empty-slice gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const RETAINED: &str = include_str!(
    "../../../fixtures/rust-verdict-corpus/subjects/authored-grid-early/src/lib.rs.txt"
);
const DIFF: &str =
    include_str!("../../../fixtures/rust-verdict-corpus/cases/grid-early-property.diff");
const ROWS: &str = "let samples: [&[u64]; 3] = [&[5], &[3, 9], &[10, 20, 30]];";

#[test]
fn fully_executed_slice_table_needs_no_assertion_for_activation() -> Result<(), String> {
    let source = ordinary_assertion_free_table();
    assert_eq!(source.matches("#[cfg(test)]").count(), 1);
    for (source, has_empty) in [
        (source.clone(), true),
        (source.replace("[&[5], &[]]", "[&[5], &[6]]"), false),
    ] {
        let finding = Subject::new(&source)?.finding()?;
        assert_eq!(
            finding
                .activation
                .observed_values
                .iter()
                .any(|fact| fact.value == "samples.len() == 0"),
            has_empty
        );
        assert_eq!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .any(|fact| fact.value == "samples.len() == 0"),
            !has_empty
        );
    }
    Ok(())
}

fn ordinary_assertion_free_table() -> String {
    let source = include_str!("../../../fixtures/rust_slice_table_try_exit/input/src/lib.rs")
        .replace("        let _ = (|| -> Result<(), ()> {\n", "")
        .replace("                Err(())?;\n", "")
        .replace("            Ok(())\n        })();\n", "");
    assert!(!source.contains("Err(())?") && !source.contains("|| -> Result"));
    source
}

#[test]
fn unresolved_calls_cannot_credit_skipped_slice_rows() -> Result<(), String> {
    let source = ordinary_assertion_free_table();
    let mut credited = Vec::new();
    for (effect, setup) in [
        ("std::process::exit(0);", ""),
        ("loop {}", ""),
        ("while true {}", ""),
        ("assert!({ loop {}; true });", ""),
        (
            "assert!(Stop == Stop);",
            "struct Stop; impl PartialEq for Stop { fn eq(&self, _: &Self) -> bool { std::process::exit(0) } }\n",
        ),
        (
            "terminate();",
            "fn terminate() { std::process::exit(0); }\n",
        ),
        ("finish(0);", "use std::process::exit as finish;\n"),
        (
            "Stop.finish();",
            "struct Stop; impl Stop { fn finish(&self) { std::process::exit(0); } }\n",
        ),
        ("assert!({ std::process::exit(0); true });", ""),
        (
            "assert!({ terminate(); true });",
            "fn terminate() { std::process::exit(0); }\n",
        ),
        (
            "assert!({ Stop.finish(); true });",
            "struct Stop; impl Stop { fn finish(&self) { std::process::exit(0); } }\n",
        ),
    ] {
        for (rows, kind) in [("[&[5], &[]]", "observed"), ("[&[5], &[6]]", "missing")] {
            for (needle, position) in [
                ("            let samples:", "before_loop"),
                ("                let _ = mean_latency(s);", "before_call"),
                ("            }\n", "after_first_call"),
            ] {
                assert_eq!(source.matches(needle).count(), 1, "{position}");
                let text = source
                    .replace("#[cfg(test)]", &format!("{setup}#[cfg(test)]"))
                    .replace("[&[5], &[]]", rows)
                    .replacen(needle, &format!("{effect}\n{needle}"), 1);
                let finding = Subject::new(&text)?.finding()?;
                if finding.class != ExposureClass::InfectionUnknown
                    || finding
                        .activation
                        .observed_values
                        .iter()
                        .map(|fact| fact.value.as_str())
                        .chain(
                            finding
                                .activation
                                .missing_discriminators
                                .iter()
                                .map(|fact| fact.value.as_str()),
                        )
                        .any(|value| value == "samples.len() == 0")
                {
                    credited.push(format!(
                        "{effect}/{kind}/{position}: {:?}",
                        finding.activation
                    ));
                }
            }
        }
    }
    assert!(
        credited.is_empty(),
        "skipped rows were credited: {credited:#?}"
    );
    Ok(())
}

#[test]
fn fallible_slice_observations_cannot_credit_later_rows() -> Result<(), String> {
    for row in ["&[]", "&[ /* empty */ ]"] {
        let source =
            candidate().replace(ROWS, &format!("let samples: [&[u64]; 2] = [{row}, &[5]];"));
        let finding = Subject::new(&source)?.finding()?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown);
        assert!(
            finding
                .activation
                .observed_values
                .iter()
                .all(|fact| fact.value != "samples.len() == 0")
        );
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .all(|fact| fact.value != "samples.len() == 0")
        );
    }
    Ok(())
}

#[test]
fn ignored_fallible_closure_cannot_credit_skipped_slice_rows() -> Result<(), String> {
    let original = "    fn mean_lies_between_min_and_max() {\n        let samples: [&[u64]; 3] = [&[5], &[3, 9], &[10, 20, 30]];\n        for s in samples {\n            let mean = mean_latency(s);\n            assert!(mean >= *s.iter().min().unwrap(), \"{s:?}\");\n            assert!(mean <= *s.iter().max().unwrap(), \"{s:?}\");\n        }\n    }";
    assert_eq!(candidate().matches(original).count(), 1);
    let mut credited = Vec::new();
    for (rows, kind) in [
        ("&[5], &[3, 9]", "missing_empty"),
        ("&[5], &[]", "observed_empty"),
    ] {
        for (before_loop, before_call, after_call, position) in [
            ("Err(())?;", "", "", "before_loop"),
            ("", "Err(())?;", "", "before_call"),
            ("", "", "Err(())?;", "after_first_call"),
        ] {
            let replacement = format!(
                "    fn mean_lies_between_min_and_max() {{\n        let _ = (|| -> Result<(), ()> {{\n            let samples: [&[u64]; 2] = [{rows}];\n            {before_loop}\n            for s in samples {{\n                {before_call}\n                let _ = mean_latency(s);\n                {after_call}\n            }}\n            Ok(())\n        }})();\n    }}"
            );
            let subject = Subject::new(&candidate().replace(original, &replacement))?;
            let finding = subject.finding()?;
            if finding.class != ExposureClass::InfectionUnknown
                || finding
                    .activation
                    .missing_discriminators
                    .iter()
                    .any(|fact| fact.value == "samples.len() == 0")
                || finding
                    .activation
                    .observed_values
                    .iter()
                    .any(|fact| fact.value == "samples.len() == 0")
            {
                credited.push(format!("{kind}/{position}: {:?}", finding.activation));
            }
        }
    }
    assert!(
        credited.is_empty(),
        "skipped rows were credited: {credited:#?}"
    );
    Ok(())
}

#[test]
fn hidden_documentation_inputs_withhold_missing_empty_claims() -> Result<(), String> {
    let base = candidate();
    let variants = [
        base.replace("pub fn mean_latency", "/// ```\n/// assert_eq!(grid_early::mean_latency(&[]), 0);\n/// ```\npub fn mean_latency"),
        base.replace("pub fn mean_latency", "#[doc = \"```\\nassert_eq!(grid_early::mean_latency(&[]), 0);\\n```\"]\npub fn mean_latency"),
        format!("#![doc = include_str!(\"../README.md\")]\n{base}"),
        format!("#![doc =\n include_str!(\"../README.md\")\n]\n{base}"),
        format!("#![doc = concat!(\"```\\nassert_eq!(grid_early::mean_\", \"latency(&[]), 0);\\n```\")]\n{base}"),
        format!("#![doc = \"```\\nassert_eq!(grid_early::mean_\\u{{006c}}atency(&[]), 0);\\n```\"]\n{base}"),
    ];
    for (case, source) in variants.iter().enumerate() {
        let subject = Subject::new(source)?;
        std::fs::write(
            subject.0.join("README.md"),
            "```\nassert_eq!(grid_early::mean_latency(&[]), 0);\n```\n",
        )
        .map_err(|e| e.to_string())?;
        let finding = subject.finding()?;
        assert_eq!(
            finding.class,
            ExposureClass::InfectionUnknown,
            "documentation case {case}"
        );
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .all(|f| f.value != "samples.len() == 0"),
            "documentation case {case}"
        );
    }
    Ok(())
}

#[test]
fn unrelated_documentation_does_not_hide_known_table_inputs() -> Result<(), String> {
    let source = format!(
        "// mean_latency_suffix and documentation of average.\n{}",
        candidate()
    );
    let subject = Subject::new(&source)?;
    let finding = subject.finding()?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|f| f.value == "samples.len() == 0")
    );
    Ok(())
}

#[test]
fn workspace_expansion_limits_withhold_partial_slice_evidence() -> Result<(), String> {
    for prefix in [
        "macro_rules! unrelated { () => {} } unrelated!();\n",
        "#[derive(Debug)] struct Unrelated;\n",
    ] {
        let subject = Subject::new(&format!("{prefix}{}", candidate()))?;
        let finding = subject.finding()?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{prefix}");
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .all(|f| f.value != "samples.len() == 0")
        );
    }
    Ok(())
}

#[test]
fn production_function_pointer_input_cannot_disappear() -> Result<(), String> {
    let source = candidate().replace("#[cfg(test)]", "fn empty_indirect() -> u64 { let f = mean_latency; f(&[]) }\n#[cfg(test)]")
        .replace("    fn check_discount", "    #[test]\n    fn empty_through_pointer() { assert_eq!(empty_indirect(), 0); }\n\n    fn check_discount");
    let subject = Subject::new(&source)?;
    let finding = subject.finding()?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .all(|f| f.value != "samples.len() == 0")
    );
    Ok(())
}

#[test]
fn recursive_empty_input_cannot_disappear_from_cardinality_evidence() -> Result<(), String> {
    let source = candidate().replace(
        "    samples.iter().sum::<u64>() / samples.len() as u64",
        "    let _ = mean_latency(&[]);\n    samples.iter().sum::<u64>() / samples.len() as u64",
    );
    let subject = Subject::new(&source)?;
    let finding = subject.finding()?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .all(|f| f.value != "samples.len() == 0")
    );
    assert!(
        finding
            .activation
            .observed_values
            .iter()
            .all(|f| f.value != "samples.len() == 0")
    );
    Ok(())
}

struct Subject(PathBuf);

impl Subject {
    fn new(source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-slice-boundary-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"grid_early\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        let line = source
            .lines()
            .position(|line| line.trim() == "if samples.len() == 0 {")
            .ok_or("missing candidate predicate")?
            + 1;
        let diff = if line == 18 {
            DIFF.to_string()
        } else {
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-    if samples.is_empty() {{\n+    if samples.len() == 0 {{\n"
            )
        };
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            base: None,
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let findings: Vec<_> = output
            .findings
            .into_iter()
            .filter(|f| {
                f.probe.expression == "samples.len() == 0"
                    && f.probe.location.file.ends_with("src/lib.rs")
            })
            .collect();
        if findings.len() != 1 {
            return Err(format!(
                "expected one real anchored finding, got {}",
                findings.len()
            ));
        }
        findings
            .into_iter()
            .next()
            .ok_or_else(|| "missing anchored finding".to_string())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn candidate() -> String {
    RETAINED.replace("if samples.is_empty() {", "if samples.len() == 0 {")
}

#[test]
fn retained_property_table_names_the_missing_empty_slice() -> Result<(), String> {
    assert_eq!(RETAINED.matches("if samples.is_empty() {").count(), 1);
    let subject = Subject::new(&candidate())?;
    let finding = subject.finding()?;
    assert_eq!(finding.probe.location.line, 18);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let missing = finding
        .activation
        .missing_discriminators
        .iter()
        .find(|f| f.value == "samples.len() == 0")
        .ok_or("missing empty-slice discriminator")?;
    assert!(
        missing
            .reason
            .contains("observed samples.len() values: 1, 2, 3"),
        "{}",
        missing.reason
    );
    Ok(())
}

#[test]
fn slice_elements_are_not_slice_lengths_and_empty_rows_are_observed() -> Result<(), String> {
    for (rows, expected, absent) in [
        (
            "let samples: [&[u64]; 2] = [&[0, 0], &[0, 0, 0]];",
            ExposureClass::WeaklyExposed,
            true,
        ),
        (
            "let samples: [&[u64]; 2] = [&[], &[0, 0]];",
            ExposureClass::WeaklyExposed,
            false,
        ),
    ] {
        let source = candidate()
            .replace(ROWS, rows)
            .replace(".min().unwrap()", ".min().unwrap_or(&0)")
            .replace(".max().unwrap()", ".max().unwrap_or(&0)");
        let subject = Subject::new(&source)?;
        let finding = subject.finding()?;
        assert_eq!(finding.class, expected);
        assert_eq!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .any(|f| f.value == "samples.len() == 0"),
            absent
        );
        if !absent {
            assert!(
                finding
                    .activation
                    .observed_values
                    .iter()
                    .any(|f| f.value == "samples.len() == 0")
            );
        }
    }
    Ok(())
}

#[test]
fn unreadable_or_skippable_slice_tables_withhold_missing_empty_claims() -> Result<(), String> {
    for (name, source) in [
        ("computed_row", candidate().replace(ROWS, "let samples: [&[u64]; 2] = [&[5], std::hint::black_box(&[])];")),
        ("mutable_table", candidate().replace(ROWS, "let mut samples: [&[u64]; 3] = [&[5], &[3, 9], &[10, 20, 30]];")),
        ("empty_table", candidate().replace(ROWS, "let samples: [&[u64]; 0] = [];")),
        ("extra_unknown_call", candidate().replace("let mean = mean_latency(s);", "let mean = mean_latency(s);\n            let _ = mean_latency(std::hint::black_box(&[]));")),
        ("conditional_loop", candidate().replace("for s in samples {", "if false {\n        for s in samples {" ).replace("assert!(mean <= *s.iter().max().unwrap(), \"{s:?}\");", "assert!(mean <= *s.iter().max().unwrap(), \"{s:?}\");\n            }")),
        ("early_return", candidate().replace("for s in samples {", "return;\n        for s in samples {")),
        ("owner_shadow", candidate().replace("    if samples.len() == 0 {", "    let samples: &[u64] = &[1];\n    if samples.len() == 0 {")),
    ] {
        let subject = Subject::new(&source)?;
        let finding = subject.finding()?;
        assert!(finding.activation.missing_discriminators.iter().all(|f| f.value != "samples.len() == 0"), "{name}: {:?}", finding.activation);
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{name}");
    }
    Ok(())
}

#[test]
fn helpers_aliases_and_opaque_macros_cannot_supply_partial_slice_evidence() -> Result<(), String> {
    let valid = candidate()
        .replace(".min().unwrap()", ".min().unwrap_or(&0)")
        .replace(".max().unwrap()", ".max().unwrap_or(&0)");
    for (name, source) in [
        ("mixed_helper", valid.replace("#[cfg(test)]", "fn mean_helper(s: &[u64]) -> u64 { mean_latency(s) }\n#[cfg(test)]")
            .replace("    fn check_discount", "    #[test]\n    fn empty_helper() { assert_eq!(mean_helper(&[]), 0); }\n\n    fn check_discount")),
        ("alias", valid.replace("let mean = mean_latency(s);", "let f = mean_latency;\n            let _ = f(&[]);\n            let mean = mean_latency(s);")),
        ("macro_rebind", valid.replace("#[cfg(test)]", "macro_rules! rebind { ($name:ident) => { let $name: &[u64] = &[]; }; }\n#[cfg(test)]")
            .replace("let mean = mean_latency(s);", "rebind!(s);\n            let mean = mean_latency(s);")),
        ("macro_early_exit", valid.replace(ROWS, "let samples: [&[u64]; 2] = [&[5], &[]];")
            .replace("#[cfg(test)]", "macro_rules! stop { () => { return; }; }\n#[cfg(test)]")
            .replace("let mean = mean_latency(s);", "let mean = mean_latency(s);\n            stop!();")),
        ("owner_macro_shadow", valid.replace("pub fn mean_latency", "macro_rules! shadow { ($name:ident) => { let $name: &[u64] = &[]; }; }\npub fn mean_latency")
            .replace("    if samples.len() == 0 {", "    shadow!(samples);\n    if samples.len() == 0 {")),
    ] {
        let subject = Subject::new(&source)?;
        let finding = subject.finding()?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{name}");
        assert!(finding.activation.missing_discriminators.iter().all(|f| f.value != "samples.len() == 0"), "{name}: {:?}", finding.activation);
        assert!(finding.activation.observed_values.iter().all(|f| f.value != "samples.len() == 0"), "{name}: {:?}", finding.activation);
    }
    Ok(())
}

#[test]
fn equality_keeps_the_matching_table_witness() -> Result<(), String> {
    let source = candidate().replace(ROWS, "let samples: [&[u64]; 2] = [&[], &[5]];")
        .replace(".min().unwrap()", ".min().unwrap_or(&0)").replace(".max().unwrap()", ".max().unwrap_or(&0)")
        .replace("    fn check_discount", "    #[test]\n    fn z_later_nonempty() {\n        let rows: [&[u64]; 2] = [&[7], &[8, 9]];\n        for s in rows {\n            let other = mean_latency(s);\n            assert!(other > 0);\n        }\n    }\n\n    fn check_discount");
    let expected_line = source
        .lines()
        .position(|line| line.contains("let mean = mean_latency(s);"))
        .ok_or("missing first call")?
        + 1;
    let subject = Subject::new(&source)?;
    let finding = subject.finding()?;
    let equality = finding
        .activation
        .observed_values
        .iter()
        .find(|f| f.value == "samples.len() == 0")
        .ok_or("missing observed equality")?;
    assert_eq!(equality.line, expected_line);
    assert!(equality.text.contains("mean_latency(s)"));
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .all(|f| f.value != "samples.len() == 0")
    );
    Ok(())
}

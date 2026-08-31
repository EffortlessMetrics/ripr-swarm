//! Discriminating tests for the repository-governed harness registry
//! (#3532): one per required fixture family, each pinning a boundary the
//! issue names — exact subjects, inert attributes, dynamic limitations,
//! lookalike and ambiguous controls, and stale/wrong-target fail-closed
//! behavior.

use super::*;
use crate::analysis::facts::build_index_with_test_harnesses;
use crate::domain::OracleKind;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-harness-{name}-{stamp}"));
    fs::create_dir_all(&root)?;
    Ok(TempDir(root))
}

fn registration(
    registration_id: &str,
    target: &str,
    kind: TestHarnessKind,
    adapter: TestHarnessAdapter,
    marker: &str,
) -> TestHarnessRegistration {
    TestHarnessRegistration {
        registration_id: registration_id.to_string(),
        target: PathBuf::from(target),
        kind,
        adapter,
        marker: marker.to_string(),
    }
}

fn custom_target_registration(target: &str) -> TestHarnessRegistration {
    registration(
        "mimic-suite",
        target,
        TestHarnessKind::CustomHarnessTarget,
        TestHarnessAdapter::LibtestMimicV1,
        "libtest_mimic",
    )
}

fn attribute_registration(target: &str, marker: &str) -> TestHarnessRegistration {
    registration(
        "contract-tests",
        target,
        TestHarnessKind::RegisteredAttribute,
        TestHarnessAdapter::ExactAttributeV1,
        marker,
    )
}

fn write_workspace(
    root: &TempDir,
    files: &[(&str, &str)],
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.0.join("src"))?;
    fs::create_dir_all(root.0.join("tests"))?;
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = 'harness-fixture'\nversion = '0.1.0'\nedition = '2024'\n",
    )?;
    for (path, source) in files {
        let full = root.0.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full, source)?;
    }
    Ok(())
}

#[test]
fn without_registrations_the_index_is_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("no-registrations")?;
    write_workspace(
        &root,
        &[("src/lib.rs", "pub fn production_control() -> i32 { 1 }\n")],
    )?;
    let plain = build_index_with_test_harnesses(&root.0, &[PathBuf::from("src/lib.rs")], &[])?;
    assert!(plain.harness_subjects.is_empty());
    assert!(plain.harness_limitations.is_empty());
    Ok(())
}

#[test]
fn harness_false_target_is_evidence_role_with_exact_trial_subjects()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("mimic-target")?;
    write_workspace(
        &root,
        &[
            (
                "src/lib.rs",
                "pub fn price(amount: i32) -> i32 {\n    amount * 2\n}\n",
            ),
            (
                "tests/price_mimic.rs",
                r#"
use libtest_mimic::{Arguments, Trial};

fn parse_args(args: &[String]) -> usize {
    args.len()
}

fn check_beta() -> Result<(), String> {
    Ok(())
}

fn static_trials() -> Vec<Trial> {
    vec![
        Trial::test("alpha_parses", || Ok(())),
        Trial::test("beta_round_trips", check_beta),
    ]
}

fn collected(name: &str) -> Trial {
    Trial::test(name, || Ok(()))
}

fn dynamic_trials(count: usize) -> Vec<Trial> {
    let mut trials = Vec::new();
    for index in 0..count {
        trials.push(Trial::test(format!("case_{index}"), || Ok(())));
    }
    trials
}

#[test]
fn ordinary_test_attribute_is_inert_without_the_harness() {
    assert_eq!(parse_args(&[]), 0);
}
"#,
            ),
            (
                "tests/ordinary.rs",
                "#[test]\nfn ordinary_libtest_runs_here() {\n    assert_eq!(1, 1);\n}\n",
            ),
        ],
    )?;
    let files = [
        PathBuf::from("src/lib.rs"),
        PathBuf::from("tests/price_mimic.rs"),
        PathBuf::from("tests/ordinary.rs"),
    ];
    let registrations = [custom_target_registration("tests/price_mimic.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;

    // Exact static trial names are the executable subjects.
    let subject_names = index
        .harness_subjects
        .iter()
        .map(|subject| subject.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subject_names, vec!["alpha_parses", "beta_round_trips"]);
    for subject in &index.harness_subjects {
        assert_eq!(subject.harness_kind, "custom_harness");
        assert_eq!(subject.adapter, "libtest_mimic_v1");
        assert_eq!(subject.marker, "libtest_mimic");
        assert_eq!(subject.file.as_path(), Path::new("tests/price_mimic.rs"));
        assert_eq!(
            subject.selector,
            HarnessSelectorCapability::NamedUnexecuted,
            "the trial name is a selector candidate, represented as unexecuted"
        );
        assert_eq!(subject.claim, HarnessSubjectClaim::NamedInvocation);
        assert_eq!(subject.provenance, "ripr.toml [analysis.test_harnesses]");
    }
    assert!(
        index
            .harness_subjects
            .iter()
            .find(|subject| subject.name == "beta_round_trips")
            .is_some_and(|subject| !subject.body.is_empty() && subject.body.contains("check_beta")),
        "the invocation body is retained so oracle evidence stays observable"
    );

    // The subjects enter the executable-test denominator.
    let test_names = index
        .tests
        .iter()
        .filter(|test| test.file.as_path() == Path::new("tests/price_mimic.rs"))
        .map(|test| test.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(test_names, vec!["alpha_parses", "beta_round_trips"]);

    // The inert `#[test]` attribute does not make the fn a test here:
    // the whole registered target is helper-evidence only.
    let inert = index
        .functions
        .iter()
        .find(|function| function.name == "ordinary_test_attribute_is_inert_without_the_harness")
        .ok_or("missing inert attribute fn")?;
    assert_eq!(inert.source_role, FunctionSourceRole::HarnessHelper);
    assert!(
        index
            .tests
            .iter()
            .all(|test| test.name != "ordinary_test_attribute_is_inert_without_the_harness"),
        "inert attributes must not enter the executable-test denominator"
    );

    // Dynamic shapes remain typed limitations.
    let codes = index
        .harness_limitations
        .iter()
        .map(|limitation| limitation.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"dynamic_trial_name"), "{codes:?}");
    assert!(codes.contains(&"dynamic_trial_registration"), "{codes:?}");

    // The ordinary libtest target beside the custom target keeps its
    // executable test.
    assert!(
        index
            .tests
            .iter()
            .any(|test| test.name == "ordinary_libtest_runs_here"),
        "an ordinary libtest target beside the custom target is unaffected"
    );
    let ordinary = index
        .functions
        .iter()
        .find(|function| function.name == "ordinary_libtest_runs_here")
        .ok_or("missing ordinary test fn")?;
    assert_eq!(ordinary.source_role, FunctionSourceRole::TestAttribute);

    // Same function name in production and harness files: the production
    // one stays a production subject.
    let production = index
        .functions
        .iter()
        .find(|function| {
            function.name == "price" && function.file.as_path() == Path::new("src/lib.rs")
        })
        .ok_or("missing production fn")?;
    assert_eq!(production.source_role, FunctionSourceRole::Production);
    Ok(())
}

#[test]
fn unanchored_and_ambiguous_trial_paths_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("trial-anchors")?;
    write_workspace(
        &root,
        &[(
            "tests/unanchored.rs",
            r#"
use other_crate::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("unanchored_case", || Ok(()))]
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/unanchored.rs")];
    let registrations = [custom_target_registration("tests/unanchored.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "unanchored_trial_path"),
        "{:?}",
        index.harness_limitations
    );

    // Same-named imports from two paths are non-clean and named.
    let root = temp_dir("trial-ambiguous")?;
    write_workspace(
        &root,
        &[(
            "tests/ambiguous.rs",
            r#"
use libtest_mimic::Trial;
use other_crate::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("ambiguous_case", || Ok(()))]
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/ambiguous.rs")];
    let registrations = [custom_target_registration("tests/ambiguous.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    let ambiguous = index
        .harness_limitations
        .iter()
        .find(|limitation| limitation.code == "ambiguous_import")
        .ok_or("missing ambiguous-import limitation")?;
    assert!(ambiguous.detail.contains("conflicting imports bind Trial"));
    Ok(())
}

#[test]
fn duplicate_trial_names_name_the_conflict_and_remove_both_subjects()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("duplicate-trials")?;
    write_workspace(
        &root,
        &[(
            "tests/duplicates.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![
        Trial::test("same_name", || Ok(())),
        Trial::test("same_name", || Ok(())),
    ]
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/duplicates.rs")];
    let registrations = [custom_target_registration("tests/duplicates.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(index.harness_subjects.len(), 0);
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "duplicate_subject"
                && limitation.detail.contains("same_name")),
        "{:?}",
        index.harness_limitations
    );
    assert!(
        index.tests.iter().all(|test| test.name != "same_name"),
        "a duplicate subject must not leave an executable test fact"
    );
    Ok(())
}

#[test]
fn production_like_target_precedence_survives_harness_registration()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("production-like-precedence")?;
    write_workspace(
        &root,
        &[(
            "tests/mimic.rs",
            "use libtest_mimic::Trial;\nfn helper() -> i32 { 1 }\nTrial::test(\"one\", || Ok(()));\n",
        )],
    )?;
    let target = PathBuf::from("tests/mimic.rs");
    let registrations = [custom_target_registration("tests/mimic.rs")];
    let mut production_like = BTreeSet::new();
    production_like.insert(target.clone());
    let index =
        crate::analysis::facts::build_index_with_test_harnesses_and_production_like_targets(
            &root.0,
            &[target],
            &registrations,
            &production_like,
        )?;
    let helper = index
        .functions
        .iter()
        .find(|function| function.name == "helper")
        .ok_or("missing helper")?;
    assert_eq!(helper.source_role, FunctionSourceRole::Production);
    assert!(index.harness_subjects.is_empty());
    Ok(())
}

#[test]
fn fully_qualified_trial_calls_match_the_marker_path() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("qualified-trials")?;
    write_workspace(
        &root,
        &[(
            "tests/qualified.rs",
            "fn trials() -> Vec<libtest_mimic::Trial> {\n    vec![libtest_mimic::Trial::test(\"qualified_case\", || Ok(())), other::libtest_mimic::Trial::test(\"suffix_lookalike\", || Ok(()))]\n}\n",
        )],
    )?;
    let files = [PathBuf::from("tests/qualified.rs")];
    let registrations = [custom_target_registration("tests/qualified.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["qualified_case"]
    );
    // The token-local bare pattern re-matches the inner `Trial` of the
    // qualified path; claiming the invocation must leave no false
    // unanchored/duplicate limitation behind.
    assert!(
        index.harness_limitations.is_empty(),
        "{:?}",
        index.harness_limitations
    );
    Ok(())
}

#[test]
fn qualified_trial_with_bare_import_emits_one_subject_and_no_false_limitation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("qualified-with-import")?;
    write_workspace(
        &root,
        &[(
            "tests/both_forms.rs",
            "use libtest_mimic::Trial;\n\nfn trials() -> Vec<Trial> {\n    vec![libtest_mimic::Trial::test(\"both_case\", || Ok(()))]\n}\n",
        )],
    )?;
    let files = [PathBuf::from("tests/both_forms.rs")];
    let registrations = [custom_target_registration("tests/both_forms.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["both_case"]
    );
    assert!(
        index.harness_limitations.is_empty(),
        "{:?}",
        index.harness_limitations
    );
    Ok(())
}

#[test]
fn plain_idents_inside_trial_bodies_never_become_oracles() -> Result<(), Box<dyn std::error::Error>>
{
    let root = temp_dir("trial-ident-noise")?;
    write_workspace(
        &root,
        &[(
            "tests/ident_noise.rs",
            "fn trials() -> Vec<libtest_mimic::Trial> {\n    vec![libtest_mimic::Trial::test(\"noise_case\", || {\n        let snapshots = load();\n        assert_eq!(snapshots, 1);\n        Ok(())\n    })]\n}\nfn load() -> i32 {\n    1\n}\n",
        )],
    )?;
    let files = [PathBuf::from("tests/ident_noise.rs")];
    let registrations = [custom_target_registration("tests/ident_noise.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    let subject_test = index
        .tests
        .iter()
        .find(|test| test.name == "noise_case")
        .ok_or("subject test present")?;
    assert_eq!(
        subject_test
            .assertions
            .iter()
            .map(|assertion| assertion.text.as_str())
            .collect::<Vec<_>>(),
        // One macro-shaped oracle with the invocation's real text; the
        // `let snapshots` ident never classifies.
        vec!["assert_eq!(snapshots, 1)"]
    );
    Ok(())
}

#[test]
fn snapshot_named_helpers_never_become_oracles_but_snapshot_asserts_do()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("leaf-snapshot-boundary")?;
    write_workspace(
        &root,
        &[(
            "tests/leaf_boundary.rs",
            "fn trials() -> Vec<libtest_mimic::Trial> {
    vec![libtest_mimic::Trial::test(\"leaf_case\", || {
        snapshot_helper!();
        assert_snapshot!(1);
        Ok(())
    })]
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/leaf_boundary.rs")];
    let registrations = [custom_target_registration("tests/leaf_boundary.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    let subject_test = index
        .tests
        .iter()
        .find(|test| test.name == "leaf_case")
        .ok_or("subject test present")?;
    assert_eq!(
        subject_test
            .assertions
            .iter()
            .map(|assertion| assertion.text.as_str())
            .collect::<Vec<_>>(),
        // The *_snapshot naming boundary keeps helper-style macros out
        // while the assert_snapshot family still classifies.
        vec!["assert_snapshot!(1)"]
    );
    Ok(())
}

#[test]
fn foreign_trial_paths_are_not_adopted_through_an_import_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("foreign-trial-path")?;
    write_workspace(
        &root,
        &[(
            "tests/foreign_trial.rs",
            "use libtest_mimic::Trial;

fn other() -> Vec<Trial> {
    vec![other_module :: Trial::test(\"foreign_case\", || Ok(()))]
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/foreign_trial.rs")];
    let registrations = [custom_target_registration("tests/foreign_trial.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(
        index.harness_subjects.is_empty(),
        "{:?}",
        index.harness_subjects
    );
    assert!(index.tests.is_empty(), "{:?}", index.tests);
    Ok(())
}

#[test]
fn record_field_shape_inside_a_macro_token_tree_still_classifies()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("record-shape-in-macro")?;
    write_workspace(
        &root,
        &[(
            "tests/record_in_macro.rs",
            "use libtest_mimic::Trial;

struct Suite {
    trials: Vec<Trial>,
}

fn suite() -> Suite {
    Suite {
        trials: vec![Suite { trials: Trial::test(\"nested_case\", || Ok(())) }.trials],
    }
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/record_in_macro.rs")];
    let registrations = [custom_target_registration("tests/record_in_macro.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    // Inside vec!'s token tree the record field is raw syntax; the
    // field-name-colon-before-{/, shape keeps the subject classified.
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["nested_case"]
    );
    Ok(())
}

#[test]
fn macro_input_data_never_becomes_a_subject() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("macro-input-data")?;
    write_workspace(
        &root,
        &[(
            "tests/macro_input.rs",
            "fn debug() -> String {
    stringify!(trial: Trial::test(\"ghost_case\", || Ok(())))
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/macro_input.rs")];
    let registrations = [custom_target_registration("tests/macro_input.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    // Macro input is inert data; a `label:` colon inside it must not
    // adopt the record-field exception.
    assert!(
        index.harness_subjects.is_empty(),
        "{:?}",
        index.harness_subjects
    );
    assert!(index.tests.is_empty(), "{:?}", index.tests);
    Ok(())
}

#[test]
fn struct_field_initializers_still_start_trial_paths() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("struct-field-trial")?;
    write_workspace(
        &root,
        &[(
            "tests/field_trial.rs",
            "use libtest_mimic::Trial;

struct Suite {
    trials: Vec<Trial>,
}

fn suite() -> Suite {
    Suite {
        trials: vec![Trial::test(\"field_case\", || Ok(()))],
    }
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/field_trial.rs")];
    let registrations = [custom_target_registration("tests/field_trial.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["field_case"]
    );
    Ok(())
}

#[test]
fn dormant_macro_templates_never_become_subjects() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("dormant-macro-template")?;
    write_workspace(
        &root,
        &[(
            "tests/dormant_template.rs",
            "macro_rules! trial_template {
    ($name:literal) => {
        libtest_mimic::Trial::test($name, || Ok(()))
    };
}

fn trials() -> Vec<libtest_mimic::Trial> {
    Vec::new()
}
",
        )],
    )?;
    let files = [PathBuf::from("tests/dormant_template.rs")];
    let registrations = [custom_target_registration("tests/dormant_template.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    // The macro body is a template, not a registration: an uninvoked
    // template must not fabricate an executable test.
    assert!(
        index.harness_subjects.is_empty(),
        "{:?}",
        index.harness_subjects
    );
    assert!(index.tests.is_empty(), "{:?}", index.tests);
    Ok(())
}

#[test]
fn trial_oracles_require_real_macros_and_preserve_source_lines()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("trial-oracles")?;
    write_workspace(
        &root,
        &[(
            "tests/oracles.rs",
            "use libtest_mimic::Trial;\nfn trials() -> Vec<Trial> {\n    vec![Trial::test(\"oracle_case\", || {\n        let snapshot_value = 1;\n        let result = Some(snapshot_value).unwrap();\n        assert_eq!(result, 1);\n        result.expect(\"present\");\n        Ok(())\n    })]\n}\n",
        )],
    )?;
    let index = build_index_with_test_harnesses(
        &root.0,
        &[PathBuf::from("tests/oracles.rs")],
        &[custom_target_registration("tests/oracles.rs")],
    )?;
    let subject = index
        .harness_subjects
        .iter()
        .find(|subject| subject.name == "oracle_case")
        .ok_or("missing oracle subject")?;
    assert_eq!(subject.assertions.len(), 3);
    assert!(
        subject
            .assertions
            .iter()
            .any(|oracle| oracle.text == "unwrap" || oracle.text == ".unwrap(")
    );
    assert!(
        subject
            .assertions
            .iter()
            .any(|oracle| oracle.text == ".expect(")
    );
    assert!(
        subject
            .assertions
            .iter()
            .any(|oracle| oracle.text.starts_with("assert_eq!"))
    );
    assert!(
        subject
            .assertions
            .iter()
            .map(|oracle| oracle.line)
            .any(|line| line > subject.start_line)
    );
    Ok(())
}

#[test]
fn qualified_trial_callbacks_keep_subjects_and_resolve_terminal_function_names()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("qualified-callbacks")?;
    write_workspace(
        &root,
        &[(
            "tests/callbacks.rs",
            r#"
use libtest_mimic::Trial;
fn check() {
    assert_eq!(1, 1);
}
fn trials() -> Vec<Trial> {
    vec![
        Trial::test("resolved_callback", callbacks::check),
        Trial::test("unresolved_callback", missing::callback),
    ]
}
"#,
        )],
    )?;
    let index = build_index_with_test_harnesses(
        &root.0,
        &[PathBuf::from("tests/callbacks.rs")],
        &[custom_target_registration("tests/callbacks.rs")],
    )?;
    assert_eq!(index.harness_subjects.len(), 2);
    let resolved = index
        .harness_subjects
        .iter()
        .find(|subject| subject.name == "resolved_callback")
        .ok_or("missing resolved callback subject")?;
    assert!(
        resolved
            .assertions
            .iter()
            .any(|oracle| oracle.text.starts_with("assert_eq!"))
    );
    assert!(index.harness_limitations.iter().any(|limitation| {
        limitation.code == "unresolved_trial_callback"
            && limitation.detail.contains("missing::callback")
    }));
    Ok(())
}

#[test]
fn trial_oracles_preserve_brace_and_bracket_macro_delimiters()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("delimited-oracles")?;
    write_workspace(
        &root,
        &[(
            "tests/delimiters.rs",
            r#"
use libtest_mimic::Trial;
fn trials() -> Vec<Trial> {
    vec![Trial::test("delimiters", || {
        let result = 1;
        assert_eq! { result, 1 };
        assert_eq! [result, 1];
        Ok(())
    })]
}
"#,
        )],
    )?;
    let index = build_index_with_test_harnesses(
        &root.0,
        &[PathBuf::from("tests/delimiters.rs")],
        &[custom_target_registration("tests/delimiters.rs")],
    )?;
    let subject = index
        .harness_subjects
        .iter()
        .find(|subject| subject.name == "delimiters")
        .ok_or("missing delimiter subject")?;
    assert_eq!(subject.assertions.len(), 2);
    assert!(subject.assertions.iter().any(|oracle| {
        oracle.kind == OracleKind::ExactValue
            && oracle.text.contains("{")
            && oracle.text.contains("}")
    }));
    assert!(subject.assertions.iter().any(|oracle| {
        oracle.kind == OracleKind::ExactValue
            && oracle.text.contains("[")
            && oracle.text.contains("]")
    }));
    Ok(())
}

#[test]
fn registered_attribute_promotes_exact_matches_only() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("registered-attribute")?;
    write_workspace(
        &root,
        &[(
            "crates/contract/tests/api.rs",
            r#"
use myco::contract_test;

pub fn price(amount: i32) -> i32 {
    amount * 2
}

#[contract_test]
fn price_round_trips() {
    assert_eq!(price(2), 4);
}

#[myco::contract_test]
fn price_rejects_zero() {
    assert_ne!(price(0), 1);
}

#[myco::contract_tests]
fn lookalike_suffix_is_not_registered() {
    assert_eq!(price(1), 2);
}

#[myco_contract_test]
fn lookalike_prefix_is_not_registered() {
    assert_eq!(price(1), 2);
}

fn plain_helper() -> i32 {
    price(3)
}
"#,
        )],
    )?;
    let target = "crates/contract/tests/api.rs";
    let files = [PathBuf::from(target)];
    let registrations = [attribute_registration(target, "myco::contract_test")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;

    let mut subject_names = index
        .harness_subjects
        .iter()
        .map(|subject| subject.name.as_str())
        .collect::<Vec<_>>();
    subject_names.sort_unstable();
    assert_eq!(
        subject_names,
        vec!["price_rejects_zero", "price_round_trips"]
    );
    for subject in &index.harness_subjects {
        assert_eq!(subject.harness_kind, "registered_attribute");
        assert_eq!(subject.adapter, "exact_attribute_v1");
        assert_eq!(subject.marker, "myco::contract_test");
        assert_eq!(subject.claim, HarnessSubjectClaim::NamedFunction);
        assert_eq!(subject.selector, HarnessSelectorCapability::NamedUnexecuted);
    }

    let promoted = index
        .functions
        .iter()
        .find(|function| function.name == "price_round_trips")
        .ok_or("missing promoted fn")?;
    assert_eq!(
        promoted.source_role,
        FunctionSourceRole::RegisteredTestAttribute
    );
    assert!(
        index
            .tests
            .iter()
            .any(|test| test.name == "price_round_trips"),
        "registered attribute tests join the executable-test denominator"
    );
    assert!(
        index.functions.iter().all(|function| function.name
            != "lookalike_suffix_is_not_registered"
            || !function.source_role.is_evidence_role()),
        "prefix/suffix lookalikes must never classify"
    );
    assert!(
        index
            .tests
            .iter()
            .all(|test| !test.name.contains("lookalike")),
        "lookalikes must not join the test denominator"
    );
    Ok(())
}

#[test]
fn same_named_unrelated_import_is_ambiguous_and_fails_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("attribute-ambiguous")?;
    write_workspace(
        &root,
        &[(
            "tests/ambiguous_attr.rs",
            r#"
use other_crate::contract_test;

#[contract_test]
fn ambiguous_owner() {
    assert_eq!(1, 1);
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/ambiguous_attr.rs")];
    let registrations = [attribute_registration(
        "tests/ambiguous_attr.rs",
        "myco::contract_test",
    )];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "ambiguous_import"),
        "{:?}",
        index.harness_limitations
    );
    let function = index
        .functions
        .iter()
        .find(|function| function.name == "ambiguous_owner")
        .ok_or("missing fn")?;
    assert_eq!(
        function.source_role,
        FunctionSourceRole::Production,
        "an ambiguous import must not classify the fn as an executable test"
    );
    Ok(())
}

#[test]
fn stale_or_foreign_registrations_grant_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("stale-registration")?;
    write_workspace(
        &root,
        &[(
            "src/lib.rs",
            "pub fn price(amount: i32) -> i32 {\n    amount * 2\n}\n",
        )],
    )?;
    // The registration names a file that is not part of this analysis
    // (stale, removed, or another package's scope).
    let registrations = [custom_target_registration("other_pkg/tests/mimic.rs")];
    let files = [PathBuf::from("src/lib.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert!(index.harness_limitations.is_empty());
    assert!(index.tests.is_empty());
    Ok(())
}

#[test]
fn bare_attribute_without_marker_import_stays_unclassified()
-> Result<(), Box<dyn std::error::Error>> {
    // `#[contract_test]` with no import binding at all cannot be established
    // to be the registered marker path; the authority names the gap
    // instead of guessing.
    let root = temp_dir("attribute-unresolved")?;
    write_workspace(
        &root,
        &[(
            "tests/unresolved_attr.rs",
            "#[contract_test]\nfn unresolved_owner() {\n    assert_eq!(1, 1);\n}\n",
        )],
    )?;
    let files = [PathBuf::from("tests/unresolved_attr.rs")];
    let registrations = [attribute_registration(
        "tests/unresolved_attr.rs",
        "myco::contract_test",
    )];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "unresolved_marker_import"),
        "{:?}",
        index.harness_limitations
    );
    Ok(())
}

#[test]
fn cached_index_applies_registrations_identically() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("cached-harness")?;
    let file = PathBuf::from("tests/cached_mimic.rs");
    let source = br#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("cached_case", || Ok(()))]
}
"#
    .to_vec();
    let files = [(file.clone(), source)];
    let registrations = [custom_target_registration("tests/cached_mimic.rs")];

    let cold = crate::analysis::facts::build_index_from_loaded_files_with_cache_and_test_harnesses(
        &root.0,
        &files,
        &registrations,
    )?;
    let warm = crate::analysis::facts::build_index_from_loaded_files_with_cache_and_test_harnesses(
        &root.0,
        &files,
        &registrations,
    )?;
    for index in [&cold.index, &warm.index] {
        assert_eq!(
            index
                .harness_subjects
                .iter()
                .map(|subject| subject.name.as_str())
                .collect::<Vec<_>>(),
            vec!["cached_case"]
        );
        assert_eq!(index.tests.len(), 1);
    }
    assert_eq!(cold.file_fact_cache.hits, 0);
    assert_eq!(warm.file_fact_cache.hits, 1);
    Ok(())
}

#[test]
fn built_in_test_attributes_precede_a_registered_marker() -> Result<(), Box<dyn std::error::Error>>
{
    // A fn carrying BOTH `#[test]` and the registered attribute must not
    // register twice: the built-in #3499 family keeps precedence.
    let root = temp_dir("builtin-precedence")?;
    write_workspace(
        &root,
        &[(
            "tests/both_attrs.rs",
            r#"
use myco::contract_test;

#[test]
#[contract_test]
fn double_attributed() {
    assert_eq!(1, 1);
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/both_attrs.rs")];
    let registrations = [
        custom_target_registration("tests/other.rs"),
        attribute_registration("tests/both_attrs.rs", "myco::contract_test"),
    ];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(
        index.harness_subjects.is_empty(),
        "the built-in attribute already owns the fn"
    );
    assert_eq!(
        index
            .tests
            .iter()
            .filter(|test| test.name == "double_attributed")
            .count(),
        1,
        "exactly one executable test fact, from the built-in family"
    );
    let function = index
        .functions
        .iter()
        .find(|function| function.name == "double_attributed")
        .ok_or("missing fn")?;
    assert_eq!(function.source_role, FunctionSourceRole::TestAttribute);
    Ok(())
}

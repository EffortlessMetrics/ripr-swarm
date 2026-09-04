//! Discriminating tests for the repository-governed harness registry
//! (#3532): one per required fixture family, each pinning a boundary the
//! issue names — exact subjects, inert attributes, dynamic limitations,
//! lookalike and ambiguous controls, and stale/wrong-target fail-closed
//! behavior.

use super::*;
use crate::analysis::facts::build_index_with_test_harnesses;
use crate::analysis::facts::model::FileFacts;
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

/// Declare a `[[test]]` target with `harness = false` in the workspace
/// manifest (#3608): the parsed Cargo metadata a valid custom-harness
/// registration must be able to point at. A registration whose target is
/// not declared this way records a conflict instead of granting
/// file-wide evidence role.
fn declare_harness_false_target(
    root: &TempDir,
    name: &str,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut manifest = std::fs::OpenOptions::new()
        .append(true)
        .open(root.0.join("Cargo.toml"))?;
    writeln!(
        manifest,
        "\n[[test]]\nname = '{name}'\npath = '{path}'\nharness = false"
    )?;
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
    declare_harness_false_target(&root, "price_mimic", "tests/price_mimic.rs")?;
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
    declare_harness_false_target(&root, "unanchored", "tests/unanchored.rs")?;
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
    declare_harness_false_target(&root, "ambiguous", "tests/ambiguous.rs")?;
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
fn duplicate_trial_names_name_the_conflict_and_stay_one_subject()
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
    declare_harness_false_target(&root, "duplicates", "tests/duplicates.rs")?;
    let registrations = [custom_target_registration("tests/duplicates.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(index.harness_subjects.len(), 1);
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "duplicate_subject"
                && limitation.detail.contains("same_name")),
        "{:?}",
        index.harness_limitations
    );
    Ok(())
}

#[test]
fn fully_qualified_trial_calls_match_the_marker_path() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("qualified-trials")?;
    write_workspace(
        &root,
        &[(
            "tests/qualified.rs",
            "fn trials() -> Vec<libtest_mimic::Trial> {\n    vec![libtest_mimic::Trial::test(\"qualified_case\", || Ok(()))]\n}\n",
        )],
    )?;
    let files = [PathBuf::from("tests/qualified.rs")];
    declare_harness_false_target(&root, "qualified", "tests/qualified.rs")?;
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
    declare_harness_false_target(&root, "both_forms", "tests/both_forms.rs")?;
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
    declare_harness_false_target(&root, "ident_noise", "tests/ident_noise.rs")?;
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
    declare_harness_false_target(&root, "leaf_boundary", "tests/leaf_boundary.rs")?;
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
    declare_harness_false_target(&root, "foreign_trial", "tests/foreign_trial.rs")?;
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
    declare_harness_false_target(&root, "record_in_macro", "tests/record_in_macro.rs")?;
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
    declare_harness_false_target(&root, "macro_input", "tests/macro_input.rs")?;
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
    declare_harness_false_target(&root, "field_trial", "tests/field_trial.rs")?;
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
    declare_harness_false_target(&root, "dormant_template", "tests/dormant_template.rs")?;
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

#[test]
fn inert_test_in_harness_target() {
    assert_eq!(1, 1);
}
"#
    .to_vec();
    let files = [(file.clone(), source)];
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = 'harness-fixture'\nversion = '0.1.0'\nedition = '2024'\n",
    )?;
    declare_harness_false_target(&root, "cached_mimic", "tests/cached_mimic.rs")?;
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
        // Only the adapter-established trial subject enters the executable test
        // denominator; the inert #[test] is demoted on both cold and warm runs (#3602).
        assert_eq!(index.tests.len(), 1);
        assert_eq!(index.tests[0].name, "cached_case");
        let file_facts = index.files.get(&file).ok_or("file facts missing")?;
        assert_eq!(file_facts.tests.len(), 1);
        assert_eq!(file_facts.tests[0].name, "cached_case");
        let inert = index
            .functions
            .iter()
            .find(|f| f.name == "inert_test_in_harness_target")
            .ok_or("inert fn present")?;
        assert_eq!(inert.source_role, FunctionSourceRole::HarnessHelper);
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

#[test]
fn demote_harness_target_functions_drops_differently_named_test_facts()
-> Result<(), Box<dyn std::error::Error>> {
    // Discriminating regression test for #3602 / chatgpt-codex review finding:
    // Verify that demote_harness_target_functions drops TestFacts overlapping a
    // demoted function's span even when the TestFact's name does NOT match the
    // function's name (which name-based demotion would miss).
    let target = Path::new("tests/custom_harness.rs");
    let mut index = RustIndex::default();
    let function = FunctionFact {
        id: crate::domain::SymbolId("custom_harness::test_fn".to_string()),
        name: "source_function_name".to_string(),
        file: target.to_path_buf(),
        start_line: 10,
        end_line: 20,
        body: "assert_eq!(1, 1);".to_string(),
        calls: Vec::new(),
        returns: Vec::new(),
        literals: Vec::new(),
        source_role: FunctionSourceRole::TestAttribute,
        attrs: vec!["#[test]".to_string()],
    };
    let test_fact = TestFact {
        name: "differently_named_test_case".to_string(),
        file: target.to_path_buf(),
        start_line: 10,
        end_line: 20,
        body: "assert_eq!(1, 1);".to_string(),
        calls: Vec::new(),
        assertions: Vec::new(),
        literals: Vec::new(),
        attrs: vec!["#[test]".to_string()],
    };
    let file_facts = FileFacts {
        path: target.to_path_buf(),
        functions: vec![function.clone()],
        tests: vec![test_fact.clone()],
        ..Default::default()
    };
    index.files.insert(target.to_path_buf(), file_facts.clone());
    index.functions.push(function);
    index.tests.push(test_fact);

    demote_harness_target_functions(&mut index, target);

    // Span overlap drops the test fact despite the name mismatch.
    assert!(
        index.tests.is_empty(),
        "differently-named TestFact overlapping demoted span must be dropped"
    );
    let facts = index.files.get(target).ok_or("file facts missing")?;
    assert!(
        facts.tests.is_empty(),
        "file-level differently-named TestFact must be dropped"
    );
    assert_eq!(
        facts.functions[0].source_role,
        FunctionSourceRole::HarnessHelper
    );
    assert_eq!(
        index.functions[0].source_role,
        FunctionSourceRole::HarnessHelper
    );
    Ok(())
}

/// #3608: a `custom_harness` registration whose target path does not
/// match any Cargo `[[test]]` target records the conflict and degrades to
/// per-function behavior — the misdeclared file keeps its ordinary
/// classification (a `#[test]` fn stays an executable test; nothing is
/// demoted and no trial subjects appear).
#[test]
fn misdeclared_target_keeps_per_function_roles_and_records_the_conflict()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("misdeclared-target")?;
    write_workspace(
        &root,
        &[(
            "src/misdeclared.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("ghost_trial", || Ok(()))]
}

#[test]
fn ordinary_libtest_still_runs_here() {
    assert_eq!(1, 1);
}
"#,
        )],
    )?;
    // The manifest declares nothing for src/misdeclared.rs: the
    // registration's target is missing from Cargo metadata.
    let files = [PathBuf::from("src/misdeclared.rs")];
    let registrations = [custom_target_registration("src/misdeclared.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(
        index.harness_subjects.is_empty(),
        "a misdeclared target must not establish trial subjects: {:?}",
        index.harness_subjects
    );
    assert!(
        index
            .tests
            .iter()
            .any(|test| test.name == "ordinary_libtest_still_runs_here"),
        "the ordinary #[test] keeps running: the misdeclared file is not demoted"
    );
    let function = index
        .functions
        .iter()
        .find(|function| function.name == "ordinary_libtest_still_runs_here")
        .ok_or("missing fn")?;
    assert_eq!(
        function.source_role,
        FunctionSourceRole::TestAttribute,
        "per-function behavior is retained: no file-wide demotion"
    );
    let conflict = index
        .harness_limitations
        .iter()
        .find(|limitation| limitation.code == "target_not_declared")
        .ok_or("missing target_not_declared limitation")?;
    assert!(
        conflict.detail.contains("src/misdeclared.rs"),
        "the conflict names the misdeclared target: {:?}",
        conflict.detail
    );
    Ok(())
}

/// #3608: a target known to Cargo only through autodiscovery still has
/// `harness = true` (Cargo's default), so a `custom_harness` registration
/// on the conventional layout records the harness-flag conflict and the
/// file keeps its ordinary libtest behavior.
#[test]
fn harness_enabled_target_records_conflict_and_keeps_executable_tests()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("harness-enabled-target")?;
    write_workspace(
        &root,
        &[(
            "tests/auto_discovered.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("never_runs_under_libtest", || Ok(()))]
}

#[test]
fn ordinary_libtest_still_runs_here() {
    assert_eq!(1, 1);
}
"#,
        )],
    )?;
    // No [[test]] entry: tests/auto_discovered.rs is autodiscovered with
    // the harness enabled.
    let files = [PathBuf::from("tests/auto_discovered.rs")];
    let registrations = [custom_target_registration("tests/auto_discovered.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert_eq!(
        index
            .tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ordinary_libtest_still_runs_here"],
        "the autodiscovered #[test] keeps running; no demotion happened"
    );
    let conflict = index
        .harness_limitations
        .iter()
        .find(|limitation| limitation.code == "harness_flag_conflict")
        .ok_or("missing harness_flag_conflict limitation")?;
    assert!(
        conflict.detail.contains("harness = true"),
        "the conflict names the still-enabled harness flag: {:?}",
        conflict.detail
    );
    Ok(())
}

/// #3608: an explicit `harness = false` declaration on the conventional
/// `tests/` layout (a name-only `[[test]]` entry) confirms the
/// registration's premise — the adapter runs and the trial subject is
/// established with no conflict recorded.
#[test]
fn explicit_harness_false_declaration_on_conventional_layout_is_accepted()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("declared-conventional")?;
    write_workspace(
        &root,
        &[(
            "tests/suite.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("declared_case", || Ok(()))]
}
"#,
        )],
    )?;
    declare_harness_false_target(&root, "suite", "tests/suite.rs")?;
    let files = [PathBuf::from("tests/suite.rs")];
    let registrations = [custom_target_registration("tests/suite.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["declared_case"]
    );
    assert!(index.harness_limitations.is_empty());
    Ok(())
}

/// #3608: without any readable owning manifest, the `harness = false`
/// premise cannot be established from Cargo metadata; the registration
/// records the typed limitation and grants nothing.
#[test]
fn unreadable_manifest_records_manifest_unavailable_and_grants_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("manifest-unavailable")?;
    fs::create_dir_all(root.0.join("tests"))?;
    fs::write(
        root.0.join("tests/orphan_mimic.rs"),
        "use libtest_mimic::Trial;\n\nfn trials() -> Vec<Trial> {\n    vec![Trial::test(\"unverifiable_case\", || Ok(()))]\n}\n",
    )?;
    // No Cargo.toml exists at the package root: the premise cannot be
    // established from Cargo metadata.
    let files = [PathBuf::from("tests/orphan_mimic.rs")];
    let registrations = [custom_target_registration("tests/orphan_mimic.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(index.harness_subjects.is_empty());
    assert!(
        index
            .harness_limitations
            .iter()
            .any(|limitation| limitation.code == "manifest_unavailable"
                && limitation.detail.contains("tests/orphan_mimic.rs")),
        "{:?}",
        index.harness_limitations
    );
    Ok(())
}

/// #3608 review: a `custom_harness` target declared `harness = false`
/// outside the conventional source directories (`qa/mimic.rs`) resolves
/// ownership by manifest presence, so the adapter runs and the trial
/// subject is established.
#[test]
fn nonconventional_directory_target_declared_harness_false_is_accepted()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("nonconventional-target")?;
    write_workspace(
        &root,
        &[(
            "qa/mimic.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("qa_case", || Ok(()))]
}
"#,
        )],
    )?;
    declare_harness_false_target(&root, "mimic", "qa/mimic.rs")?;
    let files = [PathBuf::from("qa/mimic.rs")];
    let registrations = [custom_target_registration("qa/mimic.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["qa_case"]
    );
    assert!(index.harness_limitations.is_empty());
    Ok(())
}

/// #3608 review: a readable but malformed owning manifest cannot
/// establish the registration's premise — the registry records the
/// `manifest_unavailable` limitation instead of a target typo, and the
/// target keeps its ordinary per-function behavior.
#[test]
fn malformed_manifest_records_manifest_unavailable_and_keeps_per_function_roles()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("malformed-manifest")?;
    fs::create_dir_all(root.0.join("src"))?;
    fs::write(root.0.join("Cargo.toml"), "not [ valid toml")?;
    fs::write(
        root.0.join("src/malformed_mimic.rs"),
        "use libtest_mimic::Trial;\n\nfn trials() -> Vec<Trial> {\n    vec![Trial::test(\"ghost_case\", || Ok(()))]\n}\n\n#[test]\nfn ordinary_libtest_still_runs_here() {\n    assert_eq!(1, 1);\n}\n",
    )?;
    let files = [PathBuf::from("src/malformed_mimic.rs")];
    let registrations = [custom_target_registration("src/malformed_mimic.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert!(
        index.harness_subjects.is_empty(),
        "{:?}",
        index.harness_subjects
    );
    assert!(
        index
            .tests
            .iter()
            .any(|test| test.name == "ordinary_libtest_still_runs_here"),
        "the ordinary #[test] keeps running: nothing is demoted"
    );
    let conflict = index
        .harness_limitations
        .iter()
        .find(|limitation| limitation.code == "manifest_unavailable")
        .ok_or("missing manifest_unavailable limitation")?;
    assert!(
        conflict.detail.contains("src/malformed_mimic.rs"),
        "{:?}",
        conflict.detail
    );
    Ok(())
}

/// #3608 review / cache invalidation: the per-file fact cache must not
/// bypass the Cargo target metadata validation. The cold run records the
/// conflict limitation; the warm run (same inputs, cache hit) still
/// reaches the validation and records the same limitation — the registry
/// applies after cache retrieval on every build.
#[test]
fn warm_file_fact_cache_still_reaches_cargo_target_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("warm-validation")?;
    fs::create_dir_all(root.0.join("src"))?;
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = 'harness-fixture'\nversion = '0.1.0'\nedition = '2024'\n",
    )?;
    // The manifest deliberately declares nothing for the target: the
    // registration below is misdeclared.
    let file = PathBuf::from("src/misdeclared_mimic.rs");
    let source = br#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("ghost_case", || Ok(()))]
}

#[test]
fn ordinary_libtest_still_runs_here() {
    assert_eq!(1, 1);
}
"#
    .to_vec();
    let files = [(file.clone(), source)];
    let registrations = [custom_target_registration("src/misdeclared_mimic.rs")];

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
    assert_eq!(
        warm.file_fact_cache.hits, 1,
        "the warm run must be a cache hit"
    );
    for (label, index) in [("cold", &cold.index), ("warm", &warm.index)] {
        assert!(
            index.harness_subjects.is_empty(),
            "{label}: a misdeclared target establishes no subjects"
        );
        assert!(
            index
                .harness_limitations
                .iter()
                .any(|limitation| limitation.code == "target_not_declared"
                    && limitation.detail.contains("src/misdeclared_mimic.rs")),
            "{label}: the conflict limitation must be recorded on cache hits too: {:?}",
            index.harness_limitations
        );
        let function = index
            .functions
            .iter()
            .find(|function| function.name == "ordinary_libtest_still_runs_here")
            .ok_or("missing fn")?;
        assert_eq!(
            function.source_role,
            FunctionSourceRole::TestAttribute,
            "{label}: per-function behavior is retained on the warm path too"
        );
    }
    Ok(())
}

/// #3608 review (Fe25): declaration-driven ownership — a workspace-root
/// package's explicit `harness = false` declaration claims its target even
/// when the target sits below a directory containing another (undeclaring)
/// Cargo.toml; the trial subject is established with no conflict recorded.
#[test]
fn root_declaration_claims_a_target_below_a_nested_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("root-declared-nested")?;
    write_workspace(
        &root,
        &[(
            "below/nested/manifest/dir/mimic.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("root_claimed_case", || Ok(()))]
}
"#,
        )],
    )?;
    // The nested manifest directory declares nothing for the target.
    fs::create_dir_all(root.0.join("below/nested/manifest"))?;
    fs::write(
        root.0.join("below/nested/manifest/Cargo.toml"),
        "[package]\nname = 'nested'\nversion = '0.1.0'\nedition = '2024'\n",
    )?;
    declare_harness_false_target(&root, "mimic", "below/nested/manifest/dir/mimic.rs")?;
    let files = [PathBuf::from("below/nested/manifest/dir/mimic.rs")];
    let registrations = [custom_target_registration(
        "below/nested/manifest/dir/mimic.rs",
    )];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["root_claimed_case"],
        "the root declaration claims the target across the nested manifest"
    );
    assert!(index.harness_limitations.is_empty());
    Ok(())
}

/// #3608 review round four (Gajt): Cargo permits an explicit `[[test]]`
/// path to resolve outside the declaring package's directory — a sibling
/// package's `../../shared/mimic.rs` harness = false declaration claims
/// the shared target, so the adapter establishes its trial subjects.
#[test]
fn shared_target_declared_from_a_sibling_package_is_accepted()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("shared-target")?;
    write_workspace(
        &root,
        &[(
            "shared/mimic.rs",
            r#"
use libtest_mimic::Trial;

fn trials() -> Vec<Trial> {
    vec![Trial::test("shared_case", || Ok(()))]
}
"#,
        )],
    )?;
    // The declaring package sits beside shared/ as a declared workspace
    // member; its target path escapes its own directory.
    fs::create_dir_all(root.0.join("crates/a"))?;
    fs::write(
        root.0.join("crates/a/Cargo.toml"),
        "[package]\nname = 'a'\nversion = '0.1.0'\nedition = '2024'\n\n\
         [[test]]\nname = 'mimic'\npath = '../../shared/mimic.rs'\nharness = false\n",
    )?;
    fs::write(
        root.0.join("Cargo.toml"),
        "[package]\nname = 'harness-fixture'\nversion = '0.1.0'\nedition = '2024'\n\n\
         [workspace]\nmembers = ['crates/a']\n",
    )?;
    let files = [PathBuf::from("shared/mimic.rs")];
    let registrations = [custom_target_registration("shared/mimic.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    assert_eq!(
        index
            .harness_subjects
            .iter()
            .map(|subject| subject.name.as_str())
            .collect::<Vec<_>>(),
        vec!["shared_case"],
        "the sibling package's declaration claims the shared target"
    );
    assert!(index.harness_limitations.is_empty());
    Ok(())
}

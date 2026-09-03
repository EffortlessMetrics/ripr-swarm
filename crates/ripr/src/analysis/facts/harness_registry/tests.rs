//! Discriminating tests for the repository-governed harness registry
//! (#3532): one per required fixture family, each pinning a boundary the
//! issue names — exact subjects, inert attributes, dynamic limitations,
//! lookalike and ambiguous controls, and stale/wrong-target fail-closed
//! behavior.

use super::*;
use crate::analysis::facts::build_index_with_test_harnesses;
use crate::analysis::facts::model::FileFacts;
use crate::domain::{OracleKind, OracleStrength};
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
fn helper_callback_bodies_contribute_one_level_of_subject_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    // #3603 gap 1: a trial registered as `Trial::test("name", helper_fn)`
    // exercises the helper's body. The subject's claimed span stays the
    // registration invocation, but its evidence (calls, oracles,
    // literals) must be derived from the resolved helper body one level
    // deep, with real line attribution — otherwise the subject
    // understates observation relative to an ordinary `#[test]` that
    // calls the same helper.
    let root = temp_dir("helper-callback-body")?;
    write_workspace(
        &root,
        &[(
            "tests/helper_body.rs",
            r#"
use libtest_mimic::Trial;

fn parse_config(raw: &str) -> u16 {
    raw.parse().unwrap_or(0)
}

fn check_beta() -> Result<(), String> {
    let port = parse_config("8080").unwrap();
    assert_eq!(port, 8080);
    Ok(())
}

fn trials() -> Vec<Trial> {
    vec![
        Trial::test("beta_round_trips", check_beta),
        Trial::test("gamma_foreign", other_crate::check),
    ]
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/helper_body.rs")];
    let registrations = [custom_target_registration("tests/helper_body.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;

    let beta = index
        .harness_subjects
        .iter()
        .find(|subject| subject.name == "beta_round_trips")
        .ok_or("beta subject missing")?;
    // The claimed identity span stays the registration invocation: the
    // helper widens the evidence, not the subject span or body.
    assert_eq!(beta.start_line, 16, "{:?}", beta.start_line);
    assert_eq!(beta.end_line, 16, "{:?}", beta.end_line);
    assert!(
        !beta.body.contains("parse_config"),
        "the subject body stays the invocation span: {:?}",
        beta.body
    );

    // Calls observed in the helper body join the subject with real lines.
    let beta_calls = beta
        .calls
        .iter()
        .map(|call| (call.line, call.name.as_str()))
        .collect::<Vec<_>>();
    assert!(
        beta_calls.contains(&(9, "parse_config")),
        "the helper body's call is subject evidence: {beta_calls:?}"
    );

    // One level deep only: the transitive callee's body (parse_config,
    // lines 4-6) stays unclaimed — its internals are not code the trial
    // subject's own callback text exercises directly.
    assert!(
        beta_calls.iter().all(|(line, _)| *line >= 8),
        "transitive helper bodies stay unclaimed: {beta_calls:?}"
    );

    // Oracles from the helper body carry the helper's real source lines:
    // the helper's own method unwrap (gap 2, helper path) and its macro
    // assertion.
    let unwrap_oracle = beta
        .assertions
        .iter()
        .find(|oracle| oracle.text.contains(".unwrap()"))
        .ok_or("helper unwrap oracle missing")?;
    assert_eq!(unwrap_oracle.line, 9, "{:?}", unwrap_oracle.line);
    assert_eq!(unwrap_oracle.kind, OracleKind::SmokeOnly);
    assert_eq!(unwrap_oracle.strength, OracleStrength::Smoke);
    let assert_eq = beta
        .assertions
        .iter()
        .find(|oracle| oracle.text.starts_with("assert_eq!"))
        .ok_or("helper assert_eq oracle missing")?;
    assert_eq!(assert_eq.line, 10, "{:?}", assert_eq.line);
    // Helper-body literals join the subject evidence.
    assert!(
        beta.literals
            .iter()
            .any(|literal| literal.value == "8080" && literal.line == 10),
        "{:?}",
        beta.literals
    );

    // Fail-closed control: a path callback (`other_crate::check`) is not a
    // bare identifier bound in this file, so nothing is invented for it.
    let gamma = index
        .harness_subjects
        .iter()
        .find(|subject| subject.name == "gamma_foreign")
        .ok_or("gamma subject missing")?;
    assert!(
        gamma.calls.iter().all(|call| call.name != "check"),
        "unresolved callbacks contribute no invented evidence: {:?}",
        gamma.calls
    );
    assert!(gamma.assertions.is_empty(), "{:?}", gamma.assertions);

    // The mirrored TestFact carries the same widened evidence so every
    // existing test consumer sees the subject the same way.
    let beta_test = index
        .tests
        .iter()
        .find(|test| test.name == "beta_round_trips")
        .ok_or("beta test missing")?;
    assert!(
        beta_test
            .calls
            .iter()
            .any(|call| call.name == "parse_config"),
        "{:?}",
        beta_test.calls
    );
    assert!(
        beta_test
            .assertions
            .iter()
            .any(|oracle| oracle.text.starts_with("assert_eq!") && oracle.line == 10),
        "{:?}",
        beta_test.assertions
    );
    Ok(())
}

#[test]
fn trial_method_unwrap_expect_oracles_carry_real_lines_and_receivers()
-> Result<(), Box<dyn std::error::Error>> {
    // #3603 gap 2: ordinary `#[test]` parsing records `.unwrap()` /
    // `.expect()` method calls as smoke evidence; the trial token scanner
    // must credit the same oracles for trial subjects, with real line
    // attribution and the receiver expression in the oracle text.
    let root = temp_dir("trial-method-oracles")?;
    write_workspace(
        &root,
        &[(
            "tests/method_oracles.rs",
            r#"
use libtest_mimic::Trial;

fn parse_port(raw: &str) -> Result<u16, String> {
    raw.parse::<u16>().map_err(|error| error.to_string())
}

fn trials() -> Vec<Trial> {
    vec![
        libtest_mimic::Trial::test("alpha_unwraps", || {
            let port = parse_port("8080").unwrap();
            let doubled = port.unwrap() * 2;
            assert_eq!(doubled, 16160);
            let label: Result<String, String> = Ok(format!("port={doubled}"));
            let shown = label.expect("label ready");
            let unwrapped_value = doubled;
            let _ = unwrapped_value;
            let _ = config.expect;
            println!("{}", doubled.unwrap());
            Result::<u8, u8>::unwrap(Ok(1));
            Ok(())
        }),
    ]
}

struct Config {
    expect: u8,
}
"#,
        )],
    )?;
    let files = [PathBuf::from("tests/method_oracles.rs")];
    let registrations = [custom_target_registration("tests/method_oracles.rs")];
    let index = build_index_with_test_harnesses(&root.0, &files, &registrations)?;
    let alpha = index
        .tests
        .iter()
        .find(|test| test.name == "alpha_unwraps")
        .ok_or("alpha test missing")?;

    let oracle_lines = |needle: &str| -> Vec<(usize, &String)> {
        alpha
            .assertions
            .iter()
            .filter(|oracle| oracle.text.contains(needle))
            .map(|oracle| (oracle.line, &oracle.text))
            .collect()
    };
    let unwraps = oracle_lines("unwrap()");
    let expects = oracle_lines(".expect(");
    let assert_eqs = oracle_lines("assert_eq!(");

    // Method oracles exist with receiver-ful text and real source lines.
    assert_eq!(
        unwraps
            .iter()
            .filter(|(_, text)| text.as_str() == "parse_port(\"8080\").unwrap()")
            .count(),
        1,
        "receiver call chain is the oracle text: {unwraps:?}"
    );
    assert!(
        unwraps.contains(&(11, &"parse_port(\"8080\").unwrap()".to_string())),
        "unwrap oracle carries the real source line: {unwraps:?}"
    );
    assert!(
        unwraps.contains(&(12, &"port.unwrap()".to_string())),
        "bare-receiver unwrap keeps the receiver in the text: {unwraps:?}"
    );
    assert!(
        expects.contains(&(15, &"label.expect(\"label ready\")".to_string())),
        "expect oracle with real line and message argument: {expects:?}"
    );
    // The assertion-macro oracle line is the real source line too.
    assert!(
        assert_eqs.contains(&(13, &"assert_eq!(doubled, 16160)".to_string())),
        "macro oracle lines are real source lines: {assert_eqs:?}"
    );

    // Smoke-strength parity with ordinary `#[test]` parsing.
    for oracle in alpha
        .assertions
        .iter()
        .filter(|oracle| oracle.text.ends_with(".unwrap()") || oracle.text.contains(".expect("))
    {
        assert_eq!(oracle.kind, OracleKind::SmokeOnly, "{oracle:?}");
        assert_eq!(oracle.strength, OracleStrength::Smoke, "{oracle:?}");
    }

    // Fail-closed controls: a struct field named `expect`, a path-shaped
    // `Result::unwrap` call, and anything inside a non-assertion macro's
    // input never classify.
    assert!(
        unwraps.iter().all(|(_, text)| *text != "unwrap"),
        "{unwraps:?}"
    );
    assert_eq!(
        expects.len(),
        1,
        "only the real method-position expect classifies: {expects:?}"
    );
    assert!(
        alpha
            .assertions
            .iter()
            .all(|oracle| !oracle.text.contains("::unwrap")
                && !oracle.text.contains("doubled.unwrap()")),
        "path-shaped calls and non-assertion macro inputs stay out: {:?}",
        alpha.assertions
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

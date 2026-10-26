//! Public-API controls for a changed private helper that tests reach only
//! through a public inherent method (#8999).
//!
//! The shape is a version-requirement crate: `VersionReq::matches` calls
//! `eval::matches_req(..)` (a module-qualified call), which calls
//! `matches_impl`, which calls the changed `matches_caret`. A test says
//! `req.matches(&version)` and never names any helper. The test reaches the
//! helper as `helper_owner_call` only when its receiver is syntactically
//! bound to `VersionReq`; anything less stays unrelated. The reach never
//! raises a class: the helper's result is not shown to be forwarded.

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_LINE: &str = "    ver.major >= cmp.major\n";
const NEW_LINE: &str = "    ver.major >= cmp.major + 1\n";

const LIB: &str = "mod eval;

pub struct Version {
    pub major: u64,
}

pub struct Comparator {
    pub major: u64,
}

pub struct VersionReq {
    pub comparators: Vec<Comparator>,
}

impl VersionReq {
    pub fn matches(&self, version: &Version) -> bool {
        eval::matches_req(&self.comparators, version)
    }
}
";

fn eval_source(line: &str) -> String {
    format!(
        "use crate::{{Comparator, Version}};

pub(crate) fn matches_req(comparators: &[Comparator], ver: &Version) -> bool {{
    for cmp in comparators {{
        if !matches_impl(cmp, ver) {{
            return false;
        }}
    }}
    true
}}

fn matches_impl(cmp: &Comparator, ver: &Version) -> bool {{
    matches_caret(cmp, ver)
}}

fn matches_caret(cmp: &Comparator, ver: &Version) -> bool {{
{line}}}
"
    )
}

struct TempRepo {
    root: PathBuf,
    changed_line: usize,
}

impl TempRepo {
    /// A crate whose `src/lib.rs` is `lib` and whose `tests/api.rs` is
    /// `tests`, with `matches_caret` changed in `src/eval.rs`.
    fn create(lib: &str, tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-pubapi-method-entry-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create tests directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"verreq\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write lib.rs failed: {error}"))?;
        let eval = eval_source(NEW_LINE);
        let changed_line = eval
            .lines()
            .position(|line| line == NEW_LINE.trim_end_matches('\n'))
            .map(|index| index + 1)
            .ok_or("premise: the changed line is in eval.rs")?;
        std::fs::write(root.join("src/eval.rs"), &eval)
            .map_err(|error| format!("write eval.rs failed: {error}"))?;
        std::fs::write(root.join("tests/api.rs"), tests)
            .map_err(|error| format!("write api.rs failed: {error}"))?;
        std::fs::write(
            root.join("diff.patch"),
            format!(
                "diff --git a/src/eval.rs b/src/eval.rs
index 1111111..2222222 100644
--- a/src/eval.rs
+++ b/src/eval.rs
@@ -{changed_line},1 +{changed_line},1 @@
-{OLD_LINE}+{NEW_LINE}"
            ),
        )
        .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root, changed_line })
    }

    /// Every finding the changed line produced.
    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let findings: Vec<Finding> = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == self.changed_line)
            .collect();
        if findings.is_empty() {
            return Err("premise: the changed line yields a finding".to_string());
        }
        for finding in &findings {
            let owner = finding.probe.owner.as_ref().map(|owner| owner.0.as_str());
            if !owner.is_some_and(|owner| owner.ends_with("::matches_caret")) {
                return Err(format!(
                    "premise: the finding is owned by matches_caret: {finding:?}"
                ));
            }
        }
        Ok(findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Names of the tests some finding relates to the helper, by reason.
fn tests_by(findings: &[Finding], reason: RelationReason) -> Vec<String> {
    let mut names: Vec<String> = findings
        .iter()
        .flat_map(|finding| &finding.related_tests)
        .filter(|test| test.relation_reason == Some(reason))
        .map(|test| test.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Every test name any finding relates to the helper by a call-shaped reason.
fn call_related(findings: &[Finding]) -> Vec<String> {
    let mut names: Vec<String> = [
        RelationReason::HelperOwnerCall,
        RelationReason::DirectOwnerCall,
    ]
    .into_iter()
    .flat_map(|reason| tests_by(findings, reason))
    .collect();
    names.sort();
    names.dedup();
    names
}

fn assert_no_exposed(findings: &[Finding]) {
    for finding in findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "reaching the helper through a public method never raises a class by itself: {finding:?}"
        );
    }
}

fn api_tests(body: &str) -> String {
    format!("use verreq::{{Comparator, Version, VersionReq}};\n\n{body}")
}

const LITERAL_RECEIVER: &str = "#[test]
fn caret_matches_a_newer_version() {
    let req = VersionReq {
        comparators: vec![Comparator { major: 1 }],
    };
    assert!(req.matches(&Version { major: 5 }));
    assert!(!req.matches(&Version { major: 0 }));
}
";

#[test]
fn a_typed_receiver_reaches_a_private_helper_through_the_public_method() -> Result<(), String> {
    let repo = TempRepo::create(LIB, &api_tests(LITERAL_RECEIVER))?;
    let findings = repo.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["caret_matches_a_newer_version"],
        "{findings:?}"
    );
    // The reach is real but the helper's result is not shown to reach the
    // assertion, so the class stays below exposed: it was no_static_path.
    for finding in &findings {
        assert_eq!(
            finding.class,
            ExposureClass::InfectionUnknown,
            "{finding:?}"
        );
    }
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn an_annotated_receiver_reaches_the_helper_too() -> Result<(), String> {
    let repo = TempRepo::create(
        LIB,
        &api_tests(
            "fn make() -> VersionReq {
    VersionReq { comparators: Vec::new() }
}

#[test]
fn annotated_receiver() {
    let req: VersionReq = make();
    assert!(!req.matches(&Version { major: 0 }));
}
",
        ),
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["annotated_receiver"],
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_factory_receiver_without_a_type_annotation_stays_unrelated() -> Result<(), String> {
    let repo = TempRepo::create(
        LIB,
        &api_tests(
            "fn make() -> VersionReq {
    VersionReq { comparators: Vec::new() }
}

#[test]
fn factory_receiver() {
    let req = make();
    assert!(!req.matches(&Version { major: 0 }));
}
",
        ),
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        call_related(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_same_named_method_on_a_str_is_not_the_public_method() -> Result<(), String> {
    let repo = TempRepo::create(
        LIB,
        &api_tests(
            "#[test]
fn counts_substrings() {
    let text = \"caret caret\";
    assert_eq!(text.matches(\"caret\").count(), 2);
}
",
        ),
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        call_related(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_second_method_with_the_entry_name_fails_closed() -> Result<(), String> {
    let lib = format!(
        "{LIB}
pub struct Other;

impl Other {{
    pub fn matches(&self) -> bool {{
        true
    }}
}}
"
    );
    let repo = TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?;
    let findings = repo.findings()?;
    assert_eq!(
        call_related(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_receiver_type_renamed_from_a_foreign_import_stays_unrelated() -> Result<(), String> {
    let repo = TempRepo::create(
        LIB,
        "use other_crate::Req as VersionReq;
use verreq::{Comparator, Version};

#[test]
fn foreign_receiver() {
    let req = VersionReq {
        comparators: vec![Comparator { major: 1 }],
    };
    assert!(req.matches(&Version { major: 5 }));
}
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        call_related(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_call_spelled_through_an_imported_module_name_fails_closed() -> Result<(), String> {
    // `eval` here may be another module: the import rebinds the first path
    // segment, so the hop below `matches` does not bind.
    let lib = LIB.replace(
        "mod eval;\n",
        "mod eval;\nuse elsewhere::eval as imported;\n",
    );
    let lib = lib.replace("eval::matches_req", "imported::matches_req");
    let repo = TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?;
    let findings = repo.findings()?;
    assert_eq!(
        call_related(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

fn assert_unrelated(repo: &TempRepo) -> Result<(), String> {
    let findings = repo.findings()?;
    assert!(call_related(&findings).is_empty(), "{findings:?}");
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn an_unrenamed_foreign_receiver_import_never_borrows_the_owner() -> Result<(), String> {
    let tests = format!(
        "use other_crate::VersionReq;\nuse verreq::{{Comparator, Version}};\n{LITERAL_RECEIVER}"
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_foreign_ufcs_call_does_not_borrow_a_genuine_unused_import() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn foreign_call() {\n    assert!(other_crate::VersionReq::matches(&other_crate::VersionReq::default(), &Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_factory_named_new_does_not_establish_the_returned_receiver() -> Result<(), String> {
    let lib = format!(
        "{LIB}\nimpl VersionReq {{\n    pub fn new() -> other_crate::VersionReq {{ other_crate::VersionReq::default() }}\n}}\n"
    );
    let tests = api_tests(
        "#[test]\nfn foreign_factory() {\n    let req = VersionReq::new();\n    assert!(req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn a_block_local_crate_alias_does_not_borrow_the_library_module() -> Result<(), String> {
    let lib = LIB.replace(
        "        eval::matches_req",
        "        extern crate other_crate as eval;\n        eval::matches_req",
    );
    assert_unrelated(&TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?)
}

#[test]
fn a_block_local_type_alias_does_not_borrow_the_library_module() -> Result<(), String> {
    let lib = LIB.replace(
        "        eval::matches_req",
        "        type eval = other_crate::Eval;\n        eval::matches_req",
    );
    assert_unrelated(&TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?)
}

#[test]
fn a_unicode_qualifier_neither_panics_nor_borrows_an_ascii_namesake() -> Result<(), String> {
    let lib = LIB.replace("eval::matches_req", "óeval::matches_req");
    assert_unrelated(&TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?)
}

#[test]
fn a_sibling_type_namesake_does_not_borrow_the_owner_method() -> Result<(), String> {
    let lib = format!(
        "{LIB}\npub mod sibling {{\n    pub struct VersionReq {{ pub comparators: Vec<super::Comparator> }}\n}}\n"
    );
    let tests = format!(
        "use verreq::sibling::VersionReq;\nuse verreq::{{Comparator, Version}};\n{LITERAL_RECEIVER}"
    );
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn a_nested_test_module_keeps_its_explicit_owner_import() -> Result<(), String> {
    let tests = format!("mod nested {{\n{}\n}}\n", api_tests(LITERAL_RECEIVER));
    let findings = TempRepo::create(LIB, &tests)?.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["caret_matches_a_newer_version"]
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn an_inner_receiver_binding_does_not_type_an_outer_namesake() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn outer_receiver() {\n    let req = make_foreign();\n    { let req: VersionReq = make_own(); let _ = req; }\n    assert!(req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_closure_receiver_does_not_borrow_an_outer_type() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn closure_receiver() {\n    let req: VersionReq = make_own();\n    let checks = foreign_values().map(|req| req.matches(&Version { major: 5 })).collect::<Vec<_>>();\n    assert!(checks[0]);\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_statement_macro_cannot_inject_a_qualified_hop_binding() -> Result<(), String> {
    let lib = LIB.replace(
        "        eval::matches_req",
        "        other_crate::bind_eval!();\n        eval::matches_req",
    );
    assert_unrelated(&TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?)
}

#[test]
fn a_statement_macro_cannot_inject_the_receiver_type() -> Result<(), String> {
    let tests = api_tests(
        &LITERAL_RECEIVER.replace("    let req", "    other_crate::bind_type!();\n    let req"),
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_later_binding_does_not_type_an_earlier_receiver_call() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn earlier_receiver() {\n    let req = make_foreign();\n    assert!(req.matches(&Version { major: 5 }));\n    let req: VersionReq = make_own();\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_macro_argument_closure_does_not_borrow_an_outer_receiver_type() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn hidden_closure() {\n    let req: VersionReq = make_own();\n    assert!((|req| req.matches(&Version { major: 5 }))(make_foreign()));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_matches_pattern_does_not_borrow_an_outer_receiver_type() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn pattern_receiver() {\n    let req: VersionReq = make_own();\n    assert!(matches!(make_foreign(), req if req.matches(&Version { major: 5 })));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn stringified_method_tokens_do_not_establish_reach() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn inert_method() {\n    let req: VersionReq = make_own();\n    assert_eq!(stringify!(req.matches(&Version { major: 5 })), \"text\");\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_raw_receiver_binding_is_the_same_shadowing_name() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn raw_shadow() {\n    let req: VersionReq = make_own();\n    let r#req = make_foreign();\n    assert!(req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn stringified_qualified_helper_tokens_do_not_establish_reach() -> Result<(), String> {
    let lib = LIB.replace("        eval::matches_req(&self.comparators, version)", "        let _text = stringify!(eval::matches_req(&self.comparators, version));\n        false");
    assert_unrelated(&TempRepo::create(&lib, &api_tests(LITERAL_RECEIVER))?)
}

#[test]
fn a_unicode_receiver_does_not_borrow_an_ascii_suffix_binding() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn unicode_receiver() {\n    let req: VersionReq = make_own();\n    let óreq = make_foreign();\n    assert!(óreq.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_field_receiver_does_not_borrow_a_local_namesake() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn field_receiver() {\n    let req: VersionReq = make_own();\n    let holder = make_holder();\n    assert!(holder.req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_qualified_value_receiver_does_not_borrow_a_local_namesake() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn qualified_receiver() {\n    let req: VersionReq = make_own();\n    assert!(foreign::req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_nested_import_cannot_replace_the_annotated_receiver() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn imported_receiver() {\n    let req: VersionReq = make_own();\n    { use foreign::req; assert!(req.matches(&Version { major: 5 })); }\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_nested_const_cannot_replace_the_annotated_receiver() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn constant_receiver() {\n    let req: VersionReq = make_own();\n    { const req: Foreign = FOREIGN; assert!(req.matches(&Version { major: 5 })); }\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn an_assertion_block_static_cannot_replace_the_annotated_receiver() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn static_receiver() {\n    let req: VersionReq = make_own();\n    assert!({ static req: Foreign = FOREIGN; req.matches(&Version { major: 5 }) });\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn generic_specializations_do_not_share_a_plain_impl_owner() -> Result<(), String> {
    let lib = LIB.replace(
        "pub struct VersionReq {",
        "pub struct VersionReq<const N: usize = 0> {",
    );
    let tests = api_tests(
        "use external::Matches;\n#[test]\nfn specialized_receiver() {\n    let req: VersionReq<1> = make_other_specialization();\n    assert!(req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn an_imported_trait_may_precede_the_inherent_receiver_candidate() -> Result<(), String> {
    let tests = format!("use external::Matches;\n{}", api_tests(LITERAL_RECEIVER));
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn an_underscore_trait_import_still_participates_in_dispatch() -> Result<(), String> {
    let tests = format!(
        "use external::Matches as _;\n{}",
        api_tests(LITERAL_RECEIVER)
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_prelude_consuming_method_may_precede_an_inherent_shared_receiver() -> Result<(), String> {
    let lib = LIB.replace("fn matches(", "fn into_future(");
    let tests = api_tests(&LITERAL_RECEIVER.replace(".matches(", ".into_future("));
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn a_mutable_owner_receiver_is_outside_the_bounded_entry_shape() -> Result<(), String> {
    let lib = LIB.replace("matches(&self", "matches(&mut self");
    let tests = api_tests(&LITERAL_RECEIVER.replace("let req", "let mut req"));
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn a_ref_pattern_does_not_bind_a_by_value_receiver() -> Result<(), String> {
    let tests = api_tests(&LITERAL_RECEIVER.replace("let req", "let ref req"));
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_reference_annotation_is_outside_the_bounded_entry_shape() -> Result<(), String> {
    let tests = api_tests(
        "#[test]\nfn borrowed_receiver() {\n    let req: &VersionReq = borrow_own();\n    assert!(req.matches(&Version { major: 5 }));\n}\n",
    );
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn an_inactive_data_declaration_does_not_hide_a_trait_reexport() -> Result<(), String> {
    let lib =
        format!("{LIB}\n#[cfg(any())]\npub struct Extra;\npub use external::Matches as Extra;\n");
    let tests = format!("use verreq::Extra;\n{}", api_tests(LITERAL_RECEIVER));
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn an_attribute_cannot_change_an_imported_data_declaration_into_a_trait() -> Result<(), String> {
    let lib = format!("{LIB}\n#[external::rewrite]\npub struct Extra;\n");
    let tests = format!("use verreq::Extra;\n{}", api_tests(LITERAL_RECEIVER));
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

#[test]
fn an_attribute_cannot_replace_the_test_receiver_bindings() -> Result<(), String> {
    let tests = api_tests(&LITERAL_RECEIVER.replace("#[test]", "#[test]\n#[external::rewrite]"));
    assert_unrelated(&TempRepo::create(LIB, &tests)?)
}

#[test]
fn a_grouped_self_import_does_not_borrow_a_test_only_data_declaration() -> Result<(), String> {
    let lib =
        format!("{LIB}\n#[cfg(test)]\npub struct Extra;\npub use external::Extra::{{self}};\n");
    let tests = format!("use verreq::Extra;\n{}", api_tests(LITERAL_RECEIVER));
    assert_unrelated(&TempRepo::create(&lib, &tests)?)
}

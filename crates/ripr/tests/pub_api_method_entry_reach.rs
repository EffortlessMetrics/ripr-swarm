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

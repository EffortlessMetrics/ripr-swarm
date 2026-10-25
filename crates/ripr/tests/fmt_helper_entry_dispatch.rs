//! Public-API controls for a changed helper whose only caller is a
//! `Display::fmt` impl (#8999).
//!
//! A test that formats a value of the impl's type (`merge.to_string()` inside
//! a multi-line `assert_eq!`) runs `fmt`, which calls the changed helper,
//! without ever spelling either name. The test reaches the helper through the
//! one caller as `helper_owner_call` when the formatted value's type is
//! syntactically established; anything less stays unrelated. The reach never
//! raises a class: the helper's result is not shown to be forwarded.

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_BODY: &str = "    s.to_string()\n";
const NEW_BODY: &str = "    s.replace('\\n', \" \")\n";

fn source(extra_items: &str, tests: &str, body: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    labels: Vec<String>,
}}

fn sanitize_label(s: &str) -> String {{
{body}}}

impl Merge {{
    pub fn new(labels: Vec<String>) -> Self {{
        Merge {{ labels }}
    }}
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
        for l in &self.labels {{
            writeln!(f, \"{{}}\", sanitize_label(l))?;
        }}
        Ok(())
    }}
}}
{extra_items}
#[cfg(test)]
mod tests {{
    use super::*;

{tests}}}
"
    )
}

struct TempRepo {
    root: PathBuf,
    body_line: usize,
}

impl TempRepo {
    fn create(extra_items: &str, tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fmt-helper-entry-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"merged\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let lib = source(extra_items, tests, NEW_BODY);
        let body_line = lib
            .lines()
            .position(|line| line == NEW_BODY.trim_end_matches('\n'))
            .map(|index| index + 1)
            .ok_or("premise: the changed helper body is in the source")?;
        std::fs::write(root.join("src/lib.rs"), &lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(
            root.join("diff.patch"),
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{body_line},1 +{body_line},1 @@
-{OLD_BODY}+{NEW_BODY}"
            ),
        )
        .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root, body_line })
    }

    /// Writes an integration test file under `tests/`.
    fn with_test_file(self, name: &str, content: &str) -> Result<Self, String> {
        std::fs::create_dir_all(self.root.join("tests"))
            .map_err(|error| format!("create tests directory failed: {error}"))?;
        std::fs::write(self.root.join("tests").join(name), content)
            .map_err(|error| format!("write {name} failed: {error}"))?;
        Ok(self)
    }

    /// Every finding the changed helper line produced.
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
            .filter(|finding| finding.probe.location.line == self.body_line)
            .collect();
        if findings.is_empty() {
            return Err("premise: the changed helper line yields a finding".to_string());
        }
        for finding in &findings {
            let owner = finding.probe.owner.as_ref().map(|owner| owner.0.as_str());
            if !owner.is_some_and(|owner| owner.ends_with("::sanitize_label")) {
                return Err(format!(
                    "premise: the finding is owned by sanitize_label: {finding:?}"
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

/// Sorted `family:class` pairs of every finding.
fn classes(findings: &[Finding]) -> Vec<String> {
    let mut pairs: Vec<String> = findings
        .iter()
        .map(|finding| format!("{:?}:{:?}", finding.probe.family, finding.class))
        .collect();
    pairs.sort();
    pairs
}

fn assert_no_exposed(findings: &[Finding]) {
    for finding in findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "reaching the helper through fmt never raises a class by itself: {finding:?}"
        );
    }
}

const MULTI_LINE_PIN: &str = "    #[test]
    fn labels_stay_on_one_line() {
        let merge: Merge = Merge::new(vec![\"a\\nb\".to_string()]);
        assert_eq!(
            merge.to_string(),
            \"a b\\n\"
        );
    }
";

#[test]
fn a_multi_line_to_string_pin_reaches_the_helper_through_display_fmt() -> Result<(), String> {
    let repo = TempRepo::create("", MULTI_LINE_PIN)?;
    let findings = repo.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["labels_stay_on_one_line"],
        "{findings:?}"
    );
    // The reach is real but the helper's result is not shown to reach the
    // pinned output, so the honest class is propagation_unknown, not exposed
    // (it read weakly_exposed with "no test is seen calling" before).
    assert_eq!(
        classes(&findings),
        [
            "CallDeletion:PropagationUnknown",
            "ReturnValue:PropagationUnknown"
        ]
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_struct_literal_receiver_reaches_the_helper_too() -> Result<(), String> {
    let repo = TempRepo::create(
        "",
        "    #[test]
    fn literal_receiver() {
        let merge = Merge { labels: vec![\"a\\nb\".to_string()] };
        assert_eq!(merge.to_string(), \"a b\\n\");
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["literal_receiver"],
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_factory_receiver_without_a_type_annotation_stays_unrelated() -> Result<(), String> {
    let repo = TempRepo::create(
        "",
        "    #[test]
    fn factory_receiver() {
        let merge = Merge::new(vec![\"a\\nb\".to_string()]);
        assert_eq!(
            merge.to_string(),
            \"a b\\n\"
        );
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
fn formatting_another_type_or_with_the_other_trait_stays_unrelated() -> Result<(), String> {
    let repo = TempRepo::create(
        "
pub struct Other {
    name: String,
}

impl fmt::Display for Other {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, \"{}\", self.name)
    }
}
",
        "    #[test]
    fn formats_the_other_type() {
        let other = Other { name: String::new() };
        assert_eq!(other.to_string(), \"\");
    }

    #[test]
    fn debug_placeholder_is_not_display() {
        let merge = Merge { labels: Vec::new() };
        assert_eq!(format!(\"{:?}\", merge), \"\");
    }

    #[test]
    fn never_formats_the_value() {
        let merge = Merge { labels: Vec::new() };
        assert_eq!(merge.labels.len(), 0);
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
fn a_helper_with_a_second_caller_fails_closed() -> Result<(), String> {
    let repo = TempRepo::create(
        "
pub fn describe(s: &str) -> String {
    sanitize_label(s)
}
",
        MULTI_LINE_PIN,
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
fn a_second_declaration_of_the_formatted_type_fails_closed() -> Result<(), String> {
    let repo = TempRepo::create(
        "
mod elsewhere {
    pub struct Merge;
}
",
        MULTI_LINE_PIN,
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
fn a_test_in_a_separate_file_reaches_the_helper_through_display_fmt() -> Result<(), String> {
    let repo = TempRepo::create("", "")?.with_test_file(
        "rendering.rs",
        "use merged::Merge;

#[test]
fn separate_file_pin() {
    let merge: Merge = Merge::new(vec![\"a\\nb\".to_string()]);
    assert_eq!(
        merge.to_string(),
        \"a b\\n\"
    );
}
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        tests_by(&findings, RelationReason::HelperOwnerCall),
        ["separate_file_pin"],
        "{findings:?}"
    );
    assert_no_exposed(&findings);
    Ok(())
}

#[test]
fn a_path_qualified_annotation_from_another_crate_stays_unrelated() -> Result<(), String> {
    let repo = TempRepo::create(
        "",
        "    #[test]
    fn foreign_merge() {
        let merge: other_crate::Merge = other_crate::Merge::new(vec![]);
        assert_eq!(
            merge.to_string(),
            \"\"
        );
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

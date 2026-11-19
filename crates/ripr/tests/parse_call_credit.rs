//! Public-API controls for relating `str::parse` to `FromStr::from_str`
//! (#5334).
//!
//! `"".parse::<Version>()` runs `from_str` of `impl FromStr for Version`
//! without naming it. A same-file test that parses a string literal into the
//! impl's type reaches the changed method exactly as `Version::from_str("")`
//! does, so the two spellings of one test read the same. A receiver ripr
//! cannot type, another target type, a redeclared type, a local `FromStr` trait
//! or a file that may generate impls relates nothing.

use ripr::domain::{ExposureClass, RelationReason};
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One impl plus the test body under study.
#[derive(Clone, Copy)]
struct Case {
    label: &'static str,
    /// Items placed after the impls, at file level.
    extra_items: &'static str,
    /// Items placed inside the test module, before the test.
    test_module_items: &'static str,
    /// Source of `src/other.rs`, a second workspace file.
    sibling: &'static str,
    test_body: &'static str,
}

const OLD_LINE: &str = "            return Err(Error::Bad);";
const NEW_LINE: &str = "            return Err(Error::Empty);";

fn source(case: &Case) -> String {
    format!(
        "use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub struct Version {{
    pub major: u64,
}}

#[derive(Debug, PartialEq)]
pub struct Other {{
    pub major: u64,
}}

#[derive(Debug, PartialEq)]
pub enum Error {{
    Empty,
    Bad,
}}

impl FromStr for Version {{
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {{
        if text.is_empty() {{
{new_line}
        }}
        let major = text.parse::<u64>().map_err(|_| Error::Bad)?;
        Ok(Version {{ major }})
    }}
}}

impl FromStr for Other {{
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {{
        Ok(Other {{ major: text.len() as u64 }})
    }}
}}
{extra}
#[cfg(test)]
mod tests {{
    use super::*;
{module_items}
    #[test]
    fn syntax_under_test() {{
        {body}
    }}
}}
",
        new_line = NEW_LINE,
        extra = case.extra_items,
        module_items = case.test_module_items,
        body = case.test_body,
    )
}

fn findings_of(case: &Case) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-parse-credit-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = run(case, &root);
    let _ = std::fs::remove_dir_all(&root);
    result.map_err(|message| format!("{}: {message}", case.label))
}

fn run(case: &Case, root: &Path) -> Result<Vec<Finding>, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| format!("create dir: {e}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"parse-credit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|e| format!("write Cargo.toml: {e}"))?;
    let lib = source(case);
    let line = lib
        .lines()
        .position(|line| line == NEW_LINE)
        .map(|at| at + 1)
        .ok_or("premise: the changed line is in the source")?;
    if !case.sibling.is_empty() {
        std::fs::write(root.join("src/other.rs"), case.sibling)
            .map_err(|e| format!("write sibling: {e}"))?;
    }
    std::fs::write(root.join("src/lib.rs"), &lib).map_err(|e| format!("write source: {e}"))?;
    std::fs::write(
        root.join("diff.patch"),
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{OLD_LINE}
+{NEW_LINE}
"
        ),
    )
    .map_err(|e| format!("write diff: {e}"))?;
    let output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if findings.is_empty() {
        return Err("premise: the changed line yields a finding".to_string());
    }
    Ok(findings)
}

/// Whether some finding on the changed line relates the test by a direct
/// owner call.
fn relates_directly(findings: &[Finding]) -> bool {
    findings.iter().any(|finding| {
        finding.related_tests.iter().any(|test| {
            test.name == "syntax_under_test"
                && test.relation_reason == Some(RelationReason::DirectOwnerCall)
        })
    })
}

fn classes(findings: &[Finding]) -> Vec<ExposureClass> {
    let mut classes: Vec<ExposureClass> = findings
        .iter()
        .map(|finding| finding.class.clone())
        .collect();
    classes.sort_by_key(|class| format!("{class:?}"));
    classes
}

const PARSED: Case = Case {
    label: "turbofish",
    extra_items: "",
    test_module_items: "",
    sibling: "",
    test_body: "assert_eq!(\"\".parse::<Version>(), Err(Error::Empty));",
};

/// The same assertion spelled as the direct owner call.
const NAMED: Case = Case {
    label: "named_from_str",
    test_body: "assert_eq!(Version::from_str(\"\"), Err(Error::Empty));",
    ..PARSED
};

fn with_body(label: &'static str, test_body: &'static str) -> Case {
    Case {
        label,
        test_body,
        ..PARSED
    }
}

#[test]
fn a_string_literal_parsed_into_the_type_reads_as_the_named_from_str_call() -> Result<(), String> {
    let named = findings_of(&NAMED)?;
    if !relates_directly(&named) {
        return Err("premise: the named call relates the test directly".to_string());
    }
    let named_classes = classes(&named);
    if !named_classes.contains(&ExposureClass::Exposed) {
        return Err(format!(
            "premise: the named call reads exposed, got {named_classes:?}"
        ));
    }
    let cases = [
        PARSED,
        with_body(
            "spaced_turbofish",
            "assert_eq!(\"\" . parse :: < Version > ( ), Err(Error::Empty));",
        ),
        with_body(
            "raw_string_receiver",
            "assert_eq!(r#\"\"#.parse::<Version>(), Err(Error::Empty));",
        ),
        with_body(
            "let_bound_result",
            "let parsed = \"\".parse::<Version>();\n        assert_eq!(parsed, Err(Error::Empty));",
        ),
    ];
    let mut failures = Vec::new();
    for case in &cases {
        let findings = findings_of(case)?;
        if !relates_directly(&findings) {
            failures.push(format!(
                "{}: not related by a direct owner call",
                case.label
            ));
        }
        if case.label == PARSED.label && classes(&findings) != named_classes {
            failures.push(format!(
                "{}: got {:?}, the named call reads {named_classes:?}",
                case.label,
                classes(&findings)
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn a_parse_ripr_cannot_tie_to_the_impl_stays_unrelated() -> Result<(), String> {
    let cases = [
        with_body(
            "untyped_receiver",
            "let text = \"\";\n        assert_eq!(text.parse::<Version>(), Err(Error::Empty));",
        ),
        with_body(
            "annotated_binding",
            "let parsed: Result<Version, Error> = \"\".parse();\n        assert_eq!(parsed, Err(Error::Empty));",
        ),
        with_body(
            "other_target_type",
            "assert_eq!(\"\".parse::<Other>(), Ok(Other { major: 0 }));",
        ),
        with_body(
            "stringified",
            "let _ = stringify!(\"\".parse::<Version>());\n        assert_eq!(Error::Empty, Error::Empty);",
        ),
        with_body(
            "commented",
            "// \"\".parse::<Version>()\n        assert_eq!(Error::Empty, Error::Empty);",
        ),
        Case {
            label: "type_redeclared_in_the_test_module",
            test_module_items: "\n    #[derive(Debug, PartialEq)]\n    struct Version;\n",
            ..PARSED
        },
        Case {
            label: "workspace_declares_a_from_str_trait",
            sibling: "pub trait FromStr {\n    fn from_str(text: &str) -> Self;\n}\n",
            ..PARSED
        },
        Case {
            label: "file_defines_a_macro",
            extra_items: "\nmacro_rules! noop {\n    () => {};\n}\n",
            ..PARSED
        },
    ];
    let mut failures = Vec::new();
    for case in &cases {
        if relates_directly(&findings_of(case)?) {
            failures.push(format!("{}: related by a direct owner call", case.label));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

//! Public-API controls for relating a loop or consuming adaptor to
//! `Iterator::next` (#9567, #10553).
//!
//! `Countdown(3).collect()` runs `next` of `impl Iterator for Countdown`
//! without naming it. A same-file test that does so with a value syntactically
//! of the impl's type reaches the changed method directly; a value ripr cannot
//! type to that exact type, a lazy adaptor, an adaptor the impl overrides, a
//! shadowing inherent method or syntax the test may never run does not.
//! Relating the test never raises a class by itself.

use ripr::domain::{ExposureClass, RelationReason};
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
enum Kind {
    Iterator,
}

/// One impl plus the test body under study.
#[derive(Clone, Copy)]
struct Case {
    label: &'static str,
    kind: Kind,
    /// Methods placed inside the iterator impl, after `next`.
    in_impl: &'static str,
    /// Extra items placed after the impl (inherent methods, another type).
    extra_items: &'static str,
    /// Source of `src/macros.rs`, a second workspace file.
    sibling: &'static str,
    test_body: &'static str,
}

const ITER_OLD: &str = "        Some(self.0 + 1)";
const ITER_NEW: &str = "        Some(1 + self.0)";

fn changed_lines(case: &Case) -> (&'static str, &'static str) {
    match case.kind {
        Kind::Iterator => (ITER_OLD, ITER_NEW),
    }
}

fn source(case: &Case) -> String {
    let (_, new_line) = changed_lines(case);
    let in_impl = case.in_impl;
    let item = match case.kind {
        Kind::Iterator => format!(
            "#[derive(Debug)]
pub struct Countdown(pub u32);

impl Iterator for Countdown {{
    type Item = u32;
    fn next(&mut self) -> Option<u32> {{
        if self.0 == 0 {{
            return None;
        }}
        self.0 -= 1;
{new_line}
    }}
{in_impl}}}

pub struct Other(pub u32);

impl Iterator for Other {{
    type Item = u32;
    fn next(&mut self) -> Option<u32> {{
        None
    }}
}}
"
        ),
    };
    format!(
        "use std::cell::Cell;
use std::rc::Rc;

{item}{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn syntax_under_test() {{
        {body}
    }}
}}
",
        extra = case.extra_items,
        body = case.test_body,
    )
}

fn direct_tests(case: &Case) -> Result<Vec<String>, String> {
    let findings = findings_of(case)?;
    let mut names: Vec<String> = findings
        .iter()
        .flat_map(|finding| &finding.related_tests)
        .filter(|test| test.relation_reason == Some(RelationReason::DirectOwnerCall))
        .map(|test| test.name.clone())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

fn findings_of(case: &Case) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-drop-iterator-credit-{}-{}",
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
        "[package]\nname = \"drop-iterator-credit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|e| format!("write Cargo.toml: {e}"))?;
    let lib = source(case);
    let (old_line, new_line) = changed_lines(case);
    let line = lib
        .lines()
        .position(|line| line == new_line)
        .map(|at| at + 1)
        .ok_or("premise: the changed line is in the source")?;
    if !case.sibling.is_empty() {
        std::fs::write(root.join("src/macros.rs"), case.sibling)
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
-{old_line}
+{new_line}
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

const ITERATOR: Case = Case {
    label: "iterator",
    kind: Kind::Iterator,
    in_impl: "",
    extra_items: "",
    sibling: "",
    test_body: "assert_eq!(Countdown(3).collect::<Vec<_>>(), vec![3, 2, 1]);",
};

fn with_body(case: Case, label: &'static str, test_body: &'static str) -> Case {
    Case {
        label,
        test_body,
        ..case
    }
}

fn check_all(cases: &[Case], expected: &[&str]) -> Result<(), String> {
    let mut failures = Vec::new();
    for case in cases {
        let direct = direct_tests(case)?;
        if direct != expected {
            failures.push(format!("{}: got {direct:?}, want {expected:?}", case.label));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn a_loop_or_consuming_adaptor_over_an_owned_iterator_reaches_next() -> Result<(), String> {
    let cases = [
        with_body(
            ITERATOR,
            "collect",
            "assert_eq!(Countdown(3).collect::<Vec<_>>(), vec![3, 2, 1]);",
        ),
        with_body(
            ITERATOR,
            "for_loop",
            "let mut seen = Vec::new();\n        for n in Countdown(3) {\n            seen.push(n);\n        }\n        assert_eq!(seen, vec![3, 2, 1]);",
        ),
        with_body(
            ITERATOR,
            "bound_count",
            "let it = Countdown(3);\n        assert_eq!(it.count(), 3);",
        ),
        with_body(
            ITERATOR,
            "bound_loop",
            "let it = Countdown(2);\n        let mut total = 0;\n        for n in it {\n            total += n;\n        }\n        assert_eq!(total, 3);",
        ),
        with_body(
            ITERATOR,
            "sum",
            "let total: u32 = Countdown(3).sum();\n        assert_eq!(total, 6);",
        ),
    ];
    check_all(&cases, &["syntax_under_test"])
}

#[test]
fn an_iterator_the_test_cannot_be_shown_to_drive_stays_unrelated() -> Result<(), String> {
    let cases = [
        with_body(
            ITERATOR,
            "lazy_adaptor_chain",
            "assert_eq!(Countdown(3).take(2).collect::<Vec<_>>(), vec![3, 2]);",
        ),
        with_body(
            ITERATOR,
            "other_type",
            "assert_eq!(Other(3).collect::<Vec<_>>(), Vec::<u32>::new());",
        ),
        with_body(
            ITERATOR,
            "other_type_binding",
            "let it = Other(3);\n        assert_eq!(it.count(), 0);",
        ),
        with_body(
            ITERATOR,
            "discarded_result",
            "let _ = Countdown(3).count();\n        assert_eq!(1, 1);",
        ),
        with_body(
            ITERATOR,
            "by_ref_loop",
            "let mut it = Countdown(3);\n        let mut seen = Vec::new();\n        for n in it.by_ref() {\n            seen.push(n);\n        }\n        assert_eq!(seen, vec![3, 2, 1]);",
        ),
        with_body(
            ITERATOR,
            "reference_binding",
            "let mut owned = Countdown(3);\n        let it: &mut Countdown = &mut owned;\n        assert_eq!(it.count(), 3);",
        ),
        with_body(
            ITERATOR,
            "stringified",
            "assert_eq!(stringify!(Countdown(3).count()), \"Countdown(3).count()\");",
        ),
        with_body(
            ITERATOR,
            "parameter_shadows_binding",
            "let it = Countdown(3);\n        let f = |it: Other| it.count();\n        assert_eq!(f(Other(1)), 0);\n        assert_eq!(it.take(1).count(), 1);",
        ),
    ];
    check_all(&cases, &[])
}

#[test]
fn an_inherent_method_of_the_adaptor_name_wins_over_the_trait() -> Result<(), String> {
    let case = Case {
        label: "inherent_count",
        extra_items: "
impl Countdown {
    pub fn count(self) -> usize {
        0
    }
}
",
        test_body: "assert_eq!(Countdown(3).count(), 0);",
        ..ITERATOR
    };
    check_all(&[case], &[])?;
    // Another adaptor of the same type is still the trait's.
    let other = Case {
        label: "inherent_count_beside_collect",
        extra_items: "
impl Countdown {
    pub fn count(self) -> usize {
        0
    }
}
",
        test_body: "assert_eq!(Countdown(3).collect::<Vec<_>>(), vec![3, 2, 1]);",
        ..ITERATOR
    };
    check_all(&[other], &["syntax_under_test"])
}

#[test]
fn an_iterator_type_the_test_redeclares_or_a_macro_may_change_is_not_attributed()
-> Result<(), String> {
    let redeclared = with_body(
        ITERATOR,
        "redeclared_type",
        "struct Countdown(u32);\n        impl Iterator for Countdown {\n            type Item = u32;\n            fn next(&mut self) -> Option<u32> {\n                None\n            }\n        }\n        assert_eq!(Countdown(3).count(), 0);",
    );
    let macro_in_file = Case {
        label: "macro_rules_in_file",
        extra_items: "
macro_rules! make_impl {
    ($t:ident) => {
        impl Clone for $t {
            fn clone(&self) -> Self {
                $t(self.0)
            }
        }
    };
}
",
        ..ITERATOR
    };
    let macro_in_sibling = Case {
        label: "macro_in_sibling_file",
        sibling: "macro_rules! make_impl {
    ($t:ident) => {
        impl Iterator for $t {
            type Item = u32;
            fn next(&mut self) -> Option<u32> {
                None
            }
        }
    };
}
",
        ..ITERATOR
    };
    check_all(&[redeclared, macro_in_file, macro_in_sibling], &[])
}

/// The finding on the changed line, the one whose infection is read; the other
/// findings come from the unchanged signature and guard lines.
fn on_changed_line(case: &Case) -> Result<Finding, String> {
    findings_of(case)?
        .into_iter()
        .find(|finding| finding.ripr.infect.state == ripr::domain::StageState::Yes)
        .ok_or_else(|| format!("{}: no finding with a read infection", case.label))
}

#[test]
fn a_relation_no_assertion_confirms_is_withheld_not_a_gap() -> Result<(), String> {
    let case = ITERATOR;
    // The test reaches the owner through syntax and pins an exact value, but
    // nothing here says the assertion reads the effect of the changed line, so
    // the finding is withheld rather than a "add a test" gap.
    let withheld = on_changed_line(&case)?;
    if withheld.class != ExposureClass::StaticUnknown {
        return Err(format!("{}: got {:?}", case.label, withheld.class));
    }
    let limit = withheld.static_limit_kind.map(|kind| kind.as_str());
    assert_eq!(
        limit,
        Some("rust_implicit_call_unresolved"),
        "{}",
        case.label
    );

    // A test with no exact assertion is a real gap whatever runs the owner.
    let weak = with_body(
        case,
        "weak_assertion",
        "let v: Vec<u32> = Countdown(3).collect();\n        assert!(!v.is_empty());",
    );
    let class = on_changed_line(&weak)?.class;
    if class != ExposureClass::WeaklyExposed {
        return Err(format!("{}: weak assertion got {class:?}", case.label));
    }
    Ok(())
}

#[test]
fn an_adaptor_the_iterator_impl_overrides_does_not_run_next() -> Result<(), String> {
    // `count` is provided by `Iterator`, but this impl replaces it, so the
    // call never reaches `next`. `collect` beside it still does.
    let overridden = Case {
        label: "overridden_count",
        in_impl: "    fn count(self) -> usize {\n        99\n    }\n",
        test_body: "assert_eq!(Countdown(3).count(), 99);",
        ..ITERATOR
    };
    check_all(&[overridden], &[])?;
    let beside_collect = Case {
        label: "overridden_count_beside_collect",
        test_body: "assert_eq!(Countdown(3).collect::<Vec<_>>(), vec![3, 2, 1]);",
        ..overridden
    };
    check_all(&[beside_collect], &["syntax_under_test"])
}

#[test]
fn syntax_that_may_never_run_in_the_test_relates_nothing() -> Result<(), String> {
    let cases = [
        with_body(
            ITERATOR,
            "closure_never_called",
            "let f = || {\n            for _n in Countdown(3) {}\n        };\n        assert_eq!(1, 1);\n        let _ = f;",
        ),
        with_body(
            ITERATOR,
            "nested_fn_never_called",
            "fn helper() {\n            for _n in Countdown(3) {}\n        }\n        assert_eq!(1, 1);",
        ),
        with_body(
            ITERATOR,
            "dead_branch",
            "if 1 == 2 {\n            for _n in Countdown(3) {}\n        }\n        assert_eq!(1, 1);",
        ),
        with_body(
            ITERATOR,
            "braceless_closure_never_called",
            "let f = || Countdown(3).count();\n        assert_eq!(1, 1);\n        let _ = f;",
        ),
        with_body(
            ITERATOR,
            "disabled_by_cfg",
            "#[cfg(any())]\n        for _n in Countdown(3) {}\n        assert_eq!(1, 1);",
        ),
    ];
    check_all(&cases, &[])
}

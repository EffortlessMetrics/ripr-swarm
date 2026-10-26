//! Public-API controls for crediting the end of a scope to `Drop::drop` and a
//! loop or consuming adaptor to `Iterator::next` (#9567).
//!
//! `{ let _g = Guard(..); }` runs `drop` of `impl Drop for Guard`, and
//! `Countdown(3).collect()` runs `next` of `impl Iterator for Countdown`,
//! without naming either. A same-file test that does so with a value
//! syntactically of the impl's type, and drops it before it asserts, reaches the
//! changed method directly; a value ripr cannot type to that exact type, one
//! that is moved or leaked, a lazy adaptor or a shadowing inherent method does
//! not. Relating the test never raises a class by itself.

use ripr::domain::{ExposureClass, RelationReason};
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
enum Kind {
    Iterator,
    Drop,
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
const DROP_OLD: &str = "        self.0.set(self.0.get() + 1);";
const DROP_NEW: &str = "        self.0.set(1 + self.0.get());";

fn changed_lines(case: &Case) -> (&'static str, &'static str) {
    match case.kind {
        Kind::Iterator => (ITER_OLD, ITER_NEW),
        Kind::Drop => (DROP_OLD, DROP_NEW),
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
        Kind::Drop => format!(
            "pub struct Guard(pub Rc<Cell<u32>>);

impl Drop for Guard {{
    fn drop(&mut self) {{
{new_line}
    }}
}}

pub struct Other(pub Rc<Cell<u32>>);

impl Drop for Other {{
    fn drop(&mut self) {{}}
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

const DROP: Case = Case {
    label: "drop",
    kind: Kind::Drop,
    in_impl: "",
    extra_items: "",
    sibling: "",
    test_body: "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n        }\n        assert_eq!(counter.get(), 1);",
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

#[test]
fn a_value_dropped_before_the_assertion_reaches_drop() -> Result<(), String> {
    let cases = [
        with_body(
            DROP,
            "inner_block",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n        }\n        assert_eq!(counter.get(), 1);",
        ),
        with_body(
            DROP,
            "explicit_drop",
            "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        drop(g);\n        assert_eq!(counter.get(), 1);",
        ),
        with_body(
            DROP,
            "mem_drop_temporary",
            "let counter = Rc::new(Cell::new(0));\n        std::mem::drop(Guard(counter.clone()));\n        assert_eq!(counter.get(), 1);",
        ),
        with_body(
            DROP,
            "block_value_read_as_receiver",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            let _seen = g.0.get();\n        }\n        assert_eq!(counter.get(), 1);",
        ),
    ];
    check_all(&cases, &["syntax_under_test"])
}

#[test]
fn a_value_that_is_not_dropped_before_the_assertion_stays_unrelated() -> Result<(), String> {
    let cases = [
        with_body(
            DROP,
            "lives_to_the_end",
            "let counter = Rc::new(Cell::new(0));\n        let _g = Guard(counter.clone());\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "asserted_inside_the_block",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n            assert_eq!(counter.get(), 0);\n        }",
        ),
        with_body(
            DROP,
            "moved_out_of_the_block",
            "let counter = Rc::new(Cell::new(0));\n        let kept;\n        {\n            let g = Guard(counter.clone());\n            kept = g;\n        }\n        assert_eq!(counter.get(), 0);\n        drop(kept);",
        ),
        with_body(
            DROP,
            "forgotten_in_the_test",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            std::mem::forget(g);\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "other_type",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Other(counter.clone());\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "other_type_explicit_drop",
            "let counter = Rc::new(Cell::new(0));\n        drop(Other(counter.clone()));\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "reference_dropped",
            "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        drop(&g);\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "branch_body_is_not_a_scope",
            "let counter = Rc::new(Cell::new(0));\n        if counter.get() == 0 {\n            let _g = Guard(counter.clone());\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "closure_body_is_not_a_scope",
            "let counter = Rc::new(Cell::new(0));\n        let run = || {\n            let _g = Guard(counter.clone());\n        };\n        assert_eq!(counter.get(), 0);\n        run();",
        ),
    ];
    check_all(&cases, &[])
}

#[test]
fn a_drop_type_the_test_redeclares_or_a_macro_may_change_is_not_attributed() -> Result<(), String> {
    let redeclared = with_body(
        DROP,
        "redeclared_type",
        "struct Guard(Rc<Cell<u32>>);\n        impl Drop for Guard {\n            fn drop(&mut self) {}\n        }\n        let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n        }\n        assert_eq!(counter.get(), 0);",
    );
    let macro_in_sibling = Case {
        label: "macro_in_sibling_file",
        sibling: "macro_rules! make_impl {
    ($t:ident) => {
        impl Drop for $t {
            fn drop(&mut self) {}
        }
    };
}
",
        ..DROP
    };
    check_all(&[redeclared, macro_in_sibling], &[])
}

#[test]
fn a_method_of_the_type_that_leaks_self_keeps_the_withholding() -> Result<(), String> {
    let case = Case {
        label: "disarm_forgets_self",
        extra_items: "
impl Guard {
    pub fn disarm(self) {
        std::mem::forget(self);
    }
}
",
        test_body: "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            g.disarm();\n        }\n        assert_eq!(counter.get(), 0);",
        ..DROP
    };
    // The same type without the leaking method is dropped.
    let plain = Case {
        label: "no_leaking_method",
        test_body: "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            let _seen = g.0.get();\n        }\n        assert_eq!(counter.get(), 1);",
        ..DROP
    };
    check_all(&[case], &[])?;
    check_all(&[plain], &["syntax_under_test"])
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
    for (case, weak_body) in [
        (
            ITERATOR,
            "let v: Vec<u32> = Countdown(3).collect();\n        assert!(!v.is_empty());",
        ),
        (
            DROP,
            "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n        }\n        assert!(counter.get() > 0);",
        ),
    ] {
        // The test reaches the owner through syntax and pins an exact value,
        // but nothing here says the assertion reads the effect of the changed
        // line, so the finding is withheld rather than a "add a test" gap.
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
        let weak = with_body(case, "weak_assertion", weak_body);
        let class = on_changed_line(&weak)?.class;
        if class != ExposureClass::WeaklyExposed {
            return Err(format!("{}: weak assertion got {class:?}", case.label));
        }
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
            "disabled_by_cfg",
            "#[cfg(any())]\n        for _n in Countdown(3) {}\n        assert_eq!(1, 1);",
        ),
        with_body(
            DROP,
            "disabled_by_cfg_drop",
            "let counter = Rc::new(Cell::new(0));\n        #[cfg(any())]\n        drop(Guard(counter.clone()));\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "another_function_named_drop",
            "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        helpers::drop(g);\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "braceless_closure_never_called",
            "let counter = Rc::new(Cell::new(0));\n        let f = || drop(Guard(counter.clone()));\n        assert_eq!(counter.get(), 0);\n        let _ = f;",
        ),
        with_body(
            DROP,
            "closure_never_called_scope",
            "let counter = Rc::new(Cell::new(0));\n        let f = || {\n            {\n                let _g = Guard(counter.clone());\n            }\n        };\n        assert_eq!(counter.get(), 0);\n        let _ = f;",
        ),
        with_body(
            DROP,
            "dead_branch_scope",
            "let counter = Rc::new(Cell::new(0));\n        if counter.get() == 99 {\n            counter.set(1);\n            {\n                let _g = Guard(counter.clone());\n            }\n        }\n        assert_eq!(counter.get(), 0);",
        ),
    ];
    check_all(&cases, &[])
}

#[test]
fn a_by_value_method_may_move_the_value_out_of_the_block() -> Result<(), String> {
    let stash = Case {
        label: "by_value_method",
        extra_items: "
impl Guard {
    pub fn stash(self) {
        std::mem::drop(Some(self));
    }
    pub fn peek(&self) -> u32 {
        self.0.get()
    }
}
",
        test_body: "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            g.stash();\n        }\n        assert_eq!(counter.get(), 1);",
        ..DROP
    };
    check_all(&[stash], &[])?;
    // A `&self` method of the same type does not move the value.
    let peek = Case {
        label: "by_ref_method",
        test_body: "let counter = Rc::new(Cell::new(0));\n        {\n            let g = Guard(counter.clone());\n            let _seen = g.peek();\n        }\n        assert_eq!(counter.get(), 1);",
        ..stash
    };
    check_all(&[peek], &["syntax_under_test"])
}

#[test]
fn a_drop_the_test_cannot_be_shown_to_run_relates_nothing() -> Result<(), String> {
    let cases = [
        // A test-local `drop` that keeps the guard alive past the assertion.
        with_body(
            DROP,
            "local_fn_named_drop",
            "fn drop(held: Guard) {\n            pending().push(held);\n        }\n        let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        drop(g);\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "local_mod_mem",
            "mod mem {\n            pub fn drop<T>(_: T) {}\n        }\n        let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        mem::drop(g);\n        assert_eq!(counter.get(), 0);",
        ),
        // `into` is a std method the workspace index never sees; it can move
        // the guard out of the block.
        with_body(
            DROP,
            "moved_out_by_into",
            "let counter = Rc::new(Cell::new(0));\n        let kept: Option<Guard>;\n        {\n            let g = Guard(counter.clone());\n            kept = g.into();\n        }\n        assert_eq!(counter.get(), 0);\n        drop(kept);",
        ),
        // The attribute removes the binding from the compiled test.
        with_body(
            DROP,
            "binding_disabled_by_cfg",
            "let counter = Rc::new(Cell::new(0));\n        {\n            #[cfg(any())]\n            let _g = Guard(counter.clone());\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        // The only assertion ran while the guard was alive; `asserted` is an
        // ordinary identifier, not a macro.
        with_body(
            DROP,
            "assertion_before_the_drop",
            "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        assert_eq!(counter.get(), 0);\n        drop(g);\n        let asserted = true;\n        let _ = asserted;",
        ),
    ];
    check_all(&cases, &[])
}

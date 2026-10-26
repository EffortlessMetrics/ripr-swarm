//! Public-API controls for relating the end of a scope to `Drop::drop`
//! (#10553, #9567).
//!
//! `{ let _g = Guard(..); }` runs `drop` of `impl Drop for Guard` without
//! naming it. A same-file test that drops a value syntactically of the impl's
//! type before it asserts reaches the changed method directly; a value ripr
//! cannot type to that exact type, one that is moved or leaked, or a shadowed
//! `drop` does not. Relating the test never raises a class by itself, and a
//! destructor is never credited from a shared-cell assertion (see
//! `sibling_effect_oracle`).

use ripr::domain::{ExposureClass, RelationReason};
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One impl plus the test body under study.
#[derive(Clone, Copy)]
struct Case {
    label: &'static str,
    /// Extra items placed after the impl (inherent methods, another type).
    extra_items: &'static str,
    /// The changed line of `drop` before and after.
    drop_lines: (&'static str, &'static str),
    /// Source of `src/macros.rs`, a second workspace file.
    sibling: &'static str,
    test_body: &'static str,
}

const DROP_OLD: &str = "        self.0.set(self.0.get() + 1);";
const DROP_NEW: &str = "        self.0.set(1 + self.0.get());";

fn source(case: &Case) -> String {
    format!(
        "use std::cell::Cell;
use std::rc::Rc;

pub struct Guard(pub Rc<Cell<u32>>);

impl Drop for Guard {{
    fn drop(&mut self) {{
{drop_new}
    }}
}}

pub struct Other(pub Rc<Cell<u32>>);

impl Drop for Other {{
    fn drop(&mut self) {{}}
}}
{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn syntax_under_test() {{
        {body}
    }}
}}
",
        drop_new = case.drop_lines.1,
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
    let (old_line, new_line) = case.drop_lines;
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

const DROP: Case = Case {
    label: "drop",
    drop_lines: (DROP_OLD, DROP_NEW),
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
    // The test reaches `drop` through scope end and pins the cell it writes,
    // but no authority ties that assertion to the end of the scope (it also
    // holds when the assertion reads a different cell, see
    // `sibling_effect_oracle`), so the finding is withheld rather than credited
    // or reported as an "add a test" gap.
    let withheld = on_changed_line(&DROP)?;
    if withheld.class != ExposureClass::StaticUnknown {
        return Err(format!("{}: got {:?}", DROP.label, withheld.class));
    }
    let limit = withheld.static_limit_kind.map(|kind| kind.as_str());
    assert_eq!(
        limit,
        Some("rust_implicit_call_unresolved"),
        "{}",
        DROP.label
    );

    // A test with no exact assertion is a real gap whatever runs the owner.
    let weak = with_body(
        DROP,
        "weak_assertion",
        "let counter = Rc::new(Cell::new(0));\n        {\n            let _g = Guard(counter.clone());\n        }\n        assert!(counter.get() > 0);",
    );
    let class = on_changed_line(&weak)?.class;
    if class != ExposureClass::WeaklyExposed {
        return Err(format!("weak assertion got {class:?}"));
    }
    Ok(())
}

#[test]
fn syntax_that_may_never_run_in_the_test_relates_nothing() -> Result<(), String> {
    let cases = [
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

#[test]
fn a_drop_the_test_may_rebind_or_not_run_relates_nothing() -> Result<(), String> {
    let cases = [
        // A local closure named `drop` can hold the guard past the assertion.
        with_body(
            DROP,
            "closure_named_drop",
            "let counter = Rc::new(Cell::new(0));\n        let mut held = Vec::new();\n        let mut drop = |g: Guard| held.push(g);\n        let g = Guard(counter.clone());\n        drop(g);\n        assert_eq!(counter.get(), 0);",
        ),
        // The binding sits in a branch that never runs.
        with_body(
            DROP,
            "guard_bound_in_a_dead_branch",
            "let counter = Rc::new(Cell::new(0));\n        {\n            if counter.get() == 99 {\n                let _g = Guard(counter.clone());\n            }\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        with_body(
            DROP,
            "guard_bound_in_a_closure_of_the_block",
            "let counter = Rc::new(Cell::new(0));\n        {\n            let _f = || {\n                let _g = Guard(counter.clone());\n            };\n        }\n        assert_eq!(counter.get(), 0);",
        ),
        // `mem` is another file's module.
        Case {
            label: "mem_module_from_another_file",
            extra_items: "\nuse crate::macros::mem;\n",
            sibling: "pub mod mem {\n    pub fn drop<T>(_: T) {}\n}\n",
            test_body: "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        mem::drop(g);\n        assert_eq!(counter.get(), 0);",
            ..DROP
        },
        with_body(
            DROP,
            "cfg_disabled_drop_after_binding",
            "let counter = Rc::new(Cell::new(0));\n        let g = Guard(counter.clone());\n        #[cfg(any())]\n        drop(g);\n        assert_eq!(counter.get(), 0);",
        ),
    ];
    check_all(&cases, &[])
}

#[test]
fn a_destructor_is_never_exposed_whatever_the_changed_line() -> Result<(), String> {
    // A predicate on the guard's own state: the assertion's literal matches the
    // boundary, but nothing ties the assertion to the cell `drop` reads or to
    // the end of the scope, so the credit is withheld however the test is built.
    let predicate = Case {
        label: "predicate",
        drop_lines: (DROP_OLD, "        if self.0.get() > 5 { self.0.set(6); }"),
        ..DROP
    };
    let bodies = [
        (
            "unrelated_cell",
            "let c = Rc::new(Cell::new(6));\n        let d = Rc::new(Cell::new(6));\n        {\n            let _g = Guard(c.clone());\n        }\n        assert_eq!(d.get(), 6);",
        ),
        (
            "assertion_before_the_drop",
            "let c = Rc::new(Cell::new(6));\n        let g = Guard(c.clone());\n        assert_eq!(c.get(), 6);\n        drop(g);\n        assert!(true);",
        ),
        (
            "shared_cell",
            "let c = Rc::new(Cell::new(6));\n        {\n            let _g = Guard(c.clone());\n        }\n        assert_eq!(c.get(), 6);",
        ),
        (
            // `drop` of a type with no `Drop` impl matches the owner by name.
            "free_drop_of_another_type",
            "struct Widget;\n        let c = Rc::new(Cell::new(6));\n        drop(Widget);\n        assert_eq!(c.get(), 6);",
        ),
    ];
    for (label, body) in bodies {
        let case = with_body(predicate, label, body);
        for finding in findings_of(&case)? {
            if finding.class == ExposureClass::Exposed {
                return Err(format!("{label}: a destructor was credited as exposed"));
            }
        }
    }
    Ok(())
}

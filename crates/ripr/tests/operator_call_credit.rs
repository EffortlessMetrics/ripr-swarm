//! Public-API controls for crediting operator syntax to the operator-trait
//! impl it runs (#9567).
//!
//! `a + b` runs `Add::add` of `a`'s type without naming it. A same-file test
//! whose left operand is syntactically a value of the impl's self type reaches
//! the changed method directly; an operand ripr cannot type to that exact
//! type, a shadowed type or a second impl of the trait for the type does not.
//! Relating the test never raises a class by itself.

use ripr::domain::RelationReason;
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One operator impl for `Money` plus the test body under study.
struct Case {
    label: &'static str,
    /// Trait path as written in the impl header (`Add`, `Neg`).
    trait_name: &'static str,
    method: &'static str,
    /// Signature tail after the method name, and the changed body line.
    signature: &'static str,
    old_line: &'static str,
    new_line: &'static str,
    extra_items: &'static str,
    test_body: &'static str,
}

fn source(case: &Case) -> String {
    format!(
        "use std::ops::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Money(pub i64);

impl {trait_name} for Money {{
    {signature} {{
{new_line}
    }}
}}
{extra}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn operator_under_test() {{
        {body}
    }}
}}
",
        trait_name = case.trait_name,
        signature = case.signature,
        new_line = case.new_line,
        extra = case.extra_items,
        body = case.test_body,
    )
}

fn direct_tests(case: &Case) -> Result<Vec<String>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-operator-credit-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = run(case, &root);
    let _ = std::fs::remove_dir_all(&root);
    result.map_err(|message| format!("{}: {message}", case.label))
}

fn run(case: &Case, root: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(root.join("src")).map_err(|e| format!("create dir: {e}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"operator-credit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|e| format!("write Cargo.toml: {e}"))?;
    let lib = source(case);
    let line = lib
        .lines()
        .position(|line| line == case.new_line)
        .map(|at| at + 1)
        .ok_or("premise: the changed line is in the source")?;
    std::fs::write(root.join("src/lib.rs"), &lib).map_err(|e| format!("write source: {e}"))?;
    std::fs::write(
        root.join("diff.patch"),
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{}
+{}
",
            case.old_line, case.new_line
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

const ADD: Case = Case {
    label: "add",
    trait_name: "Add",
    method: "add",
    signature: "type Output = Money;\n    fn add(self, other: Money) -> Money",
    old_line: "        Money(self.0 + other.0)",
    new_line: "        Money(other.0 + self.0)",
    extra_items: "",
    test_body: "assert_eq!(Money(2) + Money(3), Money(5));",
};

fn with_body(case: Case, label: &'static str, test_body: &'static str) -> Case {
    Case {
        label,
        test_body,
        ..case
    }
}

#[test]
fn typed_operands_reach_each_operator_impl() -> Result<(), String> {
    let cases = [
        with_body(ADD, "add", "assert_eq!(Money(2) + Money(3), Money(5));"),
        Case {
            label: "sub",
            trait_name: "Sub",
            method: "sub",
            signature: "type Output = Money;\n    fn sub(self, other: Money) -> Money",
            old_line: "        Money(self.0 - other.0)",
            new_line: "        Money(self.0 + 0 - other.0)",
            extra_items: "",
            test_body: "let a = Money(5);\n        let b = Money(3);\n        assert_eq!(a - b, Money(2));",
        },
        Case {
            label: "neg",
            trait_name: "Neg",
            method: "neg",
            signature: "type Output = Money;\n    fn neg(self) -> Money",
            old_line: "        Money(-self.0)",
            new_line: "        Money(0 - self.0)",
            extra_items: "",
            test_body: "let m = Money(4);\n        assert_eq!(-m, Money(-4));",
        },
        Case {
            label: "not",
            trait_name: "Not",
            method: "not",
            signature: "type Output = Money;\n    fn not(self) -> Money",
            old_line: "        Money(!self.0)",
            new_line: "        Money(!(self.0))",
            extra_items: "",
            test_body: "assert_eq!(!Money(0), Money(-1));",
        },
        Case {
            label: "add_assign",
            trait_name: "AddAssign",
            method: "add_assign",
            signature: "fn add_assign(&mut self, other: Money)",
            old_line: "        self.0 += other.0;",
            new_line: "        self.0 = self.0 + other.0;",
            extra_items: "",
            test_body: "let mut total = Money(1);\n        total += Money(2);\n        assert_eq!(total, Money(3));",
        },
    ];
    let mut failures = Vec::new();
    for case in &cases {
        let direct = direct_tests(case)?;
        if direct != ["operator_under_test"] {
            failures.push(format!("{} ({}): got {direct:?}", case.label, case.method));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn operands_that_are_not_established_as_the_impl_type_stay_unrelated() -> Result<(), String> {
    let bodies: [(&'static str, &'static str); 11] = [
        ("integers", "assert_eq!(1 + 2, 3);"),
        (
            "parameter of a nested fn shadows the binding",
            "let a = Money(1);\n        fn h(a: i64) -> i64 {\n            a + 1\n        }\n        assert_eq!(h(1), 2);\n        let _ = a;",
        ),
        (
            "closure parameter shadows the binding",
            "let a = Money(1);\n        let inc = |a: i64| a + 1;\n        assert_eq!(inc(1), 2);\n        let _ = a;",
        ),
        (
            "reference operands select the impl for &Money",
            "let a = Money(1);\n        let b = Money(2);\n        assert_eq!(&a + &b, Money(3));",
        ),
        (
            "method chain result",
            "let a = Money(1);\n        assert_eq!(a.clone() + Money(2), Money(3));",
        ),
        (
            "tail of a longer expression",
            "assert_eq!(2 * Money(1).0 + 3, 5);",
        ),
        (
            "field operand",
            "let pair = (Money(1), Money(2));\n        assert_eq!(pair.0 + pair.1, Money(3));",
        ),
        (
            "binding of another type",
            "let other = Tile(1);\n        assert_eq!(other + other, Tile(2));",
        ),
        (
            "result thrown away",
            "let _ = Money(1) + Money(2);\n        assert_eq!(Money(9).0, 9);",
        ),
        (
            "untyped parameter",
            "fn twice(m: Money) -> Money {\n            m + m\n        }\n        assert_eq!(twice(Money(1)), Money(2));",
        ),
        (
            "shadowing let",
            "let a = Money(1);\n        let a = 7;\n        assert_eq!(a + a, 14);",
        ),
    ];
    let mut failures = Vec::new();
    for (label, body) in bodies {
        let direct = direct_tests(&with_body(ADD, label, body))?;
        if !direct.is_empty() {
            failures.push(format!("{label}: related by a direct call: {direct:?}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn a_second_impl_of_the_trait_for_the_type_is_not_attributed() -> Result<(), String> {
    // `Money + i64` and `Money + Money` pick different impls by right-hand
    // side, which ripr does not type.
    let case = Case {
        extra_items: "
impl Add<i64> for Money {
    type Output = Money;
    fn add(self, other: i64) -> Money {
        Money(self.0 + other)
    }
}
",
        ..with_body(
            ADD,
            "two impls",
            "assert_eq!(Money(2) + Money(3), Money(5));",
        )
    };
    let direct = direct_tests(&case)?;
    if direct.is_empty() {
        Ok(())
    } else {
        Err(format!("two impls: related by a direct call: {direct:?}"))
    }
}

#[test]
fn a_type_the_test_redeclares_is_not_the_impl_type() -> Result<(), String> {
    let case = with_body(
        ADD,
        "shadowed type",
        "#[derive(Clone, Copy, Debug, PartialEq)]\n        struct Money(i64);\n        impl std::ops::Add for Money {\n            type Output = Money;\n            fn add(self, o: Money) -> Money {\n                Money(self.0 + o.0)\n            }\n        }\n        assert_eq!(Money(2) + Money(3), Money(5));",
    );
    let direct = direct_tests(&case)?;
    if direct.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "shadowed type: related by a direct call: {direct:?}"
        ))
    }
}

#[test]
fn generic_arguments_are_not_shift_operators() -> Result<(), String> {
    let shr = Case {
        label: "generic close after a tuple comma",
        trait_name: "Shr",
        method: "shr",
        signature: "type Output = Money;\n    fn shr(self, other: Money) -> Money",
        old_line: "        Money(self.0 >> other.0)",
        new_line: "        Money((self.0) >> other.0)",
        extra_items: "",
        test_body: "let v: Vec<Result<u8, Money>> = Vec::new();\n        assert!(v.is_empty());",
    };
    let direct = direct_tests(&shr)?;
    if !direct.is_empty() {
        return Err(format!(
            "generic `>>`: related by a direct call: {direct:?}"
        ));
    }
    let real = Case {
        label: "a real shift",
        test_body: "let a = Money(8);\n        assert_eq!(a >> Money(1), Money(4));",
        ..shr
    };
    let direct = direct_tests(&real)?;
    if direct == ["operator_under_test"] {
        Ok(())
    } else {
        Err(format!("a real `>>` was not credited: {direct:?}"))
    }
}

#[test]
fn a_binary_minus_after_a_block_is_not_negation() -> Result<(), String> {
    // `Money { 0: 5 } - n` runs `Sub`, not `Neg::neg` of `n`.
    let case = Case {
        label: "binary minus after a block",
        trait_name: "Neg",
        method: "neg",
        signature: "type Output = Money;\n    fn neg(self) -> Money",
        old_line: "        Money(-self.0)",
        new_line: "        Money(0 - self.0)",
        extra_items: "",
        test_body: "let n = Money(2);\n        let r = Money { 0: 5 } - n;\n        assert_eq!(r.0, 3);",
    };
    let direct = direct_tests(&case)?;
    if direct.is_empty() {
        Ok(())
    } else {
        Err(format!("related by a direct call: {direct:?}"))
    }
}

#[test]
fn a_macro_that_may_generate_a_second_impl_is_not_attributed() -> Result<(), String> {
    // The macro expands to `Add<i32> for Money`, which the index never sees.
    let case = Case {
        extra_items: "
macro_rules! add_for {
    ($t:ty) => {
        impl Add<$t> for Money {
            type Output = Money;
            fn add(self, other: $t) -> Money {
                Money(self.0 + other as i64)
            }
        }
    };
}
add_for!(i32);
",
        ..with_body(
            ADD,
            "macro-generated impl",
            "assert_eq!(Money(2) + Money(3), Money(5));",
        )
    };
    let direct = direct_tests(&case)?;
    if direct.is_empty() {
        Ok(())
    } else {
        Err(format!("related by a direct call: {direct:?}"))
    }
}

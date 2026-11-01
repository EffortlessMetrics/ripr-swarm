//! Public-API controls for crediting operator syntax to the operator-trait
//! impl it runs (#9567).
//!
//! `a + b` runs `Add::add` of `a`'s type without naming it. A same-file test
//! whose left operand is syntactically a value of the impl's self type reaches
//! the changed method directly; an operand ripr cannot type to that exact
//! type, a shadowed type or a second impl of the trait for the type does not.
//! Relating the test never raises a class by itself.

use ripr::domain::{ExposureClass, RelationReason};
use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One operator impl for `Money` plus the test body under study.
#[derive(Clone, Copy)]
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
    /// Source of `src/macros.rs`, a second workspace file.
    sibling: &'static str,
    /// Self type as written in the impl header (`Money`, `other::Money`).
    impl_type: &'static str,
    test_body: &'static str,
}

fn source(case: &Case) -> String {
    format!(
        "use std::ops::*;
{definition}
impl {trait_name} for {impl_type} {{
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
        impl_type = case.impl_type,
        definition = if case.impl_type == "Money" {
            "\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\n"
        } else {
            ""
        },
        signature = case.signature,
        new_line = case.new_line,
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

/// The findings on the changed line, in a throwaway workspace.
fn findings_of(case: &Case) -> Result<Vec<Finding>, String> {
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

fn run(case: &Case, root: &Path) -> Result<Vec<Finding>, String> {
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
    Ok(findings)
}

const ADD: Case = Case {
    label: "add",
    trait_name: "Add",
    method: "add",
    signature: "type Output = Money;\n    fn add(self, other: Money) -> Money",
    old_line: "        Money(self.0 + other.0)",
    new_line: "        Money(other.0 + self.0)",
    extra_items: "",
    sibling: "",
    impl_type: "Money",
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
            sibling: "",
            impl_type: "Money",
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
            sibling: "",
            impl_type: "Money",
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
            sibling: "",
            impl_type: "Money",
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
            sibling: "",
            impl_type: "Money",
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

/// Every binary and assigning operator in the dispatch table reaches its own
/// impl, so a defect in one row's token or method name cannot hide behind the
/// five representative cases above.
#[test]
fn every_binary_and_assigning_operator_reaches_its_impl() -> Result<(), String> {
    let binary = [
        ("Add", "+"),
        ("Sub", "-"),
        ("Mul", "*"),
        ("Div", "/"),
        ("Rem", "%"),
        ("BitAnd", "&"),
        ("BitOr", "|"),
        ("BitXor", "^"),
        ("Shl", "<<"),
        ("Shr", ">>"),
    ];
    let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
    let mut failures = Vec::new();
    for (trait_name, token) in binary {
        let method = trait_name.to_lowercase();
        let plain = Case {
            label: leak(method.clone()),
            trait_name,
            method: leak(method.clone()),
            signature: leak(format!(
                "type Output = Money;\n    fn {method}(self, other: Money) -> Money"
            )),
            old_line: leak(format!("        Money(self.0 {token} other.0)")),
            new_line: leak(format!("        Money((self.0 {token} other.0) + 0)")),
            extra_items: "",
            sibling: "",
            impl_type: "Money",
            test_body: leak(format!(
                "let a = Money(6);\n        let b = Money(2);\n        let r = a {token} b;\n        assert_eq!(r, Money(0));"
            )),
        };
        let assign_trait = leak(format!("{trait_name}Assign"));
        let assign_method = format!("{method}_assign");
        let assign = Case {
            label: leak(assign_method.clone()),
            trait_name: assign_trait,
            method: leak(assign_method.clone()),
            signature: leak(format!("fn {assign_method}(&mut self, other: Money)")),
            old_line: leak(format!("        self.0 {token}= other.0;")),
            new_line: leak(format!("        self.0 = self.0 {token} other.0;")),
            extra_items: "",
            sibling: "",
            impl_type: "Money",
            test_body: leak(format!(
                "let mut total = Money(6);\n        total {token}= Money(2);\n        let _ = total;"
            )),
        };
        for case in [plain, assign] {
            let direct = direct_tests(&case)?;
            if direct != ["operator_under_test"] {
                failures.push(format!("{} ({token}): got {direct:?}", case.label));
            }
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
    let bodies: [(&'static str, &'static str); 15] = [
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
            "untyped closure parameter shadows the binding",
            "let a = Money(1);\n        let inc = |a| a + 1;\n        assert_eq!(inc(1), 2);\n        let _ = a;",
        ),
        (
            "reference-pattern closure parameter shadows the binding",
            "let a = Money(5);\n        let v = [1i64, 2];\n        let s: i64 = v.iter().map(|&a| (a + 1)).sum();\n        assert_eq!(s, 5);\n        let _ = a;",
        ),
        (
            "tuple-pattern closure parameter shadows the binding",
            "let a = Money(5);\n        let v = [(1i64, 2i64)];\n        let s: i64 = v.iter().map(|(x, a)| (a + x)).sum();\n        assert_eq!(s, 3);\n        let _ = a;",
        ),
        (
            "array-pattern closure parameter shadows the binding",
            "let a = Money(5);\n        let v = [[1i64, 2i64]];\n        let s: i64 = v.iter().map(|[x, a]| (a + x)).sum();\n        assert_eq!(s, 3);\n        let _ = a;",
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
        sibling: "",
        impl_type: "Money",
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
    if direct != ["operator_under_test"] {
        return Err(format!("a real `>>` was not credited: {direct:?}"));
    }
    // A comparison earlier in the statement opens no generic argument.
    let after_comparison = Case {
        label: "a real shift after a comparison",
        test_body: "let a = Money(8);\n        assert!(1 < 2 && (a >> Money(1)).0 == 4);",
        ..shr
    };
    let direct = direct_tests(&after_comparison)?;
    if direct != ["operator_under_test"] {
        return Err(format!(
            "a `>>` after a comparison was not credited: {direct:?}"
        ));
    }
    // A comparison `>` must not cancel a generic opener, and a `;` inside an
    // array type must not end the statement.
    for (label, body) in [
        (
            "comparison before a turbofish generic",
            "let ok = 2 > 1 && 3 > 2 && f::<Vec<u8>, Result<u8, Money>>().is_ok();\n        assert!(ok);",
        ),
        (
            "array length inside generics",
            "let v: Vec<Vec<[Money; 2]>> = Vec::new();\n        assert!(v.is_empty());",
        ),
    ] {
        let case = Case {
            label,
            test_body: body,
            ..shr
        };
        let direct = direct_tests(&case)?;
        if !direct.is_empty() {
            return Err(format!("{label}: related by a direct call: {direct:?}"));
        }
    }
    Ok(())
}

#[test]
fn an_operator_inside_stringify_is_text_not_a_call() -> Result<(), String> {
    let text_only = with_body(
        ADD,
        "stringify only",
        "assert_eq!(stringify!(Money(1) + Money(2)), \"Money(1) + Money(2)\");",
    );
    let direct = direct_tests(&text_only)?;
    if !direct.is_empty() {
        return Err(format!("stringify! credited the impl: {direct:?}"));
    }
    let evaluated_too = with_body(
        ADD,
        "stringify beside an evaluated plus",
        "let s = stringify!(Money(1) + Money(2));\n        assert!(!s.is_empty());\n        assert_eq!(Money(2) + Money(3), Money(5));",
    );
    let direct = direct_tests(&evaluated_too)?;
    if direct == ["operator_under_test"] {
        Ok(())
    } else {
        Err(format!("the evaluated `+` was not credited: {direct:?}"))
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
        extra_items: "impl Sub for Money { type Output = Money; fn sub(self, other: Money) -> Money { Money(self.0 - other.0) } }",
        sibling: "",
        impl_type: "Money",
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

#[test]
fn a_macro_defined_in_a_sibling_file_may_generate_a_second_impl() -> Result<(), String> {
    // `add_for!(i32)` is invoked here but defined in `src/macros.rs`.
    let case = Case {
        extra_items: "
add_for!(i32);
",
        impl_type: "Money",
        sibling: "macro_rules! add_for {
    ($t:ty) => {
        impl Add<$t> for Money {
            type Output = Money;
            fn add(self, other: $t) -> Money {
                Money(self.0 + other as i64)
            }
        }
    };
}
",
        ..with_body(
            ADD,
            "sibling-file macro",
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

#[test]
fn a_comparison_before_a_plus_does_not_hide_the_operator() -> Result<(), String> {
    let case = with_body(
        ADD,
        "comparison then add",
        "let n = 1;\n        assert!(n < 5, \"{:?}\", Money(1) + Money(2));",
    );
    let direct = direct_tests(&case)?;
    if direct == ["operator_under_test"] {
        Ok(())
    } else {
        Err(format!("expected the add credited, got {direct:?}"))
    }
}

/// `&a + &b` runs `impl Add<&Money> for &Money`, never the owned impl.
const REFERENCE_IMPL: &str = "
impl Add<&Money> for &Money {
    type Output = Money;
    fn add(self, other: &Money) -> Money {
        Money(self.0 + other.0)
    }
}
";

#[test]
fn an_owned_operand_reaches_the_owned_impl_beside_a_reference_impl() -> Result<(), String> {
    let case = Case {
        extra_items: REFERENCE_IMPL,
        ..with_body(
            ADD,
            "owned operands beside a reference impl",
            "let a = Money(2);\n        let b = Money(3);\n        assert_eq!(a + b, Money(5));",
        )
    };
    let direct = direct_tests(&case)?;
    if direct.is_empty() {
        return Err("owned operands were not credited".to_string());
    }
    Ok(())
}

#[test]
fn reference_operands_do_not_reach_the_owned_impl() -> Result<(), String> {
    let bodies: [(&'static str, &'static str); 4] = [
        (
            "annotated reference bindings",
            "let a: &Money = &Money(2);\n        let b = &Money(3);\n        assert_eq!(a + b, Money(5));",
        ),
        (
            "unannotated reference bindings",
            "let a = &Money(2);\n        let b = &Money(3);\n        assert_eq!(a + b, Money(5));",
        ),
        (
            "reference literal operands",
            "assert_eq!(&Money(2) + &Money(3), Money(5));",
        ),
        (
            "reference annotation on a constructor",
            "let a: &Money = &Money(2);\n        assert_eq!(a + &Money(3), Money(5));",
        ),
    ];
    let mut failures = Vec::new();
    for (label, body) in bodies {
        let case = Case {
            extra_items: REFERENCE_IMPL,
            ..with_body(ADD, label, body)
        };
        let direct = direct_tests(&case)?;
        if !direct.is_empty() {
            failures.push(format!("{label}: credited the owned impl: {direct:?}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn classes(case: &Case) -> Result<Vec<ExposureClass>, String> {
    Ok(findings_of(case)?
        .into_iter()
        .map(|finding| finding.class)
        .collect())
}

#[test]
fn grouped_self_import_binds_the_enclosing_type_name() -> Result<(), String> {
    let base = Case {
        label: "grouped_self",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        ..ADD
    };
    let foreign = Case {
        label: "grouped_self_foreign",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod named { use super::*; use crate::other::Money::{self}; #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&foreign)?, [ExposureClass::StaticUnknown]);
    let owner = Case {
        label: "grouped_self_owner",
        extra_items: "#[cfg(test)] mod named { use super::Money::{self}; use std::ops::Add; #[test] fn named_call() { let _ = Money::add(super::Money(1), super::Money(1)); } }",
        ..base
    };
    assert_eq!(classes(&owner)?, [ExposureClass::WeaklyExposed]);
    let sibling = Case {
        label: "grouped_self_sibling",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod unrelated { use crate::other::Money::{self}; }\n#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let _ = Money::add(Money(1), Money(1)); } }",
        ..base
    };
    assert_eq!(classes(&sibling)?, [ExposureClass::WeaklyExposed]);
    Ok(())
}

#[test]
fn module_type_bindings_do_not_name_the_operator_owner() -> Result<(), String> {
    let base = Case {
        label: "module_binding",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        ..ADD
    };
    let real_owner = Case {
        label: "module_binding_owner",
        extra_items: "#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let _ = Money::add(Money(1), Money(1)); } }",
        ..base
    };
    assert_eq!(classes(&real_owner)?, [ExposureClass::WeaklyExposed]);
    let owner_import = Case {
        label: "module_binding_owner_import",
        extra_items: "#[cfg(test)] mod named { use super::Money; use std::ops::Add; #[test] fn named_call() { let _ = Money::add(Money(1), Money(1)); } }",
        ..base
    };
    assert_eq!(classes(&owner_import)?, [ExposureClass::WeaklyExposed]);

    let owner_alias = Case {
        label: "module_binding_owner_alias",
        extra_items: "#[cfg(test)] mod named { use super::*; type Money = super::Money; #[test] fn named_call() { let _ = Money::add(super::Money(1), super::Money(1)); } }",
        ..base
    };
    // The relation stage itself currently withholds a type alias, including
    // one that names the owner. This slice does not change that admission.
    assert_eq!(classes(&owner_alias)?, [ExposureClass::StaticUnknown]);

    let sibling = Case {
        label: "module_binding_sibling",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod unrelated { use crate::other::Money; }\n#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let _ = Money::add(Money(1), Money(1)); } }",
        ..base
    };
    assert_eq!(classes(&sibling)?, [ExposureClass::WeaklyExposed]);

    let foreign_import = Case {
        label: "module_binding_foreign_import",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod named { use super::*; use crate::other::Money; #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&foreign_import)?, [ExposureClass::StaticUnknown]);
    let raw_import = Case {
        label: "module_binding_raw_import",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod named { use super::*; use crate::other::r#Money; #[test] fn named_call() { let _ = r#Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&raw_import)?, [ExposureClass::StaticUnknown]);
    let nested_import = Case {
        label: "module_binding_nested_import",
        extra_items: "mod other { pub type Money = i64; }\n#[cfg(test)] mod outer { use crate::other::Money; use std::ops::Add; mod named { use super::*; #[test] fn named_call() { let _ = Money::add(1, 1); } } }",
        ..base
    };
    assert_eq!(classes(&nested_import)?, [ExposureClass::StaticUnknown]);
    let local_module = Case {
        label: "module_binding_local_module",
        extra_items: "#[cfg(test)] mod named { #[allow(non_snake_case)] mod Money { pub fn add(_: i64, _: i64) -> i64 { 2 } } #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&local_module)?, [ExposureClass::StaticUnknown]);

    let foreign = Case {
        label: "module_binding_foreign_alias",
        extra_items: "mod other { pub type Coin = i64; }\n#[cfg(test)] mod named { use super::*; use crate::other::Coin as Money; #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&foreign)?, [ExposureClass::StaticUnknown]);

    let local = Case {
        label: "module_binding_local_alias",
        extra_items: "#[cfg(test)] mod named { use super::*; type Money = i64; #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&local)?, [ExposureClass::StaticUnknown]);
    Ok(())
}

#[test]
fn a_same_module_binding_cannot_shadow_a_qualified_impl_type() -> Result<(), String> {
    // `impl Add for other::Money` with a root `type Money = i64`: the test's
    // `Money::add(1, 1)` is `i64::add`, not this impl. The owner's module was
    // accepted wholesale, so this read as a call of the impl and lifted the
    // withholding into a false `weakly_exposed`.
    let shadowed = Case {
        label: "same_module_shadow",
        trait_name: "Add",
        method: "add",
        signature: "type Output = other::Money;\n    fn add(self, other: other::Money) -> other::Money",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "mod other { pub struct Money(pub i64); }\ntype Money = i64;\n#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        sibling: "",
        impl_type: "other::Money",
        test_body: "assert_eq!((other::Money(2) + other::Money(3)).0, 5);",
    };
    assert_eq!(classes(&shadowed)?, [ExposureClass::StaticUnknown]);
    // Positive control: the genuine same-module definition and path still
    // credit (`module_binding_owner` pins the nested-module form).
    let genuine = Case {
        label: "same_module_genuine",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let _ = Money::add(Money(1), Money(1)); } }",
        ..ADD
    };
    assert_eq!(classes(&genuine)?, [ExposureClass::WeaklyExposed]);
    Ok(())
}

#[test]
fn a_qualified_call_spelling_trait_arguments_must_match_the_impl_header() -> Result<(), String> {
    // `<Money as Add<Money>>::add(..)` names the one `Add` impl for `Money`
    // (the omitted right-hand side of the header is `Money`), so it credits
    // like `<Money as Add>::add(..)`.
    let spelled = Case {
        label: "spelled_trait_args",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let sum = <Money as Add<Money>>::add(Money(1), Money(1)); assert_eq!(sum.0, 2); } }",
        ..ADD
    };
    assert_eq!(classes(&spelled)?, [ExposureClass::WeaklyExposed]);
    // An explicit `impl Add<Money> for Money` header takes the declared-arguments
    // branch; the fixture must parse as intended before the credit claim.
    let explicit = Case {
        label: "spelled_trait_args_explicit_header",
        trait_name: "Add<Money>",
        ..spelled
    };
    assert!(source(&explicit).contains("impl Add<Money> for Money {"));
    assert!(direct_tests(&explicit)?.contains(&"named_call".to_string()));
    assert_eq!(classes(&explicit)?, [ExposureClass::WeaklyExposed]);
    // Negative that exercises the argument check itself: the only `Add` impl
    // is `Add<Money>` (so the trait is unique) and the call spells `Add<i64>`.
    let mismatched = Case {
        label: "spelled_trait_args_mismatch",
        extra_items: "#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let sum = <Money as Add<i64>>::add(Money(1), 1); assert_eq!(sum.0, 2); } }",
        ..spelled
    };
    assert_eq!(classes(&mismatched)?, [ExposureClass::StaticUnknown]);
    // Negative guarded by `trait_unique` rather than the argument check: a
    // second `Add<i64>` impl exists, so the owner's trait is ambiguous. The
    // argument-mismatch rejection is covered by the mismatch case above and
    // the `type_path_calls` unit test.
    let other_impl = Case {
        label: "spelled_trait_args_other_impl",
        extra_items: "impl Add<i64> for Money { type Output = Money; fn add(self, other: i64) -> Money { Money(self.0 + other) } }\n#[cfg(test)] mod named { use super::*; #[test] fn named_call() { let sum = <Money as Add<i64>>::add(Money(1), 1); assert_eq!(sum.0, 2); } }",
        ..spelled
    };
    assert_eq!(classes(&other_impl)?, [ExposureClass::StaticUnknown]);
    Ok(())
}

#[test]
fn an_unresolved_macro_invocation_in_the_test_scope_withholds_the_credit() -> Result<(), String> {
    // A sibling-defined macro invoked in the test's enclosing module expands
    // to a foreign `Money` import; the invocation never names `Money`. The
    // expansion cannot be read statically, so the enclosing scope is not
    // clear and the type-path call stays withheld.
    let base = Case {
        label: "macro_binding",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        ..ADD
    };
    let foreign = Case {
        label: "macro_binding_foreign",
        extra_items: "mod other { pub type Money = i64; }\nmod defs { macro_rules! foreign_money { () => { use super::other::Money; } } pub(crate) use foreign_money; }\n#[cfg(test)] mod named { use super::*; use super::defs::foreign_money; foreign_money!(); #[test] fn named_call() { let _ = Money::add(1, 1); } }",
        ..base
    };
    assert_eq!(classes(&foreign)?, [ExposureClass::StaticUnknown]);
    Ok(())
}

/// Relating the test says the operator runs, not that an assertion would
/// notice the impl change. A result built from the impl's own tokens
/// (`Money(..)`) is observed by the test's `Money(..)` operands and reads
/// `exposed`; one built through `Self(..)` shares no token with the assertion,
/// so the credit is unconfirmed and the gap stays withheld rather than turning
/// into an "add a test" signal for code the test pins.
#[test]
fn a_credit_no_assertion_confirms_is_withheld_not_a_gap() -> Result<(), String> {
    let confirmed = with_body(
        ADD,
        "confirmed",
        "assert_eq!(Money(2) + Money(3), Money(5));",
    );
    assert_eq!(classes(&confirmed)?, [ExposureClass::Exposed]);

    let unconfirmed = Case {
        label: "self_constructor",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        ..ADD
    };
    let findings = findings_of(&unconfirmed)?;
    let got: Vec<_> = findings
        .iter()
        .map(|finding| finding.class.clone())
        .collect();
    if got != [ExposureClass::StaticUnknown] {
        return Err(format!("self_constructor: got {got:?}"));
    }
    let limit = findings[0].static_limit_kind.map(|kind| kind.as_str());
    assert_eq!(limit, Some("rust_implicit_call_unresolved"));

    // A result bound under the method's own name is not a call: the operator
    // still reaches the impl, no assertion confirms the changed line, and the
    // gap stays withheld.
    let bound_as_method_name = Case {
        label: "bound_as_add",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        test_body: "let add = Money(2) + Money(3);\n    assert_eq!(add, Money(5));",
        ..ADD
    };
    assert_eq!(
        classes(&bound_as_method_name)?,
        [ExposureClass::StaticUnknown]
    );

    // A second test that only exercises the operator is no more confirming.
    let two_operator_tests = Case {
        label: "two_operator_tests",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod more {
    use super::*;

    #[test]
    fn second_operator_use() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&two_operator_tests)?,
        [ExposureClass::StaticUnknown]
    );

    // A test that names the method decided reach itself, so the finding is
    // judged by its own evidence, not withheld as syntax-only.
    let one_named_call = Case {
        label: "one_named_call",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod named {
    use super::*;

    #[test]
    fn named_call() {
        let sum = Money(1).add(Money(1));
        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&one_named_call)?, [ExposureClass::WeaklyExposed]);

    // A path call on another type is not a call of this impl: still syntax-only (#10551).
    let other_type_path = Case {
        label: "other_type_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = Wrapper::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&other_type_path)?, [ExposureClass::StaticUnknown]);

    // A longer method name is not the owner (#10551).
    let longer_method_path = Case {
        label: "longer_method_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = Money::address(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&longer_method_path)?,
        [ExposureClass::StaticUnknown]
    );

    // A type whose name merely ends in the impl type is another type (#10551).
    let longer_type_path = Case {
        label: "longer_type_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = MyMoney::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&longer_type_path)?, [ExposureClass::StaticUnknown]);

    // Another trait's `add` on the same type is not a call of this impl (#10551).
    let other_trait_path = Case {
        label: "other_trait_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = <Money as Audit>::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&other_trait_path)?, [ExposureClass::StaticUnknown]);

    // A path inside `stringify!` is text, never evaluated (#10551).
    let stringified_path = Case {
        label: "stringified_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let _ = stringify!(Money::add(1, 2));\n        let sum = Money(1) + Money(1);\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&stringified_path)?, [ExposureClass::StaticUnknown]);

    // An inherent `add` wins `Money::add(..)`, so the call is not this trait impl (#10551).
    let inherent_method_wins = Case {
        label: "inherent_method_wins",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "impl Money {\n    pub fn add(a: Money, b: Money) -> Money {\n        Money(a.0 + b.0)\n    }\n}\n
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = Money::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&inherent_method_wins)?,
        [ExposureClass::StaticUnknown]
    );

    // A module path with blanks (or a masked comment) is still another module's `Money` (#10551).
    let blanked_module_path = Case {
        label: "blanked_module_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = crate::other:: Money::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&blanked_module_path)?,
        [ExposureClass::StaticUnknown]
    );

    // A `Money` the test imports from elsewhere is not the impl type (#10551).
    let imported_type_path = Case {
        label: "imported_type_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        use crate::other::Money;\n        let sum = Money::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&imported_type_path)?,
        [ExposureClass::StaticUnknown]
    );

    // A `Money` the test declares itself is not the impl type (#10551).
    let redeclared_type_path = Case {
        label: "redeclared_type_path",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        struct Money;\n        let sum = Money::add(1, 1);\n        assert_eq!(sum, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&redeclared_type_path)?,
        [ExposureClass::StaticUnknown]
    );

    // A path call that asserts nothing lifts only the withholding: the finding stays an unconfirmed `weakly_exposed`, it does not borrow evidence (#10551).
    let path_call_without_assertion = Case {
        label: "path_call_without_assertion",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let _ = Money::add(Money(1), Money(1));
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&path_call_without_assertion)?,
        [ExposureClass::WeaklyExposed]
    );

    // A path call beyond the display window still lifts the syntax-only limit;
    // renaming the test does not change the classification (#10625).
    let crowded_path_call_last = Case {
        label: "crowded_path_call_last",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod crowded {
    use super::*;

    #[test]
    fn a01_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a02_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a03_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a04_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a05_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a06_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a07_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn zz_pathed() {
        let sum = Money::add(Money(1), Money(1));
        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    let crowded_last = findings_of(&crowded_path_call_last)?;
    assert_eq!(crowded_last.len(), 1);
    assert!(
        crowded_last[0]
            .related_tests_matched_total
            .is_some_and(|total| total > 8)
    );
    assert_eq!(crowded_last[0].related_tests.len(), 8);
    assert!(
        !crowded_last[0]
            .related_tests
            .iter()
            .any(|test| test.name == "zz_pathed")
    );
    assert_eq!(crowded_last[0].class, ExposureClass::WeaklyExposed);
    let crowded_path_call_first = Case {
        label: "crowded_path_call_first",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod crowded {
    use super::*;

    #[test]
    fn a01_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a02_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a03_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a04_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a05_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a06_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a07_operator() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }

    #[test]
    fn a00_pathed() {
        let sum = Money::add(Money(1), Money(1));
        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&crowded_path_call_first)?,
        [ExposureClass::WeaklyExposed]
    );

    // `Type::method(..)` names the owner like `.method(..)`, so the finding is judged by that test's own evidence (#10551).
    let type_path_call = Case {
        label: "type_path_call",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = Money::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(classes(&type_path_call)?, [ExposureClass::WeaklyExposed]);

    // `<Type as Trait>::method(..)` names the owner too (#10551).
    let qualified_path_call = Case {
        label: "qualified_path_call",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod pathed {
    use super::*;

    #[test]
    fn pathed_call() {
        let sum = <Money as Add>::add(Money(1), Money(1));\n        assert_eq!(sum.0, 2);
    }
}
",
        ..ADD
    };
    assert_eq!(
        classes(&qualified_path_call)?,
        [ExposureClass::WeaklyExposed]
    );

    // Same-named tests in different inline modules are different tests. The
    // later one calls the method and observes the sum, so the finding is
    // judged by that test too (it was once dropped as a duplicate by name).
    let same_name_named_call_later = Case {
        label: "same_name_named_call_later",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod operator {
    use super::*;

    #[test]
    fn operator_under_test() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }
}
",
        test_body: "let other = Money(2).add(Money(3));\n        assert_eq!(other.0, 5);",
        ..ADD
    };
    assert_eq!(
        classes(&same_name_named_call_later)?,
        [ExposureClass::Exposed]
    );

    // The row for the later test must resolve to that test's own body: an
    // earlier operator-only test of the same name must not stand in for a
    // later one that calls `.add(` and confirms nothing.
    let same_name_unconfirmed_named_call_later = Case {
        label: "same_name_unconfirmed_named_call_later",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "
#[cfg(test)]
mod operator {
    use super::*;

    #[test]
    fn operator_under_test() {
        let sum = Money(1) + Money(1);
        assert_eq!(sum.0, 2);
    }
}
",
        test_body: "let r = Money(2).add(Money(3));\n        assert_eq!(r.0, 5);",
        ..ADD
    };
    assert_eq!(
        classes(&same_name_unconfirmed_named_call_later)?,
        [ExposureClass::WeaklyExposed]
    );

    // A finding whose infection is not read has a different missing leg: the
    // changed line sits behind a branch the test's operands never take.
    let branch_not_driven = Case {
        label: "branch_not_driven",
        signature: "type Output = Money;\n    fn add(self, other: Money) -> Money",
        old_line: "        if self.0 > 100 { Money(self.0 + other.0) } else { Money(0) }",
        new_line: "        if self.0 > 100 { Money(other.0 + self.0) } else { Money(0) }",
        test_body: "assert_eq!(Money(2) + Money(3), Money(0));",
        ..ADD
    };
    assert_eq!(classes(&branch_not_driven)?, [ExposureClass::WeaklyExposed]);

    // The same impl is a gap when the test never exercises the operator.
    let unexercised = Case {
        label: "self_constructor_unexercised",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        test_body: "assert_eq!(Money(5), Money(5));",
        ..ADD
    };
    let got = classes(&unexercised)?;
    assert!(
        !got.contains(&ExposureClass::StaticUnknown),
        "no operator use is a real gap: {got:?}"
    );
    Ok(())
}

const INDEX_IMPL: &str = "impl Index<usize> for Money {
    type Output = i64;
    fn index(&self, _at: usize) -> &i64 {
        &self.0
    }
}
";

/// `Money` implements `Index` only: the owner is `index`, reached by reads.
const INDEX: Case = Case {
    label: "index",
    trait_name: "Index<usize>",
    method: "index",
    signature: "type Output = i64;\n    fn index(&self, _at: usize) -> &i64",
    old_line: "        &self.0",
    new_line: "        &(self.0)",
    extra_items: "",
    sibling: "",
    impl_type: "Money",
    test_body: "let m = Money(7);\n        assert_eq!(m[0], 7);",
};

/// `Money` implements both: the owner is `index_mut`, reached by writes.
const INDEX_MUT: Case = Case {
    label: "index_mut",
    trait_name: "IndexMut<usize>",
    method: "index_mut",
    signature: "fn index_mut(&mut self, _at: usize) -> &mut i64",
    old_line: "        &mut self.0",
    new_line: "        &mut (self.0)",
    extra_items: INDEX_IMPL,
    sibling: "",
    impl_type: "Money",
    test_body: "let mut m = Money(1);\n        m[0] = 5;\n        assert_eq!(m.0, 5);",
};

/// `a[i]` reads through `Index` and writes through `IndexMut`; a site is
/// credited to the one trait it runs, and a projection (`a[i].x`) that may
/// borrow either way is credited to neither.
#[test]
fn an_index_site_reaches_the_trait_it_runs() -> Result<(), String> {
    let reads = [
        "let m = Money(7);\n        assert_eq!(m[0], 7);",
        "let m = Money(7);\n        let x = &m[0];\n        assert_eq!(*x, 7);",
    ];
    let writes = [
        "let mut m = Money(1);\n        m[0] = 5;\n        assert_eq!(m.0, 5);",
        "let mut m = Money(1);\n        m[0] += 2;\n        assert_eq!(m.0, 3);",
        "let mut m = Money(1);\n        let r = &mut m[0];\n        *r = 9;\n        assert_eq!(m.0, 9);",
    ];
    let mut failures = Vec::new();
    for body in reads {
        let credited = direct_tests(&with_body(INDEX, "index_read", body))?;
        if credited != ["operator_under_test"] {
            failures.push(format!("index read not credited to index: {body:?}"));
        }
        let wrong = direct_tests(&with_body(INDEX_MUT, "index_mut_read", body))?;
        if !wrong.is_empty() {
            failures.push(format!("index read credited to index_mut: {body:?}"));
        }
    }
    for body in writes {
        let credited = direct_tests(&with_body(INDEX_MUT, "index_mut_write", body))?;
        if credited != ["operator_under_test"] {
            failures.push(format!("index write not credited to index_mut: {body:?}"));
        }
        let wrong = direct_tests(&with_body(INDEX, "index_write", body))?;
        if !wrong.is_empty() {
            failures.push(format!("index write credited to index: {body:?}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// An index site whose receiver is not established as `Money`, whose trait is
/// ambiguous, or which runs no impl, is credited to nothing.
#[test]
fn index_sites_that_do_not_run_the_impl_stay_unrelated() -> Result<(), String> {
    let other_type = "
struct Other(i64);
impl Index<usize> for Other {
    type Output = i64;
    fn index(&self, _at: usize) -> &i64 {
        &self.0
    }
}
";
    let second_impl = "
impl Index<&str> for Money {
    type Output = i64;
    fn index(&self, _name: &str) -> &i64 {
        &self.0
    }
}
";
    let shadowed = "
mod other {
    pub struct Money(pub i64);
}
";
    let cases: [(&str, Case); 20] = [
        (
            "another type's impl",
            Case {
                extra_items: other_type,
                test_body: "let o = Other(3);\n        assert_eq!(o[0], 3);",
                ..INDEX
            },
        ),
        (
            "a vec of the type",
            with_body(
                INDEX,
                "vec_of_money",
                "let v = vec![Money(1)];\n        assert_eq!(v[0].0, 1);",
            ),
        ),
        (
            "an untyped array",
            with_body(
                INDEX,
                "untyped_array",
                "let m = Money(1);\n        let arr = [1, 2];\n        assert_eq!(arr[0] + m.0, 2);",
            ),
        ),
        (
            "a field receiver",
            with_body(
                INDEX,
                "field_receiver",
                "let holder = Holder { money: Money(1) };\n        assert_eq!(holder.money[0], 1);",
            ),
        ),
        (
            "a helper's return value",
            with_body(INDEX, "helper_return", "assert_eq!(make()[0], 1);"),
        ),
        (
            "a second impl of the trait for the type",
            Case {
                extra_items: second_impl,
                ..INDEX
            },
        ),
        (
            "stringify",
            with_body(
                INDEX,
                "stringified",
                "let m = Money(1);\n        assert_eq!(stringify!(m[0]), \"m[0]\");",
            ),
        ),
        (
            "a read whose value is discarded",
            with_body(
                INDEX,
                "discarded",
                "let m = Money(1);\n        let _ = m[0];",
            ),
        ),
        (
            "a slice pattern",
            with_body(
                INDEX,
                "slice_pattern",
                "let m = Money(1);\n        let [a, b] = [m.0, 2];\n        assert_eq!(a + b, 3);",
            ),
        ),
        (
            "an array type",
            with_body(
                INDEX,
                "array_type",
                "let m = Money(1);\n        let grid: [i64; 2] = [m.0, 2];\n        assert_eq!(grid.len(), 2);",
            ),
        ),
        (
            "a projected read that may borrow mutably",
            with_body(
                INDEX,
                "projected",
                "let mut m = Money(1);\n        assert_eq!(m[0].abs(), 1);",
            ),
        ),
        (
            "a parenthesised place",
            with_body(
                INDEX,
                "parenthesised",
                "let mut m = Money(1);\n        (m[0]).push(1);",
            ),
        ),
        (
            "a destructuring assignment",
            with_body(
                INDEX,
                "destructured",
                "let mut m = Money(1);\n        (m[0], m[1]) = (1, 2);",
            ),
        ),
        (
            "a ref mut binding",
            with_body(
                INDEX,
                "ref_mut",
                "let mut m = Money(1);\n        let ref mut r = m[0];\n        *r = 1;",
            ),
        ),
        (
            "a parenthesised mutable borrow",
            with_body(
                INDEX,
                "paren_mut_borrow",
                "let mut m = Money(1);\n        let r = &mut (m[0]);\n        *r = 9;",
            ),
        ),
        (
            "a match scrutinee with a ref mut arm",
            with_body(
                INDEX,
                "match_ref_mut",
                "let mut m = Money(1);\n        match m[0] {\n            ref mut x => *x = 9,\n        }",
            ),
        ),
        (
            "a call on the element",
            with_body(
                INDEX,
                "element_call",
                "let mut m = Money(1);\n        m[0](1);",
            ),
        ),
        (
            "a chained index",
            with_body(
                INDEX,
                "chained",
                "let m = Money(1);\n        assert_eq!(m[0][1], 1);",
            ),
        ),
        (
            "a type the test redeclares",
            Case {
                test_body: "struct Money(i64);\n        let m = Money(1);\n        assert_eq!(m[0], 1);",
                ..INDEX
            },
        ),
        (
            "a same-named type imported from another module",
            Case {
                extra_items: shadowed,
                test_body: "use other::Money;\n        let m = Money(1);\n        assert_eq!(m[0], 1);",
                ..INDEX
            },
        ),
    ];
    let mut failures = Vec::new();
    for (what, case) in &cases {
        let direct = direct_tests(case)?;
        if !direct.is_empty() {
            failures.push(format!("{what}: credited {direct:?}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Crediting an index site says the impl runs, not that an assertion notices
/// a wrong result, so an unconfirmed credit is withheld, never an actionable
/// `weakly_exposed`.
#[test]
fn an_unconfirmed_index_credit_is_withheld() -> Result<(), String> {
    let changed = Case {
        new_line: "        &self.0.min(0)",
        ..INDEX
    };
    // The changed line's finding is the one whose infection is read; the
    // other finding comes from the unchanged signature lines.
    let on_changed_line = |case: &Case| -> Result<Finding, String> {
        findings_of(case)?
            .into_iter()
            .find(|finding| finding.ripr.infect.state == ripr::domain::StageState::Yes)
            .ok_or_else(|| format!("{}: no finding with a read infection", case.label))
    };
    let withheld = on_changed_line(&changed)?;
    assert_eq!(withheld.class, ExposureClass::StaticUnknown);
    let limit = withheld.static_limit_kind.map(|kind| kind.as_str());
    assert_eq!(limit, Some("rust_implicit_call_unresolved"));

    // A test with no exact assertion is a real gap whatever runs the owner.
    let weak = Case {
        test_body: "let m = Money(7);\n        let x = m[0];\n        assert!(x > 0);",
        ..changed
    };
    assert_eq!(on_changed_line(&weak)?.class, ExposureClass::WeaklyExposed);
    Ok(())
}

/// A `}` can finish a statement or a left operand. This syntax-only reader
/// conservatively relates neither impl for an unparenthesized block statement.
#[test]
fn block_statements_followed_by_minus_stay_unrelated() -> Result<(), String> {
    let neg = Case {
        label: "neg after block statement",
        trait_name: "Neg",
        method: "neg",
        signature: "type Output = Money;\n    fn neg(self) -> Money",
        old_line: "        Money(-self.0)",
        new_line: "        Money(0 - self.0)",
        extra_items: "impl Sub for Money { type Output = Money; fn sub(self, other: Money) -> Money { Money(self.0 - other.0) } }",
        sibling: "",
        impl_type: "Money",
        test_body: "",
    };
    let sub = Case {
        label: "sub after block statement",
        trait_name: "Sub",
        method: "sub",
        signature: "type Output = Money;\n    fn sub(self, other: Money) -> Money",
        old_line: "        Money(self.0 - other.0)",
        new_line: "        Money((self.0) - other.0)",
        extra_items: "impl Neg for Money { type Output = Money; fn neg(self) -> Money { Money(-self.0) } }",
        sibling: "",
        impl_type: "Money",
        test_body: "",
    };
    for case in [&neg, &sub] {
        for body in [
            "let n = Money(2);\n        if true {} -n;\n        assert_eq!(Money(9).0, 9);",
            "let n = Money(2);\n        {} -n;\n        assert_eq!(Money(9).0, 9);",
            "let n = Money(2);\n        match true { true => (), false => () } -n;\n        assert_eq!(Money(9).0, 9);",
        ] {
            let direct = direct_tests(&Case {
                test_body: body,
                ..*case
            })?;
            assert!(direct.is_empty(), "{}: {body}: {direct:?}", case.label);
        }
    }
    // The same brace before a real typed left operand still selects Sub.
    let direct = direct_tests(&Case {
        test_body: "let n = Money(2);\n        let r = Money { 0: 5 } - n;\n        assert_eq!(r.0, 3);",
        ..sub
    })?;
    assert_eq!(direct, ["operator_under_test"]);
    // A separate, observed unary expression still selects Neg.
    let direct = direct_tests(&Case {
        test_body: "let n = Money(2);\n        if true {}\n        let r = -n;\n        assert_eq!(r, Money(-2));",
        ..neg
    })?;
    assert_eq!(direct, ["operator_under_test"]);
    Ok(())
}

/// Plain `+=` mutates its Self operand despite returning unit. Crediting the
/// relation still requires that exact operand type and one matching impl.
#[test]
fn standalone_add_assign_keeps_typed_operand_and_unique_impl_gates() -> Result<(), String> {
    let assign = Case {
        label: "standalone AddAssign",
        trait_name: "AddAssign",
        method: "add_assign",
        signature: "fn add_assign(&mut self, other: Money)",
        old_line: "        self.0 += other.0;",
        new_line: "        self.0 = self.0 + other.0;",
        extra_items: "",
        sibling: "",
        impl_type: "Money",
        test_body: "let mut total = Money(1);\n        total += Money(2);\n        assert_eq!(total, Money(3));",
    };
    assert_eq!(direct_tests(&assign)?, ["operator_under_test"]);
    let wrong_type = Case {
        label: "integer AddAssign beside unused Money",
        test_body: "let money = Money(1);\n        let mut total = 1i64;\n        total += 2;\n        assert_eq!(total, 3);\n        let _ = money;",
        ..assign
    };
    assert!(direct_tests(&wrong_type)?.is_empty());
    let second_impl = Case {
        label: "second AddAssign impl",
        extra_items: "impl AddAssign<i64> for Money { fn add_assign(&mut self, other: i64) { self.0 += other; } }",
        ..assign
    };
    assert!(direct_tests(&second_impl)?.is_empty());
    // The compound operator cannot supply a relation for Add::add.
    let add = Case {
        extra_items: "impl AddAssign for Money { fn add_assign(&mut self, other: Money) { self.0 += other.0; } }",
        ..with_body(ADD, "Add is not AddAssign", assign.test_body)
    };
    assert!(direct_tests(&add)?.is_empty());
    Ok(())
}

#[test]
fn capped_rows_do_not_hide_a_named_operator_call() -> Result<(), String> {
    let common = Case {
        label: "uncapped_named_call",
        old_line: "        Self(self.0 + other.0)",
        new_line: "        Self(other.0 + self.0)",
        extra_items: "#[cfg(test)] mod extra { use super::*; #[test] fn operator_use() { let sum = Money(1) + Money(1); assert_eq!(sum.0, 2); } }",
        test_body: "let sum = Money::add(Money(1), Money(1)); assert_ne!(sum.0, 3);",
        ..ADD
    };
    assert_eq!(classes(&common)?, [ExposureClass::WeaklyExposed]);
    let operator = Case {
        label: "capped_operator_only",
        test_body: "let sum = Money(1) + Money(1);\n        assert_ne!(sum.0, 3);\n        assert_ne!(sum.0, 4);\n        assert_ne!(sum.0, 5);\n        assert_ne!(sum.0, 6);\n        assert_ne!(sum.0, 7);\n        assert_ne!(sum.0, 8);\n        assert_ne!(sum.0, 9);\n        assert_ne!(sum.0, 10);\n        assert_ne!(sum.0, 11);",
        ..common
    };
    let operator_findings = findings_of(&operator)?;
    assert_eq!(operator_findings.len(), 1);
    assert!(
        operator_findings[0]
            .related_tests_matched_total
            .is_some_and(|total| total > 8)
    );
    assert_eq!(operator_findings[0].related_tests.len(), 8);
    assert_eq!(operator_findings[0].class, ExposureClass::StaticUnknown);
    let ignored_named = Case {
        label: "capped_ignored_named_call",
        test_body: "let sum = Money(1) + Money(1);\n        assert_eq!(sum.0, 2);\n        assert_eq!(sum.0, 1 + 1);\n        assert_eq!(sum.0, 3 - 1);\n        assert_eq!(sum.0, 4 - 2);\n        assert_eq!(sum.0, 5 - 3);\n        assert_eq!(sum.0, 6 - 4);\n        assert_eq!(sum.0, 7 - 5);\n        assert_eq!(sum.0, 8 - 6);\n        assert_eq!(sum.0, 9 - 7);",
        extra_items: "
#[cfg(test)]
mod ignored {
    use super::*;
    #[test]
    #[ignore]
    fn named_call() {
        let sum = Money::add(Money(1), Money(1));
        assert_eq!(sum.0, 2);
    }
}",
        ..operator
    };
    let ignored_findings = findings_of(&ignored_named)?;
    assert_eq!(ignored_findings.len(), 1);
    assert!(
        ignored_findings[0]
            .related_tests_matched_total
            .is_some_and(|total| total > 8)
    );
    assert_eq!(ignored_findings[0].class, ExposureClass::StaticUnknown);
    assert_eq!(
        ignored_findings[0]
            .static_limit_kind
            .map(|kind| kind.as_str()),
        Some("rust_implicit_call_unresolved")
    );
    let named = Case {
        label: "capped_named_call",
        test_body: "let sum = Money::add(Money(1), Money(1));\n        assert_ne!(sum.0, 3);\n        assert_ne!(sum.0, 4);\n        assert_ne!(sum.0, 5);\n        assert_ne!(sum.0, 6);\n        assert_ne!(sum.0, 7);\n        assert_ne!(sum.0, 8);\n        assert_ne!(sum.0, 9);\n        assert_ne!(sum.0, 10);\n        assert_ne!(sum.0, 11);",
        ..common
    };
    let named_findings = findings_of(&named)?;
    assert_eq!(named_findings.len(), 1);
    assert!(
        named_findings[0]
            .related_tests_matched_total
            .is_some_and(|total| total > 8)
    );
    assert_eq!(named_findings[0].related_tests.len(), 8);
    assert!(
        named_findings[0]
            .related_tests
            .iter()
            .any(|test| test.name == "operator_under_test"
                && test.relation_reason == Some(RelationReason::DirectOwnerCall))
    );
    assert_eq!(named_findings[0].class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn capped_method_call_must_bind_the_operator_owner() -> Result<(), String> {
    fn check(label: &str, call: &str, operators: usize) -> Result<Vec<Finding>, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-cap-receiver-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        let mut source = "use std::ops::*;\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\nimpl Add for Money {\n    type Output = Money;\n    fn add(self, other: Money) -> Money {\n        Self(other.0 + self.0)\n    }\n}\n#[cfg(test)] mod tests {\n    use super::*;\n".to_string();
        for i in 0..operators {
            source.push_str(&format!("    #[test]\n    fn a{i:02}_operator() {{\n        let sum = Money(1) + Money(1);\n        assert_eq!(sum.0, 2);\n    }}\n"));
        }
        source.push_str(&format!("    #[test]\n    fn zz_test() {{\n        let sum = Money(1) + Money(1);\n        {call}\n        assert_eq!(sum.0, 2);\n    }}\n}}\n"));
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"cap-receiver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,1 +7,1 @@\n-        Self(self.0 + other.0)\n+        Self(other.0 + self.0)\n").map_err(|e| e.to_string())?;
        let output = check_workspace(CheckInput {
            root: root.clone(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: false,
            ..CheckInput::default()
        })?;
        let findings: Vec<_> = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 7)
            .collect();
        std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        if findings.len() != 1 {
            return Err(format!(
                "{label}: expected one finding, got {}",
                findings.len()
            ));
        }
        Ok(findings)
    }
    let foreign = check(
        "foreign",
        "let n = 1i64.add(1);\n        assert_eq!(n, 2);",
        8,
    )?;
    assert!(
        foreign[0]
            .related_tests_matched_total
            .is_some_and(|total| total > 8)
    );
    assert_eq!(foreign[0].class, ExposureClass::StaticUnknown);
    let foreign_binding = check(
        "foreign-binding",
        "let n: i64 = 1;\n        let value = n.add(1);\n        assert_eq!(value, 2);",
        8,
    )?;
    assert_eq!(foreign_binding[0].class, ExposureClass::StaticUnknown);
    let uncapped = check(
        "uncapped-foreign",
        "let n = 1i64.add(1);\n        assert_eq!(n, 2);",
        1,
    )?;
    assert_eq!(uncapped[0].class, ExposureClass::WeaklyExposed);
    let operator_only = check("operator-only", "", 8)?;
    assert_eq!(operator_only[0].class, ExposureClass::StaticUnknown);
    for (label, call) in [
        (
            "constructor",
            "let result = Money(1).add(Money(1));\n        assert_eq!(result.0, 2);",
        ),
        (
            "binding",
            "let money = Money(1);\n        let result = money.add(Money(1));\n        assert_eq!(result.0, 2);",
        ),
        (
            "mixed",
            "let n = 1i64.add(1);\n        let result = Money(1).add(Money(1));\n        assert_eq!(n, 2);\n        assert_eq!(result.0, 2);",
        ),
        (
            "type_path",
            "let result = Money::add(Money(1), Money(1));\n        assert_eq!(result.0, 2);",
        ),
    ] {
        let findings = check(label, call, 8)?;
        assert!(
            findings[0]
                .related_tests_matched_total
                .is_some_and(|total| total > 8),
            "{label}"
        );
        assert_eq!(findings[0].class, ExposureClass::WeaklyExposed, "{label}");
    }
    Ok(())
}

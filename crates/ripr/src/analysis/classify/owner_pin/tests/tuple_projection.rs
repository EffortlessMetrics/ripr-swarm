//! #12796: a one-field integer tuple struct built whole by the changed tail is
//! pinned by `owner(..).0` the way `owner(..) == T(literal)` pins it, on a
//! method called through a tuple-constructor receiver (`Money(2).add(..)`),
//! under the standard operator trait the test file imports.

use super::*;

const ADD_LIB: &str = "use std::ops::Add;\n\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\n\nimpl Add for Money {\n    type Output = Money;\n    fn add(self, other: Money) -> Money {\n        Self(other.0 + self.0)\n    }\n}\n";
const ADD_CHANGED: &str = "Self(other.0 + self.0)";

fn admitted_by_add_pin(lib: &str, tests: &str) -> Result<usize, String> {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "add", ADD_CHANGED)
        .ok_or_else(|| "the owner pin must establish".to_string())?;
    Ok(admitted_texts(&index, &pin).len())
}

fn test_file(imports: &str, body: &str) -> String {
    format!("use demo::Money;\n{imports}\n#[test]\nfn adds() {{\n{body}\n}}\n")
}

#[test]
fn a_projection_of_a_bound_or_inline_result_pins_the_tuple_value() -> Result<(), String> {
    for (label, body) in [
        (
            "bound result",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        (
            "inline result",
            "    assert_eq!(Money(2).add(Money(3)).0, 5);",
        ),
        (
            "projection on the expected side",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(5, r.0);",
        ),
    ] {
        let tests = test_file("use std::ops::Add;\n", body);
        assert_eq!(admitted_by_add_pin(ADD_LIB, &tests)?, 1, "{label}");
    }
    Ok(())
}

#[test]
fn a_projection_that_does_not_read_the_changed_value_is_not_a_pin() -> Result<(), String> {
    let std_add = "use std::ops::Add;\n";
    for (label, imports, body) in [
        // The trait must be the standard one the file imports.
        (
            "no trait import",
            "",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        (
            "foreign trait import",
            "use other::Add;\n",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        (
            "renamed trait import",
            "use std::ops::Add as Plus;\n",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        // Both operands projected compares two reads, not a literal.
        (
            "projection against a projection",
            std_add,
            "    let r = Money(2).add(Money(3));\n    let s = Money(2).add(Money(3));\n    assert_eq!(r.0, s.0);",
        ),
        // The binding is read again, so it may be changed before the read.
        (
            "binding used elsewhere",
            std_add,
            "    let r = Money(2).add(Money(3));\n    let again = r;\n    assert_eq!(r.0, 5);\n    assert_eq!(again.0, 5);",
        ),
        // Another receiver type is not the owner's.
        (
            "other receiver type",
            std_add,
            "    let r = Other(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        // A projection of anything but the field of the owner's own result.
        (
            "field two",
            std_add,
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.1, 5);",
        ),
    ] {
        let tests = test_file(imports, body);
        assert_eq!(admitted_by_add_pin(ADD_LIB, &tests)?, 0, "{label}");
    }
    Ok(())
}

#[test]
fn the_struct_must_be_one_built_in_integer_field() -> Result<(), String> {
    let body = "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);";
    let tests = test_file("use std::ops::Add;\n", body);
    for (label, struct_decl) in [
        ("string field", "pub struct Money(pub String);"),
        ("float field", "pub struct Money(pub f64);"),
        ("two fields", "pub struct Money(pub i64, pub i64);"),
        ("named field", "pub struct Money { pub v: i64 }"),
        ("generic", "pub struct Money<T>(pub T);"),
    ] {
        let lib = ADD_LIB.replace("pub struct Money(pub i64);", struct_decl);
        let index = index(&[(LIB, &lib), (TESTS, &tests)]);
        let admitted = establish(&index, "add", ADD_CHANGED)
            .map_or(0, |pin| admitted_texts(&index, &pin).len());
        assert_eq!(admitted, 0, "{label}");
    }
    Ok(())
}

#[test]
fn a_tail_that_is_not_the_whole_tuple_value_has_no_projection_pin() {
    // The tuple is built from two arguments, or something else wraps it.
    for (label, lib_tail) in [
        ("two arguments", "Self(other.0 + self.0, 0)"),
        ("wrapped", "Self(other.0 + self.0).clone()"),
        ("another type", "Other(other.0 + self.0)"),
    ] {
        let lib = ADD_LIB.replace("Self(other.0 + self.0)", lib_tail);
        let tests = test_file(
            "use std::ops::Add;\n",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        );
        let index = index(&[(LIB, &lib), (TESTS, &tests)]);
        let admitted =
            establish(&index, "add", lib_tail).map_or(0, |pin| admitted_texts(&index, &pin).len());
        assert_eq!(admitted, 0, "{label}");
    }
}

/// The pin for the tuple-struct owner whose changed line used to read
/// `before`, so the closed subset can read both versions (#12808).
fn admitted_with_before(
    lib: &str,
    owner_name: &str,
    changed: &str,
    before: &str,
    tests: &str,
) -> Result<usize, String> {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let owner = owner(&index, owner_name);
    let mut probe = return_probe(owner, changed);
    probe.before = Some(before.to_string());
    let pin = OwnerReturnPin::establish(&probe, owner, &index)
        .ok_or_else(|| "the owner pin must establish".to_string())?;
    Ok(admitted_texts(&index, &pin).len())
}

const ADD_BEFORE: &str = "Self(other.0 - self.0)";
const PLUS_LIB: &str = "#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\n\npub fn plus(a: Money, b: Money) -> Money {\n    Money(a.0 + b.0)\n}\n";

#[test]
fn operands_both_versions_agree_on_are_not_a_pin() -> Result<(), String> {
    let std_add = "use std::ops::Add;\n";
    for (label, body) in [
        (
            "bound projection",
            "    let r = Money(0).add(Money(0));\n    assert_eq!(r.0, 0);",
        ),
        (
            "inline projection",
            "    assert_eq!(Money(0).add(Money(0)).0, 0);",
        ),
        (
            "whole value",
            "    let r = Money(0).add(Money(0));\n    assert_eq!(r, Money(0));",
        ),
    ] {
        let tests = test_file(std_add, body);
        let admitted = admitted_with_before(ADD_LIB, "add", ADD_CHANGED, ADD_BEFORE, &tests)?;
        assert_eq!(admitted, 0, "{label}");
    }
    // Without the old line the closed subset has nothing to compare.
    let tests = test_file(std_add, "    assert_eq!(Money(0).add(Money(0)).0, 0);");
    assert_eq!(admitted_by_add_pin(ADD_LIB, &tests)?, 1, "no old line");
    Ok(())
}

#[test]
fn operands_the_versions_tell_apart_stay_a_pin() -> Result<(), String> {
    let std_add = "use std::ops::Add;\n";
    for (label, body) in [
        // 3 - 2 = 1 before, 2 + 3 = 5 after.
        (
            "bound projection",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
        ),
        (
            "whole value",
            "    let r = Money(2).add(Money(3));\n    assert_eq!(r, Money(5));",
        ),
        // -2 + 3 = 1 after, 3 - -2 = 5 before.
        (
            "negative operand",
            "    let r = Money(-2).add(Money(3));\n    assert_eq!(r.0, 1);",
        ),
    ] {
        let tests = test_file(std_add, body);
        let admitted = admitted_with_before(ADD_LIB, "add", ADD_CHANGED, ADD_BEFORE, &tests)?;
        assert_eq!(admitted, 1, "{label}");
    }
    Ok(())
}

#[test]
fn operands_the_subset_cannot_read_keep_the_pin() -> Result<(), String> {
    let std_add = "use std::ops::Add;\n";
    for (label, body) in [
        (
            "bound operand",
            "    let a = Money(0);\n    let r = a.add(Money(0));\n    assert_eq!(r.0, 0);",
        ),
        (
            "computed operand",
            "    let r = Money(0 * 1).add(Money(0));\n    assert_eq!(r.0, 0);",
        ),
    ] {
        let tests = test_file(std_add, body);
        let admitted = admitted_with_before(ADD_LIB, "add", ADD_CHANGED, ADD_BEFORE, &tests)?;
        assert_eq!(admitted, 1, "{label}");
    }
    Ok(())
}

#[test]
fn a_free_function_tuple_owner_withholds_the_agreeing_operands() -> Result<(), String> {
    let tests =
        |body: &str| format!("use demo::{{Money, plus}};\n#[test]\nfn adds() {{\n{body}\n}}\n");
    for (label, body, expected) in [
        (
            "zero operands",
            "    assert_eq!(plus(Money(0), Money(0)), Money(0));",
            0,
        ),
        (
            "operands the versions tell apart",
            "    assert_eq!(plus(Money(2), Money(3)), Money(5));",
            1,
        ),
    ] {
        let admitted = admitted_with_before(
            PLUS_LIB,
            "plus",
            "Money(a.0 + b.0)",
            "Money(a.0 - b.0)",
            &tests(body),
        )?;
        assert_eq!(admitted, expected, "{label}");
    }
    Ok(())
}

#[test]
fn a_rewrite_equal_on_every_input_keeps_the_pin() -> Result<(), String> {
    // `other.0 + self.0` to `self.0 + other.0` returns the same value for
    // every operand pair, so it is not an operand-specific blind spot.
    let tests = test_file(
        "use std::ops::Add;\n",
        "    let r = Money(2).add(Money(3));\n    assert_eq!(r.0, 5);",
    );
    let admitted = admitted_with_before(
        ADD_LIB,
        "add",
        ADD_CHANGED,
        "Self(self.0 + other.0)",
        &tests,
    )?;
    assert_eq!(admitted, 1);
    Ok(())
}

#[test]
fn operand_order_decides_whether_the_versions_agree() -> Result<(), String> {
    // after: self * other + other; before: self + other. They agree exactly
    // when self is 0 or other is 1, so swapping receiver and argument in the
    // lowered call changes the answer.
    let lib = "#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\n\nimpl Money {\n    pub fn mix(self, other: Money) -> Money {\n        Self(self.0 * other.0 + other.0)\n    }\n}\n";
    let changed = "Self(self.0 * other.0 + other.0)";
    let before = "Self(self.0 + other.0)";
    for (label, body, expected) in [
        (
            "receiver zero: versions agree",
            "    let r = Money(0).mix(Money(7));\n    assert_eq!(r.0, 7);",
            0,
        ),
        (
            "argument zero: versions differ",
            "    let r = Money(7).mix(Money(0));\n    assert_eq!(r.0, 7);",
            1,
        ),
    ] {
        let tests = test_file("", body);
        let admitted = admitted_with_before(lib, "mix", changed, before, &tests)?;
        assert_eq!(admitted, expected, "{label}");
    }
    Ok(())
}

#[test]
fn a_parameter_named_like_the_lowered_receiver_is_not_lowered() -> Result<(), String> {
    // `self_` would alias the receiver in the lowered owner, so the owner is
    // outside the subset and the discriminating pin keeps its credit.
    let lib = "#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Money(pub i64);\n\nimpl Money {\n    pub fn mix(self, self_: Money) -> Money {\n        Self(self_.0 * self.0)\n    }\n}\n";
    let tests = test_file(
        "",
        "    let r = Money(2).mix(Money(0));\n    assert_eq!(r.0, 0);",
    );
    let admitted = admitted_with_before(
        lib,
        "mix",
        "Self(self_.0 * self.0)",
        "Self(self_.0 + self.0)",
        &tests,
    )?;
    assert_eq!(admitted, 1);
    Ok(())
}

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

/// The standard Iterator's changed Option slot has its own whole-component
/// pin; retain each independently established owner, dispatch and field gate.
#[test]
fn iterator_option_tuple_projection_establishes_and_pins_its_actual_component() -> Result<(), String>
{
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rust_same_method_owner_type_positive");
    let lib =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let source = format!(
        "{lib}\n#[cfg(test)]\nmod tests {{\n use super::*;\n #[test]\n fn observes() {{\n  let it=WhileSome {{remaining:2}};\n  assert_eq!(it.size_hint().1,Some(it.remaining));\n }}\n}}\n"
    );
    let index = index(&[(LIB, &source)]);
    let owner = owner(&index, "size_hint");
    let mut probe = return_probe(owner, "(0, Some(self.remaining))");
    probe.before = Some("(0, None)".to_string());
    probe.after = Some(probe.expression.clone());
    let pin = OwnerReturnPin::establish(&probe, owner, &index).ok_or("owner pin missing")?;
    assert_eq!(
        admitted_texts(&index, &pin).len(),
        1,
        "actual projected assertion must pin its receiver's changed slot"
    );
    Ok(())
}

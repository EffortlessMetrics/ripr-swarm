//! Receiver-value admit/decline rules (#9329, RIPR-SPEC-0158).

use super::*;
use crate::analysis::facts::OwnedRustIndex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn index_of(files: &[(&str, &str)]) -> Result<RustIndex, String> {
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    let mut by_file = BTreeMap::new();
    for (path, source) in files {
        let facts =
            crate::analysis::syntax::ra::summarize_file_with_parser(Path::new(path), source)?;
        functions.extend(facts.functions.clone());
        tests.extend(facts.tests.clone());
        by_file.insert(PathBuf::from(path), facts);
    }
    Ok(RustIndex::from_owned(OwnedRustIndex {
        functions,
        tests,
        files: by_file,
        ..Default::default()
    }))
}

const BASE: &str = "pub const KIB: u64 = 1_024;\n\
#[derive(Clone, Copy)]\n\
pub struct ByteSize(pub u64);\n\
impl ByteSize {\n\
    pub const fn kib(size: u64) -> Self {\n\
        ByteSize(size * KIB)\n\
    }\n\
}\n";

fn cell(source: &str, argument: &str) -> Result<Option<String>, String> {
    Ok(receiver_cell_value(
        argument,
        &index_of(&[("src/lib.rs", source)])?,
    ))
}

#[test]
fn tuple_literal_and_const_constructor_build_canonical_cells() -> Result<(), String> {
    assert_eq!(
        cell(BASE, "ByteSize(215)")?,
        Some("ByteSize(215)".to_string())
    );
    assert_eq!(
        cell(BASE, " ByteSize( 7 ) ")?,
        Some("ByteSize(7)".to_string())
    );
    assert_eq!(
        cell(BASE, "ByteSize::kib(1)")?,
        Some("ByteSize(1024)".to_string())
    );
    assert_eq!(
        cell(BASE, "ByteSize::kib(3)")?,
        Some("ByteSize(3072)".to_string())
    );
    // A borrow of the constructed value is the same value.
    assert_eq!(cell(BASE, "&ByteSize(5)")?, Some("ByteSize(5)".to_string()));
    assert_eq!(
        cell(BASE, "&mut ByteSize::kib(1)")?,
        Some("ByteSize(1024)".to_string())
    );
    Ok(())
}

#[test]
fn argument_shapes_other_than_strict_literals_build_no_cell() -> Result<(), String> {
    for argument in [
        "ByteSize(n)",
        "ByteSize(1 + 2)",
        "ByteSize(1_024)",
        "ByteSize(1024u64)",
        "ByteSize(-1)",
        "ByteSize(0x10)",
        "ByteSize()",
        "ByteSize(1, 2)",
        "ByteSize::kib(n)",
        "ByteSize::kib(1 + 1)",
        "ByteSize::kib(1, 2)",
        "ByteSize::kib()",
        "ByteSize::missing(1)",
        "Other::kib(1)",
        "crate::ByteSize(1)",
        "ByteSize::kib(1).0",
        "ByteSize(1).clone()",
        "&&ByteSize(1)",
        "&mutByteSize(1)",
        "Self(1)",
    ] {
        assert_eq!(cell(BASE, argument)?, None, "{argument}");
    }
    // The literal must fit the field type.
    assert_eq!(
        cell(BASE, "ByteSize(18446744073709551615)")?,
        Some("ByteSize(18446744073709551615)".to_string())
    );
    assert_eq!(cell(BASE, "ByteSize(18446744073709551616)")?, None);
    Ok(())
}

#[test]
fn constructor_product_overflowing_the_field_type_builds_no_cell() -> Result<(), String> {
    // 2^54 * 1024 = 2^64 overflows u64.
    assert_eq!(cell(BASE, "ByteSize::kib(18014398509481984)")?, None);
    assert_eq!(
        cell(BASE, "ByteSize::kib(18014398509481983)")?,
        Some("ByteSize(18446744073709550592)".to_string())
    );
    Ok(())
}

#[test]
fn struct_declared_twice_or_shadowed_by_another_item_builds_no_cell() -> Result<(), String> {
    let twice = format!("{BASE}mod inner {{ pub struct ByteSize(pub u64); }}\n");
    assert_eq!(cell(&twice, "ByteSize(1)")?, None);
    let other_file = index_of(&[
        ("src/lib.rs", BASE),
        ("tests/other.rs", "struct ByteSize(u64);\n"),
    ])?;
    assert_eq!(receiver_cell_value("ByteSize(1)", &other_file), None);
    for extra in [
        "enum ByteSize { A }\n",
        "type ByteSize = u32;\n",
        "fn ByteSize() {}\n",
        "pub use other::ByteSize;\n",
        "macro_rules! make { ($n:ident) => { struct $n(u64); }; }\n",
    ] {
        let source = format!("{BASE}{extra}");
        assert_eq!(cell(&source, "ByteSize(1)")?, None, "{extra}");
    }
    Ok(())
}

#[test]
fn non_tuple_or_non_integer_structs_build_no_cell() -> Result<(), String> {
    for declaration in [
        "pub struct ByteSize { pub n: u64 }\n",
        "pub struct ByteSize<T>(pub T);\n",
        "pub struct ByteSize(pub f64);\n",
        "pub struct ByteSize(pub u64, pub u32);\n",
        "pub struct ByteSize(pub String);\n",
        "pub struct ByteSize(pub usize);\n",
        "pub struct ByteSize;\n",
        "pub struct ByteSize();\n",
        "mod nested { pub struct ByteSize(pub u64); }\n",
    ] {
        assert_eq!(cell(declaration, "ByteSize(1)")?, None, "{declaration}");
    }
    assert_eq!(
        cell("pub struct ByteSize(pub u64, u64);\n", "ByteSize(1, 2)")?,
        Some("ByteSize(1,2)".to_string())
    );
    Ok(())
}

#[test]
fn constructors_other_than_one_const_tail_expression_build_no_cell() -> Result<(), String> {
    let with_constructor = |ctor: &str| {
        format!(
            "pub const KIB: u64 = 1_024;\npub struct ByteSize(pub u64);\nimpl ByteSize {{\n{ctor}\n}}\n"
        )
    };
    for ctor in [
        // Not `const`.
        "pub fn kib(size: u64) -> Self { ByteSize(size * KIB) }",
        // More than one statement.
        "pub const fn kib(size: u64) -> Self { let n = size * KIB; ByteSize(n) }",
        // A statement before the tail.
        "pub const fn kib(size: u64) -> Self { let _ = 1; ByteSize(size * KIB) }",
        // `return`.
        "pub const fn kib(size: u64) -> Self { return ByteSize(size * KIB); }",
        // A sum is not an evaluated shape.
        "pub const fn kib(size: u64) -> Self { ByteSize(size + KIB) }",
        // A call is not an evaluated shape.
        "pub const fn kib(size: u64) -> Self { ByteSize(scale(size)) }",
        // Casts are not evaluated.
        "pub const fn kib(size: u64) -> Self { ByteSize(size * KIB as u64) }",
        // A receiver is not a constructor.
        "pub const fn kib(&self, size: u64) -> Self { ByteSize(size * KIB) }",
        // A parameter whose type is not the field's.
        "pub const fn kib(size: u32) -> Self { ByteSize(size * KIB) }",
        // A name that is neither parameter nor constant.
        "pub const fn kib(size: u64) -> Self { ByteSize(size * LIMIT) }",
        // A different constructor type.
        "pub const fn kib(size: u64) -> Other { Other(size * KIB) }",
        // Wrong arity.
        "pub const fn kib(size: u64) -> Self { ByteSize(size * KIB, 1) }",
        // `unsafe` qualifier.
        "pub const unsafe fn kib(size: u64) -> Self { ByteSize(size * KIB) }",
    ] {
        let source = with_constructor(ctor);
        assert_eq!(cell(&source, "ByteSize::kib(1)")?, None, "{ctor}");
    }
    // `Self(..)` and a literal-only product are the other admitted forms.
    let admitted = with_constructor("pub const fn kib(size: u64) -> Self { Self(size * KIB) }");
    assert_eq!(
        cell(&admitted, "ByteSize::kib(2)")?,
        Some("ByteSize(2048)".to_string())
    );
    let literal = with_constructor("pub const fn four(_size: u64) -> Self { Self(4 * 1024) }");
    assert_eq!(
        cell(&literal, "ByteSize::four(1)")?,
        Some("ByteSize(4096)".to_string())
    );
    Ok(())
}

#[test]
fn constructor_with_a_second_definition_builds_no_cell() -> Result<(), String> {
    // A second `kib` anywhere in the workspace makes the name ambiguous.
    let twice = format!("{BASE}pub fn kib(n: u64) -> u64 {{ n }}\n");
    assert_eq!(cell(&twice, "ByteSize::kib(1)")?, None);
    let second_impl = format!(
        "{BASE}pub struct Other(pub u64);\nimpl Other {{ pub const fn kib(size: u64) -> Self {{ Other(size) }} }}\n"
    );
    assert_eq!(cell(&second_impl, "ByteSize::kib(1)")?, None);
    // A trait-impl constructor of the same name is a second definition.
    let trait_impl = format!(
        "{BASE}trait Make {{ fn kib(size: u64) -> Self; }}\nimpl Make for ByteSize {{ fn kib(size: u64) -> Self {{ ByteSize(size) }} }}\n"
    );
    assert_eq!(cell(&trait_impl, "ByteSize::kib(1)")?, None);
    Ok(())
}

#[test]
fn constants_that_are_shadowed_ambiguous_imported_or_unreadable_build_no_cell() -> Result<(), String>
{
    let with = |prefix: &str, ctor_body: &str| {
        format!(
            "{prefix}pub struct ByteSize(pub u64);\nimpl ByteSize {{\n    pub const fn kib(size: u64) -> Self {{\n        {ctor_body}\n    }}\n}}\n"
        )
    };
    // Baseline: the constant is declared once.
    let ok = with("pub const KIB: u64 = 1_024;\n", "ByteSize(size * KIB)");
    assert_eq!(
        cell(&ok, "ByteSize::kib(1)")?,
        Some("ByteSize(1024)".to_string())
    );
    // Declared twice (two inline modules).
    let twice = with(
        "pub const KIB: u64 = 1_024;\nmod other {\n    pub const KIB: u64 = 1_000;\n}\n",
        "ByteSize(size * KIB)",
    );
    assert_eq!(cell(&twice, "ByteSize::kib(1)")?, None);
    // Imported rather than declared.
    let imported = with(
        "use other::KIB;\npub const KIB: u64 = 1_024;\n",
        "ByteSize(size * KIB)",
    );
    assert_eq!(cell(&imported, "ByteSize::kib(1)")?, None);
    // Not declared in the constructor's file.
    let missing = with("", "ByteSize(size * KIB)");
    assert_eq!(cell(&missing, "ByteSize::kib(1)")?, None);
    // Not a plain integer literal.
    let computed = with("pub const KIB: u64 = 1 << 10;\n", "ByteSize(size * KIB)");
    assert_eq!(cell(&computed, "ByteSize::kib(1)")?, None);
    let suffixed = with("pub const KIB: u64 = 1024u64;\n", "ByteSize(size * KIB)");
    assert_eq!(cell(&suffixed, "ByteSize::kib(1)")?, None);
    // A `static mut` is not a constant.
    let mutable = with("pub static mut KIB: u64 = 1_024;\n", "ByteSize(size * KIB)");
    assert_eq!(cell(&mutable, "ByteSize::kib(1)")?, None);
    // Rebound inside the constructor.
    let rebound = with(
        "pub const KIB: u64 = 1_024;\n",
        "{ let KIB = 7; ByteSize(size * KIB) }",
    );
    assert_eq!(cell(&rebound, "ByteSize::kib(1)")?, None);
    Ok(())
}

#[test]
fn projection_reads_exactly_one_in_range_tuple_index() {
    assert_eq!(
        project_receiver_field("ByteSize(1024)", "0"),
        Some("1024".to_string())
    );
    assert_eq!(
        project_receiver_field("Pair(1,2)", "0"),
        Some("1".to_string())
    );
    assert_eq!(
        project_receiver_field("Pair(1,2)", "1"),
        Some("2".to_string())
    );
    // Out of range, non-index and non-canonical fields decline.
    assert_eq!(project_receiver_field("Pair(1,2)", "2"), None);
    assert_eq!(project_receiver_field("ByteSize(1024)", "1"), None);
    for field in ["", "01", "-1", "a", "0.0", "0 ", "x0"] {
        assert_eq!(
            project_receiver_field("Pair(1,2)", field),
            None,
            "{field:?}"
        );
    }
    // A scalar or a malformed cell is not a receiver cell.
    for cell in [
        "1024",
        "ByteSize(",
        "ByteSize()",
        "ByteSize(1,)",
        "ByteSize(a)",
        "(1)",
        "ByteSize(1",
    ] {
        assert_eq!(project_receiver_field(cell, "0"), None, "{cell}");
    }
    assert!(is_receiver_cell("ByteSize(1)"));
    assert!(!is_receiver_cell("1024"));
    // A scalar literal holding a parenthesis is not a constructed receiver.
    for scalar in ["\"(hi)\"", "'('", "\"f(x)\""] {
        assert!(!is_receiver_cell(scalar), "{scalar}");
    }
}

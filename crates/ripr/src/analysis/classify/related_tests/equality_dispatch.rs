//! `assert_eq!` / `assert_ne!` as a call of a user `PartialEq` method (#8999).
//!
//! `assert_eq!(a, b)` expands to `*a == *b`, which runs `PartialEq::eq` of
//! the operands' type without ever spelling `eq`. A test that compares two
//! values of a type whose `impl PartialEq for T { fn eq(..) }` changed
//! therefore calls the changed function, though no call fact names it.
//!
//! The reading is deliberately narrow and fails closed:
//!
//! - the owner is `eq` (or `ne`) of a non-generic `impl PartialEq for T`
//!   with a plain, non-generic self type `T`;
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`;
//! - the assertion is a real `assert_eq!`, `assert_ne!`, `debug_assert_eq!`
//!   or `debug_assert_ne!` (no user macro of that name in the file), and
//!   **both** compared operands have a syntactically resolvable type `T`
//!   (a `let` binding annotated or built by `T { .. }`, `T(..)`,
//!   `T::new(..)`-style constructors, an enum variant or a unit struct; any
//!   other expression is unresolved). Two operands of type `T` can only
//!   compare through `PartialEq<T> for T`, which is the owner's impl;
//! - `assert_ne!` reaches `eq` only through the default `ne`, so a custom
//!   `ne` in the same impl block withholds it, and an owner `ne` is reached
//!   by `assert_ne!` alone.
//!
//! This only relates the test to the owner. The assertion is still credited
//! or refused by the ordinary oracle pipeline; nothing here raises a class.

use super::{
    FunctionSummary, RustIndex, TestSummary, ident_boundary, initializer_is_one_head_expr,
    initializer_type_head, is_ident_byte, let_binding_mentions_type, mask_comments_and_strings,
};
use crate::analysis::classify::owner_pin::{
    OwnerScope, file_renames_to, parent_chain_shadows_type, test_module_shadows_type,
};

/// Which comparison macros reach the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EqualityOwner<'a> {
    /// Self type of the `PartialEq` impl (`Key`).
    impl_type: &'a str,
    /// `assert_eq!` runs the owner.
    via_eq: bool,
    /// `assert_ne!` runs the owner.
    via_ne: bool,
}

/// The equality reading of `owner`, or `None` when it is not `eq`/`ne` of a
/// plain `impl PartialEq for T`. `index` is read for a custom `ne` in the same
/// impl block, which `assert_ne!` calls instead of `eq`.
pub(super) fn equality_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<EqualityOwner<'a>> {
    let impl_type = partial_eq_impl_type(owner)?;
    let block_prefix = owner.id.0.strip_suffix(owner.name.as_str())?;
    let custom_ne = index.functions().iter().any(|function| {
        function.name == "ne" && function.id.0.strip_suffix("ne") == Some(block_prefix)
    });
    let (via_eq, via_ne) = if owner.name == "eq" {
        (true, !custom_ne)
    } else {
        (false, true)
    };
    Some(EqualityOwner {
        impl_type,
        via_eq,
        via_ne,
    })
}

/// The self type of the plain `impl PartialEq for T` whose `eq`/`ne` is
/// `owner`; `None` for any other function.
fn partial_eq_impl_type(owner: &FunctionSummary) -> Option<&str> {
    if !matches!(owner.name.as_str(), "eq" | "ne") {
        return None;
    }
    let rest = owner.id.0.split("::impl ").nth(1)?;
    let head = rest.rsplit_once("::")?.0;
    let (trait_ty, self_ty) = head.rsplit_once(" for ")?;
    if !matches!(
        trait_ty.trim(),
        "PartialEq" | "cmp::PartialEq" | "std::cmp::PartialEq" | "core::cmp::PartialEq"
    ) {
        return None;
    }
    // A second `::impl ` segment means the impl is nested in another impl or
    // a fn body; the id no longer names one type unambiguously.
    if rest.contains("::impl ") {
        return None;
    }
    plain_type_name(self_ty.trim())
}

/// Whether `owner` is `eq`/`ne` of a plain `impl PartialEq for T`: the only
/// owners the equality reading (and its prefilter bucket) applies to.
pub(in crate::analysis) fn is_partial_eq_method(owner: &FunctionSummary) -> bool {
    partial_eq_impl_type(owner).is_some()
}

/// The identifier of a single-segment type, with at most lifetime
/// parameters (`Key`, `Key<'k>`, `Key<'_>`): no path, type or const
/// parameters and no reference, so the impl is the one `PartialEq<Self>` of
/// that type and cannot be one of several specialisations.
fn plain_type_name(ty: &str) -> Option<&str> {
    let (name, generics) = match ty.split_once('<') {
        Some((name, rest)) => (name, Some(rest.strip_suffix('>')?)),
        None => (ty, None),
    };
    let lifetimes_only = generics.is_none_or(|params| {
        !params.is_empty()
            && params.split(',').all(|param| {
                param
                    .trim()
                    .strip_prefix('\'')
                    .is_some_and(|l| !l.is_empty() && l.bytes().all(is_ident_byte))
            })
    });
    (lifetimes_only
        && !name.is_empty()
        && name.bytes().all(is_ident_byte)
        && name.starts_with(|c: char| c.is_ascii_uppercase()))
    .then_some(name)
}

/// Whether `test` compares two values of the owner's type through an
/// `assert_eq!`/`assert_ne!` that runs the owner.
pub(super) fn test_asserts_equality_through_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    equality: EqualityOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    // Same-file tests only, with the file's own source in hand so shadowing
    // and user macros can be ruled out.
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = equality.impl_type;
    let masked = mask_comments_and_strings(&test.body);
    let reaching: Vec<_> = comparison_macros(&masked)
        .into_iter()
        .filter(|call| {
            (if call.negated {
                equality.via_ne
            } else {
                equality.via_eq
            }) && call.operands.len() >= 2
                && call.operands[..2]
                    .iter()
                    .all(|operand| operand_has_type(operand, impl_type, &masked[..call.start]))
        })
        .collect();
    if reaching.is_empty() {
        return false;
    }
    let raw_impl_type = format!("r#{impl_type}");
    let owner_scope = OwnerScope::new(owner.name.as_str(), owner.start_line, &owner.file);
    if test_module_shadows_type(test, source, impl_type, owner_scope.in_file(&test.file))
        || file_renames_to(source, impl_type)
        || file_renames_to(source, raw_impl_type.as_str())
        || parent_chain_shadows_type(test, impl_type, owner_scope, index)
    {
        return false;
    }
    let masked_source = mask_comments_and_strings(source);
    if imports_other_type(&masked_source, impl_type) {
        return false;
    }
    ![
        "assert_eq",
        "assert_ne",
        "debug_assert_eq",
        "debug_assert_ne",
    ]
    .iter()
    .any(|name| defines_macro(&masked_source, name))
}

/// Whether the file may bring a different `T` into scope by `use`: a path
/// naming `T` other than `super::T`/`self::T`/`crate::T`, or a glob other
/// than `super::*`/`self::*`/`crate::*` and the standard library's. The owner
/// id carries no module path, so `use super::b::Key;` beside `a::Key`'s impl
/// cannot be told from the owner's own type; over-refusal only withholds the
/// relation.
fn imports_other_type(masked_source: &str, impl_type: &str) -> bool {
    use_paths(masked_source).into_iter().any(|path| {
        let words: Vec<&str> = path
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '*'))
            .filter(|word| !word.is_empty())
            .collect();
        let local_root = matches!(words.first(), Some(&("super" | "self" | "crate")));
        let std_root = matches!(words.first(), Some(&("std" | "core" | "alloc")));
        let names_type = words.contains(&impl_type);
        let glob = words.contains(&"*");
        if std_root {
            return false;
        }
        if names_type {
            return !(words.len() == 2 && local_root && !glob);
        }
        glob && !(words.len() == 2 && local_root)
    })
}

/// The text of every `use` item up to its `;`, from the masked source.
fn use_paths(masked: &str) -> Vec<&str> {
    let bytes = masked.as_bytes();
    let mut paths = Vec::new();
    let mut from = 0usize;
    while let Some(relative) = masked[from..].find("use ") {
        let at = from + relative;
        from = at + 4;
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            continue;
        }
        let rest = &masked[at + 4..];
        paths.push(rest.split(';').next().unwrap_or(rest).trim());
    }
    paths
}

/// Whether `name` is bound by something other than a plain `let name`
/// before `before` ends: a `for`/`if let`/`while let`/`let` pattern holding
/// it with other tokens (tuple or variant patterns), or a closure parameter.
/// Binding resolution is flow-insensitive, so any such rebinding makes the
/// last plain `let` unreliable. Match-arm patterns are not read.
fn rebound_by_pattern(before: &str, name: &str) -> bool {
    let bytes = before.as_bytes();
    let mut from = 0usize;
    while let Some(relative) = before[from..].find(name) {
        let at = from + relative;
        from = at + name.len();
        if !ident_boundary(bytes, at, at + name.len()) {
            continue;
        }
        let line_start = before[..at].rfind([';', '{', '}']).map_or(0, |i| i + 1);
        let prefix = before[line_start..at].trim_start();
        // Text between the keyword and the name is the pattern so far: an
        // `=` (let) or ` in ` (for) before the name puts it in the
        // initializer or iterator instead.
        let in_pattern = |lead: &str| {
            prefix
                .strip_prefix(lead)
                .is_some_and(|pattern| !pattern.contains('=') && !pattern.contains(" in "))
        };
        let plain_let = prefix == "let " || prefix == "let mut ";
        if !plain_let
            && (in_pattern("for ")
                || in_pattern("if let ")
                || in_pattern("while let ")
                || in_pattern("let ")
                || prefix.matches('|').count() % 2 == 1)
        {
            return true;
        }
    }
    false
}

/// Whether a `macro_rules! name` definition appears in the masked source.
fn defines_macro(masked: &str, name: &str) -> bool {
    let needle = "macro_rules!";
    let mut search = 0usize;
    while let Some(relative) = masked[search..].find(needle) {
        let after = masked[search + relative + needle.len()..].trim_start();
        if after
            .strip_prefix(name)
            .is_some_and(|rest| !rest.bytes().next().is_some_and(is_ident_byte))
        {
            return true;
        }
        search += relative + needle.len();
    }
    false
}

#[derive(Debug)]
struct ComparisonMacro<'a> {
    /// Offset of the macro name.
    start: usize,
    negated: bool,
    /// Top-level comma-separated arguments.
    operands: Vec<&'a str>,
}

/// Every `assert_eq!`/`assert_ne!` (and `debug_` twin) in the masked body.
fn comparison_macros(masked: &str) -> Vec<ComparisonMacro<'_>> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for (name, negated) in [
        ("assert_eq!", false),
        ("assert_ne!", true),
        ("debug_assert_eq!", false),
        ("debug_assert_ne!", true),
    ] {
        let mut search = 0usize;
        while let Some(relative) = masked[search..].find(name) {
            let start = search + relative;
            search = start + name.len();
            // `debug_assert_eq!` contains no `assert_eq!` boundary hit, and
            // `my_assert_eq!` is another macro.
            if !ident_boundary(bytes, start, start + name.len()) {
                continue;
            }
            let before = &masked[..start];
            if before.ends_with("::") && !before.ends_with("std::") && !before.ends_with("core::") {
                continue;
            }
            let after = masked[search..].trim_start();
            let offset = masked.len() - after.len();
            let Some(&open) = after.as_bytes().first() else {
                continue;
            };
            if !matches!(open, b'(' | b'[' | b'{') {
                continue;
            }
            if let Some(operands) = split_arguments(masked, offset) {
                found.push(ComparisonMacro {
                    start,
                    negated,
                    operands,
                });
            }
        }
    }
    found
}

/// The top-level arguments of the bracket group opening at `open`, or `None`
/// when it is unbalanced. A comma inside generic angle brackets would split
/// wrongly; the resulting operand then fails to resolve, which fails closed.
fn split_arguments(masked: &str, open: usize) -> Option<Vec<&str>> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut args = Vec::new();
    let mut arg_start = open + 1;
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let last = masked[arg_start..index].trim();
                    if !last.is_empty() {
                        args.push(last);
                    }
                    return Some(args);
                }
            }
            b',' if depth == 1 => {
                args.push(masked[arg_start..index].trim());
                arg_start = index + 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether `operand` evaluates to a value of `impl_type`, from syntax alone.
/// `before` is the masked body up to the assertion, for `let` bindings.
fn operand_has_type(operand: &str, impl_type: &str, before: &str) -> bool {
    let mut operand = operand.trim();
    while let Some(rest) = operand
        .strip_prefix('&')
        .or_else(|| operand.strip_prefix("mut "))
    {
        operand = rest.trim_start();
    }
    if operand.is_empty() {
        return false;
    }
    if operand.bytes().all(is_ident_byte) {
        // A unit struct is its own value; any other bare name is a binding.
        return operand == impl_type
            || (!rebound_by_pattern(before, operand)
                && let_binding_mentions_type(before, operand, impl_type));
    }
    initializer_is_one_head_expr(operand)
        && !operand.contains("::<")
        && initializer_type_head(operand) == Some(impl_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macros(body: &str) -> Vec<(bool, Vec<String>)> {
        comparison_macros(body)
            .into_iter()
            .map(|call| {
                (
                    call.negated,
                    call.operands.iter().map(|a| a.to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn comparison_macros_split_top_level_operands() {
        let found = macros("assert_eq!(a, f(b, c), \"msg {}\", x);");
        assert_eq!(found.len(), 1);
        assert!(!found[0].0);
        assert_eq!(&found[0].1[..2], ["a", "f(b, c)"]);
        let found = macros("assert_ne!(Key::new(1, 2), Key { id: 3, n: 4 });");
        assert_eq!(found.len(), 1);
        assert!(found[0].0);
        assert_eq!(found[0].1, ["Key::new(1, 2)", "Key { id: 3, n: 4 }"]);
    }

    #[test]
    fn comparison_macros_ignore_other_macros_and_unbalanced_input() {
        assert!(macros("my_assert_eq!(a, b);").is_empty());
        assert!(macros("assert!(a == b);").is_empty());
        assert!(macros("assert_eq!(a, b").is_empty());
        assert_eq!(macros("debug_assert_ne!(a, b);").len(), 1);
        assert_eq!(macros("std::assert_eq!(a, b);").len(), 1);
        assert!(macros("other::assert_eq!(a, b);").is_empty());
    }

    #[test]
    fn operands_resolve_only_to_syntactically_established_types() {
        let before = "let a = Key::new(1);\nlet b: Key = make();\nlet c = make();\nlet d = Key::new(1).id();\n";
        for typed in [
            "a",
            "&a",
            "b",
            "Key::new(2)",
            "Key { id: 1 }",
            "Key(1)",
            "Key",
        ] {
            assert!(operand_has_type(typed, "Key", before), "{typed}");
        }
        for untyped in [
            "c",
            "d",
            "other",
            "make()",
            "Key::new(1).id()",
            "Key::make_other()",
            "Other::new(1)",
            "String::from(\"x\")",
            "1",
            "Key::<u8>::new(1)",
            "",
        ] {
            assert!(!operand_has_type(untyped, "Key", before), "{untyped}");
        }
    }

    #[test]
    fn a_later_shadowing_let_decides_the_binding_type() {
        let before = "let a = Key::new(1);\nlet a = make();\n";
        assert!(!operand_has_type("a", "Key", before));
    }

    #[test]
    fn user_macro_definitions_are_found_by_exact_name() {
        assert!(defines_macro(
            "macro_rules! assert_eq { () => {} }",
            "assert_eq"
        ));
        assert!(defines_macro("macro_rules!   assert_eq{}", "assert_eq"));
        assert!(!defines_macro(
            "macro_rules! assert_eq_all { () => {} }",
            "assert_eq"
        ));
        assert!(!defines_macro("fn assert_eq() {}", "assert_eq"));
    }

    #[test]
    fn plain_type_names_reject_paths_generics_and_references() {
        for plain in ["Key", "Key<'k>", "Key<'_>", "Key<'a, 'b>"] {
            assert_eq!(plain_type_name(plain), Some("Key"), "{plain}");
        }
        for other in [
            "",
            "key",
            "a::Key",
            "Key<T>",
            "Key<u8>",
            "Key<'a, T>",
            "Key<>",
            "&Key",
            "Key ",
            "Key<'a",
        ] {
            assert_eq!(plain_type_name(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_format_message_with_commas_never_changes_the_operands() {
        let masked = mask_comments_and_strings("assert_eq!(a, b, \"x,y,{}\", z);");
        let found = comparison_macros(&masked);
        assert_eq!(found.len(), 1);
        assert_eq!(&found[0].operands[..2], ["a", "b"]);
    }

    #[test]
    fn imports_that_may_name_another_key_are_refused() {
        for refused in [
            "use super::b::Key;",
            "use crate::b::Key;",
            "use other::Key;",
            "use super::{Key, X};",
            "use super::b::*;",
            "use other::*;",
        ] {
            assert!(imports_other_type(refused, "Key"), "{refused}");
        }
        for allowed in [
            "use super::*;",
            "use super::Key;",
            "use crate::Key;",
            "use std::collections::HashMap;",
            "use super::other::Thing;",
        ] {
            assert!(!imports_other_type(allowed, "Key"), "{allowed}");
        }
    }

    #[test]
    fn pattern_and_closure_rebindings_make_a_let_unreliable() {
        for rebound in [
            "let a = Key::new(1); for a in xs { ",
            "let a = Key::new(1); if let Some(a) = f() { ",
            "let a = Key::new(1); let (a, c) = f(); ",
            "let a = Key::new(1); xs.map(|a| a + 1); ",
        ] {
            assert!(rebound_by_pattern(rebound, "a"), "{rebound}");
            assert!(!operand_has_type("a", "Key", rebound), "{rebound}");
        }
        for plain in [
            "let a = Key::new(1); let b = Key::new(a); ",
            "let mut a = Key::new(1); f(a); ",
            "let a: Key = make(); for x in a.items() { ",
        ] {
            assert!(!rebound_by_pattern(plain, "a"), "{plain}");
        }
    }
}

//! `Iterator::next` run by the loop or adaptor that drives the iterator (#9567).
//!
//! `for n in Countdown(3)` and `Countdown(3).collect::<Vec<_>>()` run the
//! `next` of `impl Iterator for Countdown` without spelling `next`, so a test
//! that drives a value of that type calls the changed function though no call
//! fact names it.
//!
//! The reading is as narrow as the operator one it follows
//! ([`super::operator_dispatch`]) and fails closed:
//!
//! - the owner is the `next` of a non-generic `impl Iterator for T` with a plain
//!   self type `T` (no workspace file declares a trait named `Iterator`, none
//!   is imported by a non-standard path);
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`, and the file expands no
//!   `macro_rules!` that could add or hide an impl;
//! - the driven value is a whole operand with a syntactically resolvable type
//!   `T` (a constructor, a unit struct, a `let` annotated or built by one),
//!   owned (a reference is `&mut T`, which ripr does not read) and outside
//!   property-test macros;
//! - the driver is a `for` loop over the value, or a *consuming* adaptor
//!   called directly on it (`collect`, `sum`, `count`, `fold`, `any`, ..).
//!   A lazy adaptor (`map`, `take`) runs nothing by itself, and a method of
//!   that name defined on `T` itself wins over the trait's, so neither is read.
//!
//! This only relates the test to the owner. Whether its assertion reveals a
//! wrong item stays with the ordinary oracle pipeline; nothing here raises a
//! class.

use super::equality_dispatch::{
    associated_fn_returns_self, impl_header, operand_has_type, plain_type_name,
    std_trait_is_unshadowed, type_scope_is_clear,
};
use super::implicit_dispatch::{at_test_top_level, impl_overrides};
use super::operator_dispatch::{
    bound_as_parameter_or_pattern, file_is_clear, is_reference_operand, operand_before,
    stringified_ranges,
};
use super::{FunctionSummary, RustIndex, TestSummary, is_ident_byte, mask_comments_and_strings};
use crate::analysis::classify::implicit_call::result_is_used;

/// `Iterator` methods that take the iterator by value (or `&mut`) and run
/// `next` to completion or until the answer is known. `min` and `max` are
/// left out: `Ord::max(self, other)` shares their name.
const CONSUMING_ADAPTORS: [&str; 13] = [
    "collect", "sum", "product", "count", "last", "fold", "reduce", "for_each", "any", "all",
    "find", "position", "nth",
];

/// The iterator reading of an owner.
pub(super) struct IteratorOwner<'a> {
    /// Self type of the iterator impl (`Countdown`).
    impl_type: &'a str,
    /// File-level refusals are the same for every test of the owner's file,
    /// so they are computed once per owner on first use.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// The self type of the plain `impl Iterator for T` whose `next` is `owner`.
fn iterator_impl_type(owner: &FunctionSummary) -> Option<&str> {
    let (trait_ty, self_ty) = impl_header(owner)?;
    matches!(
        trait_ty,
        "Iterator" | "iter::Iterator" | "std::iter::Iterator" | "core::iter::Iterator"
    )
    .then_some(())?;
    (owner.name == "next").then_some(())?;
    plain_type_name(self_ty)
}

/// Whether `owner` is shaped like `next` of a plain `impl Iterator for T`.
/// Id text alone cannot tell the standard trait from a local one of the same
/// name, so this is only the candidate prefilter's superset test.
pub(super) fn has_iterator_shape(owner: &FunctionSummary) -> bool {
    iterator_impl_type(owner).is_some()
}

/// The iterator reading of `owner`, or `None` when it is not `next` of a plain
/// impl of the standard `Iterator`.
pub(super) fn iterator_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<IteratorOwner<'a>> {
    let impl_type = iterator_impl_type(owner)?;
    std_trait_is_unshadowed(owner, index, "Iterator").then(|| IteratorOwner {
        impl_type,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// Whether `test` drives a value of the owner's type through a loop or a
/// consuming adaptor.
pub(super) fn test_drives_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    iterator: &IteratorOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = iterator.impl_type;
    let masked = mask_comments_and_strings(&test.body);
    // Text inside a property-test macro is opaque to every other relation.
    let opaque: Vec<std::ops::Range<usize>> =
        crate::analysis::extract::property_macros::opaque_property_macros(&test.body)
            .into_iter()
            .map(|item| item.range)
            .collect();
    let stringified = stringified_ranges(&masked);
    let returns_self = |name: &str| associated_fn_returns_self(index, impl_type, name);
    let evaluated = |at: usize| {
        !opaque.iter().any(|range| range.contains(&at))
            && !stringified.iter().any(|range| range.contains(&at))
    };
    let owned_value = |start: usize, text: &str| {
        !bound_as_parameter_or_pattern(&masked, text)
            && !is_reference_operand(text, &masked[..start])
            && operand_has_type(text, impl_type, &masked[..start], &returns_self)
    };
    let by_loop = for_loop_operands(&masked)
        .into_iter()
        .any(|(at, start, text)| {
            evaluated(at) && at_test_top_level(&masked, at) && owned_value(start, text)
        });
    let by_adaptor = || {
        adaptor_receivers(&masked)
            .into_iter()
            // `T::count` or `T::sum` of the type's own wins over the trait's.
            .filter(|(_, name, _, _)| !has_inherent_method(index, impl_type, name))
            // An override in the impl itself (`fn count(self)`) is what runs.
            .filter(|(_, name, _, _)| !impl_overrides(index, owner, name))
            .any(|(at, _, start, text)| {
                evaluated(at)
                    && at_test_top_level(&masked, at)
                    && result_is_used(&masked, at)
                    && owned_value(start, text)
            })
    };
    if !(by_loop || by_adaptor()) || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *iterator
        .file_level_clear
        .get_or_init(|| file_is_clear(source, owner, impl_type, index))
}

/// Whether a function named `name` is defined in an inherent `impl T` block.
/// Method lookup finds it before any `Iterator` method of the same name.
fn has_inherent_method(index: &RustIndex, impl_type: &str, name: &str) -> bool {
    index.functions().iter().any(|function| {
        function.name == name
            && function.id.0.matches("::impl ").count() == 1
            && function
                .id
                .0
                .split("::impl ")
                .nth(1)
                .and_then(|rest| rest.rsplit_once("::"))
                .is_some_and(|(head, _)| {
                    !head.contains(" for ")
                        && head.split('<').next().map(str::trim) == Some(impl_type)
                })
    })
}

/// `for PATTERN in OPERAND {`: the offset of `for`, and the operand's start and
/// text, when the operand is a path with at most one `(..)` group and nothing
/// after it before the loop body.
fn for_loop_operands(masked: &str) -> Vec<(usize, usize, &str)> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for (at, _) in masked.match_indices("for") {
        let end = at + "for".len();
        let word = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes
                .get(end)
                .is_some_and(|byte| byte.is_ascii_whitespace());
        if !word {
            continue;
        }
        let Some(position) = masked[end..].find(" in ") else {
            continue;
        };
        // The pattern is on the `for` line of one statement.
        if masked[end..end + position].contains(['{', ';']) {
            continue;
        }
        let start = end + position + " in ".len();
        let start = start + (masked[start..].len() - masked[start..].trim_start().len());
        let Some(operand_end) = path_with_group_end(masked, start) else {
            continue;
        };
        if masked[operand_end..].trim_start().starts_with('{') {
            found.push((at, start, &masked[start..operand_end]));
        }
    }
    found
}

/// `OPERAND.adaptor(`: the offset of the `.`, the adaptor, and the operand's
/// start and text, for the operands that stand alone.
fn adaptor_receivers(masked: &str) -> Vec<(usize, &'static str, usize, &str)> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for name in CONSUMING_ADAPTORS {
        let call = format!(".{name}");
        for (at, _) in masked.match_indices(&call) {
            let end = at + call.len();
            // `.collect(` and `.collect::<..>(`, never `.collector(`.
            let call_follows = matches!(bytes.get(end), Some(b'(' | b':'));
            if !call_follows {
                continue;
            }
            if let Some((start, text)) = operand_before(masked, at) {
                found.push((at, name, start, text));
            }
        }
    }
    found
}

/// The end of a path (`a`, `Countdown::new`) with at most one `(..)` group,
/// starting at `start`.
fn path_with_group_end(masked: &str, start: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let mut end = start;
    while end < bytes.len() && (is_ident_byte(bytes[end]) || bytes[end] == b':') {
        end += 1;
    }
    if end == start || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
        return None;
    }
    if bytes.get(end) == Some(&b'(') {
        let mut depth = 0usize;
        let close = bytes[end..].iter().position(|&byte| {
            if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                depth -= 1;
            }
            depth == 0
        })?;
        end += close + 1;
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loops(body: &str) -> Vec<String> {
        for_loop_operands(body)
            .into_iter()
            .map(|(_, _, text)| text.to_string())
            .collect()
    }

    fn adaptors(body: &str) -> Vec<(&'static str, String)> {
        adaptor_receivers(body)
            .into_iter()
            .map(|(_, name, _, text)| (name, text.to_string()))
            .collect()
    }

    #[test]
    fn a_for_loop_over_a_whole_operand_is_read() {
        assert_eq!(
            loops("for n in Countdown(3) { v.push(n); }"),
            ["Countdown(3)"]
        );
        assert_eq!(loops("for n in it { v.push(n); }"), ["it"]);
        assert_eq!(
            loops("for n in Countdown::new(3) { v.push(n); }"),
            ["Countdown::new(3)"]
        );
    }

    #[test]
    fn a_loop_over_anything_else_is_not_read() {
        for body in [
            "for n in &it { v.push(*n); }",
            "for n in it.by_ref() { v.push(n); }",
            "for n in 0..3 { v.push(n); }",
            "for n in it.take(2) { v.push(n); }",
            "impl Iterator for Countdown { }",
        ] {
            assert!(loops(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn a_consuming_adaptor_on_a_whole_operand_is_read() {
        assert_eq!(
            adaptors("let v = Countdown(3).collect::<Vec<_>>();"),
            [("collect", "Countdown(3)".to_string())]
        );
        assert_eq!(
            adaptors("let n = it.count();"),
            [("count", "it".to_string())]
        );
    }

    #[test]
    fn an_adaptor_after_a_chain_or_a_lookalike_is_not_read() {
        for body in [
            "let v = Countdown(3).map(f).collect::<Vec<_>>();",
            "let v = it.collector();",
            "let v = x.y.count();",
            "let v = Countdown(3).take(2);",
        ] {
            assert!(adaptors(body).is_empty(), "{body}");
        }
    }

    use crate::analysis::rust_index::summarize_file;
    use std::path::PathBuf;

    /// Whether the one test of `source` drives a `Countdown`, read through the
    /// real index and the production entry point.
    fn relates(source: &str) -> bool {
        let path = PathBuf::from("src/lib.rs");
        let facts = summarize_file(path.clone(), source.to_string());
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(path, facts);
        assert_eq!(index.tests().len(), 1, "fixture must hold one test");
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "next");
        assert!(owner.is_some(), "the Iterator impl must be indexed");
        let Some(owner) = owner else { return false };
        let Some(iterator) = iterator_owner(owner, &index) else {
            panic!("the fixture owner must read as a standard Iterator impl");
        };
        test_drives_owner(index.tests().at(0), owner, &iterator, Some(source), &index)
    }

    fn fixture(prelude: &str) -> String {
        format!(
            "pub struct Countdown(pub u32);\n\
             impl Iterator for Countdown {{\n    type Item = u32;\n    \
             fn next(&mut self) -> Option<u32> {{\n        \
             if self.0 == 0 {{ None }} else {{ self.0 -= 1; Some(self.0) }}\n    }}\n}}\n\
             #[test]\n\
             fn t() {{\n    let c = Countdown(3);\n    {prelude}\n    \
             let mut n = 0;\n    for x in c {{ n += x; }}\n    assert_eq!(n, 3);\n}}\n"
        )
    }

    #[test]
    fn an_early_return_before_the_loop_yields_no_relation() {
        // Positive control: the same test without the exit is related.
        assert!(relates(&fixture("")));
        // #10805: the loop may never run when the test returns first.
        assert!(!relates(&fixture("if true { return; }")));
        assert!(!relates(&fixture("let v = Some(1).ok_or(())?;")));
    }

    #[test]
    fn exit_words_in_strings_and_comments_do_not_refuse() {
        assert!(relates(&fixture(
            "let s = \"return? panic!()\"; // return ?"
        )));
    }
}

//! `Drop::drop` run by the end of a value's scope (#9567).
//!
//! `{ let _g = Guard(counter.clone()); }` and `drop(guard)` run the `drop` of
//! `impl Drop for Guard` without spelling `drop`, so a test that lets a value of
//! that type go out of scope before it asserts calls the changed function
//! though no call fact names it.
//!
//! The reading is as narrow as the operator one it follows
//! ([`super::operator_dispatch`]) and fails closed:
//!
//! - the owner is the `drop` of a non-generic `impl Drop for T` with a plain
//!   self type `T` (no workspace file declares a trait named `Drop`, none is
//!   imported by a non-standard path);
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`, and the file expands no
//!   `macro_rules!` that could add or hide an impl;
//! - the value is a whole operand with a syntactically resolvable type `T`
//!   (a constructor, a unit struct, a `let` annotated or built by one), owned,
//!   outside property-test macros, and no workspace file that names `T`
//!   leaks anything (`mem::forget`, `ManuallyDrop`, `Box::leak`), since a
//!   by-value method of `T` could do it out of the test's sight;
//! - it is dropped before an assertion can observe the effect: `drop(x)` /
//!   `mem::drop(x)` of such a value, or a statement-position `{ .. }` block
//!   that binds one in a `let` and uses the name only as a method or field
//!   receiver, so it is not moved out. A value that lives to the end of the
//!   test is dropped after every assertion and is not read.
//!
//! This only relates the test to the owner. Whether its assertion reveals a
//! wrong effect stays with the ordinary oracle pipeline; nothing here raises a
//! class.

use super::equality_dispatch::{
    associated_fn_returns_self, impl_header, operand_has_type, plain_type_name,
    std_trait_is_unshadowed, type_scope_is_clear,
};
use super::implicit_dispatch::at_test_top_level;
use super::operator_dispatch::{
    bound_as_parameter_or_pattern, file_is_clear, is_reference_operand, stringified_ranges,
};
use super::{FunctionSummary, RustIndex, TestSummary, is_ident_byte, mask_comments_and_strings};

/// Spellings that keep a value from ever being dropped.
const LEAKS: [&str; 3] = ["forget", "ManuallyDrop", "leak"];

/// The drop reading of an owner.
pub(super) struct DropOwner<'a> {
    /// Self type of the drop impl (`Guard`).
    impl_type: &'a str,
    /// File-level refusals are the same for every test of the owner's file,
    /// so they are computed once per owner on first use.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// The self type of the plain `impl Drop for T` whose `drop` is `owner`.
fn drop_impl_type(owner: &FunctionSummary) -> Option<&str> {
    let (trait_ty, self_ty) = impl_header(owner)?;
    matches!(
        trait_ty,
        "Drop" | "ops::Drop" | "std::ops::Drop" | "core::ops::Drop"
    )
    .then_some(())?;
    (owner.name == "drop").then_some(())?;
    plain_type_name(self_ty)
}

/// Whether `owner` is shaped like `drop` of a plain `impl Drop for T`. Id text
/// alone cannot tell the standard trait from a local one of the same name, so
/// this is only the candidate prefilter's superset test.
pub(super) fn has_drop_shape(owner: &FunctionSummary) -> bool {
    drop_impl_type(owner).is_some()
}

/// The drop reading of `owner`, or `None` when it is not `drop` of a plain
/// impl of the standard `Drop`.
pub(super) fn drop_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<DropOwner<'a>> {
    let impl_type = drop_impl_type(owner)?;
    std_trait_is_unshadowed(owner, index, "Drop").then(|| DropOwner {
        impl_type,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// Whether `test` drops a value of the owner's type before an assertion.
pub(super) fn test_drops_owner_value(
    test: &TestSummary,
    owner: &FunctionSummary,
    drop: &DropOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = drop.impl_type;
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
    let owned_value = |before: usize, text: &str| {
        !bound_as_parameter_or_pattern(&masked, text)
            && !is_reference_operand(text, &masked[..before])
            && operand_has_type(text, impl_type, &masked[..before], &returns_self)
    };
    // A by-value method of the type may move `self` somewhere the test cannot
    // see (a thread-local, a wrapper), so the value is not dropped at block end.
    let by_value_method = |name: &str| takes_self_by_value(index, impl_type, name);
    let by_call = explicit_drops(&masked)
        .into_iter()
        .any(|(at, start, close, text)| {
            evaluated(at)
                && at_test_top_level(&masked, at)
                && asserts_after(&masked, close)
                && owned_value(start, text)
        });
    let by_scope = || {
        scope_blocks(&masked).into_iter().any(|(open, close)| {
            evaluated(open)
                && at_test_top_level(&masked, open)
                && asserts_after(&masked, close)
                && scope_binds_owned_value(&masked, open, close, &owned_value, &by_value_method)
        })
    };
    if !(by_call || by_scope()) || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *drop.file_level_clear.get_or_init(|| {
        file_is_clear(source, owner, impl_type, index) && !type_may_be_leaked(index, impl_type)
    })
}

/// Whether any workspace file that names the type also spells a leak. A by-value
/// method of the type (`g.disarm()`) can `mem::forget(self)` out of the test's
/// sight, so a receiver use does not prove the value is dropped.
fn type_may_be_leaked(index: &RustIndex, impl_type: &str) -> bool {
    index.files().values().any(|facts| {
        let source = facts.data().source.as_ref();
        source.contains(impl_type) && {
            let masked = mask_comments_and_strings(source);
            contains_word(&masked, impl_type)
                && LEAKS.iter().any(|leak| contains_word(&masked, leak))
        }
    })
}

/// Whether an assertion macro appears after byte `from`.
fn asserts_after(masked: &str, from: usize) -> bool {
    masked[from..].contains("assert")
}

/// Whether `word` occurs as a whole identifier, or as the last segment of a
/// path (`mem::forget`).
fn contains_word(masked: &str, word: &str) -> bool {
    let bytes = masked.as_bytes();
    masked.match_indices(word).any(|(at, _)| {
        (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes
                .get(at + word.len())
                .is_none_or(|&byte| !is_ident_byte(byte))
    })
}

/// `drop(x)`, `mem::drop(x)` and `std::mem::drop(x)`: the offset of `drop`, the
/// argument's start, the offset after the call, and the argument text when it
/// is a single path with at most one `(..)` group.
fn explicit_drops(masked: &str) -> Vec<(usize, usize, usize, &str)> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for (at, _) in masked.match_indices("drop(") {
        // `x.drop(` is no call of `Drop::drop`, and `fn drop(` declares one.
        let before = masked[..at].trim_end();
        if at > 0
            && (is_ident_byte(bytes[at - 1]) || before.ends_with('.') || before.ends_with("fn"))
        {
            continue;
        }
        let open = at + "drop".len();
        let mut depth = 0usize;
        let Some(close) = bytes[open..].iter().position(|&byte| {
            if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                depth -= 1;
            }
            depth == 0
        }) else {
            continue;
        };
        let argument = &masked[open + 1..open + close];
        let trimmed = argument.trim();
        let start = open + 1 + (argument.len() - argument.trim_start().len());
        if is_single_operand(trimmed) {
            found.push((at, start, open + close + 1, trimmed));
        }
    }
    found
}

/// A path with at most one trailing `(..)` group and nothing else.
fn is_single_operand(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() && (is_ident_byte(bytes[end]) || bytes[end] == b':') {
        end += 1;
    }
    if end == 0 || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    match bytes.get(end) {
        None => true,
        Some(b'(') => {
            let mut depth = 0usize;
            let close = bytes[end..].iter().position(|&byte| {
                if byte == b'(' {
                    depth += 1;
                } else if byte == b')' {
                    depth -= 1;
                }
                depth == 0
            });
            close.is_some_and(|close| end + close + 1 == bytes.len())
        }
        Some(_) => false,
    }
}

/// Statement-position `{ .. }` blocks: after `;`, `{`, `}` or at the start of
/// the body, so a loop, `if`, closure, `unsafe` or `async` body is not one.
/// Returns the offsets of the `{` and its matching `}`.
fn scope_blocks(masked: &str) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut blocks = Vec::new();
    for (open, _) in masked.match_indices('{') {
        let standalone = matches!(
            masked[..open].trim_end().bytes().last(),
            None | Some(b';' | b'{' | b'}')
        );
        if !standalone {
            continue;
        }
        let mut depth = 0usize;
        let close = bytes[open..].iter().position(|&byte| {
            if byte == b'{' {
                depth += 1;
            } else if byte == b'}' {
                depth -= 1;
            }
            depth == 0
        });
        if let Some(close) = close {
            blocks.push((open, open + close));
        }
    }
    blocks
}

/// The identifier that starts `text` (a field or method name after `.`).
fn method_after(text: &str) -> &str {
    let len = text.bytes().take_while(|&byte| is_ident_byte(byte)).count();
    &text[..len]
}

/// Whether some method called `name` of an `impl` for `impl_type` takes `self`
/// by value, or its signature cannot be read. A field access has no such method.
fn takes_self_by_value(index: &RustIndex, impl_type: &str, name: &str) -> bool {
    index.functions().iter().any(|function| {
        function.name == name
            && function.item.has_self_param
            && super::impl_self_type_name(&function.id.0).as_deref() == Some(impl_type)
            && !index
                .files()
                .get(&function.file)
                .and_then(|file| {
                    file.data()
                        .source
                        .as_ref()
                        .lines()
                        .nth(function.start_line.saturating_sub(1))
                        .map(|line| line.contains("&self") || line.contains("&mut self"))
                })
                .unwrap_or(false)
    })
}

/// Whether the block `open..close` has a `let [mut] name` of the owner's type
/// whose name appears nowhere else except as the receiver of a method call or
/// field access, so the value is not moved out of the block before it ends.
fn scope_binds_owned_value(
    masked: &str,
    open: usize,
    close: usize,
    owned_value: &dyn Fn(usize, &str) -> bool,
    by_value_method: &dyn Fn(&str) -> bool,
) -> bool {
    let body = &masked[open + 1..close];
    let bytes = body.as_bytes();
    body.match_indices("let ").any(|(at, _)| {
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            return false;
        }
        let rest = body[at + "let ".len()..].trim_start();
        let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
        let len = rest.bytes().take_while(|&byte| is_ident_byte(byte)).count();
        let name = &rest[..len];
        if name.is_empty() || name == "_" {
            return false;
        }
        let only_receiver = body.match_indices(name).all(|(use_at, _)| {
            let end = use_at + name.len();
            let whole = (use_at == 0 || !is_ident_byte(bytes[use_at - 1]))
                && bytes.get(end).is_none_or(|&byte| !is_ident_byte(byte));
            let is_binding = body[..use_at].trim_end().ends_with("let")
                || body[..use_at].trim_end().ends_with("let mut");
            let receiver = body[end..].starts_with('.')
                && !body[end..].starts_with("..")
                && !by_value_method(method_after(&body[end + 1..]));
            !whole || is_binding || receiver
        });
        only_receiver && owned_value(close, name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_drop_of_a_single_operand_is_read() {
        let calls = |body: &str| -> Vec<String> {
            explicit_drops(body)
                .into_iter()
                .map(|(_, _, _, text)| text.to_string())
                .collect()
        };
        assert_eq!(calls("drop(g);"), ["g"]);
        assert_eq!(
            calls("std::mem::drop(Guard(c.clone()));"),
            ["Guard(c.clone())"]
        );
        for body in [
            "g.drop();",
            "fn drop(g: G) {}",
            "drop(a, b);",
            "drop(&g);",
            "drop(g.0);",
        ] {
            assert!(calls(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn only_statement_position_blocks_are_scopes() {
        let blocks = |body: &str| scope_blocks(body).len();
        assert_eq!(blocks("let c = 1; { let g = G; } assert!(true);"), 1);
        assert_eq!(blocks("if x { let g = G; }"), 0);
        assert_eq!(blocks("for _ in 0..1 { let g = G; }"), 0);
        assert_eq!(blocks("let f = || { let g = G; };"), 0);
        assert_eq!(blocks("unsafe { let g = G; }"), 0);
    }
}

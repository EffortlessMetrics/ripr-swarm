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
    // A method call on the value may move `self` somewhere the test cannot see
    // (a thread-local, a wrapper, std's `into`), so only a method of the type
    // known to borrow leaves the value to be dropped at block end.
    let may_move_receiver = |name: &str| !takes_self_by_ref(index, impl_type, name);
    let by_call = !standard_drop_may_be_shadowed(source, &test.file, index)
        && explicit_drops(&masked)
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
                && scope_binds_owned_value(&masked, open, close, &owned_value, &may_move_receiver)
        })
    };
    if !(by_call || by_scope()) || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *drop.file_level_clear.get_or_init(|| {
        file_is_clear(source, owner, impl_type, index) && !type_may_be_leaked(index, impl_type)
    })
}

/// Whether the test's file may bind `drop` or `mem` to something other than
/// the standard function: a `fn drop` that is not a `Drop` impl's method, a
/// `mod mem`, or a `use` that renames or imports another `drop`.
pub(super) fn standard_drop_may_be_shadowed(
    source: &str,
    file: &std::path::Path,
    index: &RustIndex,
) -> bool {
    let masked = mask_comments_and_strings(source);
    let declared = masked.matches("fn drop").count();
    let impl_methods = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == "drop" && function.file == file && function.id.0.contains("::impl ")
        })
        .count();
    declared > impl_methods
        || contains_word(&masked, "mod mem")
        || masked.contains("as drop")
        || binds_name(&masked, "drop")
        || binds_name(&masked, "mem")
        || masked
            .lines()
            .filter(|line| line.trim_start().starts_with("use "))
            .any(|line| {
                (line.contains("drop") && !line.contains("mem::drop"))
                    // `use crate::util::mem;` makes `mem::drop` another module's.
                    || (contains_word(line, "mem")
                        && !line.contains("std::mem")
                        && !line.contains("core::mem"))
            })
}

/// Whether `name` is bound as a local, a closure parameter or a typed
/// parameter anywhere in `masked` (`let drop = ..`, `let mut drop`, `|drop|`,
/// `drop: F`), so a later `drop(..)` may call that value, not the function.
fn binds_name(masked: &str, name: &str) -> bool {
    let bytes = masked.as_bytes();
    masked.match_indices(name).any(|(at, _)| {
        let end = at + name.len();
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(end).is_none_or(|&byte| !is_ident_byte(byte));
        if !whole {
            return false;
        }
        let before = masked[..at].trim_end();
        let after = masked[end..].trim_start();
        let introduced = ["let", "let mut", "mut", "ref"]
            .iter()
            .any(|keyword| before.ends_with(keyword))
            || before.ends_with('|');
        let typed_parameter = after.starts_with(':') && !after.starts_with("::");
        introduced || typed_parameter
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
    let bytes = masked.as_bytes();
    masked[from..].match_indices('!').any(|(at, _)| {
        let end = from + at;
        let start = masked[..end]
            .bytes()
            .rposition(|byte| !is_ident_byte(byte))
            .map_or(0, |separator| separator + 1);
        let name = &masked[start..end];
        let name = name.strip_prefix("debug_").unwrap_or(name);
        // `assert!`, `assert_eq!`, `assert_matches!`, not `x != y`.
        name.starts_with("assert") && bytes.get(end + 1).is_some_and(|byte| *byte != b'=')
    })
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
        // `helpers::drop(x)` is another function; only the standard `drop`
        // spellings are the one that runs `Drop::drop`.
        if before.ends_with("::") {
            let path = before.trim_end_matches("::");
            let segment_start = path
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                .map_or(0, |separator| separator + 1);
            if !matches!(&path[segment_start..], "mem" | "std::mem" | "core::mem") {
                continue;
            }
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

/// Whether what follows `receiver.` leaves the value in place: a field access
/// (`g.0`), or a method call to a method `borrows` proves takes `&self`.
fn receiver_stays_borrowed(text: &str, may_move: &dyn Fn(&str) -> bool) -> bool {
    let len = text.bytes().take_while(|&byte| is_ident_byte(byte)).count();
    let call = text[len..].starts_with('(') || text[len..].starts_with("::");
    !call || !may_move(&text[..len])
}

/// Whether every method called `name` of an `impl` for `impl_type` borrows
/// `self` (`&self` or `&mut self` on its signature line), and at least one
/// exists. A method the workspace does not define, a trait method such as
/// `into`, or a signature that cannot be read has no known borrow.
fn takes_self_by_ref(index: &RustIndex, impl_type: &str, name: &str) -> bool {
    let methods: Vec<_> = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == name
                && function.item.has_self_param
                && super::impl_self_type_name(&function.id.0).as_deref() == Some(impl_type)
        })
        .collect();
    !methods.is_empty()
        && methods.iter().all(|function| {
            index
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
    may_move_receiver: &dyn Fn(&str) -> bool,
) -> bool {
    let body = &masked[open + 1..close];
    // An attribute may remove a binding from the compiled test.
    if body.contains("#[") {
        return false;
    }
    let bytes = body.as_bytes();
    body.match_indices("let ").any(|(at, _)| {
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            return false;
        }
        // A binding inside an `if`, a loop, a closure or a nested `fn` of the
        // block may never run, so only the block's own statements count.
        let depth = body[..at].bytes().fold(0isize, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth - 1,
            _ => depth,
        });
        if depth != 0
            || body[..at]
                .rsplit([';', '{', '}'])
                .next()
                .is_some_and(|head| head.contains('|'))
        {
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
                && receiver_stays_borrowed(&body[end + 1..], may_move_receiver);
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
            "helpers::drop(g);",
            "crate::helpers::drop(g);",
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

    use crate::analysis::rust_index::summarize_file;
    use std::path::PathBuf;

    /// Whether the one test of `source` drops a `Guard` before asserting, read
    /// through the real index and the production entry point.
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
            .find(|function| function.name == "drop");
        assert!(owner.is_some(), "the Drop impl must be indexed");
        let Some(owner) = owner else { return false };
        let drop = drop_owner(owner, &index);
        assert!(
            drop.is_some(),
            "the fixture owner must read as a standard Drop impl"
        );
        let Some(drop) = drop else { return false };
        test_drops_owner_value(index.tests().at(0), owner, &drop, Some(source), &index)
    }

    fn fixture(prelude: &str) -> String {
        format!(
            "pub struct Guard(pub std::rc::Rc<std::cell::Cell<u32>>);\n\
             impl Drop for Guard {{ fn drop(&mut self) {{ self.0.set(self.0.get() + 1); }} }}\n\
             #[test]\n\
             fn t() {{\n    let c = std::rc::Rc::new(std::cell::Cell::new(0));\n    \
             let g = Guard(c.clone());\n    {prelude}\n    drop(g);\n    \
             assert_eq!(c.get(), 1);\n}}\n"
        )
    }

    #[test]
    fn an_early_return_before_the_drop_yields_no_relation() {
        // Positive control: the same test without the exit is related.
        assert!(relates(&fixture("")));
        // #10805: the drop may never run when the test returns first.
        assert!(!relates(&fixture("if c.get() == 0 { return; }")));
        assert!(!relates(&fixture("let v = Some(1).ok_or(())?;")));
    }

    #[test]
    fn exit_words_in_strings_and_comments_do_not_refuse() {
        // The masker blanks string and comment contents before the guard reads.
        assert!(relates(&fixture(
            "let s = \"return? panic!()\"; // return ?"
        )));
        assert!(relates(&fixture(
            "/* if c { return; } */ let _n = 1; // x?"
        )));
    }
}

//! Operator syntax as a call of a user operator-trait method (#9567).
//!
//! `a + b` runs `Add::add` of `a`'s type without ever spelling `add`, so a
//! test that adds two values of a type whose `impl Add for T { fn add }`
//! changed calls the changed function though no call fact names it. Binary
//! and assigning operators dispatch on the left operand, unary ones on the
//! operand after them.
//!
//! The reading is as narrow as the `assert_eq!` one it follows
//! ([`super::equality_dispatch`]) and fails closed:
//!
//! - the owner is the method of a non-generic `impl <OpTrait> for T` with a
//!   plain self type `T`, `OpTrait` is the standard `core::ops` trait
//!   (no workspace file declares a trait of that name, none is imported by a
//!   non-standard path), and it is the only impl of that method for `T`, so
//!   the operator cannot pick a different right-hand-side impl;
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `T`;
//! - the operator's `Self` operand has a syntactically resolvable type `T`
//!   (a constructor, a unit struct, a `let` annotated or built by one), is a
//!   whole operand (not the tail of a longer expression, not behind `&`, not
//!   the receiver of a method chain), and sits outside property-test macros.
//!
//! This only relates the test to the owner. Whether its assertion reveals a
//! wrong result stays with the ordinary oracle pipeline; nothing here raises
//! a class.

use super::equality_dispatch::{
    associated_fn_returns_self, impl_header, imports_other_type, operand_has_type, plain_type_name,
    std_trait_is_unshadowed, type_scope_is_clear,
};
use super::{
    FunctionSummary, RustIndex, TestSummary, is_ident_byte, last_let_statement,
    mask_comments_and_strings, matching_open,
};
use crate::analysis::classify::implicit_call::{operator_positions, result_is_used};

/// One standard operator trait: its method and the token that spells it.
struct OperatorTrait {
    name: &'static str,
    method: &'static str,
    token: &'static str,
    unary: bool,
}

const fn binary(name: &'static str, method: &'static str, token: &'static str) -> OperatorTrait {
    OperatorTrait {
        name,
        method,
        token,
        unary: false,
    }
}

const fn unary(name: &'static str, method: &'static str, token: &'static str) -> OperatorTrait {
    OperatorTrait {
        name,
        method,
        token,
        unary: true,
    }
}

/// `Index` is left to the implicit-call abstention until its `IndexMut` twin
/// and the `[`-after-operand grammar are read.
const OPERATORS: [OperatorTrait; 22] = [
    binary("Add", "add", "+"),
    binary("Sub", "sub", "-"),
    binary("Mul", "mul", "*"),
    binary("Div", "div", "/"),
    binary("Rem", "rem", "%"),
    binary("BitAnd", "bitand", "&"),
    binary("BitOr", "bitor", "|"),
    binary("BitXor", "bitxor", "^"),
    binary("Shl", "shl", "<<"),
    binary("Shr", "shr", ">>"),
    binary("AddAssign", "add_assign", "+="),
    binary("SubAssign", "sub_assign", "-="),
    binary("MulAssign", "mul_assign", "*="),
    binary("DivAssign", "div_assign", "/="),
    binary("RemAssign", "rem_assign", "%="),
    binary("BitAndAssign", "bitand_assign", "&="),
    binary("BitOrAssign", "bitor_assign", "|="),
    binary("BitXorAssign", "bitxor_assign", "^="),
    binary("ShlAssign", "shl_assign", "<<="),
    binary("ShrAssign", "shr_assign", ">>="),
    unary("Neg", "neg", "-"),
    unary("Not", "not", "!"),
];

/// The operator reading of an owner.
pub(super) struct OperatorOwner<'a> {
    /// Self type of the operator impl (`Money`).
    impl_type: &'a str,
    token: &'static str,
    unary: bool,
    /// File-level refusals are the same for every test of the owner's file,
    /// so they are computed once per owner on first use.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// The operator trait `trait_ty` names (`Add`, `Add<Money>`, `std::ops::Add`),
/// as a table row, or `None` for any other trait text.
fn operator_trait(trait_ty: &str) -> Option<&'static OperatorTrait> {
    let base = trait_ty.split('<').next()?.trim();
    let name = base
        .strip_prefix("std::ops::")
        .or_else(|| base.strip_prefix("core::ops::"))
        .or_else(|| base.strip_prefix("ops::"))
        .unwrap_or(base);
    OPERATORS.iter().find(|row| row.name == name)
}

/// Whether `owner` is shaped like the method of a plain operator-trait impl.
/// Id text alone cannot tell the standard trait from a local one of the same
/// name, so this is only the candidate prefilter's superset test.
pub(super) fn has_operator_shape(owner: &FunctionSummary) -> bool {
    shape(owner).is_some()
}

fn shape(owner: &FunctionSummary) -> Option<(&'static OperatorTrait, &str)> {
    let (trait_ty, self_ty) = impl_header(owner)?;
    let row = operator_trait(trait_ty)?;
    (row.method == owner.name).then_some(())?;
    Some((row, plain_type_name(self_ty)?))
}

/// The operator reading of `owner`, or `None` when it is not the method of a
/// plain impl of a standard operator trait that is the type's only impl of it.
pub(super) fn operator_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<OperatorOwner<'a>> {
    let (row, impl_type) = shape(owner)?;
    if !std_trait_is_unshadowed(owner, index, row.name) {
        return None;
    }
    // `impl Add<i32> for T` beside `impl Add<T> for T`: the operator picks by
    // right-hand side, which ripr does not type, so neither is attributed.
    let impls = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == row.method
                && impl_header(function).is_some_and(|(trait_ty, self_ty)| {
                    operator_trait(trait_ty).is_some_and(|other| other.name == row.name)
                        && plain_type_name(self_ty) == Some(impl_type)
                })
        })
        .count();
    (impls == 1).then(|| OperatorOwner {
        impl_type,
        token: row.token,
        unary: row.unary,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// Whether `test` applies the owner's operator to a value of the owner's type.
pub(super) fn test_applies_operator_to_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    operator: &OperatorOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let impl_type = operator.impl_type;
    let masked = mask_comments_and_strings(&test.body);
    // Text inside a property-test macro is opaque to every other relation.
    let opaque: Vec<std::ops::Range<usize>> =
        crate::analysis::extract::property_macros::opaque_property_macros(&test.body)
            .into_iter()
            .map(|item| item.range)
            .collect();
    let stringified = stringified_ranges(&masked);
    let returns_self = |name: &str| associated_fn_returns_self(index, impl_type, name);
    let reaching = operator_positions(&masked, operator.token, !operator.unary)
        .into_iter()
        .filter(|site| !opaque.iter().any(|range| range.contains(&site.at)))
        .filter(|site| !stringified.iter().any(|range| range.contains(&site.at)))
        // A result thrown away cannot reach an assertion.
        .filter(|site| result_is_used(&masked, site.at))
        // `>>` and `<<` also close and open generic arguments; no other
        // operator is read that way.
        .filter(|site| {
            !matches!(operator.token, "<<" | ">>") || !in_generic_arguments(&masked, site.at)
        })
        .any(|site| {
            let operand = if operator.unary {
                operand_after(&masked, site.at + site.len)
            } else {
                operand_before(&masked, site.at)
            };
            operand.is_some_and(|(start, text)| {
                !bound_as_parameter_or_pattern(&masked, text)
                    && !is_reference_operand(text, &masked[..start])
                    && operand_has_type(text, impl_type, &masked[..start], &returns_self)
            })
        });
    if !reaching || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *operator.file_level_clear.get_or_init(|| {
        let masked = mask_comments_and_strings(source);
        // A `macro_rules!` may expand to a second impl of the operator trait
        // that the index never sees, so the unique-impl count proves nothing.
        !masked.contains("macro_rules!")
            && !imports_other_type(&masked, impl_type, &owner.file)
            && !workspace_macro_may_generate_impls(index)
    })
}

/// Whether any workspace file defines a `macro_rules!` that expands to an
/// `impl`. Invoked in the owner's file (through `#[macro_use]` or a path
/// import), it can generate a second operator impl the index never sees.
fn workspace_macro_may_generate_impls(index: &RustIndex) -> bool {
    index.files().values().any(|facts| {
        let source = facts.data().source.as_ref();
        source.contains("macro_rules!") && {
            let masked = mask_comments_and_strings(source);
            masked.contains("macro_rules!") && masked.contains("impl")
        }
    })
}

/// Whether the operand is a reference: `&x`, or a binding whose last `let` is
/// annotated `&T` or initialised by `&..`. `operand_has_type` looks through
/// `&` because equality and formatting compare through references, but an
/// operator picks the impl by the operand's exact type: `&a + &b` runs
/// `impl Add<&T> for &T`, never the owned impl this relation credits.
fn is_reference_operand(operand: &str, before: &str) -> bool {
    let operand = operand.trim();
    if operand.starts_with('&') {
        return true;
    }
    if !operand.bytes().all(is_ident_byte) {
        return false;
    }
    let Some(statement) = last_let_statement(before, operand) else {
        return false;
    };
    let Some(eq) = statement.find('=') else {
        return false;
    };
    let pattern = &statement[..eq];
    match pattern.find(':') {
        Some(colon) => pattern[colon + 1..].trim_start().starts_with('&'),
        None => statement[eq + 1..].trim_start().starts_with('&'),
    }
}

/// Whether the site sits inside an unclosed generic `<` of its statement, as
/// in `let v: Vec<Result<u8, Money>> = ..`, where `>>` is no operator. Only a
/// `<` that follows a name or `::` with no space opens generics, and only a
/// `>` that closes one is counted, so a comparison such as
/// `1 < 2 && Money(8) >> Money(1)` or `a > b && f::<Vec<Money>>()` can neither
/// hide a real shift nor cancel a generic opener. A `;` inside `[..]`
/// (`Vec<[Money; 2]>`) does not end the statement.
fn in_generic_arguments(masked: &str, at: usize) -> bool {
    let bytes = &masked.as_bytes()[..at];
    let mut generics = 0usize;
    let mut brackets = 0usize;
    for (i, &byte) in bytes.iter().enumerate() {
        match byte {
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b';' | b'{' | b'}' if brackets == 0 => generics = 0,
            b'<' if i > 0 && (is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b':') => {
                generics += 1;
            }
            b'>' if i > 0 && !matches!(bytes[i - 1], b'-' | b'=') => {
                generics = generics.saturating_sub(1);
            }
            _ => {}
        }
    }
    generics > 0
}

/// Byte ranges of `stringify!(..)` arguments: the tokens become text and are
/// never evaluated, so an operator inside runs no impl.
fn stringified_ranges(masked: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = masked.as_bytes();
    let mut ranges = Vec::new();
    for (start, _) in masked.match_indices("stringify") {
        if start > 0 && is_ident_byte(bytes[start - 1]) {
            continue;
        }
        let after = masked[start + "stringify".len()..].trim_start();
        let Some(rest) = after.strip_prefix('!') else {
            continue;
        };
        let rest = rest.trim_start();
        let open = masked.len() - rest.len();
        let Some(&delimiter) = bytes.get(open) else {
            continue;
        };
        let close_byte = match delimiter {
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            _ => continue,
        };
        let mut depth = 0usize;
        let mut end = masked.len();
        for (index, &byte) in bytes.iter().enumerate().skip(open) {
            if byte == delimiter {
                depth += 1;
            } else if byte == close_byte {
                depth -= 1;
                if depth == 0 {
                    end = index;
                    break;
                }
            }
        }
        ranges.push(open..end);
    }
    ranges
}

/// Whether the bare identifier `operand` is also bound as a parameter, closure
/// parameter, `for` or enum-payload pattern somewhere in the body, so the
/// single `let` that types it may not be the value at the site.
fn bound_as_parameter_or_pattern(masked: &str, operand: &str) -> bool {
    if operand.is_empty() || !operand.bytes().all(is_ident_byte) {
        return false;
    }
    masked.match_indices(operand).any(|(at, _)| {
        let end = at + operand.len();
        let bytes = masked.as_bytes();
        let whole = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes.get(end).is_none_or(|&byte| !is_ident_byte(byte));
        if !whole {
            return false;
        }
        let after = masked[end..].trim_start();
        let before = masked[..at].trim_end();
        // The operand's own `let` is the binding that types it.
        if before.ends_with("let") || before.ends_with("let mut") {
            return false;
        }
        let typed = after.starts_with(':') && !after.starts_with("::");
        let closure = in_closure_parameters(masked, at, end);
        let pattern = before.ends_with("for")
            || before.ends_with("Some(")
            || before.ends_with("Ok(")
            || before.ends_with("Err(")
            || before.ends_with("ref")
            || before.ends_with("ref mut");
        typed || closure || pattern
    })
}

/// Whether the name at `at..end` sits between two `|` with only pattern text
/// between: `|a|`, `|x, a|`, `|&a|`, `|(x, a)|`, `|[x, a]|`. `a | b` is the
/// operator itself, so a lone `|` on one side does not count, and anything
/// else (`=`, `;`, `{`) between the name and a `|` means it is not a parameter.
fn in_closure_parameters(masked: &str, at: usize, end: usize) -> bool {
    let pattern_byte = |byte: u8| {
        is_ident_byte(byte)
            || byte.is_ascii_whitespace()
            || matches!(byte, b',' | b'&' | b'(' | b')' | b'[' | b']' | b':')
    };
    let bytes = masked.as_bytes();
    let opens = bytes[..at]
        .iter()
        .rev()
        .find(|&&byte| !pattern_byte(byte))
        .is_some_and(|&byte| byte == b'|');
    let closes = bytes[end..]
        .iter()
        .find(|&&byte| !pattern_byte(byte))
        .is_some_and(|&byte| byte == b'|');
    opens && closes
}

/// A path (`a`, `Money::zero`) that starts like an identifier.
fn is_path_byte(byte: u8) -> bool {
    is_ident_byte(byte) || byte == b':'
}

fn starts_like_ident(text: &str) -> bool {
    text.bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
}

/// The whole operand ending just before the operator at `op_at`: a path with
/// at most one trailing `(..)`/`{..}` group, standing alone (start of the
/// body, or after `(`, `,`, `{`, `;` or a plain `=`). `a * b + c` gives no
/// operand for `+`, `&m + n` and `x.y + n` give none either. Returns the
/// operand's start offset and text.
fn operand_before(masked: &str, op_at: usize) -> Option<(usize, &str)> {
    let head = masked[..op_at].trim_end();
    let bytes = head.as_bytes();
    let end = head.len();
    let mut path_end = end;
    if let Some(&close @ (b')' | b'}')) = bytes.last() {
        let open_byte = if close == b')' { b'(' } else { b'{' };
        path_end = head[..matching_open(bytes, end - 1, open_byte, close)?]
            .trim_end()
            .len();
    }
    let mut start = path_end;
    while start > 0 && is_path_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == path_end || !starts_like_ident(&head[start..]) {
        return None;
    }
    stands_alone(&head[..start]).then_some((start, &head[start..end]))
}

/// The whole operand starting just after a unary operator: a path with at
/// most one group and no method call, field, index, `?` or call after it.
fn operand_after(masked: &str, after: usize) -> Option<(usize, &str)> {
    let rest = &masked[after..];
    let start = after + (rest.len() - rest.trim_start().len());
    let bytes = masked.as_bytes();
    let mut end = start;
    while end < bytes.len() && is_path_byte(bytes[end]) {
        end += 1;
    }
    if end == start || !starts_like_ident(&masked[start..]) {
        return None;
    }
    if let Some(&open @ (b'(' | b'{')) = bytes.get(end) {
        let close_byte = if open == b'(' { b')' } else { b'}' };
        let mut depth = 0usize;
        let close = bytes[end..].iter().position(|&byte| {
            if byte == open {
                depth += 1;
            } else if byte == close_byte {
                depth -= 1;
            }
            depth == 0
        })?;
        end += close + 1;
    }
    let next = masked[end..].trim_start().bytes().next();
    (!matches!(next, Some(b'.' | b'?' | b'[' | b'(' | b'{')))
        .then_some((start, &masked[start..end]))
}

/// Whether the text before an operand ends where an expression can begin
/// without an operator of tighter precedence taking it.
fn stands_alone(before: &str) -> bool {
    let before = before.trim_end();
    match before.bytes().last() {
        None => true,
        Some(b'(' | b',' | b'{' | b';') => true,
        Some(b'=') => !before[..before.len() - 1].ends_with(|ch: char| {
            matches!(
                ch,
                '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^'
            )
        }),
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn before(body: &str, op: &str) -> Option<String> {
        let at = body.find(op)?;
        operand_before(body, at).map(|(_, text)| text.to_string())
    }

    fn after(body: &str, op: &str) -> Option<String> {
        let at = body.find(op)?;
        operand_after(body, at + op.len()).map(|(_, text)| text.to_string())
    }

    #[test]
    fn the_left_operand_is_a_whole_path_with_one_group() {
        assert_eq!(
            before("let s = Money(2) + x;", "+").as_deref(),
            Some("Money(2)")
        );
        assert_eq!(before("let s = a + b;", "+").as_deref(), Some("a"));
        assert_eq!(
            before("f(Money { v: 1 } + x)", "+").as_deref(),
            Some("Money { v: 1 }")
        );
        assert_eq!(before("a + b", "+").as_deref(), Some("a"));
    }

    #[test]
    fn a_left_operand_that_is_only_a_tail_is_refused() {
        for body in [
            "let s = a * b + c;",
            "let s = &m + n;",
            "let s = x.y + n;",
            "let s = x.m() + n;",
            "let s = (a) + n;",
            "let s = v[0] + n;",
            "let ok = a == b + n;",
        ] {
            assert_eq!(before(body, "+ "), None, "{body}");
        }
    }

    #[test]
    fn the_unary_operand_is_a_whole_path() {
        assert_eq!(after("let n = -m;", "-").as_deref(), Some("m"));
        assert_eq!(
            after("let n = -Money(2);", "-").as_deref(),
            Some("Money(2)")
        );
        assert_eq!(after("let n = -m.abs();", "-"), None);
        assert_eq!(after("let n = !m?;", "!"), None);
        assert_eq!(after("let n = -m[0];", "-"), None);
    }
}

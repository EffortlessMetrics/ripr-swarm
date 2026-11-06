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
//! - `a[i]` is read as `Index::index` for a read and `IndexMut::index_mut` for
//!   an assignment or `&mut a[i]`; a projected result (`a[i].x`), a parenthesised or destructured
//!   place and a `ref mut` binding are credited to neither because ra_ap_syntax::ast::HasAttrs;
use it may borrow either way;
//! - the operator's `Self` operand has a syntactically resolvable type `T`
//!   (a constructor, a unit struct, a `let` annotated or built by one), is a
//!   whole operand (not the tail of a longer expression, not behind `&`, not
//!   the receiver of a method chain), and sits outside property-test macros.
//!
//! The relation reading only establishes reach. The separate bounded named
//! scalar result receipt authenticates one whole operator equality for the
//! ordinary oracle pipeline; no coarse relation raises observation or identity.

use super::equality_dispatch::{
    associated_fn_returns_self, impl_header, imports_other_type, operand_has_type, plain_type_name,
    std_trait_is_unshadowed, type_scope_is_clear,
};
use super::{
    FunctionSummary, RustIndex, TestSummary, is_ident_byte, last_let_statement,
    mask_comments_and_strings, matching_open,
};
use crate::analysis::classify::implicit_call::{operator_positions, result_is_used};

use crate::analysis::classify::{OwnerPinSyntax, ProbeContext};
use crate::analysis::facts::OracleFact;
use crate::analysis::syntax::{fn_signature::local_type_equality, parse_clean_source_file};
use crate::domain::ProbeFamily;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasGenericParams, HasName},
};

/// One standard operator trait: its method and the token that spells it.
struct OperatorTrait {
    name: &'static str,
    method: &'static str,
    token: &'static str,
    unary: bool,
    /// Set for `a[i]`: which of the two index traits the site reaches.
    access: Option<IndexAccess>,
}

/// `a[i]` runs `Index::index` when it reads and `IndexMut::index_mut` when it
/// is assigned to or mutably borrowed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IndexAccess {
    Read,
    Write,
}

const fn binary(name: &'static str, method: &'static str, token: &'static str) -> OperatorTrait {
    OperatorTrait {
        name,
        method,
        token,
        unary: false,
        access: None,
    }
}

const fn unary(name: &'static str, method: &'static str, token: &'static str) -> OperatorTrait {
    OperatorTrait {
        name,
        method,
        token,
        unary: true,
        access: None,
    }
}

const fn indexing(name: &'static str, method: &'static str, access: IndexAccess) -> OperatorTrait {
    OperatorTrait {
        name,
        method,
        token: "[",
        unary: false,
        access: Some(access),
    }
}

const OPERATORS: [OperatorTrait; 24] = [
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
    indexing("Index", "index", IndexAccess::Read),
    indexing("IndexMut", "index_mut", IndexAccess::Write),
];

/// The operator reading of an owner.
pub(super) struct OperatorOwner<'a> {
    /// Self type of the operator impl (`Money`).
    impl_type: &'a str,
    token: &'static str,
    unary: bool,
    access: Option<IndexAccess>,
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
        access: row.access,
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
    let reaching = if let Some(access) = operator.access {
        operator_positions(&masked, "[", true)
            .into_iter()
            .filter(|site| !opaque.iter().any(|range| range.contains(&site.at)))
            .filter(|site| !stringified.iter().any(|range| range.contains(&site.at)))
            .any(|site| {
                let Some(operand) = index_operand(&masked, site.at) else {
                    return false;
                };
                !bound_as_parameter_or_pattern(&masked, operand.text)
                    && operand_has_type(
                        operand.text,
                        impl_type,
                        &masked[..operand.start],
                        &returns_self,
                    )
                    && index_access(&masked, site.at, operand.mut_borrowed) == Some(access)
                    // A read whose value is thrown away cannot reach an
                    // assertion; a write changes its operand regardless.
                    && (access == IndexAccess::Write || result_is_used(&masked, site.at))
            })
    } else {
        operator_positions(&masked, operator.token, !operator.unary)
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
            })
    };
    if !reaching || !type_scope_is_clear(test, owner, source, impl_type, index) {
        return false;
    }
    *operator
        .file_level_clear
        .get_or_init(|| file_is_clear(source, owner, impl_type, index))
}

/// File-level refusals shared by every implicit-call reading: a `macro_rules!`
/// may expand to a second impl of the trait that the index never sees, so the
/// unique-impl count proves nothing, and an import may rename the type.
pub(super) fn file_is_clear(
    source: &str,
    owner: &FunctionSummary,
    impl_type: &str,
    index: &RustIndex,
) -> bool {
    let masked = mask_comments_and_strings(source);
    !masked.contains("macro_rules!")
        && !imports_other_type(&masked, impl_type, &owner.file)
        && !workspace_macro_may_generate_impls(index)
}

/// Whether any workspace file defines a `macro_rules!` that expands to an
/// `impl`. Invoked in the owner's file (through `#[macro_use]` or a path
/// import), it can generate a second operator impl the index never sees.
pub(super) fn workspace_macro_may_generate_impls(index: &RustIndex) -> bool {
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
pub(super) fn is_reference_operand(operand: &str, before: &str) -> bool {
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
pub(in crate::analysis::classify) fn stringified_ranges(
    masked: &str,
) -> Vec<std::ops::Range<usize>> {
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
pub(super) fn bound_as_parameter_or_pattern(masked: &str, operand: &str) -> bool {
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
pub(super) fn operand_before(masked: &str, op_at: usize) -> Option<(usize, &str)> {
    let (start, end) = operand_span(masked, op_at)?;
    stands_alone(&masked[..start]).then_some((start, &masked[start..end]))
}

/// The path (with at most one trailing group) ending just before `op_at`, as
/// byte offsets, whatever precedes it.
fn operand_span(masked: &str, op_at: usize) -> Option<(usize, usize)> {
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
    (start != path_end && starts_like_ident(&head[start..])).then_some((start, end))
}

/// The value an `a[i]` site indexes.
struct IndexOperand<'a> {
    start: usize,
    text: &'a str,
    /// Written `&mut a[i]`.
    mut_borrowed: bool,
}

/// The whole operand before the `[` at `at`. Indexing auto-dereferences, so a
/// leading `&` or `&mut` does not change which impl runs and is allowed; any
/// other prefix, a field (`x.items[i]`) or a method chain is not a whole
/// operand.
fn index_operand(masked: &str, at: usize) -> Option<IndexOperand<'_>> {
    let (start, end) = operand_span(masked, at)?;
    let before = masked[..start].trim_end();
    let (mut_borrowed, rest) = match before.strip_suffix("&mut") {
        Some(rest) => (true, rest),
        None => (false, before.strip_suffix('&').unwrap_or(before)),
    };
    stands_alone(rest).then_some(IndexOperand {
        start,
        text: &masked[start..end],
        mut_borrowed,
    })
}

/// Which index trait the `[` at `at` runs: `IndexMut` for an assignment,
/// compound assignment or `&mut a[i]`, `Index` for a plain read, and neither
/// when the result is projected further (`a[i].x`, `a[i][j]`, `a[i].len()`),
/// because method and field access may auto-borrow either way.
fn index_access(masked: &str, at: usize, mut_borrowed: bool) -> Option<IndexAccess> {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let close = bytes[at..].iter().position(|&byte| {
        match byte {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        depth == 0
    })? + at;
    let after = masked[close + 1..].trim_start();
    if after.starts_with(['.', '[', '(', '{']) {
        return None;
    }
    // A site inside a parenthesised or destructured place (`(m[0]).x = 1`,
    // `(m[0], m[1]) = ..`) or beside a `ref mut` binding is a mutable place
    // whatever follows the `]`, so only a plainly read position is credited.
    let statement = masked[..at]
        .rsplit([';', '{', '}'])
        .next()
        .unwrap_or_default()
        .trim_start();
    if statement.starts_with('(')
        || (!mut_borrowed && statement.contains("&mut"))
        || statement
            .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["ref", "mut"])
    {
        return None;
    }
    let assigns = (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"))
        || ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="]
            .iter()
            .any(|op| after.starts_with(op));
    Some(if assigns || mut_borrowed {
        IndexAccess::Write
    } else {
        IndexAccess::Read
    })
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

/// Assertion-local value observation for the bounded named scalar Add return.
/// The ordinary operator relation remains reach-only. This receipt additionally
/// binds the whole asserted operation, a strict literal expectation and the
/// selected unconditional named return; it never credits Self-only token gaps.
pub(in crate::analysis) fn assertion_observes_named_add_result(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
) -> bool {
    named_add_result(context, test, assertion, syntax).is_some()
}

fn parsed_function(text: &str) -> Option<ast::Fn> {
    parse_clean_source_file(text)?
        .tree()
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
}
fn text_compact(text: &str) -> String {
    mask_comments_and_strings(text)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}
fn constructor(call: &ast::CallExpr, ty: &str) -> Option<ast::Expr> {
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some()
        || path.segment()?.generic_arg_list().is_some()
        || path.segment()?.name_ref()?.text() != ty
    {
        return None;
    }
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    let [argument] = arguments.as_slice() else {
        return None;
    };
    Some(argument.clone())
}
fn scalar_constructor(expression: ast::Expr, ty: &str, integer: &str) -> bool {
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    constructor(&call, ty).is_some_and(|value| {
        let integer_atom = match &value {
            ast::Expr::Literal(literal) => matches!(literal.kind(), ast::LiteralKind::IntNumber(_)),
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
                matches!(prefix.expr(), Some(ast::Expr::Literal(literal)) if matches!(literal.kind(), ast::LiteralKind::IntNumber(_)))
            }
            _ => false,
        };
        integer_atom && super::super::owner_pin::integer_literal_fits_type(
            &text_compact(&value.syntax().text().to_string()), integer,
        )
    })
}

/// The caller has authenticated the sole primitive tuple-struct declaration.
/// Exclude exactly that declaration from the shared value-binding scan; a
/// constructor-shaped Fn, Const, Static, pattern or renamed import remains a
/// refusal. Glob closure remains owned by the existing operator import/type
/// authorities, which allow the authenticated standard `std::ops::*` surface.
fn constructor_value_binding_is_closed(
    owner: &FunctionSummary,
    index: &RustIndex,
    ty: &str,
) -> bool {
    let Some(facts) = index.files().get(&owner.file) else {
        return false;
    };
    let Some(parsed) = parse_clean_source_file(&facts.source) else {
        return false;
    };
    let name = ty.strip_prefix("r#").unwrap_or(ty);
    let declarations: Vec<_> = parsed
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter(|declaration| {
            declaration.name().is_some_and(|declared| {
                let text = declared.text().to_string();
                text.strip_prefix("r#").unwrap_or(&text) == name
            })
        })
        .collect();
    let [declaration] = declarations.as_slice() else {
        return false;
    };
    let range = declaration.syntax().text_range();
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    let mut owner_source = facts.source.to_string();
    owner_source.replace_range(start..end, &" ".repeat(end - start));
    index.files().iter().all(|(path, facts)| {
        !facts.used_lexical_fallback
            // An indexed macro definition does not authenticate its expansion.
            // An item invocation may emit a constructor-shaped callable even
            // when its tokens mention no type declaration or operator impl.
            && parse_clean_source_file(&facts.source).is_some_and(|parsed| {
                !parsed.tree().syntax().descendants()
                    .filter_map(ast::MacroCall::cast)
                    .any(|call| call.syntax().parent().is_some_and(|parent| {
                        ast::SourceFile::can_cast(parent.kind())
                            || ast::ItemList::can_cast(parent.kind())
                            || ast::MacroItems::can_cast(parent.kind())
                            || ast::StmtList::can_cast(parent.kind())
                    }))
            })
            && !crate::analysis::syntax::source_declares_value_name(
                if *path == owner.file {
                    &owner_source
                } else {
                    &facts.source
                },
                name,
            )
    })
}

fn named_add_result(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
) -> Option<()> {
    if !context.workspace_complete
        || !matches!(context.probe.family, ProbeFamily::ReturnValue)
        || context.owner_fn.is_none_or(|owner| owner.name != "add")
    {
        return None;
    }
    let owner = context.owner_fn?;
    let index = context.index;
    let operator = operator_owner(owner, index)?;
    if operator.token != "+" || operator.unary || operator.access.is_some() {
        return None;
    }
    if !syntax.admits(test, assertion, context.index)
        || crate::analysis::classify::is_ignored(test)
        || test
            .attrs
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(test.body.as_str()))
            .any(|text| text.contains("should_panic"))
    {
        return None;
    }
    // Cheap selected-assertion shape rejection precedes captured-index
    // constructor namespace/dispatch checks shared with the older receipt.
    let candidate_test = parsed_function(&test.body)?;
    let candidate_list = candidate_test.body()?.stmt_list()?;
    let candidate_statements: Vec<_> = candidate_list.statements().collect();
    if candidate_list.tail_expr().is_some()
        || !(1..=2).contains(&candidate_statements.len())
        || text_compact(&candidate_statements.last()?.syntax().text().to_string())
            .trim_end_matches(';')
            != text_compact(&assertion.text).trim_end_matches(';')
        || (candidate_statements.len() == 2
            && !matches!(candidate_statements.first(), Some(ast::Stmt::LetStmt(_))))
    {
        return None;
    }
    if !owner.attrs.is_empty()
        || !owner.impl_attrs.is_empty()
        || !super::super::call_identity::closed_method_dispatch_imports(index)
        || !std_trait_is_unshadowed(owner, index, "PartialEq")
        || !index.files().iter().all(|(file, _)| {
            ["std", "core"]
                .into_iter()
                .all(|root| index.drop_in_manifests.unbound_standard_root(file, root))
        })
        || index.files().values().any(|facts| {
            facts.source.contains("macro_use") || facts.source.contains("extern crate")
        })
    {
        return None;
    }
    let source = &index.files().get(&owner.file)?.source;
    let shape = local_type_equality(source, operator.impl_type)?;
    if shape.generic_params
        || shape.other_attributes
        || shape.qualified_derives
        || !shape.derives.iter().any(|name| name == "PartialEq")
        || shape.derives.iter().any(|name| {
            !matches!(
                name.as_str(),
                "Debug"
                    | "Clone"
                    | "Copy"
                    | "PartialEq"
                    | "Eq"
                    | "PartialOrd"
                    | "Ord"
                    | "Hash"
                    | "Default"
            )
        })
        || shape.fields.len() != 1
        || shape.fields[0].name.is_some()
        || super::super::custom_eq::pushed_element_may_compare_leniently(operator.impl_type, index)
    {
        return None;
    }
    let integer = super::super::owner_pin::indexed_owner_field_builtin_integer(
        owner,
        operator.impl_type,
        "0",
        index,
        syntax,
    )?;
    if !constructor_value_binding_is_closed(owner, index, operator.impl_type) {
        return None;
    }
    let method = parsed_function(&owner.body)?;
    let parameters = method.param_list()?;
    let receiver = parameters.self_param()?;
    let others: Vec<_> = parameters.params().collect();
    let [other] = others.as_slice() else {
        return None;
    };
    let ast::Pat::IdentPat(other_name) = other.pat()? else {
        return None;
    };
    let other_name = other_name.name()?.text().to_string();
    if method.async_token().is_some()
        || method.unsafe_token().is_some()
        || method.generic_param_list().is_some()
        || receiver.amp_token().is_some()
        || receiver.colon_token().is_some()
        || text_compact(&other.ty()?.syntax().text().to_string()) != operator.impl_type
        || text_compact(&method.ret_type()?.ty()?.syntax().text().to_string()) != operator.impl_type
    {
        return None;
    }
    let body = method.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let tail = body.tail_expr()?;
    if text_compact(&tail.syntax().text().to_string()) != text_compact(&context.probe.expression) {
        return None;
    }
    let ast::Expr::CallExpr(returned) = tail else {
        return None;
    };
    let named_method_projection =
        named_add_field_projection(owner, index, test, assertion, operator.impl_type, integer);
    // `Self` is authenticated by the indexed Add impl only for this explicit
    // method/result projection. The older implicit-operator receipt keeps its
    // named-constructor requirement and its deliberate Self refusal.
    let returned_value = constructor(&returned, operator.impl_type).or_else(|| {
        named_method_projection
            .then(|| constructor(&returned, "Self"))
            .flatten()
    })?;
    let ast::Expr::BinExpr(value) = returned_value else {
        return None;
    };
    if value.op_token()?.text() != "+" {
        return None;
    }
    let left = text_compact(&value.lhs()?.syntax().text().to_string());
    let right = text_compact(&value.rhs()?.syntax().text().to_string());
    let rhs = format!("{other_name}.0");
    if !((left == "self.0" && right == rhs) || (left == rhs && right == "self.0")) {
        return None;
    }
    if named_method_projection {
        return Some(());
    }
    let test_source = &index.files().get(&test.file)?.source;
    if !test_applies_operator_to_owner(test, owner, &operator, Some(test_source), index) {
        return None;
    }
    let test_fn = parsed_function(&test.body)?;
    let list = test_fn.body()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [statement] = statements.as_slice() else {
        return None;
    };
    if list.tail_expr().is_some()
        || text_compact(&statement.syntax().text().to_string()).trim_end_matches(';')
            != text_compact(&assertion.text).trim_end_matches(';')
    {
        return None;
    }
    let macro_call = statement
        .syntax()
        .descendants()
        .find_map(ast::MacroCall::cast)?;
    let path = text_compact(&macro_call.path()?.syntax().text().to_string());
    if !matches!(
        path.as_str(),
        "assert_eq" | "std::assert_eq" | "core::assert_eq"
    ) {
        return None;
    }
    let tokens = macro_call.token_tree()?.syntax().text().to_string();
    let inner = tokens.get(1..tokens.len().checked_sub(1)?)?;
    let function = parsed_function(&format!("fn args() {{ args({inner}) }}"))?;
    let ast::Expr::CallExpr(call) = function.body()?.stmt_list()?.tail_expr()? else {
        return None;
    };
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    let [left, right] = arguments.as_slice() else {
        return None;
    };
    [(left, right), (right, left)]
        .into_iter()
        .any(|(actual, expected)| {
            let ast::Expr::BinExpr(actual) = actual else {
                return false;
            };
            actual.op_token().is_some_and(|token| token.text() == "+")
                && actual
                    .lhs()
                    .is_some_and(|value| scalar_constructor(value, operator.impl_type, integer))
                && actual
                    .rhs()
                    .is_some_and(|value| scalar_constructor(value, operator.impl_type, integer))
                && scalar_constructor(expected.clone(), operator.impl_type, integer)
        })
        .then_some(())
}

/// Exact eager named method -> immutable result binding -> primitive tuple slot
/// equality. No intervening statement can overwrite, alias or replace the value.
fn named_add_field_projection(
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    ty: &str,
    integer: &str,
) -> bool {
    named_add_field_projection_inner(owner, index, test, assertion, ty, integer).is_some()
}
fn named_add_field_projection_inner(
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    ty: &str,
    integer: &str,
) -> Option<()> {
    let source = &index.files().get(&test.file)?.source;
    if test.file != owner.file
        || !type_scope_is_clear(test, owner, source, ty, index)
        || !file_is_clear(source, owner, ty, index)
        // A competing inherent/trait method may win explicit method dispatch.
        // This intentionally refuses even unrelated same-name workspace methods.
        || index.functions().iter().filter(|function| function.name == owner.name).count() != 1
    {
        return None;
    }
    let function = parsed_function(&test.body)?;
    let list = function.body()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [ast::Stmt::LetStmt(binding), assertion_statement] = statements.as_slice() else {
        return None;
    };
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some()
        || pattern.ref_token().is_some()
        || pattern.pat().is_some()
        || binding.attrs().next().is_some()
        || binding.let_else().is_some()
        || binding.ty().is_some()
        || list.tail_expr().is_some()
        || text_compact(&assertion_statement.syntax().text().to_string()).trim_end_matches(';')
            != text_compact(&assertion.text).trim_end_matches(';')
    {
        return None;
    }
    let binding_name = pattern.name()?.text().to_string();
    let ast::Expr::MethodCallExpr(call) = binding.initializer()? else {
        return None;
    };
    if call.name_ref()?.text() != owner.name
        || call.generic_arg_list().is_some()
        || !scalar_constructor(call.receiver()?, ty, integer)
    {
        return None;
    }
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    let [argument] = arguments.as_slice() else {
        return None;
    };
    if !scalar_constructor(argument.clone(), ty, integer) {
        return None;
    }
    let macro_call = assertion_statement
        .syntax()
        .descendants()
        .find_map(ast::MacroCall::cast)?;
    let path = text_compact(&macro_call.path()?.syntax().text().to_string());
    if !matches!(
        path.as_str(),
        "assert_eq" | "std::assert_eq" | "core::assert_eq"
    ) {
        return None;
    }
    let tokens = macro_call.token_tree()?.syntax().text().to_string();
    let inner = tokens.get(1..tokens.len().checked_sub(1)?)?;
    let arguments = parsed_function(&format!("fn args() {{ args({inner}) }}"))?;
    let ast::Expr::CallExpr(call) = arguments.body()?.stmt_list()?.tail_expr()? else {
        return None;
    };
    let operands: Vec<_> = call.arg_list()?.args().collect();
    let [left, right] = operands.as_slice() else {
        return None;
    };
    [(left, right), (right, left)]
        .into_iter()
        .any(|(actual, expected)| {
            let ast::Expr::FieldExpr(field) = actual else {
                return false;
            };
            field.name_ref().is_some_and(|name| name.text() == "0")
                && field.expr().is_some_and(|receiver| {
                    text_compact(&receiver.syntax().text().to_string()) == binding_name
                })
                && super::super::owner_pin::integer_literal_fits_type(
                    &text_compact(&expected.syntax().text().to_string()),
                    integer,
                )
        })
        .then_some(())
}

//! RIPR-SPEC-0265 rule 5: an owner whose whole body is one closure
//! (`pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 { move |x| n + x }`).
//!
//! The owner returns a function, so a test pins the closure's body only by
//! applying the returned function in the pinned expression
//! (`make_adder(2)(3)`). That application runs the closure exactly once, on
//! the arguments written there. The pin is read only when those arguments are
//! plain nonzero numeric literals that differ from one another, so no
//! identity element (`+ 0`, `* 1` against `0`) or coincident pair
//! (`2 + 2 == 2 * 2`) hides a changed operator. Every other shape keeps the
//! earlier reading.

use super::closure_adaptor::unconditional_body_over;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::FunctionSummary;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

/// An owner tail `[move] |a, b| E` admitted by RIPR-SPEC-0265 rule 5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis::classify) struct ReturnedClosureTail {
    /// How many arguments the returned closure takes.
    pub(in crate::analysis::classify) arity: usize,
    /// The whole tail, whitespace-collapsed.
    tail: String,
    /// The tail's first line, as an offset from the owner text's first line.
    pub(in crate::analysis::classify) tail_line_offset: usize,
}

impl ReturnedClosureTail {
    /// Whether the changed `expression` is the whole tail.
    pub(in crate::analysis::classify) fn tail_is(&self, expression: &str) -> bool {
        let expression = collapse(expression.trim().trim_end_matches(';'));
        let expression = expression
            .strip_prefix("return ")
            .map_or(expression.as_str(), str::trim);
        expression == self.tail
    }
}

/// The owner's returned-closure tail: the body holds no statement, only one
/// closure whose body reads its own parameters, the owner's parameters and
/// constants, and neither branches, short-circuits nor leaves early.
pub(in crate::analysis::classify) fn owner_returned_closure_tail(
    owner: &FunctionSummary,
) -> Option<ReturnedClosureTail> {
    if !owner.item.has_body {
        return None;
    }
    returned_closure_tail_of(&owner.body)
}

/// [`owner_returned_closure_tail`] over the owner's text from `fn` to its
/// closing brace.
pub(in crate::analysis::classify) fn returned_closure_tail_of(
    body: &str,
) -> Option<ReturnedClosureTail> {
    // Most owners return no closure; skip the parse for them.
    if !body.contains('|') {
        return None;
    }
    let masked = mask_comments_and_strings(body);
    if masked.contains('?') || super::has_unbounded_macro(&masked) {
        return None;
    }
    let parse = parse_clean_source_file(body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if function.async_token().is_some() || function.const_token().is_some() {
        return None;
    }
    let block = function.body()?;
    if block.statements().next().is_some() {
        return None;
    }
    let ast::Expr::ClosureExpr(closure) = block.tail_expr()? else {
        return None;
    };
    if closure.async_token().is_some()
        || closure.static_token().is_some()
        || closure.ret_type().is_some()
    {
        return None;
    }
    let mut names = Vec::new();
    for param in closure.param_list()?.params() {
        if param.ty().is_some() {
            return None;
        }
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        if ident.ref_token().is_some() || ident.mut_token().is_some() || ident.at_token().is_some()
        {
            return None;
        }
        names.push(ident.name()?.text().to_string());
    }
    if names.is_empty() {
        return None;
    }
    let mut owner_names: Vec<String> = Vec::new();
    for param in function.param_list()?.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        owner_names.push(ident.name()?.text().to_string());
    }
    let closure_body = closure.body()?;
    if !unconditional_body_over(&closure_body, &|name| {
        names
            .iter()
            .chain(owner_names.iter())
            .any(|known| known == name)
    }) {
        return None;
    }
    // A closure inside the closure may never run, or run many times.
    if closure_body
        .syntax()
        .descendants()
        .any(|node| ast::ClosureExpr::can_cast(node.kind()))
    {
        return None;
    }
    let tail = closure.syntax();
    Some(ReturnedClosureTail {
        arity: names.len(),
        tail: collapse(&tail.text().to_string()),
        tail_line_offset: body
            .get(..usize::from(tail.text_range().start()))?
            .matches('\n')
            .count(),
    })
}

/// The owner call inside `operand`, which must be that call applied to
/// `arity` arguments (`make_adder(2)(3)` → `make_adder(2)`).
pub(in crate::analysis::classify) fn strip_applied(operand: &str, arity: usize) -> Option<&str> {
    let (prefix, group) = split_trailing_group(operand.trim())?;
    (prefix.ends_with(')') && split_arguments(group)?.len() == arity).then_some(prefix)
}

/// Whether the arguments of the owner call and of the application in
/// `operand` are plain numeric literals, all nonzero and pairwise different.
pub(in crate::analysis::classify) fn applied_inputs_are_distinct_literals(
    operand: &str,
    arity: usize,
) -> bool {
    let Some((prefix, group)) = split_trailing_group(operand.trim()) else {
        return false;
    };
    let Some(applied) = split_arguments(group) else {
        return false;
    };
    if applied.len() != arity {
        return false;
    }
    let Some((_, owner_group)) = split_trailing_group(prefix) else {
        return false;
    };
    let Some(owner_arguments) = split_arguments(owner_group) else {
        return false;
    };
    let mut seen: Vec<String> = Vec::new();
    for argument in owner_arguments.iter().chain(applied.iter()) {
        let Some(value) = nonzero_numeric_literal(argument) else {
            return false;
        };
        if seen.contains(&value) {
            return false;
        }
        seen.push(value);
    }
    true
}

/// `text` as `prefix(group)`, where the final parenthesis closes the group
/// opened by the matching `(`.
fn split_trailing_group(text: &str) -> Option<(&str, &str)> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    if bytes.last() != Some(&b')') {
        return None;
    }
    let mut depth = 0usize;
    for at in (0..bytes.len()).rev() {
        match bytes[at] {
            b')' => depth += 1,
            b'(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let prefix = text.get(..at)?.trim_end();
                    let group = text.get(at + 1..bytes.len() - 1)?;
                    return (!prefix.is_empty()).then_some((prefix, group));
                }
            }
            _ => {}
        }
    }
    None
}

/// The top-level comma-separated arguments of `group`; `None` for an
/// unbalanced group.
fn split_arguments(group: &str) -> Option<Vec<&str>> {
    let masked = mask_comments_and_strings(group);
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, byte) in masked.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b',' if depth == 0 => {
                arguments.push(group.get(start..at)?.trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let last = group.get(start..)?.trim();
    if !last.is_empty() || !arguments.is_empty() {
        arguments.push(last);
    }
    // A trailing comma leaves an empty last argument; any other empty one is
    // not a call.
    if arguments.last().is_some_and(|last| last.is_empty()) {
        arguments.pop();
    }
    (!arguments.iter().any(|argument| argument.is_empty())).then_some(arguments)
}

/// The normalized text of a plain numeric literal that is not zero: digits
/// with `_` separators and an optional leading `-`, one `.` fraction and a
/// numeric type suffix (`2`, `-3`, `2.5`, `4u32`). Anything else is `None`.
fn nonzero_numeric_literal(argument: &str) -> Option<String> {
    let text = argument.trim();
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, text),
    };
    let digits_end = body
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '_' || ch == '.'))
        .unwrap_or(body.len());
    let (number, suffix) = body.split_at(digits_end);
    let suffix_ok = suffix.is_empty()
        || [
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64",
        ]
        .contains(&suffix);
    let cleaned: String = number.chars().filter(|ch| *ch != '_').collect();
    let well_formed = !cleaned.is_empty()
        && cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && cleaned.matches('.').count() <= 1
        && !cleaned.ends_with('.');
    let nonzero = cleaned.chars().any(|ch| ch.is_ascii_digit() && ch != '0');
    (suffix_ok && well_formed && nonzero)
        .then(|| format!("{}{}", if negative { "-" } else { "" }, cleaned))
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_body_closure_is_a_returned_closure_tail() {
        let tail = returned_closure_tail_of(
            "pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {\n    move |x| n + x\n}\n",
        );
        assert_eq!(tail.as_ref().map(|tail| tail.arity), Some(1));
        assert_eq!(tail.as_ref().map(|tail| tail.tail_line_offset), Some(1));
        assert_eq!(
            tail.as_ref().map(|tail| tail.tail_is("move |x|   n + x")),
            Some(true)
        );
        assert_eq!(
            tail.as_ref().map(|tail| tail.tail_is("move |x| n - x")),
            Some(false)
        );
    }

    #[test]
    fn a_branch_a_statement_or_a_nested_closure_is_not_admitted() {
        for body in [
            "pub fn f(n: i32) -> impl Fn(i32) -> i32 {\n    move |x| if x > n { x } else { n }\n}\n",
            "pub fn f(n: i32) -> impl Fn(i32) -> i32 {\n    let m = n;\n    move |x| m + x\n}\n",
            "pub fn f(n: i32) -> impl Fn(i32) -> i32 {\n    move |x| (|y| y + x)(n)\n}\n",
            "pub fn f(n: i32) -> impl Fn(i32) -> i32 {\n    move |x| x + helper(n)\n}\n",
            "pub fn f(n: i32) -> impl Fn(i32) -> i32 {\n    move |x| x.checked_add(n).unwrap_or(0)\n}\n",
        ] {
            assert!(returned_closure_tail_of(body).is_none(), "{body}");
        }
    }

    #[test]
    fn an_applied_pin_strips_to_the_owner_call() {
        assert_eq!(strip_applied("make_adder(2)(3)", 1), Some("make_adder(2)"));
        assert_eq!(strip_applied("make_adder(2)(3)", 2), None);
        assert_eq!(strip_applied("make_adder(2)", 1), None);
    }

    #[test]
    fn only_distinct_nonzero_numeric_literals_pin_the_closure() {
        for operand in [
            "make_adder(2)(3)",
            "make_adder(-2)(3)",
            "make_adder(2_i32)(3)",
        ] {
            assert!(
                applied_inputs_are_distinct_literals(operand, 1),
                "{operand}"
            );
        }
        for operand in [
            "make_adder(0)(3)",
            "make_adder(2)(0)",
            "make_adder(2)(2)",
            "make_adder(n)(3)",
            "make_adder(2)(x)",
            "make_adder(2)(f(3))",
            "make_adder(2)",
        ] {
            assert!(
                !applied_inputs_are_distinct_literals(operand, 1),
                "{operand}"
            );
        }
    }
}

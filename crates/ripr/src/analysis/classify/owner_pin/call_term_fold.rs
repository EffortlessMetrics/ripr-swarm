//! RIPR-SPEC-0360 call-in-term: the pinned run of an owner whose loop adds a
//! called function's value, `acc += double(*x).await * 1`.
//!
//! The pin credits the update only when the run's literal elements, folded
//! through the callee ripr can read, give the pinned value and every
//! neighbouring mutation of the update gives a different one. The callee is
//! evaluated here, never assumed: anything ripr cannot read (a second
//! definition of the name, a statement body, a call, a cast, an overflow, a
//! type that is not one built-in integer) refuses the credit.

use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::analysis::syntax::loop_accumulator::CallTerm;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, AstToken,
    ast::{self, HasGenericParams, HasName},
};
use std::path::Path;

/// A call term with the owner facts its evaluation needs.
#[derive(Clone, Debug)]
pub(in crate::analysis::classify) struct CallPin {
    term: CallTerm,
    /// The built-in integer type the owner returns.
    owner_type: String,
}

/// The pin for an accumulator's `term` in `owner`: `Some(None)` for a plain
/// term, `Some(Some(..))` for a call term whose owner type ripr reads, and
/// `None` for a call term it cannot (the gate must not fall back to the
/// plain-term credit for it).
pub(in crate::analysis::classify) fn admit(
    term: Option<CallTerm>,
    owner: &FunctionSummary,
) -> Option<Option<CallPin>> {
    let Some(term) = term else {
        return Some(None);
    };
    let owner_type = return_type(&owner.body)?;
    Some(Some(CallPin { term, owner_type }))
}

/// Whether folding `elements` through the pin's callee gives `expected`, and
/// every neighbouring mutant of the accumulator update gives a different
/// value (or panics, which fails the pinned test as well).
pub(in crate::analysis::classify) fn pinned_run_separates(
    pin: &CallPin,
    owner_file: &Path,
    index: &RustIndex,
    elements: &[String],
    expected: &str,
) -> bool {
    separates(pin, owner_file, index, elements, expected).unwrap_or(false)
}

fn separates(
    pin: &CallPin,
    owner_file: &Path,
    index: &RustIndex,
    elements: &[String],
    expected: &str,
) -> Option<bool> {
    let term = &pin.term;
    let callee = unique_callee(term, owner_file, index)?;
    let owner_type = pin.owner_type.clone();
    // The callee returns the accumulator's type, so `acc += call` is the
    // primitive operator over one integer type.
    if callee.return_type != owner_type {
        return None;
    }
    let range = int_range(&owner_type)?;
    let expected = integer_value(expected)?;
    if expected == 0 {
        return None;
    }
    let mut terms = Vec::with_capacity(elements.len());
    for element in elements {
        let value = integer_value(element)?;
        if !in_range(value, range) {
            return None;
        }
        let called = callee.evaluate(value, range)?;
        terms.push(called);
    }
    if terms.is_empty() {
        return None;
    }
    let scaled = |operator: char| -> Vec<Option<i128>> {
        terms
            .iter()
            .map(|value| match term.scale {
                Some((_, literal)) => checked(operator, *value, literal, range),
                None => Some(*value),
            })
            .collect()
    };
    let written = term.scale.map_or('*', |(operator, _)| operator);
    let sequence = scaled(written).into_iter().collect::<Option<Vec<_>>>()?;
    // The written update must give the pinned value, or the pin does not read
    // this owner at all.
    if fold(&sequence, update_operator(term), range)? != expected {
        return None;
    }
    // Neighbouring update operators. Starting from zero, `*= /= %= &= <<= >>=`
    // stay zero (or panic), which differs from a nonzero pin.
    let written_update = update_operator(term);
    for operator in ['+', '-', '|', '^', '='] {
        if operator == written_update {
            continue;
        }
        if let Some(value) = fold(&sequence, operator, range)
            && value == expected
        {
            return Some(false);
        }
    }
    // Neighbouring operators inside the term, apart from the one that is the
    // same function (`* 1` and `/ 1`, `+ 0` and `- 0`).
    if let Some((operator, literal)) = term.scale {
        for other in ['+', '-', '*', '/', '%'] {
            if other == operator || equivalent_operators(operator, other, literal) {
                continue;
            }
            // A mutant that panics on any element fails the test.
            let Some(values) = scaled(other).into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };
            if fold(&values, written_update, range) == Some(expected) {
                return Some(false);
            }
        }
    }
    Some(true)
}

/// The accumulator update the source writes: `+` or `-`.
fn update_operator(term: &CallTerm) -> char {
    if term.subtracts { '-' } else { '+' }
}

/// `x * 1` and `x / 1`, `x + 0` and `x - 0` are the same function for every
/// `x`, so the mutation between them is equivalent, not a gap.
fn equivalent_operators(written: char, other: char, literal: i128) -> bool {
    (literal == 1 && matches!((written, other), ('*', '/') | ('/', '*')))
        || (literal == 0 && matches!((written, other), ('+', '-') | ('-', '+')))
}

/// Fold `values` into zero with `operator`; `None` when any step panics
/// (overflow, an unsigned negative) or leaves the type's range.
fn fold(values: &[i128], operator: char, range: (i128, i128)) -> Option<i128> {
    let mut accumulator = 0i128;
    for value in values {
        accumulator = match operator {
            '=' => *value,
            '|' => accumulator | value,
            '^' => accumulator ^ value,
            other => checked(other, accumulator, *value, range)?,
        };
        if !in_range(accumulator, range) {
            return None;
        }
    }
    Some(accumulator)
}

fn checked(operator: char, left: i128, right: i128, range: (i128, i128)) -> Option<i128> {
    let value = match operator {
        '+' => left.checked_add(right)?,
        '-' => left.checked_sub(right)?,
        '*' => left.checked_mul(right)?,
        '/' => left.checked_div(right)?,
        '%' => left.checked_rem(right)?,
        _ => return None,
    };
    in_range(value, range).then_some(value)
}

fn in_range(value: i128, (low, high): (i128, i128)) -> bool {
    (low..=high).contains(&value)
}

/// The inclusive range of a built-in integer type of at most 64 bits.
fn int_range(ty: &str) -> Option<(i128, i128)> {
    let (signed, bits) = match ty {
        "i8" => (true, 8),
        "i16" => (true, 16),
        "i32" => (true, 32),
        "i64" | "isize" => (true, 64),
        "u8" => (false, 8),
        "u16" => (false, 16),
        "u32" => (false, 32),
        "u64" | "usize" => (false, 64),
        _ => return None,
    };
    Some(if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    })
}

/// A decimal integer literal with an optional sign, digit separators and a
/// built-in integer suffix.
fn integer_value(text: &str) -> Option<i128> {
    let text = text.trim();
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let text = text.replace('_', "");
    let end = text
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(text.len());
    let (digits, suffix) = text.split_at(end);
    if digits.is_empty() || (!suffix.is_empty() && int_range(suffix).is_none()) {
        return None;
    }
    let value = digits.parse::<i128>().ok()?;
    Some(if negative { -value } else { value })
}

/// The built-in integer type a function declares as its return type.
fn return_type(function_text: &str) -> Option<String> {
    let parse = parse_clean_source_file(function_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let ty = function.ret_type()?.ty()?.syntax().text().to_string();
    let ty: String = ty.chars().filter(|ch| !ch.is_whitespace()).collect();
    int_range(&ty).map(|_| ty)
}

/// A same-file function ripr can evaluate: one parameter and a return value
/// of one built-in integer type, and a body that is a single arithmetic
/// expression over that parameter and integer literals.
struct Callee {
    parameter: String,
    return_type: String,
    expression: ast::Expr,
    // Keeps the parse alive for `expression`.
    _parse: ra_ap_syntax::Parse<ra_ap_syntax::SourceFile>,
}

impl Callee {
    fn evaluate(&self, value: i128, range: (i128, i128)) -> Option<i128> {
        evaluate(&self.expression, &self.parameter, value, range)
    }
}

fn unique_callee(term: &CallTerm, owner_file: &Path, index: &RustIndex) -> Option<Callee> {
    let mut named = index
        .functions()
        .iter()
        .filter(|function| function.name == term.callee);
    let function = named.next()?;
    if named.next().is_some() || function.file != owner_file {
        return None;
    }
    let parse = parse_clean_source_file(&function.body)?;
    let item = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    if item.generic_param_list().is_some()
        || item.unsafe_token().is_some()
        || item.const_token().is_some()
        || item.name()?.text() != term.callee
    {
        return None;
    }
    let mut params = item.param_list()?.params();
    let param = params.next()?;
    if params.next().is_some() || item.param_list()?.self_param().is_some() {
        return None;
    }
    let Some(ast::Pat::IdentPat(pattern)) = param.pat() else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() {
        return None;
    }
    let parameter = pattern.name()?.text().to_string();
    let compact =
        |text: String| -> String { text.chars().filter(|ch| !ch.is_whitespace()).collect() };
    let parameter_type = compact(param.ty()?.syntax().text().to_string());
    let return_type = compact(item.ret_type()?.ty()?.syntax().text().to_string());
    if parameter_type != return_type || int_range(&return_type).is_none() {
        return None;
    }
    let list = item.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let expression = list.tail_expr()?;
    Some(Callee {
        parameter,
        return_type,
        expression,
        _parse: parse,
    })
}

/// A literal, the parameter, a parenthesized or negated expression, or one of
/// `+ - * / %` over two such expressions, every step staying in range.
fn evaluate(expr: &ast::Expr, parameter: &str, value: i128, range: (i128, i128)) -> Option<i128> {
    let result = match expr {
        ast::Expr::Literal(literal) => match literal.kind() {
            ast::LiteralKind::IntNumber(number) => integer_value(number.text())?,
            _ => return None,
        },
        ast::Expr::PathExpr(path) => {
            let path = path.path()?;
            if path.qualifier().is_some() || path.segment()?.name_ref()?.text() != parameter {
                return None;
            }
            value
        }
        ast::Expr::ParenExpr(inner) => evaluate(&inner.expr()?, parameter, value, range)?,
        ast::Expr::PrefixExpr(prefix) if prefix.op_token()?.text() == "-" => {
            evaluate(&prefix.expr()?, parameter, value, range)?.checked_neg()?
        }
        ast::Expr::BinExpr(bin) => {
            let operator = match bin.op_token()?.text() {
                "+" => '+',
                "-" => '-',
                "*" => '*',
                "/" => '/',
                "%" => '%',
                _ => return None,
            };
            let left = evaluate(&bin.lhs()?, parameter, value, range)?;
            let right = evaluate(&bin.rhs()?, parameter, value, range)?;
            checked(operator, left, right, range)?
        }
        _ => return None,
    };
    in_range(result, range).then_some(result)
}

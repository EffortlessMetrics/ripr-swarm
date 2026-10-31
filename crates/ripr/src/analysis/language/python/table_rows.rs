//! Table rows from unittest `subTest` loops, `for` tables, and pytest
//! parametrize cases.
//!
//! A strong exact oracle whose expected value is a row column cannot
//! discriminate a changed operand that no row varies. The more-itertools
//! `last(..., default=None)` empty path is the motivating case: every
//! `subTest` row passes `default=None` and asserts equality to that same
//! column, so `return default` -> `return None` still passes (#6651).
//!
//! A second #6651 shape is loguru's `t.tm_year % 100` -> `% 1000`: the year
//! column *does* vary, but every literal is 20xx, so the two expressions
//! agree on every row. When both the old and new lines are simple integer
//! arithmetic and every row binds those names to a known integer literal,
//! evaluate both expressions per row. If they never disagree, the table is
//! not an exact discriminator.
//!
//! Fail closed toward not-`exposed` only when static evidence sees the table
//! and either (1) the expected column is the same unvaried literal as a
//! changed returned parameter and that literal matches the replacement
//! return, (2) every row's bound integer literals make the old and new
//! arithmetic evaluate equal, or (3) an exact binary factor swap has bounded
//! primitive numeric rows with finite products. That last reading never
//! infers the production operands' runtime types or changes grouping.
//! An unreadable table, a non-literal cell, a
//! non-table strong test, a row that distinguishes the change, or an
//! unvaried value that would still fail after the replacement keeps today's
//! verdict.

use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{literal_value, literals_equal};
use super::parametrize::PythonParametrizeCases;
use super::related_tests::PythonRelatedCandidate;
use super::source_facts::parse_module_result;
use super::source_utils::text_for_range;
use super::static_limits::is_simple_python_identifier;
use super::{PythonAssertion, PythonOwner, PythonTest, expr_full_name};
use crate::domain::{OracleStrength, OwnerKind};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Rows beyond this bound leave the table unread.
const MAX_TABLE_ROWS: usize = 64;

/// A changed operand expression that the readable table rows do not discriminate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UnvariedTableOperand {
    pub(super) reason: String,
    pub(super) discriminator: String,
    pub(super) action: String,
}

/// When every strong related table-driven oracle takes its expected value from
/// a row column and no row varies the changed returned operand, name that
/// operand. Also names a bound arithmetic operand when every row's integer
/// literals make the old and new expressions agree, including the bounded
/// primitive numeric reading for exact factor swaps. None when a non-table
/// strong test exists, a table varies or distinguishes the operand, or the
/// table cannot be read.
pub(super) fn python_unvaried_table_operand(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
    sole_line_replacement: bool,
) -> Option<UnvariedTableOperand> {
    unvaried_returned_parameter_operand(line_text, old_line_text, owner, related_candidates, focus)
        .or_else(|| {
            unvaried_arithmetic_operand(
                line_text,
                old_line_text,
                owner,
                related_candidates,
                focus,
                sole_line_replacement,
            )
        })
}

fn unvaried_returned_parameter_operand(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<UnvariedTableOperand> {
    let operand = changed_returned_parameter(old_line_text, line_text, owner)?;
    // `return default` -> `return None` is invisible only when the unvaried
    // row value is the replacement literal. A constant `default=0` still
    // fails after `return None`, so that table remains a discriminator.
    let replacement = other_return_literal(old_line_text, line_text, &operand)?;
    let mut saw_table = false;
    for candidate in related_candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        // One selected assertion is not enough: a later unvaried table row can
        // win the equal-strength tie-break over an earlier exact oracle in the
        // same test. Any strong admitted assertion that is not this tautology
        // may still discriminate.
        let strong_assertions: Vec<&PythonAssertion> = candidate
            .test
            .assertions
            .iter()
            .filter(|assertion| {
                focus.admits(assertion)
                    && assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
            })
            .collect();
        if strong_assertions.is_empty() {
            continue;
        }
        let Some(table) = table_cases(candidate.test) else {
            // A strong oracle that is not table-driven may still discriminate.
            return None;
        };
        if strong_assertions.iter().any(|assertion| {
            !table_does_not_vary(
                candidate.test,
                owner,
                &operand,
                &replacement,
                &table,
                assertion,
            )
        }) {
            return None;
        }
        saw_table = true;
    }
    saw_table.then(|| UnvariedTableOperand {
        reason: format!("table rows do not vary {operand}"),
        discriminator: format!("a table row that varies {operand}"),
        action: format!("Add a table row that varies `{operand}`."),
    })
}

fn unvaried_arithmetic_operand(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
    sole_line_replacement: bool,
) -> Option<UnvariedTableOperand> {
    let old_line = old_line_text?;
    let (old_arith, new_arith) = changed_simple_arithmetic(old_line, line_text)?;
    // Keep the ordinary table reading for factor swaps. Even int/float
    // annotations do not establish Python runtime operand types, and a name
    // can hold a noncommutative __mul__/__rmul__ object. Literal-only products
    // already fall through below because they have no named operand.
    let factor_swap = swaps_factors(&old_arith, &new_arith);
    let operand = if factor_swap {
        let Expr::BinOp(product) = &old_arith else {
            return None;
        };
        expr_plain_name(&product.left).or_else(|| expr_plain_name(&product.right))?
    } else {
        arithmetic_operand_name(&old_arith)?
    };
    let mut saw_table = false;
    let mut saw_rebound_factor = false;
    for candidate in related_candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        let strong_assertions: Vec<&PythonAssertion> = candidate
            .test
            .assertions
            .iter()
            .filter(|assertion| {
                focus.admits(assertion)
                    && assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
            })
            .collect();
        if strong_assertions.is_empty() {
            continue;
        }
        let table = table_cases(candidate.test)?;
        if factor_swap
            && sole_line_replacement
            && observed_literal_sibling_change(old_line, line_text, owner, candidate.test, &table)
        {
            return None;
        }
        let only_strong_assertion = strong_assertions.len() == 1;
        for assertion in strong_assertions {
            if factor_swap
                && only_strong_assertion
                && factor_input_rebound(candidate.test, owner, &old_arith, &table, assertion)
            {
                saw_rebound_factor = true;
                continue;
            }
            if !arithmetic_rows_agree(
                candidate.test,
                owner,
                &old_arith,
                &new_arith,
                &table,
                assertion,
            ) {
                return None;
            }
        }
        saw_table = true;
    }
    saw_table.then(|| {
        if factor_swap {
            UnvariedTableOperand {
                reason: if saw_rebound_factor {
                    "a test rebinds a table operand, so its call value is not established"
                        .to_string()
                } else {
                    "numeric table rows agree under factor reordering".to_string()
                },
                discriminator: "a table row whose operands distinguish multiplication order"
                    .to_string(),
                action: "Add a table row whose operands distinguish multiplication order."
                    .to_string(),
            }
        } else {
            UnvariedTableOperand {
                reason: format!("table rows do not vary {operand}"),
                discriminator: format!("a table row that varies {operand}"),
                action: format!("Add a table row that varies `{operand}`."),
            }
        }
    })
}

/// A whole-result row can observe a changed literal beside the swap. This
/// intentionally admits only a direct, undecorated two-line function and a
/// direct single-assert parametrized test; uncertain bindings retain the gap.
fn observed_literal_sibling_change(
    old_line: &str,
    new_line: &str,
    owner: &PythonOwner,
    test: &PythonTest,
    table: &PythonParametrizeCases,
) -> bool {
    if owner.owner_kind != Some(OwnerKind::Function)
        || !owner.decorators.is_empty()
        || owner.end_line != owner.start_line + 1
        || owner.term_facts.may_yield()
        || !old_line.trim().starts_with("return ")
        || !new_line.trim().starts_with("return ")
        || test.parametrize.is_none()
        || !test.simple_literal_parametrize
        || test.decorators.len() != 1
        || !test
            .fixtures
            .iter()
            .all(|name| test.takes_parametrize_value(name))
        || !table
            .argnames()
            .iter()
            .all(|name| test.takes_parametrize_value(name))
    {
        return false;
    }
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("t.py"), &test.body_text) else {
        return false;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return false;
    };
    let [actual_assertion @ Stmt::Assert(_)] = function.body.as_slice() else {
        return false;
    };
    let args = &function.args;
    let columns = table.argnames();
    let signature: BTreeSet<String> = args
        .args
        .iter()
        .map(|arg| arg.def.arg.to_string())
        .collect();
    if !args.posonlyargs.is_empty()
        || !args.kwonlyargs.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || args.args.iter().any(|arg| arg.default.is_some())
        || args.args.len() != columns.len()
        || signature != columns
    {
        return false;
    }
    let Some(old) = line_value_expr(old_line).and_then(|text| parse_expr(&text)) else {
        return false;
    };
    let Some(new) = line_value_expr(new_line).and_then(|text| parse_expr(&text)) else {
        return false;
    };
    let (old_items, new_items) = match (&old, &new) {
        (Expr::Tuple(a), Expr::Tuple(b)) => (&a.elts, &b.elts),
        (Expr::List(a), Expr::List(b)) => (&a.elts, &b.elts),
        _ => return false,
    };
    if old_items.len() != 2 || new_items.len() != 2 {
        return false;
    }
    let Some(index) = (0..2).find(|&i| swaps_factors(&old_items[i], &new_items[i])) else {
        return false;
    };
    let Expr::BinOp(product) = &old_items[index] else {
        return false;
    };
    let (Expr::Name(left), Expr::Name(right)) = (product.left.as_ref(), product.right.as_ref())
    else {
        return false;
    };
    if !owner.term_facts.is_stable_parameter(left.id.as_str())
        || !owner.term_facts.is_stable_parameter(right.id.as_str())
    {
        return false;
    }
    let names = owner_names(owner, test);
    let Some((owner_call, expected)) = owner_call_and_expected_from_stmt(actual_assertion, &names)
    else {
        return false;
    };
    let Expr::Call(call) = owner_call else {
        return false;
    };
    if !call.keywords.is_empty()
        || call.args.len() != owner.parameters.len()
        || owner.parameters.iter().any(|p| p.keyword_only)
    {
        return false;
    }
    let Some(expected_name) = expr_plain_name(expected).filter(|name| columns.contains(name))
    else {
        return false;
    };
    let Some(bindings) = row_argument_env(owner_call, owner, |arg| {
        expr_plain_name(arg).filter(|name| columns.contains(name))
    }) else {
        return false;
    };
    let (Some(left_column), Some(right_column)) = (
        bindings.get(left.id.as_str()),
        bindings.get(right.id.as_str()),
    ) else {
        return false;
    };
    let sibling = 1 - index;
    let (Some(NumericFactor::Integer(old_sibling)), Some(NumericFactor::Integer(new_sibling))) = (
        numeric_factor_literal(&old_items[sibling]),
        numeric_factor_literal(&new_items[sibling]),
    ) else {
        return false;
    };
    if old_sibling == new_sibling {
        return false;
    }
    table.cases.iter().any(|row| {
        let Some(expected) = row.get(&expected_name).and_then(|text| parse_expr(text)) else { return false; };
        let expected_items = match (&old, &expected) {
            (Expr::Tuple(_), Expr::Tuple(value)) => &value.elts,
            (Expr::List(_), Expr::List(value)) => &value.elts,
            _ => return false,
        };
        if expected_items.len() != 2 || !matches!(numeric_factor_literal(&expected_items[sibling]), Some(NumericFactor::Integer(value)) if value == old_sibling) { return false; }
        let Some(left) = row.get(left_column).and_then(|text| parse_expr(text)).and_then(|expr| numeric_factor_literal(&expr)) else { return false; };
        let Some(right) = row.get(right_column).and_then(|text| parse_expr(text)).and_then(|expr| numeric_factor_literal(&expr)) else { return false; };
        let Some(expected_product) = numeric_factor_literal(&expected_items[index]) else { return false; };
        match (left, right, expected_product) {
            (NumericFactor::Integer(a), NumericFactor::Integer(b), NumericFactor::Integer(want)) => a.checked_mul(b) == Some(want),
            (NumericFactor::Float(a), NumericFactor::Float(b), NumericFactor::Float(want)) => {
                let product = a * b;
                product.is_finite() && product.partial_cmp(&want) == Some(std::cmp::Ordering::Equal)
            }
            _ => false,
        }
    })
}

/// Syntactic factor swapping selects order-sensitive guidance. It never grants
/// equivalence or exposure: names can hold overloaded multiplication operands.
fn swaps_factors(old: &Expr, new: &Expr) -> bool {
    let (Expr::BinOp(old), Expr::BinOp(new)) = (old, new) else {
        return false;
    };
    if !matches!(old.op, ast::Operator::Mult) || !matches!(new.op, ast::Operator::Mult) {
        return false;
    }
    let (Some(old_left), Some(old_right), Some(new_left), Some(new_right)) = (
        factor_spelling(&old.left),
        factor_spelling(&old.right),
        factor_spelling(&new.left),
        factor_spelling(&new.right),
    ) else {
        return false;
    };
    old_left != old_right && old_left == new_right && old_right == new_left
}

fn factor_spelling(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(format!("name:{}", name.id)),
        _ => match numeric_factor_literal(expr)? {
            NumericFactor::Integer(value) => Some(format!("int:{value}")),
            NumericFactor::Float(value) => Some(format!("float:{}", value.to_bits())),
        },
    }
}

/// Bounded primitive values only. This is a row discriminator, not a type
/// inference for the production function or a floating-point algebra engine.
#[derive(Clone, Copy)]
enum NumericFactor {
    Integer(i128),
    Float(f64),
}

fn numeric_factor_literal(expr: &Expr) -> Option<NumericFactor> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Float(value) if value.is_finite() => Some(NumericFactor::Float(*value)),
            value => constant_i128(value).map(NumericFactor::Integer),
        },
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd) => {
            numeric_factor_literal(&unary.operand)
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::USub) => {
            // The positive magnitude of i128::MIN is outside i128. Apply the
            // source sign before range checking, matching the integer reader.
            if let Expr::Constant(constant) = unary.operand.as_ref()
                && let ast::Constant::Int(value) = &constant.value
            {
                return parse_decimal_i128(&format!("-{}", value.to_str_radix(10)))
                    .map(NumericFactor::Integer);
            }
            match numeric_factor_literal(&unary.operand)? {
                NumericFactor::Integer(value) => value.checked_neg().map(NumericFactor::Integer),
                NumericFactor::Float(value) => Some(NumericFactor::Float(-value)),
            }
        }
        _ => None,
    }
}

fn bounded_float(value: NumericFactor) -> Option<f64> {
    match value {
        // Check in the integer domain before conversion. This deliberately
        // narrows mixed rows; no rounding can admit an out-of-bound integer.
        NumericFactor::Integer(value) => i32::try_from(value).ok().map(f64::from),
        NumericFactor::Float(value) => Some(value),
    }
}

fn primitive_product_is_finite(left: NumericFactor, right: NumericFactor) -> bool {
    if matches!(
        (left, right),
        (NumericFactor::Integer(_), NumericFactor::Integer(_))
    ) {
        // Python integer multiplication has arbitrary precision. The bounded
        // input literals are genuine integers, never user objects or booleans.
        return true;
    }
    match (bounded_float(left), bounded_float(right)) {
        (Some(left), Some(right)) => (left * right).is_finite(),
        _ => false,
    }
}

fn changed_returned_parameter(
    old_line_text: Option<&str>,
    new_line_text: &str,
    owner: &PythonOwner,
) -> Option<String> {
    // `return default` -> `return None` names the operand on the old line.
    let old = old_line_text.and_then(|old| returned_parameter(old, owner));
    let new = returned_parameter(new_line_text, owner);
    match (old, new) {
        (Some(old), Some(new)) if old == new => None,
        (Some(old), _) => Some(old),
        (None, Some(new)) => Some(new),
        (None, None) => None,
    }
}

fn returned_parameter(line_text: &str, owner: &PythonOwner) -> Option<String> {
    let expr = returned_expr(line_text)?;
    if !is_simple_python_identifier(expr) {
        return None;
    }
    owner
        .parameters
        .iter()
        .any(|parameter| parameter.name == expr)
        .then(|| expr.to_string())
}

fn returned_expr(line_text: &str) -> Option<&str> {
    let expr = line_text.trim().strip_prefix("return ")?.trim();
    Some(expr.split('#').next()?.trim())
}

/// Literal on the side of the return that is not the changed parameter.
fn other_return_literal(
    old_line_text: Option<&str>,
    new_line_text: &str,
    operand: &str,
) -> Option<String> {
    [Some(new_line_text), old_line_text]
        .into_iter()
        .flatten()
        .find_map(|line| {
            let expr = returned_expr(line)?;
            (expr != operand).then(|| literal_value(expr)).flatten()
        })
}

fn table_cases(test: &PythonTest) -> Option<PythonParametrizeCases> {
    if let Some(parametrize) = &test.parametrize
        && !parametrize.cases.is_empty()
    {
        return Some(parametrize.clone());
    }
    loop_table_cases(&test.body_text)
}

fn table_does_not_vary(
    test: &PythonTest,
    owner: &PythonOwner,
    operand: &str,
    replacement: &str,
    table: &PythonParametrizeCases,
    assertion: &PythonAssertion,
) -> bool {
    let columns = table.argnames();
    if columns.is_empty() {
        return false;
    }
    let names = owner_names(owner, test);
    let Some(stmts) = assertion_stmts(&assertion.gate_text) else {
        return false;
    };
    let Some((owner_call, expected)) = owner_call_and_expected(&stmts, &names) else {
        return false;
    };
    let Some(expected_name) = expr_plain_name(expected).filter(|name| columns.contains(name))
    else {
        return false;
    };
    let Some(column) = operand_column(owner_call, operand, owner, &columns) else {
        return false;
    };
    let Some(operand_value) = unvaried_column_value(&table.cases, &column) else {
        return false;
    };
    let Some(expected_value) = unvaried_column_value(&table.cases, &expected_name) else {
        return false;
    };
    literals_equal(&operand_value, &expected_value) == Some(true)
        && literals_equal(&operand_value, replacement) == Some(true)
}

fn arithmetic_rows_agree(
    test: &PythonTest,
    owner: &PythonOwner,
    old_arith: &Expr,
    new_arith: &Expr,
    table: &PythonParametrizeCases,
    assertion: &PythonAssertion,
) -> bool {
    let columns = table.argnames();
    if columns.is_empty() || table.cases.is_empty() {
        return false;
    }
    let names = owner_names(owner, test);
    let Some(stmts) = assertion_stmts(&assertion.gate_text) else {
        return false;
    };
    let Some(owner_call) = owner_call_from_equality(&stmts, &names) else {
        return false;
    };
    table.cases.iter().all(|row| {
        if swaps_factors(old_arith, new_arith) {
            return numeric_swap_row_agrees(old_arith, owner_call, owner, row, &columns);
        }
        let Some(env) = row_integer_env(owner_call, owner, row, &columns) else {
            return false;
        };
        match (
            eval_simple_arithmetic(old_arith, &env),
            eval_simple_arithmetic(new_arith, &env),
        ) {
            (Some(old), Some(new)) => old == new,
            _ => false,
        }
    })
}

fn factor_input_rebound(
    test: &PythonTest,
    owner: &PythonOwner,
    product: &Expr,
    table: &PythonParametrizeCases,
    assertion: &PythonAssertion,
) -> bool {
    let Expr::BinOp(product) = product else {
        return false;
    };
    let Some(stmts) = assertion_stmts(&assertion.gate_text) else {
        return false;
    };
    let Some(call) = owner_call_from_equality(&stmts, &owner_names(owner, test)) else {
        return false;
    };
    let Some(bindings) = row_argument_env(call, owner, |argument| Some(argument.clone())) else {
        return false;
    };
    let columns = table.argnames();
    [product.left.as_ref(), product.right.as_ref()]
        .into_iter()
        .filter_map(expr_plain_name)
        .any(|name| {
            bindings
                .get(&name)
                .and_then(expr_plain_name)
                .is_some_and(|column| {
                    test.takes_parametrize_value(&column)
                        && test.parametrize_argnames.contains(&column)
                        && !columns.contains(&column)
                })
        })
}

fn numeric_swap_row_agrees(
    product: &Expr,
    owner_call: &Expr,
    owner: &PythonOwner,
    row: &BTreeMap<String, String>,
    columns: &BTreeSet<String>,
) -> bool {
    let Expr::BinOp(product) = product else {
        return false;
    };
    let Some(env) = row_argument_env(owner_call, owner, |argument| Some(argument.clone())) else {
        return false;
    };
    let factor = |expr: &Expr| match expr {
        Expr::Name(name) => {
            let argument = env.get(name.id.as_str())?;
            match argument {
                Expr::Name(column) if columns.contains(column.id.as_str()) => {
                    numeric_factor_literal(&parse_expr(row.get(column.id.as_str())?)?)
                }
                literal => numeric_factor_literal(literal),
            }
        }
        expr => numeric_factor_literal(expr),
    };
    match (factor(&product.left), factor(&product.right)) {
        (Some(left), Some(right)) => primitive_product_is_finite(left, right),
        _ => false,
    }
}

fn changed_simple_arithmetic(old_line: &str, new_line: &str) -> Option<(Expr, Expr)> {
    let old_text = line_value_expr(old_line)?;
    let new_text = line_value_expr(new_line)?;
    let old_expr = parse_expr(&old_text)?;
    let new_expr = parse_expr(&new_text)?;
    let (old_arith, new_arith) = differing_simple_arithmetic(&old_expr, &new_expr)?;
    Some((old_arith.clone(), new_arith.clone()))
}

fn line_value_expr(line: &str) -> Option<String> {
    let trimmed = line.trim().trim_end_matches(',').trim();
    let expr = if let Some(rest) = trimmed.strip_prefix("return ") {
        rest.trim()
    } else if let Some(value) = dict_field_value(trimmed) {
        value
    } else if let Some(value) = assignment_rhs(trimmed) {
        value
    } else {
        trimmed
    };
    // Let Python's parser distinguish comments from quoted hashes. Splitting
    // source text at '#' can erase an actual changed arithmetic expression.
    (!expr.is_empty()).then(|| expr.to_string())
}

fn dict_field_value(line: &str) -> Option<&str> {
    let trimmed = line.trim().trim_end_matches(',').trim();
    let rest = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split_once('"')?.1
    } else if let Some(rest) = trimmed.strip_prefix('\'') {
        rest.split_once('\'')?.1
    } else {
        return None;
    };
    rest.trim()
        .strip_prefix(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn assignment_rhs(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.contains("==")
        || trimmed.contains("!=")
        || trimmed.contains(">=")
        || trimmed.contains("<=")
    {
        return None;
    }
    let (lhs, rhs) = trimmed.split_once('=')?;
    is_simple_python_identifier(lhs.trim())
        .then(|| rhs.trim())
        .filter(|value| !value.is_empty())
}

fn parse_expr(text: &str) -> Option<Expr> {
    let source = format!("_ = {text}\n");
    let module = parse_module_result(Path::new("t.py"), &source).ok()?;
    match module {
        Mod::Module(module) => match module.body.into_iter().next()? {
            Stmt::Assign(assign) if assign.targets.len() == 1 => Some(*assign.value),
            _ => None,
        },
        _ => None,
    }
}

fn differing_simple_arithmetic<'a>(old: &'a Expr, new: &'a Expr) -> Option<(&'a Expr, &'a Expr)> {
    match (old, new) {
        (Expr::Dict(old_dict), Expr::Dict(new_dict))
            if old_dict.values.len() == new_dict.values.len() =>
        {
            unique_differing_pair(
                old_dict
                    .values
                    .iter()
                    .zip(new_dict.values.iter())
                    .filter_map(|(old_value, new_value)| {
                        differing_simple_arithmetic(old_value, new_value)
                    }),
            )
        }
        (Expr::List(old_list), Expr::List(new_list))
            if old_list.elts.len() == new_list.elts.len() =>
        {
            unique_differing_pair(
                old_list
                    .elts
                    .iter()
                    .zip(new_list.elts.iter())
                    .filter_map(|(old_elt, new_elt)| differing_simple_arithmetic(old_elt, new_elt)),
            )
        }
        (Expr::Tuple(old_tuple), Expr::Tuple(new_tuple))
            if old_tuple.elts.len() == new_tuple.elts.len() =>
        {
            unique_differing_pair(
                old_tuple
                    .elts
                    .iter()
                    .zip(new_tuple.elts.iter())
                    .filter_map(|(old_elt, new_elt)| differing_simple_arithmetic(old_elt, new_elt)),
            )
        }
        (Expr::Call(old_call), Expr::Call(new_call))
            if old_call.args.len() == new_call.args.len()
                && old_call.keywords.len() == new_call.keywords.len() =>
        {
            let arg_diff = old_call
                .args
                .iter()
                .zip(new_call.args.iter())
                .filter_map(|(old_arg, new_arg)| differing_simple_arithmetic(old_arg, new_arg));
            let kw_diff = old_call
                .keywords
                .iter()
                .zip(new_call.keywords.iter())
                .filter_map(|(old_kw, new_kw)| {
                    differing_simple_arithmetic(&old_kw.value, &new_kw.value)
                });
            unique_differing_pair(arg_diff.chain(kw_diff))
        }
        _ if swaps_factors(old, new) => Some((old, new)),
        _ if is_simple_arithmetic(old)
            && is_simple_arithmetic(new)
            && contains_binop(old)
            && contains_binop(new)
            && !arithmetic_equal(old, new) =>
        {
            Some((old, new))
        }
        _ => None,
    }
}

fn unique_differing_pair<'a, I>(mut pairs: I) -> Option<(&'a Expr, &'a Expr)>
where
    I: Iterator<Item = (&'a Expr, &'a Expr)>,
{
    let first = pairs.next()?;
    pairs.next().is_none().then_some(first)
}

fn is_simple_arithmetic(expr: &Expr) -> bool {
    match expr {
        Expr::Name(_) => true,
        Expr::Constant(constant) => constant_i128(&constant.value).is_some(),
        Expr::Attribute(attribute) => is_simple_arithmetic(&attribute.value),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            is_simple_arithmetic(&unary.operand)
        }
        Expr::BinOp(bin_op) if allowed_int_op(&bin_op.op) => {
            is_simple_arithmetic(&bin_op.left) && is_simple_arithmetic(&bin_op.right)
        }
        _ => false,
    }
}

fn contains_binop(expr: &Expr) -> bool {
    match expr {
        Expr::BinOp(_) => true,
        Expr::UnaryOp(unary) => contains_binop(&unary.operand),
        Expr::Attribute(attribute) => contains_binop(&attribute.value),
        _ => false,
    }
}

fn arithmetic_equal(left: &Expr, right: &Expr) -> bool {
    match (left, right) {
        (Expr::Name(left), Expr::Name(right)) => left.id == right.id,
        (Expr::Constant(left), Expr::Constant(right)) => {
            constant_i128(&left.value) == constant_i128(&right.value)
                && constant_i128(&left.value).is_some()
        }
        (Expr::Attribute(left), Expr::Attribute(right)) => {
            left.attr == right.attr && arithmetic_equal(&left.value, &right.value)
        }
        (Expr::UnaryOp(left), Expr::UnaryOp(right)) if left.op == right.op => {
            arithmetic_equal(&left.operand, &right.operand)
        }
        (Expr::BinOp(left), Expr::BinOp(right)) if left.op == right.op => {
            arithmetic_equal(&left.left, &right.left) && arithmetic_equal(&left.right, &right.right)
        }
        _ => false,
    }
}

fn allowed_int_op(op: &ast::Operator) -> bool {
    matches!(
        op,
        ast::Operator::Add
            | ast::Operator::Sub
            | ast::Operator::Mult
            | ast::Operator::Mod
            | ast::Operator::FloorDiv
            | ast::Operator::Pow
    )
}

fn arithmetic_operand_name(expr: &Expr) -> Option<String> {
    let mut names = Vec::new();
    let mut year_attr = false;
    collect_binding_names(expr, &mut names, &mut year_attr)?;
    if year_attr {
        return Some("year".to_string());
    }
    names.into_iter().next()
}

fn collect_binding_names(expr: &Expr, names: &mut Vec<String>, year_attr: &mut bool) -> Option<()> {
    match expr {
        Expr::Name(name) => {
            let ident = name.id.to_string();
            if !names.iter().any(|existing| existing == &ident) {
                names.push(ident);
            }
            Some(())
        }
        Expr::Constant(constant) => constant_i128(&constant.value).map(|_| ()),
        Expr::Attribute(attribute) => {
            if matches!(attribute.attr.as_str(), "year" | "tm_year") {
                *year_attr = true;
            }
            collect_binding_names(&attribute.value, names, year_attr)
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd | ast::UnaryOp::USub) => {
            collect_binding_names(&unary.operand, names, year_attr)
        }
        Expr::BinOp(bin_op) if allowed_int_op(&bin_op.op) => {
            collect_binding_names(&bin_op.left, names, year_attr)?;
            collect_binding_names(&bin_op.right, names, year_attr)
        }
        _ => None,
    }
}

fn owner_call_from_equality<'a>(stmts: &'a [Stmt], owner_names: &[String]) -> Option<&'a Expr> {
    stmts.iter().find_map(|stmt| {
        let (left, right) = match stmt {
            Stmt::Assert(assert_stmt) => match assert_stmt.test.as_ref() {
                Expr::Compare(compare)
                    if compare.ops.len() == 1
                        && matches!(compare.ops[0], ast::CmpOp::Eq)
                        && compare.comparators.len() == 1 =>
                {
                    (compare.left.as_ref(), &compare.comparators[0])
                }
                _ => return None,
            },
            Stmt::Expr(expr_stmt) => match expr_stmt.value.as_ref() {
                Expr::Call(call) if is_equality_assertion(call) => compared_operands(call)?,
                _ => return None,
            },
            _ => return None,
        };
        unwrap_owner_call(left, owner_names).or_else(|| unwrap_owner_call(right, owner_names))
    })
}

fn unwrap_owner_call<'a>(expr: &'a Expr, owner_names: &[String]) -> Option<&'a Expr> {
    match expr {
        Expr::Call(_) if is_owner_call(expr, owner_names) => Some(expr),
        Expr::Subscript(subscript) => unwrap_owner_call(&subscript.value, owner_names),
        Expr::Attribute(attribute) => unwrap_owner_call(&attribute.value, owner_names),
        _ => None,
    }
}

fn row_integer_env(
    owner_call: &Expr,
    owner: &PythonOwner,
    row: &BTreeMap<String, String>,
    columns: &BTreeSet<String>,
) -> Option<BTreeMap<String, i128>> {
    row_argument_env(owner_call, owner, |argument| {
        integer_from_table_arg(argument, row, columns)
    })
}

fn row_argument_env<T>(
    owner_call: &Expr,
    owner: &PythonOwner,
    read_argument: impl Fn(&Expr) -> Option<T>,
) -> Option<BTreeMap<String, T>> {
    let Expr::Call(call) = owner_call else {
        return None;
    };
    if call
        .args
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
        || call.keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }
    let skip = receiver_skip(owner)?;
    let mut env = BTreeMap::new();
    for keyword in &call.keywords {
        let Some(name) = keyword.arg.as_ref() else {
            continue;
        };
        let value = read_argument(&keyword.value)?;
        env.insert(name.to_string(), value);
    }
    let positional: Vec<&str> = owner
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .skip(skip)
        .map(|parameter| parameter.name.as_str())
        .collect();
    for (parameter, argument) in positional.iter().zip(call.args.iter()) {
        let value = read_argument(argument)?;
        env.insert((*parameter).to_string(), value);
    }
    (!env.is_empty()).then_some(env)
}

fn integer_from_table_arg(
    arg: &Expr,
    row: &BTreeMap<String, String>,
    columns: &BTreeSet<String>,
) -> Option<i128> {
    match arg {
        Expr::Name(name) if columns.contains(name.id.as_str()) => {
            integer_from_cell(row.get(name.id.as_str())?)
        }
        Expr::Call(call) if is_datetime_ctor(call) => {
            integer_from_table_arg(datetime_year_arg(call)?, row, columns)
        }
        _ => None,
    }
}

fn integer_from_cell(text: &str) -> Option<i128> {
    let trimmed = text.trim();
    if let Some(lit) = literal_value(trimmed) {
        return parse_decimal_i128(&lit);
    }
    datetime_year_from_source(trimmed)
}

fn datetime_year_from_source(text: &str) -> Option<i128> {
    let expr = parse_expr(text)?;
    match expr {
        Expr::Call(call) if is_datetime_ctor(&call) => {
            integer_literal_expr(datetime_year_arg(&call)?)
        }
        _ => None,
    }
}

fn is_datetime_ctor(call: &ast::ExprCall) -> bool {
    expr_full_name(&call.func).is_some_and(|name| {
        matches!(
            name.rsplit('.').next().unwrap_or(name.as_str()),
            "datetime" | "date"
        )
    })
}

fn datetime_year_arg(call: &ast::ExprCall) -> Option<&Expr> {
    call.keywords
        .iter()
        .find(|keyword| {
            keyword
                .arg
                .as_ref()
                .is_some_and(|name| name.as_str() == "year")
        })
        .map(|keyword| &keyword.value)
        .or_else(|| call.args.first())
}

fn integer_literal_expr(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Constant(constant) => constant_i128(&constant.value),
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::USub) => {
            integer_literal_expr(&unary.operand)?.checked_neg()
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd) => {
            integer_literal_expr(&unary.operand)
        }
        _ => None,
    }
}

fn eval_simple_arithmetic(expr: &Expr, env: &BTreeMap<String, i128>) -> Option<i128> {
    match expr {
        Expr::Name(name) => env.get(name.id.as_str()).copied(),
        Expr::Constant(constant) => constant_i128(&constant.value),
        Expr::Attribute(attribute) if matches!(attribute.attr.as_str(), "year" | "tm_year") => {
            eval_simple_arithmetic(&attribute.value, env)
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::UAdd) => {
            eval_simple_arithmetic(&unary.operand, env)
        }
        Expr::UnaryOp(unary) if matches!(unary.op, ast::UnaryOp::USub) => {
            eval_simple_arithmetic(&unary.operand, env)?.checked_neg()
        }
        Expr::BinOp(bin_op) => {
            let left = eval_simple_arithmetic(&bin_op.left, env)?;
            let right = eval_simple_arithmetic(&bin_op.right, env)?;
            eval_int_op(&bin_op.op, left, right)
        }
        _ => None,
    }
}

fn eval_int_op(op: &ast::Operator, left: i128, right: i128) -> Option<i128> {
    match op {
        ast::Operator::Add => left.checked_add(right),
        ast::Operator::Sub => left.checked_sub(right),
        ast::Operator::Mult => left.checked_mul(right),
        ast::Operator::Mod => python_mod(left, right),
        ast::Operator::FloorDiv => python_floor_div(left, right),
        ast::Operator::Pow => python_pow(left, right),
        _ => None,
    }
}

fn python_floor_div(left: i128, right: i128) -> Option<i128> {
    if right == 0 {
        return None;
    }
    let quot = left.checked_div(right)?;
    let rem = left.checked_rem(right)?;
    if rem != 0 && (left < 0) != (right < 0) {
        quot.checked_sub(1)
    } else {
        Some(quot)
    }
}

fn python_mod(left: i128, right: i128) -> Option<i128> {
    let quot = python_floor_div(left, right)?;
    left.checked_sub(quot.checked_mul(right)?)
}

fn python_pow(left: i128, right: i128) -> Option<i128> {
    let exp = u32::try_from(right).ok()?;
    left.checked_pow(exp)
}

fn constant_i128(value: &ast::Constant) -> Option<i128> {
    match value {
        ast::Constant::Int(number) => parse_decimal_i128(&number.to_str_radix(10)),
        _ => None,
    }
}

fn parse_decimal_i128(text: &str) -> Option<i128> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('.') {
        return None;
    }
    trimmed.parse().ok()
}

fn owner_names(owner: &PythonOwner, test: &PythonTest) -> Vec<String> {
    let mut names = vec![owner.name.clone()];
    names.extend(
        test.imports
            .iter()
            .filter(|import| import.imported == owner.name && import.alias != owner.name)
            .map(|import| import.alias.clone()),
    );
    names
}

fn assertion_stmts(gate_text: &str) -> Option<Vec<Stmt>> {
    let body = gate_text
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!("def _():\n{body}\n");
    let module = parse_module_result(Path::new("t.py"), &source).ok()?;
    match module {
        Mod::Module(module) => match module.body.into_iter().next()? {
            Stmt::FunctionDef(function) => Some(function.body),
            _ => None,
        },
        _ => None,
    }
}

fn owner_call_and_expected<'a>(
    stmts: &'a [Stmt],
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    stmts
        .iter()
        .find_map(|stmt| owner_call_and_expected_from_stmt(stmt, owner_names))
}

fn owner_call_and_expected_from_stmt<'a>(
    stmt: &'a Stmt,
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    match stmt {
        Stmt::Assert(assert_stmt) => match assert_stmt.test.as_ref() {
            Expr::Compare(compare)
                if compare.ops.len() == 1
                    && matches!(compare.ops[0], ast::CmpOp::Eq)
                    && compare.comparators.len() == 1 =>
            {
                sides(compare.left.as_ref(), &compare.comparators[0], owner_names)
            }
            _ => None,
        },
        Stmt::Expr(expr_stmt) => match expr_stmt.value.as_ref() {
            Expr::Call(call) if is_equality_assertion(call) => {
                let (first, second) = compared_operands(call)?;
                sides(first, second, owner_names)
            }
            _ => None,
        },
        _ => None,
    }
}

fn sides<'a>(
    left: &'a Expr,
    right: &'a Expr,
    owner_names: &[String],
) -> Option<(&'a Expr, &'a Expr)> {
    match (
        is_owner_call(left, owner_names),
        is_owner_call(right, owner_names),
    ) {
        (true, false) => Some((left, right)),
        (false, true) => Some((right, left)),
        _ => None,
    }
}

fn is_owner_call(expr: &Expr, owner_names: &[String]) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };
    expr_full_name(call.func.as_ref()).is_some_and(|name| {
        owner_names
            .iter()
            .any(|owner| name == *owner || name.ends_with(&format!(".{owner}")))
    })
}

fn is_equality_assertion(call: &ast::ExprCall) -> bool {
    expr_full_name(&call.func).is_some_and(|name| {
        matches!(
            name.rsplit('.').next().unwrap_or(name.as_str()),
            "assertEqual"
        )
    })
}

fn compared_operands(call: &ast::ExprCall) -> Option<(&Expr, &Expr)> {
    let keyword = |arg: &str| {
        call.keywords
            .iter()
            .find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == arg)
            })
            .map(|keyword| &keyword.value)
    };
    let mut positional = call.args.iter();
    let first = positional.next().or_else(|| keyword("first"))?;
    let second = positional.next().or_else(|| keyword("second"))?;
    Some((first, second))
}

fn expr_plain_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        _ => None,
    }
}

fn operand_column(
    owner_call: &Expr,
    operand: &str,
    owner: &PythonOwner,
    columns: &BTreeSet<String>,
) -> Option<String> {
    let Expr::Call(call) = owner_call else {
        return None;
    };
    if call
        .args
        .iter()
        .any(|argument| matches!(argument, Expr::Starred(_)))
        || call.keywords.iter().any(|keyword| keyword.arg.is_none())
    {
        return None;
    }
    let skip = receiver_skip(owner)?;
    for keyword in &call.keywords {
        if keyword
            .arg
            .as_ref()
            .is_some_and(|name| name.as_str() == operand)
            && let Some(name) = expr_plain_name(&keyword.value)
            && columns.contains(&name)
        {
            return Some(name);
        }
    }
    let positional: Vec<&str> = owner
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .skip(skip)
        .map(|parameter| parameter.name.as_str())
        .collect();
    for (parameter, argument) in positional.iter().zip(call.args.iter()) {
        if *parameter == operand
            && let Some(name) = expr_plain_name(argument)
            && columns.contains(&name)
        {
            return Some(name);
        }
    }
    None
}

fn receiver_skip(owner: &PythonOwner) -> Option<usize> {
    match owner.owner_kind {
        Some(OwnerKind::Method) => Some(1),
        Some(OwnerKind::ClassMethod) => Some(usize::from(
            !owner
                .decorators
                .iter()
                .any(|decorator| decorator.ends_with("staticmethod")),
        )),
        Some(OwnerKind::Function) => Some(0),
        _ => None,
    }
}

fn unvaried_column_value(cases: &[BTreeMap<String, String>], column: &str) -> Option<String> {
    if cases.is_empty() {
        return None;
    }
    let mut values = Vec::with_capacity(cases.len());
    for case in cases {
        let raw = case.get(column)?;
        values.push(literal_value(raw)?);
    }
    let first = values.first()?.clone();
    values
        .iter()
        .all(|value| literals_equal(value, &first) == Some(true))
        .then_some(first)
}

fn loop_table_cases(source: &str) -> Option<PythonParametrizeCases> {
    let module = parse_module_result(Path::new("t.py"), source).ok()?;
    let stmts = match module {
        Mod::Module(module) => module.body,
        _ => return None,
    };
    let mut tables = Vec::new();
    match stmts.as_slice() {
        [Stmt::FunctionDef(function)] => walk_tables(&function.body, source, &mut tables),
        [Stmt::AsyncFunctionDef(function)] => walk_tables(&function.body, source, &mut tables),
        _ => walk_tables(&stmts, source, &mut tables),
    }
    (tables.len() == 1).then(|| tables.remove(0))
}

fn walk_tables(stmts: &[Stmt], source: &str, out: &mut Vec<PythonParametrizeCases>) {
    for stmt in stmts {
        match stmt {
            Stmt::For(for_stmt) => {
                if let Some(table) = table_from_for(&for_stmt.target, &for_stmt.iter, source) {
                    out.push(table);
                }
                walk_tables(&for_stmt.body, source, out);
                walk_tables(&for_stmt.orelse, source, out);
            }
            Stmt::AsyncFor(for_stmt) => {
                if let Some(table) = table_from_for(&for_stmt.target, &for_stmt.iter, source) {
                    out.push(table);
                }
                walk_tables(&for_stmt.body, source, out);
                walk_tables(&for_stmt.orelse, source, out);
            }
            Stmt::With(with_stmt) => walk_tables(&with_stmt.body, source, out),
            Stmt::AsyncWith(with_stmt) => walk_tables(&with_stmt.body, source, out),
            Stmt::If(if_stmt) => {
                walk_tables(&if_stmt.body, source, out);
                walk_tables(&if_stmt.orelse, source, out);
            }
            Stmt::While(while_stmt) => {
                walk_tables(&while_stmt.body, source, out);
                walk_tables(&while_stmt.orelse, source, out);
            }
            Stmt::Try(try_stmt) => {
                walk_tables(&try_stmt.body, source, out);
                walk_tables(&try_stmt.orelse, source, out);
                walk_tables(&try_stmt.finalbody, source, out);
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_tables(&handler.body, source, out);
                }
            }
            Stmt::TryStar(try_stmt) => {
                walk_tables(&try_stmt.body, source, out);
                walk_tables(&try_stmt.orelse, source, out);
                walk_tables(&try_stmt.finalbody, source, out);
                for handler in &try_stmt.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_tables(&handler.body, source, out);
                }
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    walk_tables(&case.body, source, out);
                }
            }
            _ => {}
        }
    }
}

fn table_from_for(target: &Expr, iter: &Expr, source: &str) -> Option<PythonParametrizeCases> {
    let names = for_target_names(target)?;
    if names.is_empty() || names.len() > 8 {
        return None;
    }
    let rows = match iter {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    if rows.is_empty() || rows.len() > MAX_TABLE_ROWS {
        return None;
    }
    let bare_single = names.len() == 1;
    let mut cases = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row_cells(row, names.len(), bare_single)?;
        let mut case = BTreeMap::new();
        for (name, value) in names.iter().zip(values) {
            let text = text_for_range(source, value.range()).trim().to_string();
            if text.is_empty() {
                return None;
            }
            case.insert(name.clone(), text);
        }
        cases.push(case);
    }
    Some(PythonParametrizeCases { cases })
}

fn for_target_names(target: &Expr) -> Option<Vec<String>> {
    match target {
        Expr::Name(name) => Some(vec![name.id.to_string()]),
        Expr::Tuple(tuple) => names_from_elements(&tuple.elts),
        Expr::List(list) => names_from_elements(&list.elts),
        _ => None,
    }
}

fn names_from_elements(elements: &[Expr]) -> Option<Vec<String>> {
    elements
        .iter()
        .map(|element| match element {
            Expr::Name(name) => Some(name.id.to_string()),
            _ => None,
        })
        .collect()
}

fn row_cells(row: &Expr, arity: usize, bare_single: bool) -> Option<Vec<&Expr>> {
    if bare_single {
        return (!matches!(row, Expr::Starred(_))).then(|| vec![row]);
    }
    let elements = match row {
        Expr::Tuple(tuple) => &tuple.elts,
        Expr::List(list) => &list.elts,
        _ => return None,
    };
    if elements
        .iter()
        .any(|element| matches!(element, Expr::Starred(_)))
    {
        return None;
    }
    (elements.len() == arity).then(|| elements.iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_swap_numeric_reader_preserves_primitive_boundaries() -> Result<(), String> {
        for source in ["True", "'3'", "float('nan')", "1e999", "int(3)", "custom"] {
            let expr = parse_expr(source).ok_or_else(|| format!("parse {source}"))?;
            assert!(numeric_factor_literal(&expr).is_none(), "{source}");
        }
        let minimum =
            parse_expr("-170141183460469231731687303715884105728").ok_or("signed minimum")?;
        assert!(matches!(
            numeric_factor_literal(&minimum),
            Some(NumericFactor::Integer(i128::MIN))
        ));
        assert!(!primitive_product_is_finite(
            NumericFactor::Float(1e308),
            NumericFactor::Float(1e308)
        ));
        assert!(!primitive_product_is_finite(
            NumericFactor::Integer(i128::from(i32::MAX) + 1),
            NumericFactor::Float(1.0)
        ));
        for (old, new) in [
            ("a * -0.0", "0.0 * a"),
            ("a * 3", "3.0 * a"),
            ("(a * b) * c", "a * (b * c)"),
        ] {
            let old = parse_expr(old).ok_or("old expression")?;
            let new = parse_expr(new).ok_or("new expression")?;
            assert!(!swaps_factors(&old, &new));
        }
        Ok(())
    }

    fn cases(source: &str) -> Option<Vec<BTreeMap<String, String>>> {
        loop_table_cases(source).map(|table| table.cases)
    }

    #[test]
    fn subtest_table_records_unvaried_default_column() -> Result<(), String> {
        let source = "def test_default(self):\n    for iterable, default in [\n        ([], None),\n        (iter([]), None),\n    ]:\n        with self.subTest(iterable=iterable, default=default):\n            self.assertEqual(last(iterable, default=default), default)\n";
        let table = cases(source).ok_or_else(|| "subTest table should parse".to_string())?;
        if table.len() != 2 {
            return Err(format!("expected 2 rows, got {}", table.len()));
        }
        let first = table
            .first()
            .ok_or_else(|| "first row".to_string())?
            .get("default")
            .map(String::as_str);
        let second = table
            .get(1)
            .ok_or_else(|| "second row".to_string())?
            .get("default")
            .map(String::as_str);
        if first != Some("None") || second != Some("None") {
            return Err(format!("default column: {first:?}, {second:?}"));
        }
        if unvaried_column_value(&table, "default").as_deref() != Some("None") {
            return Err("default column should be unvaried None".to_string());
        }
        if unvaried_column_value(&table, "iterable").is_some() {
            return Err("iterable column is not a scalar literal".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_row_that_varies_default_is_not_unvaried() -> Result<(), String> {
        let source = "def test_default(self):\n    for iterable, default in [([], None), (iter([]), 0)]:\n        with self.subTest(iterable=iterable):\n            self.assertEqual(last(iterable, default=default), default)\n";
        let table = cases(source).ok_or_else(|| "varied table should parse".to_string())?;
        if unvaried_column_value(&table, "default").is_some() {
            return Err("varied default column must not look unvaried".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_loop_over_a_variable_records_nothing() {
        let source = "def test_default(self):\n    for iterable, default in rows:\n        self.assertEqual(last(iterable, default), default)\n";
        assert!(cases(source).is_none());
    }

    #[test]
    fn twentyxx_years_agree_on_percent_100_vs_1000() -> Result<(), String> {
        let env_2015 = BTreeMap::from([("year".to_string(), 2015_i128)]);
        let env_1999 = BTreeMap::from([("year".to_string(), 1999_i128)]);
        let old = parse_expr("year % 100").ok_or_else(|| "old arith".to_string())?;
        let new = parse_expr("year % 1000").ok_or_else(|| "new arith".to_string())?;
        if eval_simple_arithmetic(&old, &env_2015) != eval_simple_arithmetic(&new, &env_2015) {
            return Err("2015 % 100 and % 1000 should agree".to_string());
        }
        if eval_simple_arithmetic(&old, &env_1999) == eval_simple_arithmetic(&new, &env_1999) {
            return Err("1999 % 100 and % 1000 should differ".to_string());
        }
        Ok(())
    }

    #[test]
    fn datetime_cell_extracts_the_year_literal() {
        assert_eq!(integer_from_cell("datetime(2015, 1, 1)"), Some(2015));
        assert_eq!(
            integer_from_cell("datetime.datetime(1999, 12, 31)"),
            Some(1999)
        );
        assert_eq!(integer_from_cell("years"), None);
    }

    #[test]
    fn python_mod_follows_sign_of_divisor() {
        assert_eq!(python_mod(7, 3), Some(1));
        assert_eq!(python_mod(-7, 3), Some(2));
        assert_eq!(python_mod(7, -3), Some(-2));
        assert_eq!(python_mod(2015, 100), Some(15));
        assert_eq!(python_mod(2015, 1000), Some(15));
        assert_eq!(python_mod(1999, 100), Some(99));
        assert_eq!(python_mod(1999, 1000), Some(999));
        assert_eq!(python_mod(1, 0), None);
    }
}

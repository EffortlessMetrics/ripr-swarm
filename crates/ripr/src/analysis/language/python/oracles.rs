use super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::{PythonAssertion, PythonOracleShape, expr_full_name};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;

pub(super) fn collect_assertions_from_statements(
    statements: &[Stmt],
    source: &SourceText<'_>,
) -> Vec<PythonAssertion> {
    let mut out = Vec::new();
    collect_assertions(statements, source, &mut out);
    out
}

fn collect_assertions(
    statements: &[Stmt],
    source: &SourceText<'_>,
    out: &mut Vec<PythonAssertion>,
) {
    for stmt in statements {
        match stmt {
            Stmt::Assert(assert_stmt) => {
                out.push(assertion_from_assert(assert_stmt, source));
            }
            Stmt::Expr(expr_stmt) => {
                if let Some(assertion) = assertion_from_expr(expr_stmt.value.as_ref(), source) {
                    out.push(assertion);
                }
            }
            Stmt::If(if_stmt) => {
                collect_assertions(&if_stmt.body, source, out);
                collect_assertions(&if_stmt.orelse, source, out);
            }
            Stmt::For(for_stmt) => {
                collect_assertions(&for_stmt.body, source, out);
                collect_assertions(&for_stmt.orelse, source, out);
            }
            Stmt::AsyncFor(for_stmt) => {
                collect_assertions(&for_stmt.body, source, out);
                collect_assertions(&for_stmt.orelse, source, out);
            }
            Stmt::While(while_stmt) => {
                collect_assertions(&while_stmt.body, source, out);
                collect_assertions(&while_stmt.orelse, source, out);
            }
            Stmt::With(with_stmt) => {
                collect_with_item_assertions(&with_stmt.items, &with_stmt.body, source, out);
                collect_assertions(&with_stmt.body, source, out);
            }
            Stmt::AsyncWith(with_stmt) => {
                collect_with_item_assertions(&with_stmt.items, &with_stmt.body, source, out);
                collect_assertions(&with_stmt.body, source, out);
            }
            Stmt::Try(try_stmt) => {
                collect_assertions(&try_stmt.body, source, out);
                collect_except_handler_assertions(&try_stmt.handlers, source, out);
                collect_assertions(&try_stmt.orelse, source, out);
                collect_assertions(&try_stmt.finalbody, source, out);
            }
            Stmt::TryStar(try_stmt) => {
                collect_assertions(&try_stmt.body, source, out);
                collect_except_handler_assertions(&try_stmt.handlers, source, out);
                collect_assertions(&try_stmt.orelse, source, out);
                collect_assertions(&try_stmt.finalbody, source, out);
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
                    collect_assertions(&case.body, source, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_with_item_assertions(
    items: &[ast::WithItem],
    body: &[Stmt],
    source: &SourceText<'_>,
    out: &mut Vec<PythonAssertion>,
) {
    for item in items {
        if let Some(mut assertion) = assertion_from_expr(&item.context_expr, source) {
            // An exception assertion compares no operand: what it observes is
            // the call its `with` body makes (RIPR-SPEC-0233 rule 4), so the
            // gates read the item together with that body.
            if assertion.oracle_shape == PythonOracleShape::ExceptionAssertion
                && let (Some(first), Some(last)) = (body.first(), body.last())
            {
                let body_range = TextRange::new(first.range().start(), last.range().end());
                assertion.gate_text = format!(
                    "{}\n{}",
                    assertion.gate_text,
                    text_for_range(source, body_range).trim()
                );
            }
            out.push(assertion);
        }
    }
}

fn collect_except_handler_assertions(
    handlers: &[ast::ExceptHandler],
    source: &SourceText<'_>,
    out: &mut Vec<PythonAssertion>,
) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        collect_assertions(&handler.body, source, out);
    }
}

fn assertion_from_assert(
    assert_stmt: &ast::StmtAssert,
    source: &SourceText<'_>,
) -> PythonAssertion {
    let (oracle_kind, oracle_strength, oracle_shape) =
        oracle_for_assert_expr(assert_stmt.test.as_ref(), source);
    PythonAssertion {
        text: text_for_range(source, assert_stmt.range).trim().to_string(),
        // The failure message observes nothing (RIPR-SPEC-0233 rule 4).
        gate_text: format!(
            "assert {}",
            assert_gate_expr(assert_stmt.test.as_ref(), source)
        ),
        line: line_for_range_start(source, assert_stmt.range),
        oracle_kind,
        oracle_strength,
        oracle_shape,
    }
}

/// The text of an `assert` expression the gates read. An `and` chain is
/// credited by its strongest conjunct (rule 15), so only the conjuncts of that
/// strength are kept: an owner named only in a weaker conjunct
/// (`fee(10) > 0 and other == 5`) is not observed by the strong one.
fn assert_gate_expr(expr: &Expr, source: &SourceText<'_>) -> String {
    let Expr::BoolOp(bool_op) = expr else {
        return text_for_range(source, expr.range()).trim().to_string();
    };
    if bool_op.op != ast::BoolOp::And {
        return text_for_range(source, expr.range()).trim().to_string();
    }
    let strength = |value: &Expr| oracle_for_assert_expr(value, source).1.rank();
    let strongest = bool_op
        .values
        .iter()
        .map(strength)
        .max()
        .unwrap_or_default();
    bool_op
        .values
        .iter()
        .filter(|value| strength(value) == strongest)
        .map(|value| assert_gate_expr(value, source))
        .collect::<Vec<_>>()
        .join(" and ")
}

fn assertion_from_expr(expr: &Expr, source: &SourceText<'_>) -> Option<PythonAssertion> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let (oracle_kind, oracle_strength, oracle_shape) = oracle_for_call(call, source)?;
    let text = text_for_range(source, call.range).trim().to_string();
    let gate_text = compared_operands(call)
        .map(|(first, second)| {
            format!(
                "{}({}, {})",
                text_for_range(source, call.func.range()).trim(),
                text_for_range(source, first.range()).trim(),
                text_for_range(source, second.range()).trim()
            )
        })
        .unwrap_or_else(|| text.clone());
    Some(PythonAssertion {
        text,
        gate_text,
        line: line_for_range_start(source, call.range),
        oracle_kind,
        oracle_strength,
        oracle_shape,
    })
}

fn oracle_for_assert_expr(
    expr: &Expr,
    source: &str,
) -> (OracleKind, OracleStrength, PythonOracleShape) {
    match expr {
        Expr::Compare(compare) => oracle_for_compare(compare, source),
        // `assert a and b` fails when any conjunct is false, so it asserts each
        // conjunct; the strongest one is the assertion's oracle. `or` and
        // other boolean shapes stay smoke.
        Expr::BoolOp(bool_op) if bool_op.op == ast::BoolOp::And => bool_op
            .values
            .iter()
            .map(|value| oracle_for_assert_expr(value, source))
            .max_by_key(|(_, strength, _)| strength.rank())
            .unwrap_or((
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
                PythonOracleShape::BroadSmokeAssertion,
            )),
        Expr::Call(call) => {
            if expr_full_name(call.func.as_ref()).is_some_and(|name| name == "isinstance") {
                (
                    OracleKind::RelationalCheck,
                    OracleStrength::Weak,
                    PythonOracleShape::BoundaryAssertion,
                )
            } else {
                oracle_for_call(call, source).unwrap_or((
                    OracleKind::SmokeOnly,
                    OracleStrength::Smoke,
                    PythonOracleShape::BroadSmokeAssertion,
                ))
            }
        }
        _ => (
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
            PythonOracleShape::BroadSmokeAssertion,
        ),
    }
}

fn oracle_for_compare(
    compare: &ast::ExprCompare,
    source: &str,
) -> (OracleKind, OracleStrength, PythonOracleShape) {
    let has_exact = compare.ops.iter().any(|op| matches!(op, ast::CmpOp::Eq));
    // RIPR-SPEC-0233 rule 1: `==` pins a value only when every operator in the
    // chain is `==` and the compared operands are not one token sequence
    // (`owner(1) == owner(1)` runs the changed code on both sides).
    let all_exact = compare.ops.iter().all(|op| matches!(op, ast::CmpOp::Eq));
    let tautology = std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| same_tokens(source, pair[0], pair[1]));
    let (kind, strength) = if has_exact && all_exact && !tautology {
        (OracleKind::ExactValue, OracleStrength::Strong)
    } else {
        (OracleKind::RelationalCheck, OracleStrength::Weak)
    };
    let shape = if compare_observes_output(compare) {
        PythonOracleShape::OutputAssertion
    } else if compare_observes_status_code(compare) {
        PythonOracleShape::StatusCodeAssertion
    } else if compare_observes_field(compare) {
        PythonOracleShape::FieldAssertion
    } else if compare.ops.iter().any(|op| {
        matches!(
            op,
            ast::CmpOp::Lt | ast::CmpOp::LtE | ast::CmpOp::Gt | ast::CmpOp::GtE
        )
    }) {
        PythonOracleShape::BoundaryAssertion
    } else if has_exact {
        PythonOracleShape::ExactAssertion
    } else {
        PythonOracleShape::BoundaryAssertion
    };
    (kind, strength, shape)
}

fn compare_observes_output(compare: &ast::ExprCompare) -> bool {
    expr_observes_output(compare.left.as_ref())
        || compare.comparators.iter().any(expr_observes_output)
}

fn compare_observes_status_code(compare: &ast::ExprCompare) -> bool {
    expr_observes_status_code(compare.left.as_ref())
        || compare.comparators.iter().any(expr_observes_status_code)
}

fn compare_observes_field(compare: &ast::ExprCompare) -> bool {
    expr_observes_field(compare.left.as_ref())
        || compare.comparators.iter().any(expr_observes_field)
}

fn expr_observes_output(expr: &Expr) -> bool {
    expr_full_name(expr).is_some_and(|name| {
        name == "caplog.text"
            || name == "capsys.readouterr.out"
            || name.ends_with(".output")
            || name.ends_with(".stdout")
            || name.ends_with(".stderr")
            || name.ends_with(".text")
    }) || match expr {
        Expr::Call(call) => {
            expr_full_name(call.func.as_ref()).is_some_and(|name| name == "capsys.readouterr")
                || call.args.iter().any(expr_observes_output)
                || call
                    .keywords
                    .iter()
                    .any(|keyword| expr_observes_output(&keyword.value))
        }
        Expr::Attribute(attribute) => expr_observes_output(attribute.value.as_ref()),
        Expr::Subscript(subscript) => {
            expr_observes_output(subscript.value.as_ref())
                || expr_observes_output(subscript.slice.as_ref())
        }
        Expr::BoolOp(bool_op) => bool_op.values.iter().any(expr_observes_output),
        _ => false,
    }
}

fn expr_observes_status_code(expr: &Expr) -> bool {
    expr_full_name(expr).is_some_and(|name| {
        name.ends_with(".status_code") || name.ends_with(".status") || name.ends_with(".exit_code")
    })
}

fn expr_observes_field(expr: &Expr) -> bool {
    match expr {
        Expr::Attribute(attribute) => {
            !expr_observes_status_code(expr)
                && !expr_observes_output(expr)
                && !expr_observes_output(attribute.value.as_ref())
        }
        Expr::Subscript(_) => true,
        Expr::Call(call) => {
            call.args.iter().any(expr_observes_field)
                || call
                    .keywords
                    .iter()
                    .any(|keyword| expr_observes_field(&keyword.value))
        }
        Expr::BoolOp(bool_op) => bool_op.values.iter().any(expr_observes_field),
        _ => false,
    }
}

fn oracle_for_call(
    call: &ast::ExprCall,
    source: &str,
) -> Option<(OracleKind, OracleStrength, PythonOracleShape)> {
    let name = expr_full_name(call.func.as_ref())?;
    let last_segment = name.rsplit('.').next().unwrap_or(name.as_str());
    if matches!(last_segment, "assertEqual" | "assertDictEqual")
        && compared_operands(call).is_some_and(|(first, second)| same_tokens(source, first, second))
    {
        // RIPR-SPEC-0233 rule 1: comparing an expression with itself pins nothing.
        return Some((
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            oracle_shape_for_call_arguments(call, PythonOracleShape::BoundaryAssertion),
        ));
    }
    match last_segment {
        "assertEqual" => Some((
            OracleKind::ExactValue,
            OracleStrength::Strong,
            oracle_shape_for_call_arguments(call, PythonOracleShape::ExactAssertion),
        )),
        "assertDictEqual" => Some((
            OracleKind::ExactValue,
            OracleStrength::Strong,
            oracle_shape_for_call_arguments(call, PythonOracleShape::FieldAssertion),
        )),
        "assertIn" | "assertRegex" => Some((
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            oracle_shape_for_call_arguments(call, PythonOracleShape::FieldAssertion),
        )),
        "assertNotEqual" => Some((
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            oracle_shape_for_call_arguments(call, PythonOracleShape::BoundaryAssertion),
        )),
        "assertTrue" | "assertFalse" => Some((
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
            PythonOracleShape::BroadSmokeAssertion,
        )),
        "assertRaisesRegex"
            if call
                .args
                .get(1)
                .or_else(|| keyword_value(call, "expected_regex"))
                .is_some_and(|pattern| is_trivial_message_pattern(pattern, source)) =>
        {
            Some((
                OracleKind::BroadError,
                OracleStrength::Weak,
                PythonOracleShape::ExceptionAssertion,
            ))
        }
        "assertRaisesRegex" => Some((
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
            PythonOracleShape::ExceptionAssertion,
        )),
        "assertRaises" => Some((
            OracleKind::BroadError,
            OracleStrength::Weak,
            PythonOracleShape::ExceptionAssertion,
        )),
        "raises" if name == "pytest.raises" || name == "raises" => {
            let pattern = keyword_value(call, "match");
            if pattern.is_some_and(|pattern| !is_trivial_message_pattern(pattern, source)) {
                Some((
                    OracleKind::ExactErrorVariant,
                    OracleStrength::Strong,
                    PythonOracleShape::ExceptionAssertion,
                ))
            } else {
                Some((
                    OracleKind::BroadError,
                    OracleStrength::Weak,
                    PythonOracleShape::ExceptionAssertion,
                ))
            }
        }
        "assert_called"
        | "assert_called_once"
        | "assert_called_with"
        | "assert_called_once_with"
        | "assert_any_call"
        | "assert_has_calls"
        | "assert_not_called" => Some((
            OracleKind::MockExpectation,
            OracleStrength::Medium,
            PythonOracleShape::MockExpectation,
        )),
        _ if looks_like_custom_assertion_helper(&name) => Some((
            OracleKind::Unknown,
            OracleStrength::Unknown,
            PythonOracleShape::UnknownCustomHelper,
        )),
        _ => None,
    }
}

/// The two operands a comparison assertion call compares, positional or named
/// (`first=`, `second=`); never its `msg`.
fn compared_operands(call: &ast::ExprCall) -> Option<(&Expr, &Expr)> {
    let name = expr_full_name(call.func.as_ref())?;
    let last_segment = name.rsplit('.').next().unwrap_or(name.as_str());
    if !matches!(
        last_segment,
        "assertEqual" | "assertDictEqual" | "assertNotEqual" | "assertIn"
    ) {
        return None;
    }
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

/// Whether two expressions are the same token sequence, ignoring whitespace
/// outside string literals (RIPR-SPEC-0233 rule 1).
fn same_tokens(source: &str, left: &Expr, right: &Expr) -> bool {
    let left = text_for_range(source, left.range());
    let right = text_for_range(source, right.range());
    !left.is_empty() && code_without_spacing(&left) == code_without_spacing(&right)
}

fn code_without_spacing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in text.chars() {
        match quote {
            Some(open) => {
                out.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == open {
                    quote = None;
                }
            }
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                out.push(ch);
            }
            None if ch.is_whitespace() => {}
            None => out.push(ch),
        }
    }
    out
}

fn keyword_value<'a>(call: &'a ast::ExprCall, name: &str) -> Option<&'a Expr> {
    call.keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
        .map(|keyword| &keyword.value)
}

/// RIPR-SPEC-0233 rule 2: a literal message pattern that admits any message
/// (`''`, `.*`, `.+`, `^`, `$`, `^.*$`, `(?s).*`), or an alternation that can
/// admit both the old and the new message (`'empty|blank'`), pins only the
/// exception class. A bare name reads as the string literal it is bound to
/// when the file assigns that name exactly once, to a literal (Decision 5).
fn is_trivial_message_pattern(pattern: &Expr, source: &str) -> bool {
    let text = match pattern {
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::Str(text),
            ..
        }) => text.as_str(),
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::Bytes(bytes),
            ..
        }) => return std::str::from_utf8(bytes).is_ok_and(pattern_admits_any_message),
        Expr::Name(name) => {
            return name_bound_once_to_literal(source, name.id.as_str())
                .is_some_and(|text| pattern_admits_any_message(&text));
        }
        _ => return false,
    };
    pattern_admits_any_message(text)
}

/// The string literal `name` is bound to when the file has exactly one
/// assignment to it (`name = '.*'`) and that assignment is a plain literal.
fn name_bound_once_to_literal(source: &str, name: &str) -> Option<String> {
    let mut assignments = source.lines().filter_map(|line| {
        let (target, value) = line.split_once('=')?;
        let value = value.strip_prefix('=').map_or(Some(value), |_| None)?;
        (target.trim() == name).then(|| value.trim())
    });
    let value = assignments.next()?;
    if assignments.next().is_some() {
        return None;
    }
    let value = value.strip_prefix('r').unwrap_or(value);
    let quote = value.chars().next().filter(|ch| matches!(ch, '\'' | '"'))?;
    let inner = value.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.contains(quote)).then(|| inner.to_string())
}

fn pattern_admits_any_message(text: &str) -> bool {
    if matches!(text, "" | ".*" | ".+" | "^" | "$" | "^.*$" | "(?s).*") {
        return true;
    }
    let mut escaped = false;
    let mut in_class = false;
    for ch in text.chars() {
        match ch {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '[' => in_class = true,
            ']' => in_class = false,
            '|' if !in_class => return true,
            _ => {}
        }
    }
    false
}

fn oracle_shape_for_call_arguments(
    call: &ast::ExprCall,
    fallback: PythonOracleShape,
) -> PythonOracleShape {
    if call.args.iter().any(expr_observes_output)
        || call
            .keywords
            .iter()
            .any(|keyword| expr_observes_output(&keyword.value))
    {
        PythonOracleShape::OutputAssertion
    } else if call.args.iter().any(expr_observes_status_code)
        || call
            .keywords
            .iter()
            .any(|keyword| expr_observes_status_code(&keyword.value))
    {
        PythonOracleShape::StatusCodeAssertion
    } else if call.args.iter().any(expr_observes_field)
        || call
            .keywords
            .iter()
            .any(|keyword| expr_observes_field(&keyword.value))
    {
        PythonOracleShape::FieldAssertion
    } else {
        fallback
    }
}

fn looks_like_custom_assertion_helper(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .is_some_and(|segment| segment.starts_with("assert_") || segment == "assert_that")
}

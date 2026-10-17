use super::source_utils::{SourceText, line_for_range_start, text_for_range};
use super::{PythonAssertion, PythonOracleShape, expr_full_name};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
pub(super) mod unittest_list;

pub(super) fn collect_assertions_from_statements(
    statements: &[Stmt],
    source: &SourceText<'_>,
    standard_unittest_list: bool,
) -> Vec<PythonAssertion> {
    let mut out = Vec::new();
    collect_assertions(statements, source, &mut out);
    if standard_unittest_list
        && let [Stmt::Expr(expr)] = statements
        && let Expr::Call(call) = expr.value.as_ref()
        && unittest_list::literal_call(call)
        && let Some(assertion) = out.first_mut()
    {
        assertion.oracle_kind = OracleKind::ExactValue;
        assertion.oracle_strength = OracleStrength::Strong;
        assertion.oracle_shape = PythonOracleShape::ExactAssertion;
    }
    out
}

fn collect_assertions(
    statements: &[Stmt],
    source: &SourceText<'_>,
    out: &mut Vec<PythonAssertion>,
) {
    for (index, stmt) in statements.iter().enumerate() {
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
                let after = &statements[index + 1..];
                collect_with_item_assertions(&with_stmt.items, &with_stmt.body, after, source, out);
                collect_assertions(&with_stmt.body, source, out);
            }
            Stmt::AsyncWith(with_stmt) => {
                let after = &statements[index + 1..];
                collect_with_item_assertions(&with_stmt.items, &with_stmt.body, after, source, out);
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
    after: &[Stmt],
    source: &SourceText<'_>,
    out: &mut Vec<PythonAssertion>,
) {
    for item in items {
        if let Some(mut assertion) = assertion_from_expr(&item.context_expr, source) {
            // An exception assertion compares no operand: what it observes is
            // the call its `with` body makes (RIPR-SPEC-0238 rule 4), so the
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
                // Rule 14: the raised error compared by value after the `with`
                // pins it; the comparison joins the item's gate text.
                if let Some(comparison) = bound_exception_comparison(item, after, source) {
                    assertion.oracle_kind = OracleKind::ExactErrorVariant;
                    assertion.oracle_strength = OracleStrength::Strong;
                    assertion.gate_text = format!("{}\n{comparison}", assertion.gate_text);
                }
            }
            out.push(assertion);
        }
    }
}

/// RIPR-SPEC-0238 rule 14: the text of the first statement after a
/// `raises(...) as N` / `assertRaises(...) as N` item that compares a
/// value-preserving read of the raised error with `==`, when nothing in
/// between rebinds or mutates `N`.
fn bound_exception_comparison(
    item: &ast::WithItem,
    after: &[Stmt],
    source: &SourceText<'_>,
) -> Option<String> {
    let Some(Expr::Name(bound)) = item.optional_vars.as_deref() else {
        return None;
    };
    let bound = bound.id.as_str();
    let Expr::Call(call) = &item.context_expr else {
        return None;
    };
    let name = expr_full_name(call.func.as_ref())?;
    let field = match name.as_str() {
        "self.assertRaises" | "self.assertRaisesRegex" => "exception",
        "pytest.raises" | "raises" => "value",
        _ => return None,
    };
    for stmt in after {
        if let Some((left, right)) = equality_operands(stmt) {
            // One operand reads the error; the other must not, or the
            // comparison can hold for any message (`str(e.value) ==
            // str(e.value.args[0])`), as a tautology does (rule 1).
            let pinned = |read: &Expr, other: &Expr| {
                reads_raised_error(read, bound, field) && !mentions_name(source, other, bound)
            };
            if !same_tokens(source, left, right) && (pinned(left, right) || pinned(right, left)) {
                // The failure message observes nothing (rule 4).
                return Some(format!(
                    "assert {} == {}",
                    text_for_range(source, left.range()).trim(),
                    text_for_range(source, right.range()).trim()
                ));
            }
        }
        if statement_touches_name(stmt, bound, source) {
            return None;
        }
    }
    None
}

/// The two operands of `assert A == B` or `self.assertEqual(A, B)`.
fn equality_operands(stmt: &Stmt) -> Option<(&Expr, &Expr)> {
    match stmt {
        Stmt::Assert(assert_stmt) => match assert_stmt.test.as_ref() {
            Expr::Compare(compare)
                if compare.ops.len() == 1 && matches!(compare.ops[0], ast::CmpOp::Eq) =>
            {
                Some((compare.left.as_ref(), compare.comparators.first()?))
            }
            _ => None,
        },
        Stmt::Expr(expr_stmt) => match expr_stmt.value.as_ref() {
            Expr::Call(call)
                if expr_full_name(call.func.as_ref())
                    .is_some_and(|name| name.rsplit('.').next() == Some("assertEqual")) =>
            {
                compared_operands(call)
            }
            _ => None,
        },
        _ => None,
    }
}

/// `N.value` (or `N.exception`) alone, as `str(...)` or `repr(...)` of it, or
/// its `.args` or a literal index of `.args`.
fn reads_raised_error(expr: &Expr, bound: &str, field: &str) -> bool {
    let is_error = |expr: &Expr| {
        matches!(expr, Expr::Attribute(attribute)
            if attribute.attr.as_str() == field
                && matches!(attribute.value.as_ref(), Expr::Name(name) if name.id.as_str() == bound))
    };
    let is_args = |expr: &Expr| {
        matches!(expr, Expr::Attribute(attribute)
            if attribute.attr.as_str() == "args" && is_error(attribute.value.as_ref()))
    };
    match expr {
        Expr::Call(call) => {
            matches!(call.func.as_ref(), Expr::Name(name) if matches!(name.id.as_str(), "str" | "repr"))
                && call.keywords.is_empty()
                && call.args.len() == 1
                && is_error(&call.args[0])
        }
        Expr::Subscript(subscript) => {
            is_args(subscript.value.as_ref())
                && matches!(
                    subscript.slice.as_ref(),
                    Expr::Constant(ast::ExprConstant {
                        value: ast::Constant::Int(_),
                        ..
                    })
                )
        }
        _ => is_error(expr) || is_args(expr),
    }
}

/// Whether a statement assigns `name`, or an attribute or subscript reached
/// through it (directly or inside a tuple, list or starred target, or by a
/// walrus), or calls a method on it or on such an attribute, or passes it to
/// a call (`setattr(exc.value, ...)`).
fn statement_touches_name(stmt: &Stmt, name: &str, source: &SourceText<'_>) -> bool {
    if text_for_range(source, stmt.range()).contains(":=") {
        return true;
    }
    let rooted = |expr: &Expr| target_rooted_at(expr, name);
    // An assigned value that reads the error can alias it
    // (`msg = str(N.value)` then `assert str(N.value) == msg`).
    let reads = |value: &Expr| mentions_name(source, value, name);
    match stmt {
        Stmt::Assign(assign) => assign.targets.iter().any(rooted) || reads(&assign.value),
        Stmt::AugAssign(assign) => rooted(&assign.target) || reads(&assign.value),
        Stmt::AnnAssign(assign) => {
            rooted(&assign.target) || assign.value.as_deref().is_some_and(reads)
        }
        Stmt::Delete(delete) => delete.targets.iter().any(rooted),
        Stmt::Expr(expr_stmt) => calls_into(expr_stmt.value.as_ref(), name),
        Stmt::Assert(assert_stmt) => calls_into(assert_stmt.test.as_ref(), name),
        Stmt::Pass(_) => false,
        // A nested block may rebind the name in a way the scan does not follow.
        _ => true,
    }
}

fn target_rooted_at(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Tuple(tuple) => tuple.elts.iter().any(|elt| target_rooted_at(elt, name)),
        Expr::List(list) => list.elts.iter().any(|elt| target_rooted_at(elt, name)),
        Expr::Starred(starred) => target_rooted_at(starred.value.as_ref(), name),
        _ => expr_root_name(expr) == Some(name),
    }
}

/// Whether `expr` holds a method call on `name` (or on an attribute reached
/// through it), or a call other than a pure read (`str`, `repr`, `len`,
/// `type`, `isinstance`) passed such a value. Reads
/// through the expression shapes an `assert` usually holds; anything else
/// counts as touching, which only withholds credit.
fn calls_into(expr: &Expr, name: &str) -> bool {
    let recurse = |expr: &Expr| calls_into(expr, name);
    match expr {
        Expr::Call(call) => {
            matches!(call.func.as_ref(), Expr::Attribute(attribute)
                if expr_root_name(attribute.value.as_ref()) == Some(name))
                || (!matches!(call.func.as_ref(), Expr::Name(callee)
                        if matches!(callee.id.as_str(), "str" | "repr" | "len" | "type" | "isinstance"))
                    && call
                        .args
                        .iter()
                        .chain(call.keywords.iter().map(|keyword| &keyword.value))
                        .any(|argument| expr_root_name(argument) == Some(name)))
                || recurse(call.func.as_ref())
                || call.args.iter().any(recurse)
                || call.keywords.iter().any(|keyword| recurse(&keyword.value))
        }
        Expr::Compare(compare) => {
            recurse(compare.left.as_ref()) || compare.comparators.iter().any(recurse)
        }
        Expr::BoolOp(bool_op) => bool_op.values.iter().any(recurse),
        Expr::UnaryOp(unary) => recurse(unary.operand.as_ref()),
        Expr::BinOp(bin_op) => recurse(bin_op.left.as_ref()) || recurse(bin_op.right.as_ref()),
        Expr::Attribute(attribute) => recurse(attribute.value.as_ref()),
        Expr::Subscript(subscript) => {
            recurse(subscript.value.as_ref()) || recurse(subscript.slice.as_ref())
        }
        Expr::Tuple(tuple) => tuple.elts.iter().any(recurse),
        Expr::List(list) => list.elts.iter().any(recurse),
        Expr::Name(_) | Expr::Constant(_) => false,
        _ => true,
    }
}

/// Whether `expr`'s text names `name` as a whole identifier.
fn mentions_name(source: &str, expr: &Expr, name: &str) -> bool {
    super::mentions_python_name(&text_for_range(source, expr.range()), name)
}

fn expr_root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => expr_root_name(attribute.value.as_ref()),
        Expr::Subscript(subscript) => expr_root_name(subscript.value.as_ref()),
        _ => None,
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
        callable_call: None,
        // The failure message observes nothing (RIPR-SPEC-0238 rule 4).
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
    // Rule 13: the callable form calls its callable, so the gates read that
    // call as they read the body of the `with` form.
    if let Some((callable, arguments)) = exception_assertion_callable(call) {
        // Both unittest and pytest pass keyword arguments on to the callable
        // (`match=` included in pytest's callable form).
        let arguments = arguments
            .iter()
            .map(|argument| text_for_range(source, argument.range()).trim().to_string())
            .chain(
                call.keywords
                    .iter()
                    .map(|keyword| text_for_range(source, keyword.range()).trim().to_string()),
            )
            .collect::<Vec<_>>()
            .join(", ");
        let callable_call = format!(
            "{}({arguments})",
            text_for_range(source, callable.range()).trim()
        );
        return Some(PythonAssertion {
            gate_text: format!("{text}\n{callable_call}"),
            callable_call: Some(callable_call),
            text,
            line: line_for_range_start(source, call.range),
            oracle_kind,
            oracle_strength,
            oracle_shape,
        });
    }
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
        callable_call: None,
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
    // RIPR-SPEC-0238 rule 1: `==` pins a value only when every operator in the
    // chain is `==` and some link compares two different token sequences
    // (`owner(1) == owner(1)` runs the changed code on both sides, while
    // `owner(1) == owner(1) == 30` still pins the result through its last link).
    let all_exact = compare.ops.iter().all(|op| matches!(op, ast::CmpOp::Eq));
    let tautology = std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .collect::<Vec<_>>()
        .windows(2)
        .all(|pair| same_tokens(source, pair[0], pair[1]));
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
    if matches!(
        last_segment,
        "assertEqual" | "assertDictEqual" | "assertAlmostEqual"
    ) && (last_segment != "assertAlmostEqual" || name == "self.assertAlmostEqual")
        && compared_operands(call).is_some_and(|(first, second)| same_tokens(source, first, second))
    {
        // RIPR-SPEC-0238 rule 1: comparing an expression with itself pins nothing.
        return Some((
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            oracle_shape_for_call_arguments(call, PythonOracleShape::BoundaryAssertion),
        ));
    }
    match last_segment {
        // Only the bounded standard unittest context above promotes this call.
        "assertListEqual" => Some((
            OracleKind::Unknown,
            OracleStrength::Unknown,
            PythonOracleShape::UnknownCustomHelper,
        )),
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
        // RIPR-SPEC-0238 rule 12: at its default tolerance the call pins the
        // value; an explicit band may admit the old value. Only `self.` is the
        // unittest method; another receiver may be a project helper.
        "assertAlmostEqual" if name == "self.assertAlmostEqual" => {
            let explicit = |position: usize, keyword: &str| {
                call.args
                    .get(position)
                    .or_else(|| keyword_value(call, keyword))
                    .is_some_and(|value| !is_none_literal(value))
            };
            // A spread (`*rest`, `**tolerance`) may carry `places` or `delta`.
            let spread = call
                .args
                .iter()
                .any(|argument| matches!(argument, Expr::Starred(_)))
                || call.keywords.iter().any(|keyword| keyword.arg.is_none());
            if spread || explicit(2, "places") || explicit(4, "delta") {
                Some((
                    OracleKind::RelationalCheck,
                    OracleStrength::Weak,
                    oracle_shape_for_call_arguments(call, PythonOracleShape::BoundaryAssertion),
                ))
            } else {
                Some((
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                    oracle_shape_for_call_arguments(call, PythonOracleShape::ExactAssertion),
                ))
            }
        }
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
            // Rule 13: in the callable form pytest passes `match=` to the
            // callable instead of checking the message.
            let pattern = keyword_value(call, "match").filter(|_| call.args.len() < 2);
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
        _ if looks_like_custom_assertion_helper(&name)
            || chain_calls_assertion_helper(call.func.as_ref()) =>
        {
            Some((
                OracleKind::Unknown,
                OracleStrength::Unknown,
                PythonOracleShape::UnknownCustomHelper,
            ))
        }
        _ => None,
    }
}

/// RIPR-SPEC-0238 rule 3: a fluent chain whose callee contains an
/// assertion-helper call (`assert_that(x).is_equal_to(y)`).
fn chain_calls_assertion_helper(func: &Expr) -> bool {
    match func {
        Expr::Attribute(attribute) => chain_calls_assertion_helper(attribute.value.as_ref()),
        Expr::Call(call) => {
            expr_full_name(call.func.as_ref())
                .is_some_and(|name| looks_like_custom_assertion_helper(&name))
                || chain_calls_assertion_helper(call.func.as_ref())
        }
        _ => false,
    }
}

fn is_none_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::None,
            ..
        })
    )
}

/// RIPR-SPEC-0238 rule 13: the callable an exception assertion calls, and the
/// positional arguments it passes, when the callable is a bare or dotted name
/// (`assertRaisesRegex(KeyError, 'empty', parse, '')`).
pub(super) fn exception_assertion_callable(call: &ast::ExprCall) -> Option<(&Expr, &[Expr])> {
    let name = expr_full_name(call.func.as_ref())?;
    let last_segment = name.rsplit('.').next().unwrap_or(name.as_str());
    let position = match last_segment {
        "assertRaises" => 1,
        "assertRaisesRegex" => 2,
        "raises" if name == "pytest.raises" || name == "raises" => 1,
        _ => return None,
    };
    let callable = call.args.get(position)?;
    expr_full_name(callable)
        .filter(|_| matches!(callable, Expr::Name(_) | Expr::Attribute(_)))
        .map(|_| (callable, &call.args[position + 1..]))
}

/// The two operands a comparison assertion call compares, positional or named
/// (`first=`, `second=`); never its `msg`.
fn compared_operands(call: &ast::ExprCall) -> Option<(&Expr, &Expr)> {
    let name = expr_full_name(call.func.as_ref())?;
    let last_segment = name.rsplit('.').next().unwrap_or(name.as_str());
    if !matches!(
        last_segment,
        "assertEqual" | "assertDictEqual" | "assertAlmostEqual" | "assertNotEqual" | "assertIn"
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
/// outside string literals (RIPR-SPEC-0238 rule 1).
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

/// RIPR-SPEC-0238 rule 2: a literal message pattern that admits any message
/// (`''`, `.*`, `.+`, `^`, `$`, `^.*$`, `(?s).*`), or an alternation that can
/// admit both the old and the new message (`'empty|blank'`), pins only the
/// exception class, as does a literal `None` (matching disabled). A bare name reads as the string literal it is bound to
/// when the file assigns that name exactly once, to a literal (Decision 5).
fn is_trivial_message_pattern(pattern: &Expr, source: &str) -> bool {
    let text = match pattern {
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::Str(text),
            ..
        }) => text.as_str(),
        // `match=None` / `assertRaisesRegex(E, None)` disables message
        // matching in pytest and unittest, so it pins only the class.
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::None,
            ..
        }) => return true,
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

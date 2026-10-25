//! Bounded local snapshots of `caplog.text` and `caplog.messages` (#10151).

use super::boundary::CallRow;
use super::source_utils::SourceText;
use super::static_limits::{CaplogMembership, CaplogRead};
use super::{PythonAssertion, PythonOracleShape, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
use rustpython_parser::{Mode, Tok, lexer, parse};

/// Only straight-line statements and the existing capture-level block are
/// traversed. A branch, nested function or comprehension is not an alias scope.
fn straight_statements<'a>(body: &'a [Stmt], out: &mut Vec<&'a Stmt>) -> Option<()> {
    for stmt in body {
        match stmt {
            Stmt::With(with)
                if with.items.len() == 1
                    && with.items[0].optional_vars.is_none()
                    && matches!(&with.items[0].context_expr, Expr::Call(call)
                    if expr_full_name(&call.func).as_deref() == Some("caplog.at_level")) =>
            {
                straight_statements(&with.body, out)?;
            }
            Stmt::Assign(_) | Stmt::Assert(_) | Stmt::Expr(_) | Stmt::Pass(_) => out.push(stmt),
            _ => return None,
        }
    }
    Some(())
}

fn memberships(
    expr: &Expr,
    alias: &str,
    read: CaplogRead,
    uses: &mut Vec<TextRange>,
    out: &mut Vec<CaplogMembership>,
) -> Option<()> {
    if let Expr::BoolOp(boolean) = expr {
        if boolean.op != ast::BoolOp::And {
            return None;
        }
        for value in &boolean.values {
            memberships(value, alias, read, uses, out)?;
        }
        return Some(());
    }
    let Expr::Compare(compare) = expr else {
        return None;
    };
    let [op @ (ast::CmpOp::In | ast::CmpOp::NotIn)] = compare.ops.as_slice() else {
        return None;
    };
    let [Expr::Name(name)] = compare.comparators.as_slice() else {
        return None;
    };
    if name.id.as_str() != alias {
        return None;
    }
    let Expr::Constant(constant) = compare.left.as_ref() else {
        return None;
    };
    let ast::Constant::Str(literal) = &constant.value else {
        return None;
    };
    uses.push(name.range());
    out.push(CaplogMembership {
        literal: literal.clone(),
        read,
        negated: *op == ast::CmpOp::NotIn,
    });
    Some(())
}

/// Return literals only when every use of the single local alias is its
/// assignment or a membership operand in the selected assertion. Token ranges
/// exclude strings/comments and catch all other stores, escapes and mutations.
/// Failure conservatively leaves the existing unsupported/gap verdict intact.
pub(super) fn required_literals(
    test: &PythonTest,
    assertion: &PythonAssertion,
    rows: &[CallRow],
) -> Option<Vec<CaplogMembership>> {
    if assertion.helper_derived
        || test
            .assertions
            .iter()
            .filter(|other| other.line == assertion.line)
            .count()
            != 1
    {
        return None;
    }
    let body = &test.body_text;
    if !body.contains("caplog") {
        return None;
    }
    let ast::Mod::Module(module) = parse(body, Mode::Module, "<caplog-alias>").ok()? else {
        return None;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return None;
    };
    let mut statements = Vec::new();
    straight_statements(&function.body, &mut statements)?;
    let source = SourceText::new(body);
    let mut matching_assertions = statements.iter().filter_map(|stmt| match stmt {
        Stmt::Assert(value)
            if source.line_for_offset(usize::from(value.range.start())) + test.line - 1
                == assertion.line =>
        {
            Some(value)
        }
        _ => None,
    });
    let selected = matching_assertions.next()?;
    if matching_assertions.next().is_some() {
        return None;
    }
    let mut assignments = statements.iter().filter_map(|stmt| {
        let Stmt::Assign(value) = stmt else {
            return None;
        };
        let [Expr::Name(name)] = value.targets.as_slice() else {
            return None;
        };
        let read = match expr_full_name(&value.value).as_deref() {
            Some("caplog.text") => CaplogRead::Text,
            Some("caplog.messages") => CaplogRead::MessageEquals,
            _ => return None,
        };
        Some((value, name, read))
    });
    let (assignment, name, read) = assignments.next()?;
    if assignments.next().is_some() {
        return None;
    }
    let assigned_line =
        source.line_for_offset(usize::from(assignment.range.start())) + test.line - 1;
    if assigned_line >= assertion.line || rows.iter().any(|row| row.line >= assigned_line) {
        return None;
    }
    let mut uses = vec![name.range()];
    let mut out = Vec::new();
    memberships(&selected.test, name.id.as_str(), read, &mut uses, &mut out)?;
    for token in lexer::lex(body, Mode::Module) {
        let (token, range) = token.ok()?;
        // The lexer keeps embedded f-string expressions inside a string
        // token, so a walrus or escape there has no standalone Name token.
        if matches!(&token, Tok::String { kind, .. } if kind.is_any_fstring()) {
            return None;
        }
        if matches!(token, Tok::Name { name: ref token_name } if token_name == name.id.as_str())
            && !uses.contains(&range)
        {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Keep extraction, family selection and displayed evidence on the same
/// output assertion. The evaluator separately checks call/snapshot ordering.
pub(super) fn mark_output_assertions(test: &mut PythonTest) {
    let output_lines: Vec<usize> = test
        .assertions
        .iter()
        .filter(|assertion| required_literals(test, assertion, &[]).is_some())
        .map(|assertion| assertion.line)
        .collect();
    for assertion in &mut test.assertions {
        if output_lines.contains(&assertion.line) {
            assertion.oracle_shape = PythonOracleShape::OutputAssertion;
        }
    }
}

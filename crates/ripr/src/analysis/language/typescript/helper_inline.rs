//! Test-local assertion helpers, read as the assertions they run (#9636).
//!
//! A test that calls a same-file `checkPass(50, true)` whose body is only
//! `assert.equal(passes(score), want)` runs exactly the assertion the helper
//! holds, with the call's literal arguments in place of the parameters. The
//! extractor reads assertions out of a test's own callback, so such a test
//! read as "0 related tests" and abstained.
//!
//! Like the suite-factory expansion (RIPR-SPEC-0243 rule 13), this pass
//! rewrites the file's source and lets the unchanged extractor run on it. Each
//! straight-line helper-call statement of a top-level test becomes the
//! helper's assertion statements with the literal arguments substituted, on
//! the same physical line, so no line number moves. It is credit-or-unknown
//! only: it adds no finding kind, and the rewritten assertions go through the
//! ordinary oracle admission, so a weak or unrecognised assertion is no
//! stronger than when written inline. Anything it cannot prove stays as
//! written and therefore stays unknown:
//!
//! - the helper is a top-level, non-exported, synchronous `function` with
//!   plain identifier parameters (no defaults, rest, destructuring) whose
//!   body is only `assert...(...)` / `expect(...)...` call statements, and
//!   every mention of its name in the file is its declaration or a direct
//!   call;
//! - the call is a top-level statement of a top-level `test`/`it` callback
//!   that takes no parameters and holds only expression statements (no
//!   declaration can shadow what the helper's assertions read), with exactly
//!   as many literal arguments as the helper has parameters;
//! - a parameter appears in the helper's assertions only as a plain value
//!   (never as a member name, in a string or template, or assigned), and the
//!   assertions hold no closure or object literal;
//! - the file does not use `eval`, `with` or `\u` escapes.
//!
//! A helper defined in another file, passed as a value, called under a
//! condition or loop, or reached from a nested `describe` is never inlined.

use super::suite_factory::SuiteFactoryExpansion;
use super::tests_extract::{
    byte_preserving_code_mask, file_is_opaque, identifier_occurrences, word_occurrences,
};
use oxc_ast::ast::{Argument, BindingPattern, Expression, Statement, UnaryOperator};
use std::collections::BTreeMap;

struct Helper {
    parameters: Vec<String>,
    /// Each assertion statement's source text and its masked text.
    statements: Vec<(String, String)>,
}

/// The source with each provable helper-call statement replaced, wrapped as an
/// expansion with no appended lines, or `None` when nothing qualifies.
pub(crate) fn inline_test_helpers(
    program: &[Statement<'_>],
    source: &str,
) -> Option<SuiteFactoryExpansion> {
    let code = byte_preserving_code_mask(source)?;
    if code.len() != source.len() || file_is_opaque(&code) {
        return None;
    }
    let helpers = collect_helpers(program, source, &code);
    if helpers.is_empty() {
        return None;
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in program {
        let Some(callback) = test_callback_body(stmt) else {
            continue;
        };
        if !callback
            .iter()
            .all(|statement| matches!(statement, Statement::ExpressionStatement(_)))
        {
            continue;
        }
        for statement in callback {
            let Some((name, arguments, start, end)) = helper_call(statement, source) else {
                continue;
            };
            let Some(helper) = helpers.get(name) else {
                continue;
            };
            if helper.parameters.len() != arguments.len()
                || !statement_owns_its_lines(source, start, end)
            {
                continue;
            }
            let Some(replacement) = substitute_all(helper, &arguments) else {
                continue;
            };
            edits.push((start, end, replacement));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let mut rewritten = source.to_string();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in edits {
        rewritten.replace_range(start..end, &replacement);
    }
    Some(SuiteFactoryExpansion {
        source: rewritten,
        call_lines: Vec::new(),
        covered: Vec::new(),
    })
}

/// The statements of a top-level `test(title, () => {...})` / `it(...)`
/// callback with no parameters.
fn test_callback_body<'s, 'a>(stmt: &'s Statement<'a>) -> Option<&'s [Statement<'a>]> {
    let Statement::ExpressionStatement(statement) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &statement.expression else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if !matches!(callee.name.as_str(), "test" | "it") || call.arguments.len() != 2 {
        return None;
    }
    match call.arguments.get(1)? {
        Argument::ArrowFunctionExpression(arrow)
            if !arrow.r#async && arrow.params.items.is_empty() && arrow.params.rest.is_none() =>
        {
            Some(arrow.body.statements.as_slice())
        }
        Argument::FunctionExpression(function)
            if !function.r#async
                && !function.generator
                && function.id.is_none()
                && function.params.items.is_empty()
                && function.params.rest.is_none() =>
        {
            function
                .body
                .as_ref()
                .map(|body| body.statements.as_slice())
        }
        _ => None,
    }
}

fn collect_helpers(
    program: &[Statement<'_>],
    source: &str,
    code: &str,
) -> BTreeMap<String, Helper> {
    let mut helpers = BTreeMap::new();
    for stmt in program {
        let Statement::FunctionDeclaration(function) = stmt else {
            continue;
        };
        if function.r#async
            || function.generator
            || function.this_param.is_some()
            || function.declare
            || function.params.rest.is_some()
        {
            continue;
        }
        let (Some(id), Some(body)) = (function.id.as_ref(), function.body.as_ref()) else {
            continue;
        };
        let name = id.name.as_str();
        let mut parameters = Vec::new();
        let mut plain = true;
        for parameter in &function.params.items {
            match &parameter.pattern {
                BindingPattern::BindingIdentifier(identifier)
                    if parameter.initializer.is_none()
                        && !parameter.optional
                        && parameter.decorators.is_empty()
                        && parameter.accessibility.is_none()
                        && !parameter.readonly =>
                {
                    parameters.push(identifier.name.to_string());
                }
                _ => plain = false,
            }
        }
        if !plain || !body.directives.is_empty() || body.statements.is_empty() {
            continue;
        }
        let mut statements = Vec::new();
        for statement in &body.statements {
            let Statement::ExpressionStatement(expression) = statement else {
                plain = false;
                break;
            };
            if !is_assertion_call(&expression.expression) {
                plain = false;
                break;
            }
            let (start, end) = (expression.span.start as usize, expression.span.end as usize);
            let (Some(text), Some(masked)) = (source.get(start..end), code.get(start..end)) else {
                plain = false;
                break;
            };
            if text.contains('\n') {
                plain = false;
                break;
            }
            statements.push((text.to_string(), masked.to_string()));
        }
        if !plain {
            continue;
        }
        // Declared once; every mention is the declaration or a direct call.
        let declared = program
            .iter()
            .filter(|other| {
                matches!(other, Statement::FunctionDeclaration(f)
                    if f.id.as_ref().is_some_and(|other_id| other_id.name == id.name))
            })
            .count();
        if declared != 1
            || !word_occurrences(code, name)
                .all(|at| at == id.span.start as usize || is_direct_call_at(code, at, name))
        {
            continue;
        }
        helpers.insert(
            name.to_string(),
            Helper {
                parameters,
                statements,
            },
        );
    }
    helpers
}

/// `assert.x(...)`, `assert(...)`, `expect(...).x(...)`: a call whose callee
/// chain is rooted at `assert` or `expect`.
fn is_assertion_call(expression: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    let mut callee = &call.callee;
    loop {
        match callee {
            Expression::Identifier(identifier) => {
                return matches!(identifier.name.as_str(), "assert" | "expect");
            }
            Expression::StaticMemberExpression(member) => callee = &member.object,
            Expression::CallExpression(inner) => callee = &inner.callee,
            _ => return false,
        }
    }
}

fn is_direct_call_at(code: &str, at: usize, name: &str) -> bool {
    let before = code[..at].trim_end();
    let declares = before.strip_suffix("function").is_some_and(|rest| {
        !rest.ends_with(|ch: char| ch == '_' || ch == '$' || ch.is_alphanumeric())
    });
    !before.ends_with(['.', '#', '*']) && !declares && code[at + name.len()..].starts_with('(')
}

/// `name(literal, ...)` as an expression statement: the helper name, the
/// source text of each literal argument (negative numbers parenthesised), and
/// the statement's byte span.
fn helper_call<'s>(
    statement: &'s Statement<'_>,
    source: &str,
) -> Option<(&'s str, Vec<String>, usize, usize)> {
    let Statement::ExpressionStatement(expression) = statement else {
        return None;
    };
    let Expression::CallExpression(call) = &expression.expression else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if call.type_arguments.is_some() || call.optional {
        return None;
    }
    let mut arguments = Vec::new();
    for argument in &call.arguments {
        let (span, negative) = match argument {
            Argument::NumericLiteral(literal) => (literal.span, false),
            Argument::StringLiteral(literal) => (literal.span, false),
            Argument::BooleanLiteral(literal) => (literal.span, false),
            Argument::NullLiteral(literal) => (literal.span, false),
            Argument::UnaryExpression(unary)
                if matches!(
                    unary.operator,
                    UnaryOperator::UnaryNegation | UnaryOperator::UnaryPlus
                ) && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                (unary.span, true)
            }
            _ => return None,
        };
        let text = source.get(span.start as usize..span.end as usize)?;
        arguments.push(if negative {
            format!("({text})")
        } else {
            text.to_string()
        });
    }
    Some((
        callee.name.as_str(),
        arguments,
        expression.span.start as usize,
        expression.span.end as usize,
    ))
}

fn statement_owns_its_lines(source: &str, start: usize, end: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[end..].find('\n').map_or(source.len(), |at| end + at);
    source[line_start..start].trim().is_empty()
        && source[end..line_end].trim().is_empty()
        && !source[start..end].contains('\n')
}

fn substitute_all(helper: &Helper, arguments: &[String]) -> Option<String> {
    let mut parts = Vec::new();
    for (text, masked) in &helper.statements {
        let mut part = substitute(text, masked, &helper.parameters, arguments)?;
        if !part.trim_end().ends_with(';') {
            part.push(';');
        }
        parts.push(part);
    }
    Some(parts.join(" "))
}

fn substitute(
    text: &str,
    masked: &str,
    parameters: &[String],
    arguments: &[String],
) -> Option<String> {
    // Closures, object literals, templates and assignment can rebind or
    // capture a parameter name; none is read here.
    if text.len() != masked.len()
        || text.contains(['`', '{', '\\'])
        || ["=>", "function", "++", "--"]
            .iter()
            .any(|needle| masked.contains(needle))
    {
        return None;
    }
    let mut without_comparisons = masked.to_string();
    for comparison in ["===", "!==", "==", "!=", "<=", ">="] {
        without_comparisons = without_comparisons.replace(comparison, "  ");
    }
    if without_comparisons.contains('=') {
        return None;
    }
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (position, parameter) in parameters.iter().enumerate() {
        let any = word_occurrences(text, parameter).count();
        let in_code = word_occurrences(masked, parameter).count();
        let plain = identifier_occurrences(masked, parameter).count();
        // Every mention is a plain value in code: not inside a string or
        // comment, not a member name.
        if any != in_code || in_code != plain {
            return None;
        }
        for at in identifier_occurrences(masked, parameter) {
            // `{` was refused, so a key or shorthand cannot occur; a ternary
            // `?:` or call argument keeps its operand.
            spans.push((at, at + parameter.len(), position));
        }
    }
    spans.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, position) in spans {
        out.push_str(text.get(cursor..start)?);
        out.push_str(arguments.get(position)?);
        cursor = end;
    }
    out.push_str(text.get(cursor..)?);
    Some(out)
}

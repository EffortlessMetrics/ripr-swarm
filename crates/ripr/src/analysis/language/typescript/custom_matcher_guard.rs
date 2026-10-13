//! RIPR-SPEC-0259: a custom matcher the test file registers with
//! `expect.extend`, whose result passes only when `received === expected`,
//! compares as strictly as `toBe`.
//!
//! The matcher is read only in the shapes below; any other registration in
//! the file, or any other body, leaves every custom matcher `unknown`.

use super::*;
use oxc_ast::ast::{BinaryOperator, LogicalOperator, PropertyKind};

/// Names of the matchers the file's `expect.extend({ ... })` calls register
/// whose `pass` requires strict equality of the received and expected
/// values (rule 1). Empty when any registration cannot be read whole.
pub(crate) fn strict_equality_matchers(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &str,
) -> Vec<String> {
    let mut registered: Vec<(String, bool)> = Vec::new();
    let mut extend_calls = 0;
    for statement in statements {
        let Some(object) = expect_extend_object(statement, source) else {
            continue;
        };
        extend_calls += 1;
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                // A spread registers matchers this reader cannot name.
                return Vec::new();
            };
            let Some(name) = property.key.static_name() else {
                return Vec::new();
            };
            if property.computed || property.kind != PropertyKind::Init {
                return Vec::new();
            }
            let strict = matcher_function(&property.value)
                .is_some_and(|(params, body)| passes_only_on_strict_equality(params, body, source));
            registered.push((name.to_string(), strict));
        }
    }
    // Every mention of `extend` in the file must be a top-level statement
    // read above; one in a hook, a helper or a nested scope (however spelled:
    // `expect.extend (`, `expect["extend"](`) may register the same name with
    // another body. A computed member of `expect` may spell `extend` in
    // pieces, so any `expect[` refuses.
    if extend_calls == 0
        || extend_mentions(source) != extend_calls
        || source.contains("expect[")
        || source.contains("expect?.[")
    {
        return Vec::new();
    }
    registered
        .iter()
        .filter(|(name, strict)| {
            *strict
                && registered.iter().filter(|(other, _)| other == name).count() == 1
                && oracle_for_matcher(name).0 == OracleKind::Unknown
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The word `extend` anywhere in `source` (not `extends`, `extended`),
/// comments and strings included, which only over-count and so refuse.
fn extend_mentions(source: &str) -> usize {
    source
        .match_indices("extend")
        .filter(|(index, matched)| {
            source.get(index + matched.len()..).is_some_and(|rest| {
                !rest.starts_with(|next: char| {
                    next.is_ascii_alphanumeric() || next == '_' || next == '$'
                })
            })
        })
        .count()
}

/// The object literal of a top-level `expect.extend({ ... })` statement,
/// with `extend` spelled out in the source (an escaped name such as
/// `ext\u0065nd` would hide from `extend_mentions`).
fn expect_extend_object<'s, 'a>(
    statement: &'s Statement<'a>,
    source: &str,
) -> Option<&'s oxc_ast::ast::ObjectExpression<'a>> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let Expression::CallExpression(call) = statement.expression.get_inner_expression() else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let spelled =
        source.get(member.property.span.start as usize..member.property.span.end as usize);
    if spelled != Some("extend")
        || !matches!(&member.object, Expression::Identifier(object) if object.name == "expect")
        || call.arguments.len() != 1
    {
        return None;
    }
    match call
        .arguments
        .first()?
        .as_expression()?
        .get_inner_expression()
    {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    }
}

/// The parameter names and body statements of a synchronous, non-generator
/// method, function expression or arrow with exactly two distinct plain
/// identifier parameters (sloppy mode accepts `(r, r)`). An expression-bodied arrow's single expression statement
/// matches no body shape `passes_only_on_strict_equality` reads.
fn matcher_function<'s, 'a>(
    value: &'s Expression<'a>,
) -> Option<([&'s str; 2], &'s oxc_allocator::Vec<'a, Statement<'a>>)> {
    let (params, body) = match value.get_inner_expression() {
        Expression::FunctionExpression(function) if !function.r#async && !function.generator => {
            (&function.params, &function.body.as_ref()?.statements)
        }
        Expression::ArrowFunctionExpression(arrow) if !arrow.r#async => {
            (&arrow.params, &arrow.body.statements)
        }
        _ => return None,
    };
    if params.rest.is_some() || params.items.len() != 2 {
        return None;
    }
    let name = |index: usize| match &params.items.get(index)?.pattern {
        BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    };
    let (received, expected) = (name(0)?, name(1)?);
    if received == expected {
        return None;
    }
    Some(([received, expected], body))
}

/// `true` when the body is `return { pass: X, ... }` or
/// `const pass = X; return { pass, ... }`, with `X` a `&&` chain one of whose
/// terms is `received === expected` (either order) or
/// `Object.is(received, expected)`, and no term assigns or updates anything.
fn passes_only_on_strict_equality(
    [received, expected]: [&str; 2],
    body: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &str,
) -> bool {
    let pass = match body.as_slice() {
        [Statement::ReturnStatement(returned)] => returned_pass(returned, None),
        [
            Statement::VariableDeclaration(declaration),
            Statement::ReturnStatement(returned),
        ] => {
            let [declarator] = declaration.declarations.as_slice() else {
                return false;
            };
            if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
                || !matches!(&declarator.id, BindingPattern::BindingIdentifier(id) if id.name == "pass")
            {
                return false;
            }
            declarator
                .init
                .as_ref()
                .and_then(|init| returned_pass(returned, Some(init)))
        }
        _ => None,
    };
    let Some(pass) = pass else {
        return false;
    };
    let mut terms = Vec::new();
    conjunction_terms(pass, &mut terms);
    let object_is = object_is_unshadowed(source);
    terms
        .iter()
        .any(|term| is_strict_equality(term, received, expected, object_is))
        && terms.iter().all(|term| {
            source
                .get(term.span().start as usize..term.span().end as usize)
                .is_some_and(|text| !writes_anything(text))
        })
}

/// The `pass` expression of `return { pass: X, ... }`, or `declared` when
/// the object holds the shorthand `pass`. `pass` is keyed once, with no
/// spread, method or computed key, and every other value is inert (a
/// function, a name or a literal): JavaScript evaluates the properties in
/// order, so another value (or an earlier `pass`) could write a parameter
/// before the comparison runs.
fn returned_pass<'s, 'a>(
    returned: &'s oxc_ast::ast::ReturnStatement<'a>,
    declared: Option<&'s Expression<'a>>,
) -> Option<&'s Expression<'a>> {
    let Expression::ObjectExpression(object) = returned.argument.as_ref()?.get_inner_expression()
    else {
        return None;
    };
    let mut pass = None;
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        if property.computed || property.kind != PropertyKind::Init {
            return None;
        }
        if property.key.static_name().as_deref() != Some("pass") {
            if !inert_value(&property.value) {
                return None;
            }
            continue;
        }
        if pass.is_some() {
            return None;
        }
        pass = Some(match (property.shorthand, declared) {
            (true, Some(declared)) => declared,
            (false, None) => &property.value,
            _ => return None,
        });
    }
    pass
}

/// A property value whose evaluation runs no code.
fn inert_value(value: &Expression<'_>) -> bool {
    match value.get_inner_expression() {
        Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::Identifier(_)
        | Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(template) => template.expressions.is_empty(),
        _ => false,
    }
}

/// `true` when every mention of the word `Object` in `source` is an
/// `Object.is(` call, so no declaration, import or assignment can replace
/// it.
fn object_is_unshadowed(source: &str) -> bool {
    word_mentions(source, "Object") == source.matches("Object.is(").count()
}

/// The word `word` in `source`, not inside a longer identifier.
fn word_mentions(source: &str, word: &str) -> usize {
    let is_identifier = |ch: char| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$';
    source
        .match_indices(word)
        .filter(|(index, matched)| {
            let before = source
                .get(..*index)
                .and_then(|text| text.chars().next_back());
            let after = source
                .get(index + matched.len()..)
                .and_then(|text| text.chars().next());
            !before.is_some_and(is_identifier) && !after.is_some_and(is_identifier)
        })
        .count()
}

fn conjunction_terms<'s, 'a>(expression: &'s Expression<'a>, terms: &mut Vec<&'s Expression<'a>>) {
    match expression.get_inner_expression() {
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
            conjunction_terms(&logical.left, terms);
            conjunction_terms(&logical.right, terms);
        }
        inner => terms.push(inner),
    }
}

fn is_strict_equality(
    term: &Expression<'_>,
    received: &str,
    expected: &str,
    object_is: bool,
) -> bool {
    let is_name = |expression: &Expression<'_>, name: &str| matches!(expression.get_inner_expression(), Expression::Identifier(identifier) if identifier.name == name);
    let pair = |left: &Expression<'_>, right: &Expression<'_>| {
        (is_name(left, received) && is_name(right, expected))
            || (is_name(left, expected) && is_name(right, received))
    };
    match term {
        Expression::BinaryExpression(binary) => {
            binary.operator == BinaryOperator::StrictEquality && pair(&binary.left, &binary.right)
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            object_is
                && matches!(&member.object, Expression::Identifier(object) if object.name == "Object")
                && member.property.name == "is"
                && call.arguments.len() == 2
                && match (
                    call.arguments
                        .first()
                        .and_then(|argument| argument.as_expression()),
                    call.arguments
                        .get(1)
                        .and_then(|argument| argument.as_expression()),
                ) {
                    (Some(left), Some(right)) => pair(left, right),
                    _ => false,
                }
        }
        _ => false,
    }
}

/// `true` when `text` may assign or update anything: an `=` outside `==`,
/// `===`, `!=`, `!==`, `<=` and `>=` (so `=>`, `<<=`, `>>=`, `||=` and the
/// like count), `++`, `--`, or the words `eval` and `arguments`,
/// which reach the parameters without naming them.
fn writes_anything(text: &str) -> bool {
    let bytes = text.as_bytes();
    let at = |index: Option<usize>| index.and_then(|index| bytes.get(index)).copied();
    let assigns = bytes.iter().enumerate().any(|(index, byte)| {
        if *byte != b'=' {
            return false;
        }
        let before = at(index.checked_sub(1));
        let after = at(index.checked_add(1));
        if before == Some(b'=') || after == Some(b'=') {
            return false;
        }
        match before {
            Some(b'!') => false,
            Some(b'<') | Some(b'>') => at(index.checked_sub(2)) == before,
            _ => true,
        }
    });
    assigns
        || text.contains("++")
        || text.contains("--")
        || ["eval", "arguments"]
            .iter()
            .any(|word| word_mentions(text, word) > 0)
}

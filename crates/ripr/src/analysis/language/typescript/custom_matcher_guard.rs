//! RIPR-SPEC-0262: a custom matcher the test file registers with
//! `expect.extend`, whose result passes only when `received === expected`,
//! compares as strictly as `toBe`.
//!
//! The matcher is read only in the shapes below; any other registration in
//! the file, or any other body, leaves every custom matcher `unknown`.

use super::*;
use oxc_ast::ast::{BinaryOperator, ImportDeclarationSpecifier, LogicalOperator, PropertyKind};

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
        let Some(object) = expect_extend_object(statement) else {
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
    // another body.
    if extend_calls == 0
        || extend_mentions(source) != extend_calls
        || !names_expect_plainly(statements, source)
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

/// `true` when the file reaches `expect` only by name: no escaped name
/// (`ext\u0065nd`), every mention of the word `expect` is `expect(`,
/// `expect.` or a plain `import { expect }`, no computed member access
/// builds a name (`ns["ext" + "end"]`), and no global or module object that
/// holds `expect` under another spelling is named. Comments and strings
/// count too, so a mention only refuses.
fn names_expect_plainly(statements: &oxc_allocator::Vec<'_, Statement<'_>>, source: &str) -> bool {
    if source.contains("\\u") || computed_member_access(source) {
        return false;
    }
    let mut imports = 0;
    let mut imported_expect = 0;
    for statement in statements {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        imports += 1;
        for specifier in import.specifiers.iter().flatten() {
            match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                    // Only a plain `import { expect }`; `expect as e` and
                    // `fake as expect` both refuse.
                    if specifier.local.name == "expect" || specifier.imported.name() == "expect" {
                        let text =
                            source.get(specifier.span.start as usize..specifier.span.end as usize);
                        if text != Some("expect") {
                            return false;
                        }
                        imported_expect += 1;
                    }
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => return false,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
            }
        }
    }
    let other_mentions = source
        .match_indices("expect")
        .filter(|(index, matched)| {
            word_at(source, *index, matched.len())
                && !source
                    .get(index + matched.len()..)
                    .is_some_and(|rest| rest.starts_with('(') || rest.starts_with('.'))
        })
        .count();
    other_mentions == imported_expect
        && word_mentions(source, "import") == imports
        && word_mentions(source, "global") == source.matches("declare global").count()
        && GLOBAL_ROOTS
            .iter()
            .all(|word| word_mentions(source, word) == 0)
}

/// Names that hold the runner's `expect` (the global object, a module
/// object or a code string) under a spelling the word scan cannot see.
const GLOBAL_ROOTS: &[&str] = &[
    "globalThis",
    "window",
    "self",
    "this",
    "document",
    "constructor",
    "Reflect",
    "Function",
    "eval",
    "require",
    "requireActual",
    "requireMock",
    "importActual",
    "importMock",
    "createMockFromModule",
];

/// `true` when `source` holds a `[` that may index a value by a name other
/// than a number literal: one after a name, `)`, `]`, `!` or `?.`. A `[`
/// after a keyword (`return [a]`, `const [a]`) starts an array or a
/// pattern instead; one in a type (`number[]`) is empty.
fn computed_member_access(source: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "const",
        "let",
        "var",
        "return",
        "in",
        "of",
        "typeof",
        "void",
        "await",
        "yield",
        "case",
        "new",
        "delete",
        "instanceof",
        "else",
        "do",
        "throw",
        "extends",
        "keyof",
        "readonly",
    ];
    let is_identifier = |ch: char| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$';
    source.match_indices('[').any(|(index, _)| {
        let before = source.get(..index).unwrap_or_default().trim_end();
        let Some(last) = before.chars().next_back() else {
            return false;
        };
        if is_identifier(last) {
            let word_start = before
                .char_indices()
                .rev()
                .take_while(|(_, ch)| is_identifier(*ch))
                .last()
                .map_or(0, |(start, _)| start);
            if before
                .get(word_start..)
                .is_some_and(|word| KEYWORDS.contains(&word))
            {
                return false;
            }
        } else if !matches!(last, ')' | ']' | '!' | '.') {
            return false;
        }
        let key = source
            .get(index + 1..)
            .and_then(|rest| rest.split(']').next())
            .unwrap_or_default()
            .trim();
        !key.chars().all(|ch| ch.is_ascii_digit())
    })
}

/// The object literal of a top-level `expect.extend({ ... })` statement.
/// `names_expect_plainly` refuses an escaped name such as `ext\u0065nd`,
/// which would hide from `extend_mentions`.
fn expect_extend_object<'s, 'a>(
    statement: &'s Statement<'a>,
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
    if member.property.name != "extend"
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
    // A default (`e = r`) runs code and may bind `expected` to `received`.
    if params.rest.is_some()
        || params.items.len() != 2
        || params.items.iter().any(|param| param.initializer.is_some())
    {
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
/// terms is `received === expected` (either order), and no term assigns or
/// updates anything.
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
    terms
        .iter()
        .any(|term| is_strict_equality(term, received, expected))
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

/// The word `word` in `source`, not inside a longer identifier.
fn word_mentions(source: &str, word: &str) -> usize {
    source
        .match_indices(word)
        .filter(|(index, matched)| word_at(source, *index, matched.len()))
        .count()
}

/// `true` when `source[index..index + len]` is not inside a longer
/// identifier.
fn word_at(source: &str, index: usize, len: usize) -> bool {
    let is_identifier = |ch: char| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$';
    let before = source
        .get(..index)
        .and_then(|text| text.chars().next_back());
    let after = source
        .get(index + len..)
        .and_then(|text| text.chars().next());
    !before.is_some_and(is_identifier) && !after.is_some_and(is_identifier)
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

fn is_strict_equality(term: &Expression<'_>, received: &str, expected: &str) -> bool {
    let is_name = |expression: &Expression<'_>, name: &str| matches!(expression.get_inner_expression(), Expression::Identifier(identifier) if identifier.name == name);
    let Expression::BinaryExpression(binary) = term else {
        return false;
    };
    binary.operator == BinaryOperator::StrictEquality
        && ((is_name(&binary.left, received) && is_name(&binary.right, expected))
            || (is_name(&binary.left, expected) && is_name(&binary.right, received)))
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

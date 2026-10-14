//! RIPR-SPEC-0247 rules 2 and 3: matchers whose expected value is a literal
//! written in the test read exact.
//!
//! Rule 2: legacy `node:assert` `equal` / `deepEqual` and chai's `equal`
//! compare with loose `==`. Loose equality only differs from strict equality
//! when coercion makes two different values compare equal, so an expected
//! literal that no other value of a different type can equal pins the actual
//! value as `strictEqual` does. `0`, `1`, `""`, whitespace, numeric strings,
//! booleans, `null`, `undefined` and `NaN` are the values coercion can reach;
//! they keep the RIPR-SPEC-0027 `relational_check` / weak reading.
//!
//! Rule 3: `toMatchInlineSnapshot` with one literal argument compares the
//! serialized value with text written in the test, so it pins the value the
//! way `toEqual` does. A committed `toMatchSnapshot` file and an inline
//! snapshot with property matchers or interpolation keep `snapshot` / medium.

use super::*;
use oxc_ast::ast::{PropertyKind, UnaryOperator};

/// `true` when a loose (`==`) equality assertion with `expected` pins the
/// actual value exactly (rule 2). `deep` admits object and array literals
/// whose every leaf is such a literal, for legacy `deepEqual`.
pub(crate) fn loose_equality_pins_literal(expected: &Argument<'_>, deep: bool) -> bool {
    expected
        .as_expression()
        .is_some_and(|expression| expression_pins_loosely(expression, deep))
}

fn expression_pins_loosely(expression: &Expression<'_>, deep: bool) -> bool {
    match expression.get_inner_expression() {
        Expression::StringLiteral(literal) => string_pins_loosely(literal.value.as_str()),
        Expression::TemplateLiteral(template) => {
            template.expressions.is_empty()
                && template
                    .quasis
                    .iter()
                    .all(|quasi| quasi.value.cooked.is_some())
                && string_pins_loosely(
                    &template
                        .quasis
                        .iter()
                        .filter_map(|quasi| quasi.value.cooked.as_deref())
                        .collect::<String>(),
                )
        }
        Expression::NumericLiteral(literal) => number_pins_loosely(literal.value),
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
            matches!(
                unary.argument.get_inner_expression(),
                Expression::NumericLiteral(literal) if number_pins_loosely(-literal.value)
            )
        }
        Expression::ObjectExpression(object) if deep => {
            object.properties.iter().all(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) => {
                    !property.computed
                        && !property.method
                        && !property.shorthand
                        && property.kind == PropertyKind::Init
                        // `__proto__: {...}` sets the prototype, not a key.
                        && property
                            .key
                            .static_name()
                            .is_some_and(|key| key != "__proto__")
                        && expression_pins_loosely(&property.value, deep)
                }
                ObjectPropertyKind::SpreadProperty(_) => false,
            })
        }
        Expression::ArrayExpression(array) if deep => array.elements.iter().all(|element| {
            element
                .as_expression()
                .is_some_and(|element| expression_pins_loosely(element, deep))
        }),
        _ => false,
    }
}

/// A string no value of another type equals under `==`: not empty, not
/// whitespace, and not numeric (`"1" == true`, `"0" == false`, `"20" == 20`).
fn string_pins_loosely(value: &str) -> bool {
    // JavaScript's `ToNumber` also strips U+FEFF, which `str::trim` keeps.
    let trimmed = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    !trimmed.is_empty() && trimmed.parse::<f64>().is_err() && !is_js_numeric_prefix(trimmed)
}

/// JavaScript also reads `0x1f`, `0b1` and `0o7` as numbers, which Rust's
/// `f64` parser does not (it already reads `Infinity` and `1e3`).
fn is_js_numeric_prefix(trimmed: &str) -> bool {
    let unsigned = trimmed.trim_start_matches(['+', '-']);
    ["0x", "0X", "0b", "0B", "0o", "0O"]
        .iter()
        .any(|prefix| unsigned.starts_with(prefix))
}

/// A number no value of another type equals under `==` except its own
/// numeric string: `0` equals `false` and `""`, `1` equals `true`.
fn number_pins_loosely(value: f64) -> bool {
    // Bit patterns, so `-0.0` reads as `0` and the comparison is exact.
    value.is_finite()
        && value.abs().to_bits() != 0.0_f64.to_bits()
        && value.to_bits() != 1.0_f64.to_bits()
}

/// The text of `expected` when it is a finite number literal, optionally
/// negated, parenthesized or under `as`: `0`, `-1`, `(1 as number)` reads
/// `1`. Rule 2 leaves `0` and `1` weak because a boolean or string coerces
/// to them; against an owner declared to return `number` they still pin the
/// value (`ts_typed_loose_number_facts`), and this text is the expected
/// value the pin records.
pub(crate) fn finite_number_literal_text<'s>(
    expected: &Argument<'_>,
    source: &'s str,
) -> Option<&'s str> {
    let inner = expected.as_expression()?.get_inner_expression();
    let finite = match inner {
        Expression::NumericLiteral(literal) => literal.value.is_finite(),
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
            matches!(
                unary.argument.get_inner_expression(),
                Expression::NumericLiteral(literal) if literal.value.is_finite()
            )
        }
        _ => false,
    };
    let span = inner.span();
    finite
        .then(|| source.get(span.start as usize..span.end as usize))
        .flatten()
}

/// `true` when `matcher_call` is `toMatchInlineSnapshot` with exactly one
/// argument that is a non-empty literal without interpolation (rule 3).
pub(crate) fn inline_snapshot_is_literal(
    matcher: &str,
    matcher_call: &oxc_ast::ast::CallExpression<'_>,
) -> bool {
    if matcher != "toMatchInlineSnapshot" || matcher_call.arguments.len() != 1 {
        return false;
    }
    let Some(expression) = matcher_call.arguments[0].as_expression() else {
        return false;
    };
    match expression.get_inner_expression() {
        Expression::TemplateLiteral(template) => {
            template.expressions.is_empty()
                && template.quasis.iter().all(|quasi| {
                    quasi
                        .value
                        .cooked
                        .as_deref()
                        .is_some_and(|text| !text.trim().is_empty())
                })
        }
        Expression::StringLiteral(literal) => !literal.value.trim().is_empty(),
        _ => false,
    }
}

/// A test-local `const` whose primitive literal initializer a loose equality
/// may read in place of the name (RIPR-SPEC-0247 rule 2, amended for #6654):
/// declared once at the top level of the test body, before the assertion.
/// Only primitives qualify: `const` stops reassignment, not mutation, so an
/// object or array the test fills in later (`want.push(...)`) is not the
/// literal it was declared with.
#[derive(Clone, Debug)]
pub(crate) struct LooseLiteralConst {
    name: String,
    text: String,
    declared_end: u32,
}

/// The test body's top-level `const <name> = <literal>;` bindings whose name
/// no other top-level declaration reuses. Only plain identifiers bound to a
/// literal are kept; the literal's own coercion reach is decided per
/// assertion by [`resolve_loose_literal_const`].
pub(crate) fn loose_literal_consts(
    statements: &oxc_allocator::Vec<'_, Statement<'_>>,
    source: &str,
) -> Vec<LooseLiteralConst> {
    let mut declared: Vec<String> = Vec::new();
    let mut candidates = Vec::new();
    for stmt in statements {
        match stmt {
            Statement::VariableDeclaration(declaration) => {
                for declarator in &declaration.declarations {
                    declared.extend(
                        declarator
                            .id
                            .get_binding_identifiers()
                            .iter()
                            .map(|ident| ident.name.to_string()),
                    );
                    if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const {
                        continue;
                    }
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let Some(init) = &declarator.init else {
                        continue;
                    };
                    // `deep: false` admits no object or array literal.
                    if !expression_pins_loosely(init, false) {
                        continue;
                    }
                    // The literal itself, without parentheses or `as`.
                    let span = init.get_inner_expression().span();
                    let Some(text) = source.get(span.start as usize..span.end as usize) else {
                        continue;
                    };
                    candidates.push(LooseLiteralConst {
                        name: id.name.to_string(),
                        text: text.to_string(),
                        declared_end: declaration.span.end,
                    });
                }
            }
            Statement::FunctionDeclaration(function) => {
                declared.extend(function.id.as_ref().map(|id| id.name.to_string()));
            }
            Statement::ClassDeclaration(class) => {
                declared.extend(class.id.as_ref().map(|id| id.name.to_string()));
            }
            _ => {}
        }
    }
    // Defensive: a valid program cannot declare a top-level name twice, but
    // the parser recovers from that error and must not credit either one.
    candidates.retain(|candidate| {
        declared
            .iter()
            .filter(|name| **name == candidate.name)
            .count()
            == 1
    });
    candidates
}

/// The literal a loose equality's `expected` identifier stands for, when it
/// names one of `consts` declared before the assertion at `call_start`.
pub(crate) fn resolve_loose_literal_const<'c>(
    consts: &'c [LooseLiteralConst],
    expected: &Argument<'_>,
    call_start: u32,
) -> Option<&'c str> {
    let Some(Expression::Identifier(ident)) = expected
        .as_expression()
        .map(|expression| expression.get_inner_expression())
    else {
        return None;
    };
    consts
        .iter()
        .find(|candidate| candidate.name == ident.name.as_str())
        .filter(|candidate| candidate.declared_end <= call_start)
        .map(|candidate| candidate.text.as_str())
}

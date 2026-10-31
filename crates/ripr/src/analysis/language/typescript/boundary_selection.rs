//! Changed predicate identity comes from parser-owned comparison spans (#10399).

use super::*;
use oxc_parser::config::TokensParserConfig;
use oxc_span::Span;
use std::borrow::Cow;

struct BoundaryToken {
    text: String,
    start: usize,
    end: usize,
}

struct BoundarySite {
    first: usize,
    last: usize,
    text: String,
}

struct BoundaryLine {
    tokens: Vec<BoundaryToken>,
    sites: Vec<BoundarySite>,
    returned_condition: Option<Vec<String>>,
    direct: bool,
}

fn operand_delimiter(text: &str) -> bool {
    COMPARISON_OPERATORS.contains(&text)
        || matches!(
            text,
            "??" | "&&"
                | "||"
                | "=>"
                | "?"
                | ":"
                | "="
                | "("
                | ")"
                | "{"
                | "}"
                | "["
                | "]"
                | ","
                | ";"
                | "return"
                | "if"
                | "else"
                | "while"
                | "for"
                | "case"
                | "yield"
        )
}

fn ungroup<'a>(mut expression: &'a Expression<'a>) -> &'a Expression<'a> {
    while let Expression::ParenthesizedExpression(group) = expression {
        expression = &group.expression;
    }
    expression
}

fn comparison(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::BinaryExpression(binary) => {
            binary.operator.is_compare() || binary.operator.is_equality()
        }
        Expression::LogicalExpression(logical) => logical.operator.as_str() == "??",
        _ => false,
    }
}

fn token_text(tokens: &[BoundaryToken]) -> String {
    let mut text = String::new();
    let mut previous_end = None;
    for token in tokens {
        if previous_end.is_some_and(|end| end < token.start) {
            text.push(' ');
        }
        text.push_str(&token.text);
        previous_end = Some(token.end);
    }
    text
}

// Preserve the established arithmetic-prefix limitation, but never erase
// computed access, calls, groups, type wrappers or nested predicate ownership.
fn admitted_operand(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Identifier(_)
        | Expression::ThisExpression(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::BigIntLiteral(_) => true,
        Expression::StaticMemberExpression(member) => admitted_operand(&member.object),
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc_ast::ast::ChainElement::StaticMemberExpression(member) => {
                admitted_operand(&member.object)
            }
            _ => false,
        },
        Expression::BinaryExpression(binary) if binary.operator.is_arithmetic() => {
            admitted_operand(&binary.left) && admitted_operand(&binary.right)
        }
        Expression::UnaryExpression(unary) if matches!(unary.operator.as_str(), "+" | "-") => {
            matches!(&unary.argument, Expression::NumericLiteral(_))
        }
        // `typeof x === "function"` is the kind check main's whole-line
        // reader labeled (`typeof x == "function"`); the selector must keep
        // naming it instead of refusing the boundary (#10399 qualification
        // golden drift on typescript_typeof_kind_*).
        Expression::UnaryExpression(unary) if unary.operator.as_str() == "typeof" => {
            admitted_operand(&unary.argument)
        }
        _ => false,
    }
}

fn site_for(expression: &Expression<'_>, tokens: &[BoundaryToken]) -> Option<BoundarySite> {
    let span = expression.span();
    let first = tokens
        .iter()
        .position(|token| token.start >= span.start as usize)?;
    let last = tokens
        .iter()
        .rposition(|token| token.end <= span.end as usize)?;
    let text = token_text(tokens.get(first..=last)?);
    // The legacy label reader cannot interpret nested operands or operators
    // inside literal values. Do not turn such values into another comparison.
    let (left, right) = match expression {
        Expression::BinaryExpression(binary) => (&binary.left, &binary.right),
        Expression::LogicalExpression(logical) => (&logical.left, &logical.right),
        _ => return None,
    };
    let opaque_operator = |span: Span| {
        tokens
            .iter()
            .filter(|token| token.start >= span.start as usize && token.end <= span.end as usize)
            .any(|token| {
                COMPARISON_OPERATORS
                    .iter()
                    .any(|operator| token.text.contains(operator))
                    || token.text.contains("??")
            })
    };
    Some(BoundarySite {
        first,
        last,
        text: if !admitted_operand(left)
            || !admitted_operand(right)
            || opaque_operator(left.span())
            || opaque_operator(right.span())
        {
            String::new()
        } else {
            text
        },
    })
}

fn parse_line(file: &Path, line: &str, allocator: &Allocator) -> Option<BoundaryLine> {
    let leading = line.trim_start().trim_start_matches('}').trim_start();
    let leading = leading.strip_prefix("else ").unwrap_or(leading);
    let returned = returned_expression(leading);
    let content = returned.unwrap_or(leading);
    // A declaration's concise arrow body and a return expression share one
    // expression wrapper. Control headers need only their missing block close.
    for suffix in ["", "\n}"] {
        let prefix = if returned.is_some() {
            "async function* __ripr_boundary__() {\nreturn "
        } else {
            "async function* __ripr_boundary__() {\n"
        };
        let source = format!("{prefix}{content}{suffix}\n}}");
        let parsed = Parser::new(allocator, &source, source_type_for(file))
            .with_config(TokensParserConfig)
            .parse();
        if parsed.panicked || !parsed.errors.is_empty() || parsed.program.body.len() != 1 {
            continue;
        }
        let Statement::FunctionDeclaration(wrapper) = &parsed.program.body[0] else {
            continue;
        };
        let statements = &wrapper.body.as_ref()?.statements;
        if statements.len() != 1 {
            continue;
        }
        let (expression, is_returned) = match &statements[0] {
            Statement::IfStatement(statement) => (&statement.test, false),
            Statement::WhileStatement(statement) => (&statement.test, false),
            Statement::ForStatement(statement) => (statement.test.as_ref()?, false),
            Statement::ReturnStatement(statement) => (statement.argument.as_ref()?, true),
            Statement::ExpressionStatement(statement) => match &statement.expression {
                Expression::YieldExpression(value) => (value.argument.as_ref()?, false),
                expression => (expression, false),
            },
            _ => return None,
        };
        let tokens: Vec<BoundaryToken> = parsed
            .tokens
            .iter()
            .filter_map(|token| {
                let start = token.start() as usize;
                let end = token.end() as usize;
                (start >= prefix.len() && end <= prefix.len() + content.len() && start < end).then(
                    || BoundaryToken {
                        text: source[start..end].to_string(),
                        start,
                        end,
                    },
                )
            })
            .collect();
        let root = ungroup(expression);
        let returned_condition = match root {
            Expression::ConditionalExpression(conditional) if is_returned => Some(
                tokens
                    .iter()
                    .filter(|token| {
                        token.start >= conditional.test.span().start as usize
                            && token.end <= conditional.test.span().end as usize
                    })
                    .map(|token| token.text.clone())
                    .collect(),
            ),
            _ => None,
        };
        let mut pending = vec![root];
        let mut sites = Vec::new();
        while let Some(expression) = pending.pop() {
            let expression = ungroup(expression);
            if comparison(expression) {
                sites.push(site_for(expression, &tokens)?);
                continue;
            }
            match expression {
                Expression::LogicalExpression(logical) => {
                    pending.push(&logical.right);
                    pending.push(&logical.left);
                }
                Expression::ConditionalExpression(conditional) => pending.push(&conditional.test),
                Expression::UnaryExpression(unary) if unary.operator.as_str() == "!" => {
                    pending.push(&unary.argument)
                }
                // Calls, callbacks, arrays, computed members and type syntax
                // own their nested expressions; they are not this predicate.
                _ => {}
            }
        }
        return Some(BoundaryLine {
            tokens,
            sites,
            returned_condition,
            direct: comparison(root),
        });
    }
    None
}

fn selected_text(new: &BoundaryLine, old: Option<&BoundaryLine>) -> Option<String> {
    let touched: Vec<&BoundarySite> = if let Some(old) = old {
        let shared = new.tokens.len().min(old.tokens.len());
        let prefix = (0..shared)
            .take_while(|&i| new.tokens[i].text == old.tokens[i].text)
            .count();
        if prefix == new.tokens.len() && new.tokens.len() == old.tokens.len() {
            return None;
        }
        let suffix = (0..shared - prefix)
            .take_while(|&back| {
                new.tokens[new.tokens.len() - 1 - back].text
                    == old.tokens[old.tokens.len() - 1 - back].text
            })
            .count();
        let end = new.tokens.len() - suffix;
        let removed_delimiter = old.tokens[prefix..old.tokens.len() - suffix]
            .iter()
            .any(|token| operand_delimiter(&token.text));
        new.sites
            .iter()
            .filter(|site| {
                // Two edits may surround an unchanged comparison. The edit
                // envelope alone cannot claim that middle comparison changed.
                let site_tokens = &new.tokens[site.first..=site.last];
                let unchanged_inside_edit = old.tokens[prefix..old.tokens.len() - suffix]
                    .windows(site_tokens.len())
                    .any(|window| {
                        window
                            .iter()
                            .zip(site_tokens)
                            .all(|(old, new)| old.text == new.text)
                    });
                if unchanged_inside_edit {
                    return false;
                }
                if prefix == end {
                    if removed_delimiter {
                        site.first < prefix && prefix <= site.last
                    } else {
                        site.first <= prefix && prefix <= site.last + 1
                    }
                } else {
                    prefix <= site.last && end > site.first
                }
            })
            .collect()
    } else {
        new.sites.iter().collect()
    };
    match touched.as_slice() {
        [site] => Some(site.text.clone()),
        [] if new.sites.len() == 1
            && new.returned_condition.is_some()
            && old.is_some_and(|old| old.returned_condition == new.returned_condition) =>
        {
            Some(new.sites[0].text.clone())
        }
        _ => None,
    }
}

/// One changed comparison, never a literal, nested expression or unchanged
/// neighbour. A failed parse or ambiguous edit retains the predicate refusal.
pub(crate) fn typescript_changed_boundary_text<'a>(
    file: &Path,
    family: &ProbeFamily,
    old_line: Option<&str>,
    new_line: &'a str,
) -> Cow<'a, str> {
    if *family != ProbeFamily::Predicate {
        return Cow::Borrowed(new_line);
    }
    let parse = |line: &str| {
        let line = one_line_function_first_statement(line);
        parse_on_worker(file, line, |file, line, allocator| {
            parse_line(file, line, allocator)
        })
        .ok()
        .flatten()
    };
    let Some(parsed_new) = parse(new_line) else {
        return Cow::Borrowed("");
    };
    let parsed_old = match old_line {
        Some(old) => match parse(old) {
            Some(parsed) => Some(parsed),
            None => return Cow::Borrowed(""),
        },
        None => None,
    };
    let Some(selected) = selected_text(&parsed_new, parsed_old.as_ref()) else {
        return Cow::Borrowed("");
    };
    let direct = parsed_new.direct;
    // Preserve whole-line source provenance for the existing module-bound
    // input reader only when the sole root comparison yields the same label.
    let boundary = typescript_boundary_discriminator_with_shape(&selected);
    if direct
        && boundary.is_some()
        && typescript_boundary_discriminator_with_shape(new_line) == boundary
    {
        Cow::Borrowed(new_line)
    } else {
        Cow::Owned(selected)
    }
}

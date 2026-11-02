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
    /// Token span `(first, last)` of the comparison that is the line's whole
    /// condition: the root expression, or the test of a root conditional,
    /// with only `!` and parentheses around it (#12281). `None` when any
    /// other operator (`&&`, `||`, a nested conditional) surrounds it.
    whole_condition: Option<(usize, usize)>,
}

/// The comparison that decides the whole condition of `root`, with only `!`
/// and parentheses stripped: the root itself, or the test of a root
/// conditional. Any logical, nested conditional or other wrapper fails
/// closed to `None` (#12281).
fn whole_condition_comparison<'a>(root: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let mut expression = ungroup(root);
    if let Expression::ConditionalExpression(conditional) = expression {
        expression = ungroup(&conditional.test);
    }
    while let Expression::UnaryExpression(unary) = expression {
        if unary.operator.as_str() != "!" {
            return None;
        }
        expression = ungroup(&unary.argument);
    }
    comparison(expression).then_some(expression)
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
        let whole_condition = whole_condition_comparison(root).and_then(|expression| {
            let span = expression.span();
            let first = tokens
                .iter()
                .position(|token| token.start >= span.start as usize)?;
            let last = tokens
                .iter()
                .rposition(|token| token.end <= span.end as usize)?;
            Some((first, last))
        });
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
            whole_condition,
        });
    }
    None
}

fn mirrored_operator(operator: &str) -> Option<&'static str> {
    Some(match operator {
        ">" => "<",
        "<" => ">",
        ">=" => "<=",
        "<=" => ">=",
        "==" => "==",
        "===" => "===",
        "!=" => "!=",
        "!==" => "!==",
        _ => return None,
    })
}

fn swap_parts<'a>(
    line: &'a BoundaryLine,
    site: &BoundarySite,
) -> Option<(Vec<&'a str>, &'a str, Vec<&'a str>)> {
    let tokens = &line.tokens[site.first..=site.last];
    let mut operators = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| COMPARISON_OPERATORS.contains(&token.text.as_str()));
    let (index, operator) = operators.next()?;
    if operators.next().is_some() {
        return None;
    }
    let left: Vec<&str> = tokens[..index].iter().map(|t| t.text.as_str()).collect();
    let right: Vec<&str> = tokens[index + 1..]
        .iter()
        .map(|t| t.text.as_str())
        .collect();
    Some((left, operator.text.as_str(), right))
}

/// A pure operand swap: the site's operands exchange places around its single
/// comparison operator, which becomes its mirror (unchanged for equality), so
/// the predicate is the same boundary written the other way round. Anything
/// else (`a > b` -> `b > a`, a swap plus another edit) is not a swap.
fn operand_swap(new: &BoundaryLine, old: &BoundaryLine, site: &BoundarySite) -> bool {
    if new.tokens.len() != old.tokens.len() {
        return false;
    }
    let Some(prior) = old
        .sites
        .iter()
        .find(|prior| prior.first == site.first && prior.last == site.last)
    else {
        return false;
    };
    // The swap must be the whole edit: every token outside the swapped site
    // is identical at the same index.
    let outside_identical = (0..new.tokens.len())
        .filter(|i| !(site.first..=site.last).contains(i))
        .all(|i| new.tokens[i].text == old.tokens[i].text);
    if !outside_identical {
        return false;
    }
    let (Some((new_left, new_op, new_right)), Some((old_left, old_op, old_right))) =
        (swap_parts(new, site), swap_parts(old, prior))
    else {
        return false;
    };
    !new_left.is_empty()
        && !new_right.is_empty()
        && new_left == old_right
        && new_right == old_left
        && mirrored_operator(old_op) == Some(new_op)
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
        // #11442: an added comparison clause may not be independently
        // steerable, so it names no boundary.
        if new.sites.len() > old.sites.len() {
            return None;
        }
        // #11442: more than one token replaced by more than zero tokens inside
        // one clause is not a single discriminator, unless it is a pure
        // operand swap of one comparison.
        let replaced_old = old.tokens.len() - suffix - prefix;
        let replaced_new = end - prefix;
        let multi_token_replacement =
            replaced_old > 0 && replaced_new > 0 && replaced_old.max(replaced_new) > 1;
        let removed_delimiter = old.tokens[prefix..old.tokens.len() - suffix]
            .iter()
            .any(|token| operand_delimiter(&token.text));
        new.sites
            .iter()
            .filter(|site| {
                // Two edits may surround an unchanged comparison. The edit
                // envelope alone cannot claim that middle comparison changed.
                if multi_token_replacement && !operand_swap(new, old, site) {
                    return false;
                }
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

/// `true` when `line` is a complete, self-contained `if (...)`,
/// `} else if (...)`, `while (...)` header or a `return ...;` statement:
/// parentheses and brackets balance on the line (outside string literals),
/// a `return` ends with `;`, and the line ends in no operator or opener that
/// would continue the expression on the next line (#12281 review). Any other
/// line (a bare expression, a continuation, a declaration, a one-line
/// function) fails closed to `false`.
fn self_contained_condition_line(line: &str) -> bool {
    let text = line.trim();
    let head = text.trim_start_matches('}').trim_start();
    let head = head.strip_prefix("else").map_or(head, str::trim_start);
    let keyword = ["if (", "if(", "while (", "while(", "return "]
        .into_iter()
        .find(|keyword| head.starts_with(keyword));
    let Some(keyword) = keyword else {
        return false;
    };
    if keyword == "return " && !text.ends_with(';') {
        return false;
    }
    const CONTINUATIONS: [&str; 17] = [
        "&&", "||", "??", "?", ":", ",", "(", "[", "+", "-", "*", "/", "%", "=", "!", "<", ">",
    ];
    if CONTINUATIONS.iter().any(|end| text.ends_with(end)) || text.ends_with(['|', '&']) {
        return false;
    }
    let mut depth_paren = 0_i64;
    let mut depth_bracket = 0_i64;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in text.chars() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => quote = Some(ch),
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            _ => {}
        }
        if depth_paren < 0 || depth_bracket < 0 {
            return false;
        }
    }
    quote.is_none() && depth_paren == 0 && depth_bracket == 0
}

/// The negated form of an equality operator: `===` and `!==`, `==` and `!=`.
fn negated_equality(operator: &str) -> Option<&'static str> {
    Some(match operator {
        "===" => "!==",
        "!==" => "===",
        "==" => "!=",
        "!=" => "==",
        _ => return None,
    })
}

/// `true` when the whole edit replaces exactly one equality operator of one
/// admitted comparison with its negation (`typeof input === "function"` to
/// `typeof input !== "function"`) and changes nothing else (#12281). Every
/// input that evaluates such a comparison gets the opposite result, so the
/// branch it takes flips for an input on either side of the comparison, not
/// only for the value or kind the comparison names. The comparison must be
/// the line's whole condition, with only `!` and parentheses around it: a
/// clause joined by `&&` / `||` or nested in another conditional may never be
/// evaluated, or its flip may not change the outcome, for a given input. A
/// failed parse, a multi-token edit, an added or removed clause, a compound
/// condition, or a site the boundary reader refuses all fail closed to
/// `false`.
pub(crate) fn typescript_changed_boundary_is_equality_negation(
    file: &Path,
    family: &ProbeFamily,
    old_line: Option<&str>,
    new_line: &str,
) -> bool {
    if *family != ProbeFamily::Predicate {
        return false;
    }
    let Some(old_line) = old_line else {
        return false;
    };
    // The changed line alone is parsed, so a continuation line of a
    // multi-line condition (`flag &&` above, `typeof input !== "function"`
    // here) would read as a whole condition. Only a self-contained header
    // line can carry the whole condition (#12281 review).
    if !self_contained_condition_line(old_line) || !self_contained_condition_line(new_line) {
        return false;
    }
    let parse = |line: &str| {
        let line = one_line_function_first_statement(line);
        parse_on_worker(file, line, |file, line, allocator| {
            parse_line(file, line, allocator)
        })
        .ok()
        .flatten()
    };
    let (Some(new), Some(old)) = (parse(new_line), parse(old_line)) else {
        return false;
    };
    if new.tokens.len() != old.tokens.len() || new.sites.len() != old.sites.len() {
        return false;
    }
    let mut changed = new
        .tokens
        .iter()
        .zip(&old.tokens)
        .enumerate()
        .filter(|(_, (new, old))| new.text != old.text);
    let Some((index, (new_token, old_token))) = changed.next() else {
        return false;
    };
    if changed.next().is_some()
        || negated_equality(&old_token.text) != Some(new_token.text.as_str())
    {
        return false;
    }
    // The flipped operator must be the single operator of one comparison the
    // boundary reader admits, never an operator nested inside another site.
    let mut owning = new
        .sites
        .iter()
        .filter(|site| site.first <= index && index <= site.last);
    match (owning.next(), owning.next()) {
        (Some(site), None) => {
            !site.text.is_empty()
                && new.whole_condition == Some((site.first, site.last))
                && old.whole_condition == Some((site.first, site.last))
                && swap_parts(&new, site).is_some_and(|(_, operator, _)| operator == new_token.text)
        }
        _ => false,
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

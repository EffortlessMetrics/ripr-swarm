//! Inline `test.each` / `it.each` table rows as concrete cases.
//!
//! `test.each([[999, "silver"], [1000, "gold"]])("...", (points, tier) =>
//! { expect(loyaltyTier(points)).toBe(tier); })` runs the callback once per
//! row. When every row is an inline array of literals in the same call and
//! the callback's plain parameters bind the row positions, each row is read
//! as the callback body with the parameters replaced by that row's literals:
//! the input literals reach the boundary witness and the expected literal is
//! the assertion's expected value, exactly as a hand-written test per row.
//!
//! Fail-closed: anything else returns `None` and the caller keeps the
//! row-parameter assertions (`typescript_table_case_unresolved`). That
//! covers a table that is a variable, a spread or a non-literal cell, object
//! rows, destructured, defaulted or rest parameters, rows whose length
//! differs from the parameter count, a parameter used anywhere other than a
//! plain read inside parentheses or brackets (a member name, an object key
//! or shorthand, an assignment, a statement of its own), a parameter
//! re-declared or shadowed in the body, a nested `function`, template
//! substitutions, regular-expression literals, and a substituted body that
//! does not re-parse.

use super::*;

/// Rows beyond this bound leave the table unresolved.
const MAX_TABLE_ROWS: usize = 64;

/// The assertions of every row of an inline literal table, or `None` when
/// the table is not resolvable (see the module documentation).
pub(crate) fn table_row_assertions(
    call: &oxc_ast::ast::CallExpression<'_>,
    callback: &oxc_ast::ast::Argument<'_>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let Expression::CallExpression(each_call) = &call.callee else {
        return None;
    };
    let rows = literal_table_rows(each_call, source)?;
    let (params, callback_span, body_span) = plain_callback_parameters(callback)?;
    if rows.iter().any(|row| row.len() != params.len()) {
        return None;
    }
    let text: &str = source;
    let body = text.get(body_span.clone())?;
    if params
        .iter()
        .any(|param| local_identifier_declared_in_test_body(body, param))
    {
        return None;
    }
    let mut assertions = Vec::new();
    for row in &rows {
        let substituted = substitute_row(body, &params, row)?;
        let callback_text = format!(
            "{}{}{}",
            text.get(callback_span.start..body_span.start)?,
            substituted,
            text.get(body_span.end..callback_span.end)?
        );
        assertions.extend(reparse_row_assertions(
            text,
            callback_span.start,
            &callback_text,
            bindings,
        )?);
    }
    Some(assertions)
}

/// The cell texts of `each(<array of arrays of literals>)`.
fn literal_table_rows(
    each_call: &oxc_ast::ast::CallExpression<'_>,
    source: &SourceText<'_>,
) -> Option<Vec<Vec<String>>> {
    let [table] = each_call.arguments.as_slice() else {
        return None;
    };
    let oxc_ast::ast::Argument::ArrayExpression(table) = table else {
        return None;
    };
    if table.elements.is_empty() || table.elements.len() > MAX_TABLE_ROWS {
        return None;
    }
    let text: &str = source;
    let mut rows = Vec::new();
    for element in &table.elements {
        let Expression::ArrayExpression(row) = element.as_expression()? else {
            return None;
        };
        let mut cells = Vec::new();
        for cell in &row.elements {
            let cell = cell.as_expression()?;
            if !is_literal_cell(cell) {
                return None;
            }
            let span = cell.span();
            let cell_text = text.get(span.start as usize..span.end as usize)?;
            if cell_text.contains(['\n', '\r']) {
                return None;
            }
            cells.push(cell_text.to_string());
        }
        rows.push(cells);
    }
    Some(rows)
}

fn is_literal_cell(cell: &Expression<'_>) -> bool {
    match cell {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::BigIntLiteral(_) => true,
        Expression::Identifier(ident) => ident.name == "undefined",
        Expression::UnaryExpression(unary) => {
            matches!(
                unary.operator,
                oxc_ast::ast::UnaryOperator::UnaryNegation | oxc_ast::ast::UnaryOperator::UnaryPlus
            ) && matches!(unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

/// Plain identifier parameter names, the callback span and its body span.
fn plain_callback_parameters(
    callback: &oxc_ast::ast::Argument<'_>,
) -> Option<(Vec<String>, std::ops::Range<usize>, std::ops::Range<usize>)> {
    let (params, callback_span, body_span) = match callback {
        oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
            (&arrow.params, arrow.span, arrow.body.span)
        }
        oxc_ast::ast::Argument::FunctionExpression(function) => (
            &function.params,
            function.span,
            function.body.as_ref()?.span,
        ),
        _ => return None,
    };
    if params.rest.is_some() || params.items.is_empty() {
        return None;
    }
    let mut names = Vec::new();
    for param in &params.items {
        let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return None;
        };
        if param.initializer.is_some() {
            return None;
        }
        names.push(ident.name.to_string());
    }
    Some((
        names,
        callback_span.start as usize..callback_span.end as usize,
        body_span.start as usize..body_span.end as usize,
    ))
}

const BINDING_KEYWORDS: [&str; 6] = ["const", "let", "var", "catch", "function", "class"];

/// The body with every read of a parameter replaced by its row cell, or
/// `None` when any occurrence is not a plain read (see the module docs).
fn substitute_row(body: &str, params: &[String], row: &[String]) -> Option<String> {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut brackets: Vec<u8> = Vec::new();
    let mut idx = 0;
    let mut copied = 0;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match byte {
            b'/' => match bytes.get(idx + 1) {
                Some(b'/') => idx = body[idx..].find('\n').map_or(bytes.len(), |end| idx + end),
                Some(b'*') => idx += 2 + body.get(idx + 2..)?.find("*/")? + 2,
                // A regular expression or a division: neither is lexed.
                _ => return None,
            },
            b'"' | b'\'' | b'`' => idx = skip_quoted(bytes, idx)?,
            b'(' | b'[' | b'{' => {
                brackets.push(byte);
                idx += 1;
            }
            b')' | b']' | b'}' => {
                brackets.pop();
                idx += 1;
            }
            _ if is_identifier_start(byte) => {
                let end = idx
                    + bytes[idx..]
                        .iter()
                        .take_while(|byte| is_identifier_part(**byte))
                        .count();
                let token = &body[idx..end];
                if token == "function" {
                    return None;
                }
                if let Some(position) = params.iter().position(|param| param == token) {
                    if !plain_read(body, idx, end, brackets.last().copied()) {
                        return None;
                    }
                    out.push_str(&body[copied..idx]);
                    out.push_str(row.get(position)?);
                    copied = end;
                }
                idx = end;
            }
            _ if byte.is_ascii_digit() => {
                idx += bytes[idx..]
                    .iter()
                    .take_while(|byte| is_identifier_part(**byte) || **byte == b'.')
                    .count();
            }
            _ => idx += 1,
        }
    }
    out.push_str(&body[copied..]);
    Some(out)
}

/// The offset after a quoted string or a substitution-free template that
/// starts at `start`; `None` for an unterminated one or a `${` substitution.
fn skip_quoted(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = bytes[start];
    let mut idx = start + 1;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\\' => idx += 2,
            b'$' if quote == b'`' && bytes.get(idx + 1) == Some(&b'{') => return None,
            byte if byte == quote => return Some(idx + 1),
            b'\n' if quote != b'`' => return None,
            _ => idx += 1,
        }
    }
    None
}

/// A parameter occurrence at `start..end` is a plain read: inside `(` or
/// `[`, not a member name, not an object key, not assigned, not an arrow
/// parameter and not a declaration.
fn plain_read(body: &str, start: usize, end: usize, innermost: Option<u8>) -> bool {
    if !matches!(innermost, Some(b'(' | b'[')) {
        return false;
    }
    let before = body[..start].trim_end();
    if before.ends_with('.') || before.ends_with("...") {
        return false;
    }
    let previous_word = before
        .trim_end_matches(['(', '[', ' ', '\t', '\n', '\r', ','])
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .next()
        .unwrap_or_default();
    if BINDING_KEYWORDS.contains(&previous_word) {
        return false;
    }
    let after = body[end..].trim_start();
    if after.starts_with(':')
        || after.starts_with("=>")
        || after.starts_with("++")
        || after.starts_with("--")
    {
        return false;
    }
    if after.starts_with('=') && !after.starts_with("==") {
        return false;
    }
    // Compound assignment (`+=`, `??=`, ...).
    let operator: String = after
        .chars()
        .take_while(|ch| "+-*/%&|^<>?!".contains(*ch))
        .collect();
    if !operator.is_empty()
        && after[operator.len()..].starts_with('=')
        && !after[operator.len()..].starts_with("==")
        && !matches!(operator.as_str(), "<" | ">" | "!")
    {
        return false;
    }
    if before.ends_with("++") || before.ends_with("--") {
        return false;
    }
    // An arrow parameter list: `(a, points) =>`.
    if innermost == Some(b'(') && arrow_parameter_list_follows(after) {
        return false;
    }
    true
}

/// `true` when `after` (the text following an identifier inside `(`) closes
/// a parameter list of identifiers and commas that an `=>` follows.
fn arrow_parameter_list_follows(after: &str) -> bool {
    let rest = after.trim_start_matches(|ch: char| {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == ',' || ch.is_whitespace()
    });
    rest.strip_prefix(')')
        .is_some_and(|rest| rest.trim_start().starts_with("=>"))
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_part(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

/// Re-parse one row's callback in place so assertion lines stay the
/// original lines: every byte before the callback becomes a space (newlines
/// kept) with `(` just before it, then the callback text and `);`.
fn reparse_row_assertions(
    original: &str,
    callback_start: usize,
    callback_text: &str,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let prefix = original.get(..callback_start)?;
    if callback_start == 0 || prefix.ends_with('\n') || prefix.ends_with('\r') {
        return None;
    }
    let mut synthesized: String = prefix
        .bytes()
        .map(|byte| if byte == b'\n' { '\n' } else { ' ' })
        .collect();
    synthesized.pop();
    synthesized.push('(');
    synthesized.push_str(callback_text);
    synthesized.push_str(");");
    for source_type in [SourceType::ts(), SourceType::tsx()] {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, &synthesized, source_type).parse();
        if !ret.errors.is_empty() {
            continue;
        }
        let [Statement::ExpressionStatement(statement)] = ret.program.body.as_slice() else {
            return None;
        };
        let statements = match statement.expression.get_inner_expression() {
            Expression::ArrowFunctionExpression(arrow) => &arrow.body.statements,
            Expression::FunctionExpression(function) => &function.body.as_ref()?.statements,
            _ => return None,
        };
        return Some(collect_assertions_in_statements_with_bindings(
            statements,
            &SourceText::new(&synthesized),
            None,
            bindings,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_row_replaces_plain_reads_only() {
        let params = ["points".to_string(), "tier".to_string()];
        let row = ["1000".to_string(), "\"gold\"".to_string()];
        assert_eq!(
            substitute_row(
                "{\n  expect(loyaltyTier(points)).toBe(tier);\n}",
                &params,
                &row
            )
            .as_deref(),
            Some("{\n  expect(loyaltyTier(1000)).toBe(\"gold\");\n}")
        );
        for body in [
            "{ points; }",
            "{ expect(o.points).toBe(tier); }",
            "{ expect(f({ points })).toBe(tier); }",
            "{ expect(f({ a: points })).toBe(tier); }",
            "{ const points = 2; expect(f(points)).toBe(tier); }",
            "{ expect(f((points) => 1)).toBe(tier); }",
            "{ expect(f(points = 2)).toBe(tier); }",
            "{ expect(f(points += 2)).toBe(tier); }",
            "{ expect(f(`${points}`)).toBe(tier); }",
            "{ expect(f(points / 2)).toBe(tier); }",
            "{ expect(f(function () { return points; })).toBe(tier); }",
        ] {
            assert_eq!(substitute_row(body, &params, &row), None, "{body}");
        }
        assert_eq!(
            substitute_row(
                "{ expect(f(\"points\", points == 1)).toBe(tier); // points\n}",
                &params,
                &row
            )
            .as_deref(),
            Some("{ expect(f(\"points\", 1000 == 1)).toBe(\"gold\"); // points\n}")
        );
    }
}

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
//! substitutions, regular-expression literals, a non-ASCII parameter or
//! body, and a substituted body that does not re-parse.

use super::*;

/// Rows beyond this bound leave the table unresolved.
const MAX_TABLE_ROWS: usize = 64;

/// Lexer-independent ban on a statement list that holds dynamic-code routes
/// (`eval`, `with`, a `\u` escape) anywhere, even in a string: such a list
/// could rewrite a table the scanner counted as untouched. Shared by every
/// path that decides a table is never reassigned by counting its name.
pub(crate) fn statement_list_has_dynamic_code(list_text: &str) -> bool {
    super::match_object_guard::whole_word_count(list_text, "eval") > 0
        || super::match_object_guard::whole_word_count(list_text, "with") > 0
        || list_text.contains("\\u")
}

/// The one guard every row-table entry path runs on its whole call text
/// (registration plus callback) before a row literal is substituted.
///
/// A row table is read as concrete literals only when nothing in the call
/// can reach the shared rows or the parameters by a route the scanner does
/// not follow: `eval`, `Function`, `with`, `arguments`, `this`, `new.target`,
/// and any backslash outside a string or template (`\u` identifier escapes)
/// are refused wherever they appear in code, and so is a `/` that could open
/// a regular expression. The lexer pass skips quoted text and comments, but
/// the whole-word backstop below does not: those words are refused even
/// inside strings and comments, so a title such as `"this works"` or
/// `"handles input with spaces"` is deliberately over-refused (fail closed).
/// Fails closed on anything it cannot lex.
pub(crate) fn row_text_is_inert(text: &str) -> bool {
    // Lexer-independent backstop: the same words anywhere, even in a string,
    // so a lexing gap (an odd regex or quote) cannot hide one.
    if ["eval", "with", "Function", "arguments", "this"]
        .iter()
        .any(|word| super::match_object_guard::whole_word_count(text, word) > 0)
        || text.contains("\\u")
    {
        return false;
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Template,
        Expr(usize),
    }
    let bytes = text.as_bytes();
    let mut modes: Vec<Mode> = vec![Mode::Expr(0)];
    let mut idx = 0;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match modes.last().copied() {
            Some(Mode::Template) => match byte {
                b'\\' => idx += 2,
                b'`' => {
                    modes.pop();
                    idx += 1;
                }
                b'$' if bytes.get(idx + 1) == Some(&b'{') => {
                    modes.push(Mode::Expr(0));
                    idx += 2;
                }
                _ => idx += 1,
            },
            Some(Mode::Expr(depth)) => match byte {
                b'\\' => return false,
                b'`' => {
                    modes.push(Mode::Template);
                    idx += 1;
                }
                b'"' | b'\'' => {
                    let mut at = idx + 1;
                    loop {
                        match bytes.get(at) {
                            None | Some(b'\n') => return false,
                            Some(b'\\') => at += 2,
                            Some(quote) if *quote == byte => break,
                            Some(_) => at += 1,
                        }
                    }
                    idx = at + 1;
                }
                b'/' => match bytes.get(idx + 1) {
                    Some(b'/') => {
                        idx = text[idx..].find('\n').map_or(bytes.len(), |end| idx + end);
                    }
                    Some(b'*') => match text.get(idx + 2..).and_then(|rest| rest.find("*/")) {
                        Some(end) => idx += 2 + end + 2,
                        None => return false,
                    },
                    // Any other `/` is a division or a regex literal, and a
                    // lexer cannot tell them apart without parsing: refuse.
                    _ => return false,
                },
                b'{' => {
                    if let Some(last) = modes.last_mut() {
                        *last = Mode::Expr(depth + 1);
                    }
                    idx += 1;
                }
                b'}' => {
                    if depth == 0 {
                        if modes.len() == 1 {
                            return false;
                        }
                        modes.pop();
                    } else if let Some(last) = modes.last_mut() {
                        *last = Mode::Expr(depth - 1);
                    }
                    idx += 1;
                }
                _ if is_identifier_start(byte) => {
                    let end = idx
                        + bytes[idx..]
                            .iter()
                            .take_while(|byte| is_identifier_part(**byte))
                            .count();
                    let word = &text[idx..end];
                    if matches!(word, "eval" | "with" | "Function" | "arguments" | "this") {
                        return false;
                    }
                    if word == "new" && text[end..].trim_start().starts_with('.') {
                        return false;
                    }
                    idx = end;
                }
                _ if byte.is_ascii_whitespace() => idx += 1,
                _ => {
                    idx += 1;
                }
            },
            None => return false,
        }
    }
    modes.len() == 1
}

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
    let text: &str = source;
    if !text
        .get(call.span.start as usize..call.span.end as usize)
        .is_some_and(row_text_is_inert)
    {
        return None;
    }
    let Some(rows) = literal_table_rows(each_call, source) else {
        // Object rows (`{ input, out }`) read through `row.key` (#11196).
        return object_each_assertions(each_call, callback, source, bindings);
    };
    callback_case_assertions(callback, &rows, source, bindings, true)
}

/// The assertions of `callback` read once per row, each row's cells
/// replacing the callback's plain parameters one to one, or `None` when
/// any row fails closed (see the module documentation). Shared with the
/// RIPR-SPEC-0246 nested-callback forms (`[1, 2].forEach(cb)`, a returned
/// `p.then(cb)`).
pub(crate) fn callback_case_assertions(
    callback: &oxc_ast::ast::Argument<'_>,
    rows: &[Vec<String>],
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
    returns_to_runner: bool,
) -> Option<Vec<TypeScriptAssertion>> {
    let (params, callback_span, body_span) = plain_callback_parameters(callback)?;
    if rows.iter().any(|row| row.len() != params.len()) {
        return None;
    }
    let text: &str = source;
    let body = text.get(body_span.clone())?;
    // A non-ASCII identifier could abut a parameter name the ASCII
    // scanner would then substitute inside it: fail closed.
    if !body.is_ascii() {
        return None;
    }
    if params
        .iter()
        .any(|param| local_identifier_declared_in_test_body(body, param))
    {
        return None;
    }
    let mut assertions = Vec::new();
    for row in rows {
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
            returns_to_runner,
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
    literal_array_rows(table, source)
}

/// The cell texts of an inline array of arrays of one-line literals.
pub(crate) fn literal_array_rows(
    table: &oxc_ast::ast::ArrayExpression<'_>,
    source: &SourceText<'_>,
) -> Option<Vec<Vec<String>>> {
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

pub(crate) fn is_literal_cell(cell: &Expression<'_>) -> bool {
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
        // The substitution scanner reads ASCII identifiers only.
        if !ident.name.is_ascii() {
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
pub(crate) fn substitute_row(body: &str, params: &[String], row: &[String]) -> Option<String> {
    substitute_row_with(body, params, row, false)
}

/// [`substitute_row`], optionally accepting a spread of a container cell
/// (`f(...cell)`): only the destructured object-row route does. A scalar
/// local is a copy of its cell; a container local references the row's array
/// or object and is safe only because the callback is one `expect(..)`
/// statement that reads the local at most once as a direct argument.
pub(crate) fn substitute_row_with(
    body: &str,
    params: &[String],
    row: &[String],
    allow_container_spread: bool,
) -> Option<String> {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut brackets: Vec<u8> = Vec::new();
    let mut opens: Vec<usize> = Vec::new();
    let mut container_seen: Vec<usize> = Vec::new();
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
                opens.push(idx);
                idx += 1;
            }
            b')' | b']' | b'}' => {
                brackets.pop();
                opens.pop();
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
                    let cell = row.get(position)?;
                    // A container cell is read only as a bare value
                    // (argument or element), never through a further
                    // member that could mutate the shared row.
                    let container = cell.starts_with(['[', '{']);
                    // (see `container_read_is_direct`: one direct read only)
                    if !plain_read_with(
                        body,
                        idx,
                        end,
                        brackets.last().copied(),
                        container && allow_container_spread,
                    ) || (container
                        && (container_seen.contains(&position)
                            || !super::object_rows::single_expect_statement(body)
                            || !super::object_rows::container_read_is_direct(
                                body,
                                idx,
                                end,
                                opens.last().copied(),
                            )))
                    {
                        return None;
                    }
                    if container {
                        container_seen.push(position);
                    }
                    out.push_str(&body[copied..idx]);
                    out.push_str(cell);
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
pub(crate) fn skip_quoted(bytes: &[u8], start: usize) -> Option<usize> {
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
/// Optionally accepting a spread (`f(...cell)`): only a container cell,
/// whose spread reads the same elements as the literal.
pub(crate) fn plain_read_with(
    body: &str,
    start: usize,
    end: usize,
    innermost: Option<u8>,
    allow_spread: bool,
) -> bool {
    if !matches!(innermost, Some(b'(' | b'[')) {
        return false;
    }
    let before = body[..start].trim_end();
    if before.ends_with("...") {
        if !allow_spread {
            return false;
        }
    } else if before.ends_with('.') {
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
pub(crate) fn reparse_row_assertions(
    original: &str,
    callback_start: usize,
    callback_text: &str,
    bindings: &TypeScriptAssertionBindings,
    returns_to_runner: bool,
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
        let (statements, expression_body) = match statement.expression.get_inner_expression() {
            Expression::ArrowFunctionExpression(arrow) => {
                (&arrow.body.statements, arrow.expression)
            }
            Expression::FunctionExpression(function) => {
                (&function.body.as_ref()?.statements, false)
            }
            _ => return None,
        };
        return Some(collect_callback_assertions(
            statements,
            &SourceText::new(&synthesized),
            None,
            bindings,
            returns_to_runner,
            // A discarded callback result (`forEach`) leaves an
            // expression-bodied assertion floating.
            expression_body && returns_to_runner,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn row_text_guard_refuses_hidden_mutation_routes_outside_quotes() {
        for ok in [
            "test.each([{ a: 1 }])(\"plain title\", (t) => { expect(f(t.a)).toBe(1); })",
            "test(`${t.a} -> x`, () => { expect(f(t.a)).toBe(1); // note\n })",
            "test(\"x\", () => { /* a note */ expect(f(t.a)).toBe(1); })",
        ] {
            assert!(super::row_text_is_inert(ok), "{ok}");
        }
        for bad in [
            "(t) => { expect(f(t.a, arguments[0].out = 1)).toBe(t.out); }",
            "(t) => { expect(f(t.a, eval(\"t.out = 1\"))).toBe(t.out); }",
            "(t) => { expect(f(t.a, (0, eval)(\"1\"))).toBe(t.out); }",
            "(t) => { expect(f(t.a, globalThis.eval(\"1\"))).toBe(t.out); }",
            "(t) => { expect(f(t.a, new Function(\"1\")())).toBe(t.out); }",
            "(t) => { expect(f(t.a, this.x)).toBe(t.out); }",
            "(t) => { expect(f(t.a, new.target)).toBe(t.out); }",
            "(t) => { expect(f(t.a, t.\\u006fut = 1)).toBe(t.out); }",
            "(t) => { with (t) { expect(f(a)).toBe(out); } }",
            "(t) => { expect(f(t.a, x = /\"/)).toBe(t.out); }",
            "(t) => { expect(f(t.a / 2)).toBe(t.out); }",
            "(t) => { /* unterminated expect(f(t.a)).toBe(t.out); }",
            "typeof /'a/, (t) => { expect(f(t.a, eval(\"t.out=3\"), \"it's\")).toBe(t.out); }, typeof /\"a/",
            "typeof /'a/, (t) => { expect(f(t.a, arguments[0].out = 3, \"it's\")).toBe(t.out); }, typeof /\"a/",
            "typeof /'a/, (t) => { with (t) { expect(f(a, \"it's\")).toBe(out); } }, typeof /\"a/",
            "(t) => { expect(f(t.a, \"this\")).toBe(t.out); }",
            "test(\"handles input with spaces\", (t) => { expect(f(t.a)).toBe(t.out); })",
            "(t) => { expect(f(`${arguments[0]}`)).toBe(t.out); }",
        ] {
            assert!(!super::row_text_is_inert(bad), "{bad}");
        }
    }

    use super::*;

    /// The row re-parse fails closed: a synthesized callback that does not
    /// parse, or a callback at the start of a line (no room for the `(`
    /// prefix), yields `None`, which leaves the whole table unresolved. A
    /// well-formed callback yields its assertion on the original line.
    #[test]
    fn reparse_row_assertions_fails_closed() -> Result<(), String> {
        let bindings = TypeScriptAssertionBindings::default();
        let original = "test.each([[1, 2]])(\"t\", (a, b) => {\n  expect(f(a)).toBe(b);\n});\n";
        let start = original
            .find("(a, b)")
            .ok_or_else(|| "fixture has no callback".to_string())?;
        let parsed = reparse_row_assertions(
            original,
            start,
            "(a, b) => {\n  expect(f(1)).toBe(2);\n}",
            &bindings,
            true,
        );
        assert_eq!(parsed.as_ref().map(Vec::len), Some(1), "{parsed:?}");
        assert_eq!(
            parsed
                .as_ref()
                .and_then(|assertions| assertions.first())
                .map(|assertion| assertion.line),
            Some(2)
        );
        assert_eq!(
            reparse_row_assertions(
                original,
                start,
                "(a, b) => {\n  expect(f(1)).toBe(2;\n}",
                &bindings,
                true
            ),
            None
        );
        assert_eq!(
            reparse_row_assertions("(a) => {}", 0, "(a) => {}", &bindings, true),
            None
        );
        Ok(())
    }

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

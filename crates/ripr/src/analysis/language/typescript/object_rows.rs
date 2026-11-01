//! Object-row tables as concrete cases (RIPR-SPEC-0243 rule 11, amended for
//! #11196).
//!
//! `const tests = [{ input: "a b", out: "a%20b" }, ...];
//! for (const t of tests) { test(t.input, () => { expect(encode(t.input))
//! .toBe(t.out); }); }` and `test.each([{ input, out }, ...])("..", (t) =>
//! { ... t.out ... })` run the callback once per row, so each row is read as
//! the callback with `t.input` / `t.out` replaced by that row's literals,
//! exactly as the array-row forms in [`table_rows`](super::table_rows) are.
//! The expected value is then a concrete literal and the existing classifier
//! decides the finding; this module never classifies and never grants credit
//! itself. The resulting assertions are the concatenation over every row.
//!
//! Fail-closed. A `for...of` row table must be a `const` array literal of
//! object literals declared in the same statement list as the loop, named
//! exactly twice in that list (its declaration and the loop header: no
//! reassignment, `push`, spread, `concat`, alias, export or second read can
//! reach it), with no `eval`, `with` or `\u` escape, and the loop body must be
//! exactly one test registration whose callback takes no parameter. A row is
//! an object literal without spread, computed key, method, accessor, duplicate
//! key or `__proto__`. Every key the callback reads must hold a one-line
//! literal in EVERY row (scalar, `undefined`, or an all-literal nested array
//! or object); one row that misses the key or holds an identifier, call or
//! spread there leaves the whole table unresolved and the caller keeps the
//! dynamic expected value. The row parameter may appear only as a plain
//! `param.key` read: a bare `param`, a computed `param[key]`, an optional
//! chain, an assignment, a declaration, a shadowing function parameter or a
//! container cell that is read through a further member (it could be mutated
//! through the shared row) fails closed.

use super::table_rows::{plain_read_with, skip_quoted};
use super::*;

/// Rows beyond this bound leave the table unresolved.
const MAX_OBJECT_ROWS: usize = 64;

/// One row of an object table: each key's one-line literal text, or `None`
/// for a key whose value is not a literal.
#[derive(Clone, Debug)]
pub(crate) struct ObjectRow {
    cells: Vec<(String, Option<String>)>,
}

impl ObjectRow {
    fn cell(&self, key: &str) -> Option<&str> {
        self.cells
            .iter()
            .find(|(name, _)| name == key)
            .and_then(|(_, cell)| cell.as_deref())
    }
}

/// A `for (const <param> of <table const>)` loop whose single registration
/// may read the table's rows, and the scope depth that registration sits at.
#[derive(Clone, Debug)]
pub(crate) struct RowLoop {
    depth: usize,
    param: String,
    rows: Vec<ObjectRow>,
}

impl RowLoop {
    /// Whether a registration list at `levels` scope levels is the loop's
    /// own body list (the loop pushes one level, its body list another).
    pub(crate) fn applies_at(&self, levels: usize) -> bool {
        self.depth + 1 == levels
    }
}

/// A one-line literal a row cell may hold.
fn is_row_literal(expression: &Expression<'_>) -> bool {
    is_literal_cell(expression) || is_literal_container_expression(expression)
}

/// The rows of an inline array of object literals, or `None` when any
/// element is not a plain object literal (see the module documentation).
pub(crate) fn object_array_rows(
    table: &oxc_ast::ast::ArrayExpression<'_>,
    source: &SourceText<'_>,
) -> Option<Vec<ObjectRow>> {
    if table.elements.is_empty() || table.elements.len() > MAX_OBJECT_ROWS {
        return None;
    }
    let text: &str = source;
    let mut rows = Vec::new();
    for element in &table.elements {
        let Expression::ObjectExpression(object) = element.as_expression()? else {
            return None;
        };
        let mut cells: Vec<(String, Option<String>)> = Vec::new();
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return None;
            };
            if property.computed
                || property.method
                || !matches!(property.kind, oxc_ast::ast::PropertyKind::Init)
            {
                return None;
            }
            let key = match &property.key {
                PropertyKey::StaticIdentifier(id) => id.name.to_string(),
                PropertyKey::StringLiteral(literal) => literal.value.to_string(),
                _ => return None,
            };
            if key == "__proto__" || cells.iter().any(|(name, _)| *name == key) {
                return None;
            }
            let cell = if property.shorthand || !is_row_literal(&property.value) {
                None
            } else {
                let span = property.value.span();
                text.get(span.start as usize..span.end as usize)
                    .filter(|cell| !cell.contains(['\n', '\r']))
                    .map(str::to_string)
            };
            cells.push((key, cell));
        }
        rows.push(ObjectRow { cells });
    }
    Some(rows)
}

/// The row table a `for (const <param> of <table>) <one registration>` loop
/// iterates, when the table is a never-touched `const` array of object
/// literals declared before the loop in `statements`.
pub(crate) fn row_loop_of(
    stmt: &Statement<'_>,
    statements: &[Statement<'_>],
    loop_body: &[Statement<'_>],
    source: &SourceText<'_>,
    depth: usize,
) -> Option<RowLoop> {
    let Statement::ForOfStatement(for_of) = stmt else {
        return None;
    };
    if for_of.r#await {
        return None;
    }
    let oxc_ast::ast::ForStatementLeft::VariableDeclaration(declaration) = &for_of.left else {
        return None;
    };
    if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const {
        return None;
    }
    let [declarator] = declaration.declarations.as_slice() else {
        return None;
    };
    let oxc_ast::ast::BindingPattern::BindingIdentifier(param) = &declarator.id else {
        return None;
    };
    let Expression::Identifier(table_name) = for_of.right.get_inner_expression() else {
        return None;
    };
    let param = param.name.to_string();
    let table_name = table_name.name.to_string();
    if !param.is_ascii() || param == table_name {
        return None;
    }
    // The loop registers exactly one test and nothing else, so no sibling
    // statement (`continue`, an early `return`, a reassignment) can make a
    // row skip it or change what it reads.
    let [only] = loop_body else {
        return None;
    };
    let Statement::ExpressionStatement(registration) = only else {
        return None;
    };
    let Expression::CallExpression(call) = &registration.expression else {
        return None;
    };
    let (first, last) = (statements.first()?, statements.last()?);
    let text: &str = source;
    if !text
        .get(registration.span.start as usize..registration.span.end as usize)
        .is_some_and(super::table_rows::row_text_is_inert)
    {
        return None;
    }
    let list_text = text.get(first.span().start as usize..last.span().end as usize)?;
    if super::match_object_guard::whole_word_count(list_text, &table_name) != 2
        || super::table_rows::statement_list_has_dynamic_code(list_text)
    {
        return None;
    }
    let mut table = None;
    for statement in statements {
        let Statement::VariableDeclaration(declaration) = statement else {
            continue;
        };
        for declarator in &declaration.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };
            if id.name != table_name.as_str() {
                continue;
            }
            if declaration.kind != oxc_ast::ast::VariableDeclarationKind::Const
                || declaration.declare
                || declaration.span.end > for_of.span.start
                || table.is_some()
            {
                return None;
            }
            let Some(Expression::ArrayExpression(array)) = declarator
                .init
                .as_ref()
                .map(Expression::get_inner_expression)
            else {
                return None;
            };
            table = object_array_rows(array, source);
        }
    }
    let rows = table?;
    if !registration_arguments_are_pure(call, &param, &rows, source) {
        return None;
    }
    Some(RowLoop { depth, param, rows })
}

/// The registration's callee and every argument other than the callback must
/// not use the loop parameter, because `test(fix(t), ..)` or
/// `test(String(Object.assign(t.input, ..)), ..)` could change the shared
/// row before the callback runs. The one allowed use is a title that is
/// exactly `t.key` or `t.key.toString()` over a scalar cell in every row.
fn registration_arguments_are_pure(
    call: &oxc_ast::ast::CallExpression<'_>,
    param: &str,
    rows: &[ObjectRow],
    source: &SourceText<'_>,
) -> bool {
    use super::match_object_guard::whole_word_count;
    let text: &str = source;
    let slice = |span: oxc_span::Span| text.get(span.start as usize..span.end as usize);
    let Some(callee) = slice(call.callee.span()) else {
        return false;
    };
    if whole_word_count(callee, param) > 0 {
        return false;
    }
    let callback_index = call.arguments.iter().rposition(|argument| {
        matches!(
            argument,
            oxc_ast::ast::Argument::ArrowFunctionExpression(_)
                | oxc_ast::ast::Argument::FunctionExpression(_)
        )
    });
    for (index, argument) in call.arguments.iter().enumerate() {
        if Some(index) == callback_index {
            continue;
        }
        let Some(argument_text) = slice(argument.span()) else {
            return false;
        };
        if whole_word_count(argument_text, param) == 0 {
            continue;
        }
        // A template title whose every substitution is a plain scalar
        // `t.key` read is as pure as the bare read.
        if let oxc_ast::ast::Argument::TemplateLiteral(template) = argument {
            let pure = template.expressions.iter().all(|expression| {
                slice(expression.span()).is_some_and(|inner| {
                    scalar_key_title(&inner.split_whitespace().collect::<String>(), param, rows)
                })
            });
            if pure {
                continue;
            }
            return false;
        }
        let compact: String = argument_text.split_whitespace().collect();
        if !scalar_key_title(&compact, param, rows) {
            return false;
        }
    }
    true
}

/// `true` when `compact` is exactly `param.key` or `param.key.toString()`
/// over a scalar (non-container) cell in every row.
fn scalar_key_title(compact: &str, param: &str, rows: &[ObjectRow]) -> bool {
    let Some(rest) = compact
        .strip_prefix(param)
        .and_then(|r| r.strip_prefix('.'))
    else {
        return false;
    };
    let key = rest.strip_suffix(".toString()").unwrap_or(rest);
    !key.is_empty()
        && key.bytes().all(ident_part)
        && rows.iter().all(|row| {
            row.cell(key)
                .is_some_and(|cell| !cell.starts_with(['[', '{']))
        })
}

/// The assertions of a registration inside a [`RowLoop`] read once per row,
/// or `None` when the callback is not that shape (see the module docs).
pub(crate) fn row_loop_assertions(
    callback: &oxc_ast::ast::Argument<'_>,
    row_loop: &RowLoop,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let (no_parameters, callback_span, body_span) = callback_parts(callback)?;
    if !no_parameters {
        return None;
    }
    member_case_assertions(
        callback_span,
        body_span,
        &row_loop.param,
        &row_loop.rows,
        source,
        bindings,
    )
}

/// The assertions of `test.each([{..}, ..])(title, (row) => ..)` read once
/// per row, for a callback with one plain identifier parameter or one object
/// pattern of plain keys; `None` for anything else.
pub(crate) fn object_each_assertions(
    each_call: &oxc_ast::ast::CallExpression<'_>,
    callback: &oxc_ast::ast::Argument<'_>,
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let [oxc_ast::ast::Argument::ArrayExpression(table)] = each_call.arguments.as_slice() else {
        return None;
    };
    let rows = object_array_rows(table, source)?;
    let params = match callback {
        oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => &arrow.params,
        oxc_ast::ast::Argument::FunctionExpression(function) => &function.params,
        _ => return None,
    };
    let (_, callback_span, body_span) = callback_parts(callback)?;
    let [param] = params.items.as_slice() else {
        return None;
    };
    if params.rest.is_some() || param.initializer.is_some() {
        return None;
    }
    match &param.pattern {
        oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => {
            if !ident.name.is_ascii() {
                return None;
            }
            member_case_assertions(
                callback_span,
                body_span,
                ident.name.as_str(),
                &rows,
                source,
                bindings,
            )
        }
        oxc_ast::ast::BindingPattern::ObjectPattern(pattern) => {
            if pattern.rest.is_some() {
                return None;
            }
            // `({ input, out: want }) => ..`: local name -> row key.
            let mut locals: Vec<(String, String)> = Vec::new();
            for property in &pattern.properties {
                if property.computed {
                    return None;
                }
                let PropertyKey::StaticIdentifier(key) = &property.key else {
                    return None;
                };
                let oxc_ast::ast::BindingPattern::BindingIdentifier(local) = &property.value else {
                    return None;
                };
                if !local.name.is_ascii() || !key.name.is_ascii() {
                    return None;
                }
                // A repeated key (`{ input: a, input: b }`) binds two names to
                // one shared cell: refuse it.
                if locals.iter().any(|(name, row_key)| {
                    name.as_str() == local.name.as_str() || row_key.as_str() == key.name.as_str()
                }) {
                    return None;
                }
                locals.push((local.name.to_string(), key.name.to_string()));
            }
            if locals.is_empty() {
                return None;
            }
            destructured_case_assertions(callback_span, body_span, &locals, &rows, source, bindings)
        }
        _ => None,
    }
}

/// Whether the callback has no parameter, its span and its body span.
fn callback_parts(
    callback: &oxc_ast::ast::Argument<'_>,
) -> Option<(bool, std::ops::Range<usize>, std::ops::Range<usize>)> {
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
    Some((
        params.items.is_empty() && params.rest.is_none(),
        callback_span.start as usize..callback_span.end as usize,
        body_span.start as usize..body_span.end as usize,
    ))
}

fn member_case_assertions(
    callback_span: std::ops::Range<usize>,
    body_span: std::ops::Range<usize>,
    param: &str,
    rows: &[ObjectRow],
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let text: &str = source;
    let body = text.get(body_span.clone())?;
    // A non-ASCII identifier could abut the parameter name the ASCII scanner
    // would then substitute inside it: fail closed.
    if !body.is_ascii() || local_identifier_declared_in_test_body(body, param) {
        return None;
    }
    let mut assertions = Vec::new();
    for row in rows {
        let (substituted, reads) = substitute_member_row(body, param, row)?;
        // A callback that never reads the row would repeat itself per row.
        if reads == 0 {
            return None;
        }
        assertions.extend(reparse_row_callback(
            text,
            &callback_span,
            &body_span,
            &substituted,
            bindings,
        )?);
    }
    Some(assertions)
}

fn destructured_case_assertions(
    callback_span: std::ops::Range<usize>,
    body_span: std::ops::Range<usize>,
    locals: &[(String, String)],
    rows: &[ObjectRow],
    source: &SourceText<'_>,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let text: &str = source;
    let body = text.get(body_span.clone())?;
    if !body.is_ascii()
        || locals
            .iter()
            .any(|(local, _)| local_identifier_declared_in_test_body(body, local))
    {
        return None;
    }
    let names: Vec<String> = locals.iter().map(|(local, _)| local.clone()).collect();
    let mut assertions = Vec::new();
    for row in rows {
        let cells: Vec<String> = locals
            .iter()
            .map(|(_, key)| row.cell(key).map(str::to_string))
            .collect::<Option<_>>()?;
        let substituted = super::table_rows::substitute_row_with(body, &names, &cells, true)?;
        assertions.extend(reparse_row_callback(
            text,
            &callback_span,
            &body_span,
            &substituted,
            bindings,
        )?);
    }
    Some(assertions)
}

fn reparse_row_callback(
    text: &str,
    callback_span: &std::ops::Range<usize>,
    body_span: &std::ops::Range<usize>,
    substituted: &str,
    bindings: &TypeScriptAssertionBindings,
) -> Option<Vec<TypeScriptAssertion>> {
    let callback_text = format!(
        "{}{}{}",
        text.get(callback_span.start..body_span.start)?,
        substituted,
        text.get(body_span.end..callback_span.end)?
    );
    reparse_row_assertions(text, callback_span.start, &callback_text, bindings, true)
}

/// The body with every `param.key` read replaced by that row's literal for
/// `key`, and how many were replaced; `None` when any occurrence of the
/// parameter is not a plain `param.key` read, or any key read is not a
/// literal in this row.
pub(crate) fn substitute_member_row(
    body: &str,
    param: &str,
    row: &ObjectRow,
) -> Option<(String, usize)> {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut brackets: Vec<u8> = Vec::new();
    let mut opens: Vec<usize> = Vec::new();
    let mut container_reads: Vec<String> = Vec::new();
    let mut idx = 0;
    let mut copied = 0;
    let mut reads = 0;
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
            _ if ident_start(byte) => {
                let end = idx
                    + bytes[idx..]
                        .iter()
                        .take_while(|byte| ident_part(**byte))
                        .count();
                let token = &body[idx..end];
                if token == "function" {
                    return None;
                }
                if token != param {
                    idx = end;
                    continue;
                }
                // Only `param.key`, with no space, as one plain read.
                if bytes.get(end) != Some(&b'.')
                    || !bytes.get(end + 1).is_some_and(|b| ident_start(*b))
                {
                    return None;
                }
                let key_end = end
                    + 1
                    + bytes[end + 1..]
                        .iter()
                        .take_while(|byte| ident_part(**byte))
                        .count();
                let cell = row.cell(&body[end + 1..key_end])?;
                let container = cell.starts_with(['[', '{']);
                if !plain_read_with(body, idx, key_end, brackets.last().copied(), container) {
                    return None;
                }
                // Every row read, scalar or container, is trusted only in a
                // callback that is exactly one `expect(..).matcher(..)`
                // statement and only as a direct argument of the call that
                // is the whole argument of `expect(` or of the matcher call.
                // Another statement or any wrapping (`delete t.out`, a comma
                // list, `void`, an array, a member) could change what the
                // runtime reads, so it is never credited on the literal.
                if !single_expect_statement(body)
                    || !container_read_is_direct(body, idx, key_end, opens.last().copied())
                {
                    return None;
                }
                // A container cell is shared by every read of the row, so it
                // is read at most once.
                if container {
                    let key = &body[end + 1..key_end];
                    if container_reads.iter().any(|read| read == key) {
                        return None;
                    }
                    container_reads.push(key.to_string());
                }
                out.push_str(&body[copied..idx]);
                out.push_str(cell);
                copied = key_end;
                reads += 1;
                idx = key_end;
            }
            _ if byte.is_ascii_digit() => {
                idx += bytes[idx..]
                    .iter()
                    .take_while(|byte| ident_part(**byte) || **byte == b'.')
                    .count();
            }
            _ => idx += 1,
        }
    }
    out.push_str(&body[copied..]);
    Some((out, reads))
}

fn ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn ident_part(byte: u8) -> bool {
    ident_start(byte) || byte.is_ascii_digit()
}

/// `true` when `body` (a callback body, braces optional) is exactly one
/// `expect(..).matcher(..)` chain: at bracket depth zero only the `expect`
/// identifier (optionally after `await`/`return`) and `.member` names, no
/// string, comment, second statement or `;` before the end.
pub(crate) fn single_expect_statement(body: &str) -> bool {
    let mut text = body.trim();
    if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        text = inner.trim();
    }
    let text = text.strip_suffix(';').unwrap_or(text).trim();
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut idx = 0;
    let mut seen_expect = false;
    let mut previous_dot = false;
    while idx < bytes.len() {
        let byte = bytes[idx];
        match byte {
            b'(' | b'[' | b'{' => {
                depth += 1;
                idx += 1;
            }
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                idx += 1;
            }
            _ if depth > 0 => match byte {
                b'"' | b'\'' | b'`' => match skip_quoted(bytes, idx) {
                    Some(next) => idx = next,
                    None => return false,
                },
                b'/' if matches!(bytes.get(idx + 1), Some(b'/' | b'*')) => return false,
                _ => idx += 1,
            },
            b'.' => {
                previous_dot = true;
                idx += 1;
            }
            _ if byte.is_ascii_whitespace() => idx += 1,
            _ if ident_start(byte) => {
                let end = idx + bytes[idx..].iter().take_while(|b| ident_part(**b)).count();
                let word = &text[idx..end];
                let allowed = if seen_expect {
                    previous_dot
                } else {
                    match word {
                        "await" | "return" => true,
                        "expect" => {
                            seen_expect = true;
                            true
                        }
                        _ => false,
                    }
                };
                if !allowed {
                    return false;
                }
                previous_dot = false;
                idx = end;
            }
            _ => return false,
        }
    }
    depth == 0 && seen_expect
}

/// A container read is direct when it is an argument of the call that is
/// the whole argument of `expect(` (the value under test), or the argument
/// of the matcher call (`.toEqual(t.out)`), is not followed by a member, and
/// is not wrapped in its own parentheses.
pub(crate) fn container_read_is_direct(
    body: &str,
    read_start: usize,
    read_end: usize,
    open: Option<usize>,
) -> bool {
    let Some(open) = open else { return false };
    if body.as_bytes().get(open) != Some(&b'(') {
        return false;
    }
    let after = body[read_end..].trim_start();
    if !after.starts_with([',', ')']) {
        return false;
    }
    // The read must follow `(` or `,` (optionally spread), not an operator.
    let before = body[..read_start].trim_end();
    let before = before.strip_suffix("...").unwrap_or(before).trim_end();
    if !before.ends_with(['(', ',']) {
        return false;
    }
    // Callee: the identifier run before `(`.
    let head = body[..open].trim_end();
    let name_start = head
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .map_or(0, |at| at + 1);
    let name = &head[name_start..];
    if name.is_empty() {
        return false;
    }
    let before_name = head[..name_start].trim_end();
    // `expect(owner(<read>))`: the callee is the whole argument of expect(.
    if before_name.strip_suffix("expect(").is_some_and(|rest| {
        !rest.ends_with(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.'))
    }) {
        return true;
    }
    // `expect(..).matcher(<read>)`.
    name.starts_with("to")
        && before_name
            .strip_suffix('.')
            .is_some_and(|b| b.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_expect_statement_accepts_only_one_expect_chain() {
        for body in [
            "{ expect(f(x)).toBe(y); }",
            "expect(f(x)).toEqual(y)",
            "{ await expect(f(x)).resolves.toBe(y) }",
            "{ return expect(f(x)).not.toBe(y); }",
        ] {
            assert!(single_expect_statement(body), "{body}");
        }
        for body in [
            "{ Object.assign(a, b); expect(f(a)).toBe(y); }",
            "{ fix(a)\n expect(f(a)).toBe(y) }",
            "{ expect(f(a)).toBe(y); expect(g(a)).toBe(y); }",
            "{ const z = 1; expect(f(a)).toBe(z); }",
            "{ if (a) expect(f(a)).toBe(y); }",
            "{ expect(f(a)).toBe(y) // note\n }",
            "{ \"s\"; expect(f(a)).toBe(y); }",
            "{ }",
            "{ foo(expect(f(a)).toBe(y)); }",
        ] {
            assert!(!single_expect_statement(body), "{body}");
        }
    }

    #[test]
    fn container_read_needs_a_word_bounded_direct_expect_or_matcher() {
        let direct = |body: &str, key: &str| {
            let at = body.find(key).unwrap_or(0);
            let open = body[..at].rfind('(');
            container_read_is_direct(body, at, at + key.len(), open)
        };
        assert!(direct("expect(f(t.input)).toEqual(1)", "t.input"));
        assert!(direct("expect(f(1)).toEqual(t.out)", "t.out"));
        assert!(!direct("myexpect(f(t.input)).toEqual(1)", "t.input"));
        assert!(!direct("x.expect(f(t.input)).toEqual(1)", "t.input"));
        assert!(!direct("expect(f(g(t.input))).toEqual(1)", "t.input"));
        assert!(!direct("expect(f((t.input).pop())).toEqual(1)", "t.input"));
    }
}

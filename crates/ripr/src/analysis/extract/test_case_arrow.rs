//! The exact equality a `#[test_case(args => literal)]` row asserts of the
//! value its function returns (RIPR-SPEC-0231, harness-compared return).

use super::extract_assertions;
use crate::analysis::facts::OracleFact;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, ast};

/// One synthesized `assert_eq!(<returned expression>, <literal>)` oracle per
/// `#[test_case(.. => literal)]` row, keyed to the function's `fn` line, with
/// the row's literal arguments written in for the parameters so the
/// comparison reads like the plain assertion it stands for (an input pair that
/// cannot tell an edit apart is graded as one).
/// All or nothing: empty unless every `test_case` row of the function has
/// plain literal arguments, one per parameter, and a plain literal after its
/// arrow, the body is a single returned expression that mentions each
/// parameter only as a plain path (no closure, `let` or macro rebinding or
/// reading it) and the function is neither `#[should_panic]` nor `#[ignore]`. `=> matches`,
/// `=> with`, `=> panics`, `=> it`/`is` and every non-literal expectation leave
/// the whole function unread. `function_text` is the function's source from
/// its `fn` token.
pub(crate) fn arrow_row_oracles(
    attrs: &[String],
    function_text: &str,
    start_line: usize,
) -> Vec<OracleFact> {
    synthesized(attrs, start_line, || {
        let parse = parse_clean_source_file(function_text)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        returned_template(&function)
    })
}

/// [`arrow_row_oracles`] over a function already parsed, for callers that
/// hold its syntax and must not re-parse it per row.
pub(crate) fn arrow_row_oracles_for_fn(
    attrs: &[String],
    function: &ast::Fn,
    start_line: usize,
) -> Vec<OracleFact> {
    synthesized(attrs, start_line, || returned_template(function))
}

fn synthesized(
    attrs: &[String],
    start_line: usize,
    returned: impl FnOnce() -> Option<ReturnedTemplate>,
) -> Vec<OracleFact> {
    // Decide from the attributes alone first: the parse below is paid only by
    // a function that actually carries arrow rows.
    let Some(rows) = arrow_rows(attrs) else {
        return Vec::new();
    };
    let Some(template) = returned() else {
        return Vec::new();
    };
    // Rows that agree on every argument and the literal read as one comparison.
    let mut seen = std::collections::BTreeSet::new();
    let mut comparisons = Vec::new();
    for row in &rows {
        let Some(returned) = template.with_arguments(&row.arguments) else {
            // A row whose arguments do not fit the parameters leaves the whole
            // function unread.
            return Vec::new();
        };
        if seen.insert((returned.clone(), row.expected.clone())) {
            comparisons.push(format!("assert_eq!({returned}, {});", row.expected));
        }
    }
    comparisons
        .iter()
        .flat_map(|comparison| extract_assertions(comparison, start_line))
        .collect()
}

/// One `#[test_case(args => literal)]` row: its literal arguments and the
/// literal it is compared with.
struct ArrowRow {
    arguments: Vec<String>,
    expected: String,
}

/// Every `test_case` row of the function, or `None` when the function has no
/// `test_case` row, any row is not a readable arrow row, or the harness does
/// not compare the return (`#[should_panic]`, `#[ignore]`).
fn arrow_rows(attrs: &[String]) -> Option<Vec<ArrowRow>> {
    if skips_comparison(attrs) {
        return None;
    }
    let rows = attrs
        .iter()
        .filter(|attribute| is_test_case_attribute(attribute))
        .map(|attribute| arrow_row(attribute))
        .collect::<Option<Vec<_>>>()?;
    (!rows.is_empty()).then_some(rows)
}

/// Whether `attribute` is a `#[test_case(.. => literal)]` row that
/// [`arrow_row_oracles_for_fn`] reads for `function`.
pub(crate) fn arrow_row_is_read(
    function_attrs: &[String],
    attribute: &str,
    function: &ast::Fn,
) -> bool {
    !skips_comparison(function_attrs)
        && arrow_row(attribute).is_some_and(|row| {
            returned_template(function)
                .is_some_and(|template| template.with_arguments(&row.arguments).is_some())
        })
}

fn skips_comparison(attrs: &[String]) -> bool {
    attrs.iter().any(|attribute| {
        let attribute = attribute.trim_start();
        attribute.starts_with("#[should_panic") || attribute.starts_with("#[ignore")
    })
}

fn is_test_case_attribute(attribute: &str) -> bool {
    attribute
        .split_once('(')
        .is_some_and(|(path, _)| is_test_case_path(path))
}

fn is_test_case_path(path: &str) -> bool {
    let compact = path
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact == "#[test_case" || compact == "#[test_case::test_case"
}

/// The one expression a function body returns, as source text with a slot
/// where each parameter is read: a block with no statements and a one-line
/// tail expression.
struct ReturnedTemplate {
    pieces: Vec<Piece>,
    parameters: usize,
}

enum Piece {
    Text(String),
    Parameter(usize),
}

impl ReturnedTemplate {
    /// The expression with the row's arguments written in, or `None` when the
    /// row supplies a different number of arguments than the function takes.
    fn with_arguments(&self, arguments: &[String]) -> Option<String> {
        if arguments.len() != self.parameters {
            return None;
        }
        Some(
            self.pieces
                .iter()
                .map(|piece| match piece {
                    Piece::Text(text) => text.clone(),
                    // Parenthesized so a negated literal keeps its sign as a
                    // method receiver or cast operand (`(-5).abs()`).
                    Piece::Parameter(index) => format!("({})", arguments[*index]),
                })
                .collect(),
        )
    }
}

fn returned_template(function: &ast::Fn) -> Option<ReturnedTemplate> {
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let tail = list.tail_expr()?;
    let text = tail.syntax().text().to_string();
    let text = text.trim();
    if text.is_empty() || text.contains('\n') || text.contains(';') {
        return None;
    }
    // Plain named parameters only: a pattern, `self` or a `mut` binding could
    // be read through another name.
    let mut names = Vec::new();
    if function.param_list()?.self_param().is_some() {
        return None;
    }
    for parameter in function.param_list()?.params() {
        match parameter.pat()? {
            ast::Pat::IdentPat(ident)
                if ident.mut_token().is_none() && ident.at_token().is_none() =>
            {
                names.push(ident.name()?.text().to_string());
            }
            _ => return None,
        }
    }
    // A closure parameter or `let` in the tail could shadow a parameter.
    if tail
        .syntax()
        .descendants()
        .any(|node| ast::IdentPat::can_cast(node.kind()))
    {
        return None;
    }
    let mut pieces = Vec::new();
    let mut run = String::new();
    for element in tail.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        let name = token.text();
        match names.iter().position(|candidate| candidate == name) {
            Some(index) if token.kind() == ra_ap_syntax::SyntaxKind::IDENT => {
                // Read only as a one-segment path expression: not a field, a
                // method or path segment, nor text in a macro.
                let plain = token
                    .parent()
                    .filter(|parent| ast::NameRef::can_cast(parent.kind()))
                    .and_then(|parent| parent.parent())
                    .filter(|segment| ast::PathSegment::can_cast(segment.kind()))
                    .and_then(|segment| segment.parent())
                    .and_then(ast::Path::cast)
                    .is_some_and(|path| {
                        path.qualifier().is_none()
                            && path
                                .syntax()
                                .parent()
                                .is_some_and(|parent| ast::PathExpr::can_cast(parent.kind()))
                    });
                if !plain {
                    return None;
                }
                pieces.push(Piece::Text(std::mem::take(&mut run)));
                pieces.push(Piece::Parameter(index));
            }
            _ => run.push_str(name),
        }
    }
    pieces.push(Piece::Text(run));
    Some(ReturnedTemplate {
        pieces,
        parameters: names.len(),
    })
}

/// The literal arguments before and the literal after the one top-level `=>`
/// of a `#[test_case(..)]` attribute, before an optional `; "name"` suffix.
fn arrow_row(attribute: &str) -> Option<ArrowRow> {
    // The attribute path precedes its first `(`, whatever whitespace sits
    // around it (`#[test_case (..)]`), as `is_test_case_attribute` reads it.
    let (path, rest) = attribute.split_once('(')?;
    if !is_test_case_path(path) {
        return None;
    }
    let inner = rest.trim_end().strip_suffix(")]")?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut previous = '\0';
    let mut arrow = None;
    let mut semicolon = None;
    for (at, character) in inner.char_indices() {
        if in_string {
            if character == '"' && previous != '\\' {
                in_string = false;
            }
        } else {
            match character {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.checked_sub(1)?,
                '>' if depth == 0 && previous == '=' => {
                    arrow = match arrow {
                        None => Some(at + 1),
                        Some(_) => return None,
                    };
                }
                ';' if depth == 0 && arrow.is_some() => {
                    semicolon = Some(at);
                    break;
                }
                _ => {}
            }
        }
        previous = character;
    }
    let arrow = arrow?;
    let expected = inner[arrow..semicolon.unwrap_or(inner.len())].trim();
    if !is_plain_literal(expected) {
        return None;
    }
    let arguments = literal_arguments(&inner[..arrow - 2])?;
    Some(ArrowRow {
        arguments,
        expected: expected.to_string(),
    })
}

/// The comma-separated arguments of a row, each a plain literal. An empty
/// argument list is no arguments.
fn literal_arguments(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut arguments = Vec::new();
    let mut in_string = false;
    let mut previous = '\0';
    let mut start = 0usize;
    for (at, character) in text.char_indices() {
        if in_string {
            if character == '"' && previous != '\\' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == ',' {
            arguments.push(text[start..at].trim());
            start = at + 1;
        }
        previous = character;
    }
    arguments.push(text[start..].trim());
    arguments
        .into_iter()
        // A string reaches the parameter through `FromStr`, which runs user
        // code: the row gate (`case_row_literals`) refuses it, so no oracle
        // is built for it either.
        .map(|argument| {
            (is_plain_literal(argument) && !argument.starts_with('"')).then(|| argument.to_string())
        })
        .collect()
}

/// An integer literal (optionally negated, with an optional type suffix),
/// boolean or plain string literal.
fn is_plain_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let integer = digits.chars().next().is_some_and(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    let quoted = text.len() >= 2
        && text.starts_with('"')
        && text.ends_with('"')
        && !text[1..text.len() - 1].contains(['"', '\\']);
    integer || text == "true" || text == "false" || quoted
}

//! The exact equality a `#[test_case(args => literal)]` row asserts of the
//! value its function returns (RIPR-SPEC-0231, harness-compared return).

use super::extract_assertions;
use crate::analysis::facts::OracleFact;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast};

/// One synthesized `assert_eq!(<returned expression>, <literal>)` oracle per
/// `#[test_case(.. => literal)]` row, keyed to the function's `fn` line.
/// All or nothing: empty unless every `test_case` row of the function has a
/// plain literal after its arrow, the body is a single returned expression and
/// the function is neither `#[should_panic]` nor `#[ignore]`. `=> matches`,
/// `=> with`, `=> panics`, `=> it`/`is` and every non-literal expectation leave
/// the whole function unread. `function_text` is the function's source from
/// its `fn` token.
pub(crate) fn arrow_row_oracles(
    attrs: &[String],
    function_text: &str,
    start_line: usize,
) -> Vec<OracleFact> {
    synthesized(attrs, start_line, || returned_expression(function_text))
}

/// [`arrow_row_oracles`] over a function already parsed, for callers that
/// hold its syntax and must not re-parse it per row.
pub(crate) fn arrow_row_oracles_for_fn(
    attrs: &[String],
    function: &ast::Fn,
    start_line: usize,
) -> Vec<OracleFact> {
    synthesized(attrs, start_line, || returned_expression_of(function))
}

fn synthesized(
    attrs: &[String],
    start_line: usize,
    returned: impl FnOnce() -> Option<String>,
) -> Vec<OracleFact> {
    // Decide from the attributes alone first: the parse below is paid only by
    // a function that actually carries arrow rows.
    let Some(expectations) = arrow_expectations(attrs) else {
        return Vec::new();
    };
    let Some(returned) = returned() else {
        return Vec::new();
    };
    // Two rows with one literal read as one comparison.
    let mut seen = std::collections::BTreeSet::new();
    expectations
        .iter()
        .filter(|expected| seen.insert(expected.as_str()))
        .flat_map(|expected| {
            extract_assertions(&format!("assert_eq!({returned}, {expected});"), start_line)
        })
        .collect()
}

/// The literal after each row's arrow, or `None` when the function has no
/// `test_case` row, any row is not a readable arrow row, or the harness does
/// not compare the return (`#[should_panic]`, `#[ignore]`).
fn arrow_expectations(attrs: &[String]) -> Option<Vec<String>> {
    if skips_comparison(attrs) {
        return None;
    }
    let expectations = attrs
        .iter()
        .filter(|attribute| is_test_case_attribute(attribute))
        .map(|attribute| arrow_literal_expectation(attribute))
        .collect::<Option<Vec<_>>>()?;
    (!expectations.is_empty()).then_some(expectations)
}

/// Whether `attribute` is a `#[test_case(.. => literal)]` row that
/// [`arrow_row_oracles_for_fn`] reads for `function`.
pub(crate) fn arrow_row_is_read(
    function_attrs: &[String],
    attribute: &str,
    function: &ast::Fn,
) -> bool {
    !skips_comparison(function_attrs)
        && arrow_literal_expectation(attribute).is_some()
        && returned_expression_of(function).is_some()
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

/// The one expression a function body returns: a block with no statements
/// and a one-line tail expression.
fn returned_expression(function_text: &str) -> Option<String> {
    let parse = parse_clean_source_file(function_text)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    returned_expression_of(&function)
}

fn returned_expression_of(function: &ast::Fn) -> Option<String> {
    let list = function.body()?.stmt_list()?;
    if list.statements().next().is_some() {
        return None;
    }
    let tail = list.tail_expr()?.syntax().text().to_string();
    let tail = tail.trim();
    (!tail.is_empty() && !tail.contains('\n') && !tail.contains(';')).then(|| tail.to_string())
}

/// The literal after the one top-level `=>` of a `#[test_case(..)]`
/// attribute, before an optional `; "name"` suffix.
fn arrow_literal_expectation(attribute: &str) -> Option<String> {
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
    let expected = inner[arrow?..semicolon.unwrap_or(inner.len())].trim();
    is_plain_literal(expected).then(|| expected.to_string())
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

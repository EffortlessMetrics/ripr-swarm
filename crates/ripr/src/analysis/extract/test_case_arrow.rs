//! The exact equality a `#[test_case(args => literal)]` row asserts of the
//! value its function returns (RIPR-SPEC-0231, harness-compared return).

use super::extract_assertions;
use crate::analysis::facts::OracleFact;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, ast};

/// One synthesized `assert_eq!(<returned expression>, <literal>)` oracle per
/// `#[test_case(.. => literal)]` row, keyed to the function's `fn` line.
/// Empty unless the function body is a single returned expression and the
/// function is neither `#[should_panic]` nor `#[ignore]`. `=> matches`,
/// `=> with`, `=> panics`, `=> it`/`is` and every non-literal expectation stay
/// unread. `function_text` is the function's source, attributes excluded.
pub(crate) fn arrow_row_oracles(
    attrs: &[String],
    function_text: &str,
    start_line: usize,
) -> Vec<OracleFact> {
    if skips_comparison(attrs) {
        return Vec::new();
    }
    let Some(returned) = returned_expression(function_text) else {
        return Vec::new();
    };
    let mut oracles = Vec::new();
    for attribute in attrs
        .iter()
        .filter(|attribute| is_test_case_attribute(attribute))
    {
        let Some(expected) = arrow_literal_expectation(attribute) else {
            continue;
        };
        let snippet = format!("assert_eq!({returned}, {expected});");
        oracles.extend(extract_assertions(&snippet, start_line));
    }
    oracles
}

/// Whether `attribute` is a `#[test_case(.. => literal)]` row that
/// [`arrow_row_oracles`] reads for a function with `function_attrs` and
/// `function_text`.
pub(crate) fn arrow_row_is_read(
    function_attrs: &[String],
    attribute: &str,
    function_text: &str,
) -> bool {
    !skips_comparison(function_attrs)
        && arrow_literal_expectation(attribute).is_some()
        && returned_expression(function_text).is_some()
}

fn skips_comparison(attrs: &[String]) -> bool {
    attrs.iter().any(|attribute| {
        let attribute = attribute.trim_start();
        attribute.starts_with("#[should_panic") || attribute.starts_with("#[ignore")
    })
}

fn is_test_case_attribute(attribute: &str) -> bool {
    let compact = attribute
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.starts_with("#[test_case(") || compact.starts_with("#[test_case::test_case(")
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
    let open = attribute.find("test_case(")? + "test_case(".len();
    let inner = attribute[open..].trim_end().strip_suffix(")]")?;
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
                    if arrow.replace(at + 1).is_some() {
                        return None;
                    }
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

/// An integer, boolean or plain string/char literal (an integer may be negated).
fn is_plain_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let integer = digits.chars().next().is_some_and(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    let quoted = |quote: char| {
        text.len() >= 2
            && text.starts_with(quote)
            && text.ends_with(quote)
            && !text[1..text.len() - 1].contains(quote)
            && !text[1..text.len() - 1].contains('\\')
    };
    integer || text == "true" || text == "false" || quoted('"') || quoted('\'')
}

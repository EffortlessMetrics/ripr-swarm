//! Whether a boundary operand that is not an owner parameter is a local
//! alias of one (`if let Some(op) = param`, `match param { Some(op) .. }`,
//! `let op = param`).
//!
//! Every boundary seam of an owner asks this about the same owner body,
//! and each answer scans that body several times per parameter, so an
//! owner with N seams cost O(N x body) (#15189). The answer depends only on
//! the body text, the parameters and the operand, so a per-thread memo
//! keyed by that exact text answers repeat questions without rescanning.

use std::cell::Cell;
use std::collections::HashMap;

/// One owner body and the answers already computed against it.
struct BodyAnswers {
    body: String,
    answers: HashMap<(String, Vec<String>), Option<usize>>,
}

thread_local! {
    // Seams are evaluated owner by owner, so one body per thread is
    // enough; a different body replaces it. `Cell::take` keeps this free
    // of borrow panics.
    static LAST_BODY: Cell<Option<BodyAnswers>> = const { Cell::new(None) };
}

/// The first parameter `operand` aliases in the owner `body`, memoized.
pub(super) fn boundary_local_operand_parameter_index(
    body: &str,
    parameters: &[String],
    operand: &str,
) -> Option<usize> {
    if operand.is_empty() {
        return None;
    }
    let key = (operand.to_string(), parameters.to_vec());
    let mut entry = LAST_BODY
        .with(Cell::take)
        .filter(|entry| entry.body == body)
        .unwrap_or_else(|| BodyAnswers {
            body: body.to_string(),
            answers: HashMap::new(),
        });
    let answer = *entry
        .answers
        .entry(key)
        .or_insert_with(|| scan_local_operand_parameter_index(body, parameters, operand));
    LAST_BODY.with(|cell| cell.set(Some(entry)));
    answer
}

/// The uncached reading: the first parameter `operand` aliases in `body`.
fn scan_local_operand_parameter_index(
    body: &str,
    parameters: &[String],
    operand: &str,
) -> Option<usize> {
    if operand.is_empty() {
        return None;
    }
    for (index, parameter) in parameters.iter().enumerate() {
        if body_contains_wrapped_local_alias(body, "Some", operand, parameter)
            || body_contains_wrapped_local_alias(body, "Ok", operand, parameter)
            || body_contains_direct_local_alias(body, operand, parameter)
        {
            return Some(index);
        }
    }
    None
}

fn body_contains_wrapped_local_alias(
    body: &str,
    wrapper: &str,
    operand: &str,
    parameter: &str,
) -> bool {
    // Built once per body: this runs per seam, related test and parameter.
    let prefix = format!("if let {wrapper}({operand}) = ");
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        line.strip_prefix(&prefix)
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    }) || (body_contains_match_parameter(body, parameter)
        && body_contains_wrapper_pattern(body, wrapper, operand))
}

fn body_contains_match_parameter(body: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        if is_comment_line(line) {
            return false;
        }
        line.find("match ")
            .map(|index| &line[index + "match ".len()..])
            .is_some_and(|rest| starts_with_identifier_token(rest, parameter))
    })
}

fn body_contains_wrapper_pattern(body: &str, wrapper: &str, operand: &str) -> bool {
    let pattern = format!("{wrapper}({operand})");
    body.lines().any(|line| {
        let line = code_line_before_comment(line);
        !is_comment_line(line) && line.contains(&pattern)
    })
}

fn code_line_before_comment(line: &str) -> &str {
    let line = line.trim();
    let line = line.split_once("//").map_or(line, |(code, _comment)| code);
    line.split_once("/*")
        .map_or(line, |(code, _comment)| code)
        .trim()
}

fn is_comment_line(line: &str) -> bool {
    line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
}

fn starts_with_identifier_token(text: &str, token: &str) -> bool {
    let text = text.trim_start();
    let end = text
        .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .unwrap_or(text.len());
    end > 0 && &text[..end] == token
}

fn body_contains_direct_local_alias(body: &str, operand: &str, parameter: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim().trim_end_matches(';').trim();
        let Some(binding) = line.strip_prefix("let ") else {
            return false;
        };
        let Some((left, right)) = binding.split_once('=') else {
            return false;
        };
        let local_name = left.split_once(':').map(|(name, _)| name).unwrap_or(left);
        local_name.trim() == operand && right.trim() == parameter
    })
}

#[cfg(test)]
mod tests;

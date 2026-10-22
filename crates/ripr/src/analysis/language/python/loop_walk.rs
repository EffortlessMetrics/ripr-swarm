//! Loop-counter walk for a changed `while` bound (RIPR-SPEC-0238 rule 4).
//!
//! `while start > 0: ...; start -= 1` visits every integer from the input down
//! to the bound, so a test that passes `start = 3` does reach `start == 0`
//! inside the loop even though no call binds the parameter to 0. The walk is
//! read only when the counter's single update is an unconditional unit step in
//! the loop body and nothing in the body can leave the loop early.

use super::guard_activation::leading_whitespace;

/// The counter a `while` bound compares and the direction it moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LoopStep {
    pub(super) local: String,
    /// `local -= 1` (true) or `local += 1` (false).
    pub(super) decreasing: bool,
}

impl LoopStep {
    /// Whether a counter started at `start` is compared against `bound` on the
    /// way: a decreasing counter passes every integer at or below its start.
    pub(super) fn reaches(&self, start: i128, bound: i128) -> bool {
        if self.decreasing {
            start >= bound
        } else {
            start <= bound
        }
    }
}

/// Words that can end a loop early, skip its update or rebind the counter's
/// scope; any of them in the body leaves the walk unread.
const EXITS: &[&str] = &[
    "break", "return", "continue", "raise", "try", "nonlocal", "global", "await", "async", "del",
];

/// The counter step for the `while` header at 1-based `line`, read from the
/// new-side `owner_source`. None unless `local` has exactly one body update,
/// a unit step at the body's own indentation, and the body has no exit.
pub(super) fn python_loop_step(owner_source: &str, line: usize, local: &str) -> Option<LoopStep> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let header = lines.get(line.checked_sub(1)?)?;
    if !header.trim_start().starts_with("while ") || !header.trim_end().ends_with(':') {
        return None;
    }
    let header_indent = leading_whitespace(header);
    let mut body = Vec::new();
    for raw in lines.iter().skip(line) {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_whitespace(raw) <= header_indent {
            break;
        }
        body.push(*raw);
    }
    let body_indent = leading_whitespace(body.first()?);
    let mut step = None;
    for raw in &body {
        let trimmed = raw.trim();
        if words(trimmed).any(|word| EXITS.contains(&word)) {
            return None;
        }
        if !mentions_assignment_to(trimmed, local) {
            continue;
        }
        let decreasing = match trimmed.split_once(' ') {
            Some((name, rest)) if name == local => match rest.trim() {
                "-= 1" => true,
                "+= 1" => false,
                _ => return None,
            },
            _ => return None,
        };
        if step.is_some() || leading_whitespace(raw) != body_indent {
            return None;
        }
        step = Some(decreasing);
    }
    step.map(|decreasing| LoopStep {
        local: local.to_string(),
        decreasing,
    })
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
}

/// Whether the statement binds `local`: it is on the left of an assignment
/// (plain, augmented or tuple), the target of a `for`, or inside a walrus.
fn mentions_assignment_to(statement: &str, local: &str) -> bool {
    if statement.contains(":=") {
        return words(statement).any(|word| word == local);
    }
    if let Some(rest) = statement.strip_prefix("for ") {
        let target = rest.split(" in ").next().unwrap_or(rest);
        return words(target).any(|word| word == local);
    }
    let bytes = statement.as_bytes();
    let Some(eq) = (0..bytes.len()).find(|&i| {
        bytes[i] == b'='
            && bytes.get(i + 1) != Some(&b'=')
            && (i == 0 || !matches!(bytes[i - 1], b'=' | b'!' | b'<' | b'>'))
    }) else {
        return false;
    };
    words(&statement[..eq]).any(|word| word == local)
}

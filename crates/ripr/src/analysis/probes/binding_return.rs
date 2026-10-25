//! Changed `let` binding to returned-expression relation (RIPR-SPEC-0357).
//!
//! A rewrite of `let end = sku.find('-').unwrap_or(sku.len());` is not a
//! predicate and not the returned expression, so the changed line has no
//! probe family of its own. When the binding reaches the function's tail
//! expression unchanged, this relation names that flow so the finding can say
//! what the unresolved edge is. It never retargets the probe and never adds
//! credit: the probe stays on the changed line and keeps its
//! `static_unknown` reading.
//!
//! Fail-closed shape: the changed `let` and the tail both sit directly in the
//! function body block, no statement between them mentions the binding, and
//! none returns early or uses `?`. Any doubt yields `None`.

use crate::analysis::language::mask_rust_comments_and_strings;

use super::binding_predicate::{masked_brace_delta, masked_paren_delta};

/// How the binding reaches the returned expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReturnFlowKind {
    /// The tail is the binding itself.
    Whole,
    /// The tail slices a value with the binding as a range bound
    /// (`&text[..end]`, `text.get_unchecked(..end)`).
    RangeBound,
}

impl ReturnFlowKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Whole => "whole binding",
            Self::RangeBound => "range bound",
        }
    }
}

/// One `changed_binding -> returned_expression` flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangedBindingReturnFlow {
    pub(crate) binding: String,
    /// The changed initializer text (the `…` of `let ident = …;`).
    pub(crate) initializer: String,
    /// The trimmed tail expression text.
    pub(crate) return_expression: String,
    /// Absolute (new-file) line of the tail expression.
    pub(crate) return_line: usize,
    pub(crate) kind: ReturnFlowKind,
}

/// Resolve the return flow for a changed `let` line against its owning
/// function body. `body_start_line` is the function's absolute start line and
/// `changed_line` the changed declaration's absolute line.
pub(crate) fn resolve_changed_binding_return(
    binding: &str,
    initializer: &str,
    body: &str,
    body_start_line: usize,
    changed_line: usize,
) -> Option<ChangedBindingReturnFlow> {
    let changed_offset = changed_line.checked_sub(body_start_line)?;
    let masked = mask_rust_comments_and_strings(body);
    let lines: Vec<&str> = masked.lines().collect();
    if changed_offset >= lines.len() {
        return None;
    }
    let changed_text = lines.get(changed_offset)?.trim_start();
    if !changed_text.starts_with("let ") || !contains_token(changed_text, binding) {
        return None;
    }
    // The signature may span lines; the body block is the first brace the
    // scan opens, and the changed `let` must sit directly inside it.
    let mut depth = 0isize;
    let mut body_depth_before_let = None;
    for (offset, line) in lines.iter().enumerate() {
        if offset == changed_offset {
            body_depth_before_let = Some(depth);
        }
        if masked_paren_delta(line) != 0 && offset > changed_offset {
            // A statement running over several lines is not modeled.
            return None;
        }
        depth = depth.saturating_add(masked_brace_delta(line));
    }
    // After the whole body the depth is back to zero; the let is directly in
    // the function block when exactly one brace (the function's) is open.
    if body_depth_before_let != Some(1) || depth != 0 {
        return None;
    }
    let mut running = 1isize;
    let mut statements: Vec<(usize, &str)> = Vec::new();
    for (offset, line) in lines.iter().enumerate().skip(changed_offset + 1) {
        let trimmed = line.trim();
        let before = running;
        running = running.saturating_add(masked_brace_delta(line));
        if trimmed.is_empty() {
            continue;
        }
        if before == 1 && running == 0 && trimmed == "}" {
            break;
        }
        if mentions_early_exit(trimmed) {
            return None;
        }
        statements.push((offset, trimmed));
    }
    let (tail_offset, tail_text) = statements.last().copied()?;
    // A tail that continues an unterminated line is a different expression.
    let previous = statements
        .len()
        .checked_sub(2)
        .and_then(|at| statements.get(at))
        .map(|(_, line)| *line);
    if let Some(previous) = previous
        && !(previous.ends_with(';') || previous.ends_with('{') || previous.ends_with('}'))
    {
        return None;
    }
    if previous.is_none() {
        let before_tail = lines.get(changed_offset)?.trim_end();
        if !before_tail.ends_with(';') {
            return None;
        }
    }
    let flow_kind = tail_flow(tail_text, binding)?;
    // No statement between the let and the tail mentions the binding, which
    // also rules out a shadow, a reassignment, a borrow and a capture.
    if statements
        .iter()
        .take(statements.len().saturating_sub(1))
        .any(|(_, line)| contains_token(line, binding))
    {
        return None;
    }
    Some(ChangedBindingReturnFlow {
        binding: binding.to_string(),
        initializer: initializer.to_string(),
        return_expression: tail_text.to_string(),
        return_line: body_start_line + tail_offset,
        kind: flow_kind,
    })
}

/// An initializer with no statement boundary: a `;` inside a literal or
/// comment is data, so the check reads the masked text.
pub(crate) fn is_single_statement(initializer: &str) -> bool {
    !mask_rust_comments_and_strings(initializer).contains(';')
}

fn mentions_early_exit(line: &str) -> bool {
    contains_token(line, "return") || line.contains('?') || contains_token(line, "break")
}

/// Classify a tail expression that is exactly one of the supported flows.
fn tail_flow(tail: &str, binding: &str) -> Option<ReturnFlowKind> {
    let tail = strip_unsafe_wrapper(tail.trim());
    if tail == binding {
        return Some(ReturnFlowKind::Whole);
    }
    let range_bounds = [format!("..{binding}"), format!("{binding}..")];
    if let Some(inner) = tail.strip_prefix('&')
        && let Some((base, rest)) = inner.split_once('[')
        && is_plain_path(base.trim())
        && let Some(range) = rest.strip_suffix(']')
        && range_bounds.iter().any(|bound| range.trim() == bound)
    {
        return Some(ReturnFlowKind::RangeBound);
    }
    for method in ["get_unchecked", "get"] {
        let marker = format!(".{method}(");
        if let Some((base, rest)) = tail.split_once(marker.as_str())
            && is_plain_path(base.trim())
            && let Some(range) = rest.strip_suffix(')')
            && range_bounds.iter().any(|bound| range.trim() == bound)
        {
            return Some(ReturnFlowKind::RangeBound);
        }
    }
    None
}

fn strip_unsafe_wrapper(tail: &str) -> &str {
    tail.strip_prefix("unsafe")
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('{'))
        .and_then(|rest| rest.trim_end().strip_suffix('}'))
        .map_or(tail, str::trim)
}

fn is_plain_path(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
        && !text.starts_with('.')
        && !text.ends_with('.')
}

fn contains_token(text: &str, token: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|part| part == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(body: &str, binding: &str, line_offset: usize) -> Option<ChangedBindingReturnFlow> {
        resolve_changed_binding_return(binding, "INIT", body, 10, 10 + line_offset)
    }

    const FAMILY: &str = "pub fn family(sku: &str) -> &str {\n    let end = sku.find('-').unwrap_or(sku.len());\n    &sku[..end]\n}\n";

    #[test]
    fn a_range_bounded_slice_tail_is_a_return_flow() -> Result<(), String> {
        let flow = resolve(FAMILY, "end", 1).ok_or("expected a return flow")?;
        assert_eq!(flow.kind, ReturnFlowKind::RangeBound);
        assert_eq!(flow.return_expression, "&sku[..end]");
        assert_eq!(flow.return_line, 12);
        Ok(())
    }

    #[test]
    fn an_unsafe_get_unchecked_tail_is_a_return_flow() -> Result<(), String> {
        let body = "pub fn family(sku: &str) -> &str {\n    let end = sku.find('-').unwrap_or(sku.len());\n    // SAFETY: in bounds.\n    unsafe { sku.get_unchecked(..end) }\n}\n";
        let flow = resolve(body, "end", 1).ok_or("expected a return flow")?;
        assert_eq!(flow.kind, ReturnFlowKind::RangeBound);
        assert_eq!(flow.return_line, 13);
        Ok(())
    }

    #[test]
    fn the_bare_binding_tail_is_a_whole_flow() -> Result<(), String> {
        let body = "fn f(x: Option<u32>) -> u32 {\n    let v = x.unwrap_or(7);\n    v\n}\n";
        let flow = resolve(body, "v", 1).ok_or("expected a return flow")?;
        assert_eq!(flow.kind, ReturnFlowKind::Whole);
        Ok(())
    }

    #[test]
    fn a_rebound_binding_is_not_a_return_flow() {
        let body = "fn f(s: &str) -> &str {\n    let mut end = s.find('-').unwrap_or(s.len());\n    end = end.min(4);\n    &s[..end]\n}\n";
        assert_eq!(resolve(body, "end", 1), None);
    }

    #[test]
    fn a_shadowed_binding_is_not_a_return_flow() {
        let body = "fn f(s: &str) -> &str {\n    let end = s.len();\n    let end = end.min(4);\n    &s[..end]\n}\n";
        assert_eq!(resolve(body, "end", 1), None);
    }

    #[test]
    fn a_computed_tail_is_not_a_return_flow() {
        let body = "fn f(s: &str) -> usize {\n    let end = s.len();\n    end + 1\n}\n";
        assert_eq!(resolve(body, "end", 1), None);
    }

    #[test]
    fn an_intervening_use_or_early_exit_is_not_a_return_flow() {
        let used =
            "fn f(s: &str) -> &str {\n    let end = s.len();\n    log(end);\n    &s[..end]\n}\n";
        assert_eq!(resolve(used, "end", 1), None);
        let early = "fn f(s: &str) -> Option<&str> {\n    let end = s.len();\n    let t = s.get(..2)?;\n    Some(&s[..end])\n}\n";
        assert_eq!(resolve(early, "end", 1), None);
    }

    #[test]
    fn a_let_inside_a_nested_block_is_not_a_return_flow() {
        let body = "fn f(s: &str, b: bool) -> &str {\n    if b {\n        let end = s.len();\n        return &s[..end];\n    }\n    s\n}\n";
        assert_eq!(resolve(body, "end", 2), None);
    }

    #[test]
    fn an_unterminated_previous_line_makes_the_tail_a_different_expression() {
        let body = "fn f(s: &str, base: usize) -> usize {\n    let end = s.len();\n    base +\n    end\n}\n";
        assert_eq!(resolve(body, "end", 1), None);
        let slice = "fn f(s: &str, p: &str) -> String {\n    let end = s.len();\n    p.to_string() +\n    &s[..end]\n}\n";
        assert_eq!(resolve(slice, "end", 1), None);
    }

    #[test]
    fn each_early_exit_form_blocks_the_flow() {
        for exit in ["return;", "let _t = s.get(..1)?;", "loop { break; }"] {
            let body = format!(
                "fn f(s: &str) -> &str {{\n    let end = s.len();\n    {exit}\n    &s[..end]\n}}\n"
            );
            assert_eq!(resolve(&body, "end", 1), None, "{exit}");
        }
    }

    #[test]
    fn a_start_bound_and_get_tail_are_range_bound_flows() -> Result<(), String> {
        let start = "fn f(s: &str) -> &str {\n    let n = s.len();\n    &s[n..]\n}\n";
        assert_eq!(
            resolve(start, "n", 1).ok_or("start")?.kind,
            ReturnFlowKind::RangeBound
        );
        let get = "fn f(s: &str) -> Option<&str> {\n    let n = s.len();\n    s.get(..n)\n}\n";
        assert_eq!(
            resolve(get, "n", 1).ok_or("get")?.kind,
            ReturnFlowKind::RangeBound
        );
        for bad in ["&s[..=n]", "&s[a..n]", "&other(s)[..n]", "&s[..n2]"] {
            let body = format!("fn f(s: &str) -> &str {{\n    let n = s.len();\n    {bad}\n}}\n");
            assert_eq!(resolve(&body, "n", 1), None, "{bad}");
        }
        Ok(())
    }

    #[test]
    fn the_changed_line_must_be_the_let() {
        assert_eq!(resolve(FAMILY, "end", 2), None);
    }

    #[test]
    fn a_semicolon_inside_a_literal_is_not_a_second_statement() {
        assert!(is_single_statement("choice.unwrap_or(';')"));
        assert!(is_single_statement("choice.unwrap_or(\";\") // a; b"));
        assert!(!is_single_statement("s.len(); let end = end.min(4)"));
    }
}

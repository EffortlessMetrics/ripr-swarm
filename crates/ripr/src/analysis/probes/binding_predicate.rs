//! Bounded changed-binding → predicate-operand relation (#3294, P2 of
//! #3215).
//!
//! A diff that changes a `let` initializer changes the value later
//! predicates branch on, but the lexical probe scanner cannot see that
//! link: a changed `let end = …;` line lands in the catch-all
//! static-unknown family with a generic "changed syntax is not mapped"
//! finding even when the same function contains the exact `end == start`
//! predicate the tests discriminate.
//!
//! This module resolves that link producer-side, under narrow rules:
//!
//! - one simple `let <ident> = <init>;` line (no destructuring, no
//!   pattern);
//! - uses searched only within the owning function's body (sibling
//!   functions never relate);
//! - the binding must reach the use without a re-binding (shadowing,
//!   including a destructuring re-bind) or a reassignment between the
//!   changed declaration and the use — any such event ends the live
//!   span at that point;
//! - the use must be a direct identifier operand of a comparison
//!   (`==`, `!=`, `<`, `<=`, `>`, `>=`), a direct boolean test
//!   (`if ident`, `while ident`), or a `match` scrutinee, with comment
//!   and string text masked;
//! - a use line that is a macro invocation or carries closure pipes
//!   fails closed: it is recorded as the explicit blocker, never
//!   related.
//!
//! The relation never evaluates the initializer. It names the earliest
//! unsupported initializer operation when one is present, so the finding
//! can state the exact predicate and the exact unresolved edge instead
//! of a generic syntax limitation.

use crate::analysis::language::{changed_let_binding, mask_rust_comments_and_strings};

/// Which operand position the binding occupies at the predicate use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PredicateOperandSide {
    Left,
    Right,
    /// Direct boolean test (`if ident`, `while ident`).
    Boolean,
    /// Direct `match` scrutinee.
    MatchScrutinee,
}

/// Why the relation did not reach a direct use. Retained on the typed
/// relation (and asserted in tests) so the scope decision is explicit
/// instead of a silent drop (#3294: shadowing, closure capture,
/// mutation, and macro ambiguity are named limitations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingUseScope {
    /// A later `let` re-bound the identifier before any predicate use.
    ShadowedBeforeUse,
    /// The identifier was reassigned between the changed binding and
    /// the predicate use.
    ReassignedBeforeUse,
    /// The only candidate use sits inside a closure body; ripr does not
    /// model capture or call timing.
    ClosureCaptureUnsupported,
    /// The only candidate use sits inside a macro invocation; ripr does
    /// not expand macros.
    MacroExpansionUnsupported,
    /// No direct predicate use of the binding exists in the function.
    NoPredicateUse,
}

/// Whether the changed initializer's operand value can be named without
/// evaluating anything. A bare literal or identifier copy is textual;
/// anything else is unresolved and names its earliest operation (the
/// leftmost operation token in the initializer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingValueResolution {
    ResolvedToText(String),
    Unresolved { earliest_operation: String },
}

/// One direct `changed_binding -> predicate_operand` use (#3294).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangedBindingPredicateUse {
    pub(crate) binding: String,
    /// The changed initializer text (the `…` of `let ident = …;`).
    pub(crate) initializer: String,
    /// The exact predicate line text at the use site.
    pub(crate) predicate_expression: String,
    /// Absolute (new-file) line of the predicate use.
    pub(crate) predicate_line: usize,
    pub(crate) operand_side: PredicateOperandSide,
    pub(crate) value_resolution: BindingValueResolution,
}

/// The relation outcome for one changed binding: every direct use that
/// survives the scope rules, or the first blocker when none does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingPredicateResolution {
    DirectUses(Vec<ChangedBindingPredicateUse>),
    NoDirectUse { scope: BindingUseScope },
}

/// Comparison operators the relation supports. Two-character operators
/// must be checked before their one-character prefixes.
const COMPARISON_OPERATORS: [&str; 6] = ["==", "!=", "<=", ">=", "<", ">"];

/// Resolve the relation for a changed `let` line against the owning
/// function body. `body_start_line` is the function's absolute start
/// line (body line offset 0 sits on it), and `changed_line` is the
/// changed declaration's absolute line: only uses after that line, with
/// no intervening shadow or reassignment, relate.
pub(crate) fn resolve_changed_binding_uses(
    binding: &str,
    initializer: &str,
    body: &str,
    body_start_line: usize,
    changed_line: usize,
) -> BindingPredicateResolution {
    let masked = mask_rust_comments_and_strings(body);
    let mut uses = Vec::new();
    let mut live = false;
    let mut blocker: Option<BindingUseScope> = None;
    for (offset, (raw_line, masked_line)) in body.lines().zip(masked.lines()).enumerate() {
        let absolute = body_start_line + offset;
        let masked_trimmed = masked_line.trim();
        if masked_trimmed.starts_with("let ") {
            if let Some((declared, _)) = changed_let_binding(masked_trimmed) {
                if declared == binding {
                    if absolute == changed_line {
                        live = true;
                    } else if live {
                        live = false;
                        blocker.get_or_insert(BindingUseScope::ShadowedBeforeUse);
                    }
                    continue;
                }
                // Another binding's `let` initializer may still carry a
                // closure capture or macro use of ours. V1 relates no
                // predicate inside a foreign initializer (bounded rule:
                // control statements and scrutinees only), but the
                // closure/macro ambiguity is recorded as the explicit
                // blocker rather than silently dropped.
                if live && direct_predicate_operand(masked_trimmed, binding).is_some() {
                    if has_closure_pipes(masked_trimmed) {
                        blocker.get_or_insert(BindingUseScope::ClosureCaptureUnsupported);
                    } else if is_macro_invocation_line(masked_trimmed) {
                        blocker.get_or_insert(BindingUseScope::MacroExpansionUnsupported);
                    }
                }
                continue;
            }
            // Non-simple `let` (destructuring or pattern): it can still
            // re-bind the identifier, which ends the live span.
            if live && pattern_rebinds_binding(masked_trimmed, binding) {
                live = false;
                blocker.get_or_insert(BindingUseScope::ShadowedBeforeUse);
            }
            continue;
        }
        // `if let`/`while let`/`for` bind their own pattern variable: a
        // same-named binding there is a re-bind, and the control line is
        // never a predicate use of ours.
        if live && pattern_control_rebinds(masked_trimmed, binding) {
            live = false;
            blocker.get_or_insert(BindingUseScope::ShadowedBeforeUse);
            continue;
        }
        if live && is_reassignment(masked_trimmed, binding) {
            live = false;
            blocker.get_or_insert(BindingUseScope::ReassignedBeforeUse);
            continue;
        }
        if !live {
            continue;
        }
        let Some(side) = direct_predicate_operand(masked_trimmed, binding) else {
            continue;
        };
        if is_macro_invocation_line(masked_trimmed) {
            blocker.get_or_insert(BindingUseScope::MacroExpansionUnsupported);
            continue;
        }
        if has_closure_pipes(masked_trimmed) {
            blocker.get_or_insert(BindingUseScope::ClosureCaptureUnsupported);
            continue;
        }
        uses.push(ChangedBindingPredicateUse {
            binding: binding.to_string(),
            initializer: initializer.to_string(),
            predicate_expression: raw_line.trim().to_string(),
            predicate_line: absolute,
            operand_side: side,
            value_resolution: value_resolution(initializer),
        });
    }
    if uses.is_empty() {
        BindingPredicateResolution::NoDirectUse {
            scope: blocker.unwrap_or(BindingUseScope::NoPredicateUse),
        }
    } else {
        BindingPredicateResolution::DirectUses(uses)
    }
}

/// A non-simple `let` line whose binding pattern mentions the
/// identifier (e.g. `let (end, other) = pair;`) re-binds it.
fn pattern_rebinds_binding(let_line: &str, binding: &str) -> bool {
    let Some((lhs, _)) = let_line.split_once('=') else {
        return false;
    };
    lhs.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token == binding)
}

/// An `if let`/`while let`/`for` line whose pattern binds the
/// identifier (e.g. `if let Some(end) = opt {`, `for end in items {`)
/// re-binds it; the control line is a pattern, never a predicate use.
fn pattern_control_rebinds(line: &str, binding: &str) -> bool {
    let line = line.trim_start().strip_prefix("else ").unwrap_or(line);
    let pattern = if let Some(rest) = line
        .strip_prefix("if let ")
        .or_else(|| line.strip_prefix("while let "))
    {
        rest.split_once('=').map_or(rest, |(pattern, _)| pattern)
    } else if let Some(rest) = line.strip_prefix("for ") {
        rest.split_once(" in ").map_or(rest, |(pattern, _)| pattern)
    } else {
        return false;
    };
    pattern
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token == binding)
}

/// `<ident> = …` / `<ident> += …` (and friends) on a live binding —
/// but never `==`, `!=`, `<=`, `>=`, or a `let`.
fn is_reassignment(line: &str, binding: &str) -> bool {
    for operator in ["+=", "-=", "*=", "/=", "|=", "&=", "^=", "<<=", ">>="] {
        if let Some((lhs, _)) = line.split_once(operator)
            && binding_is_plain_operand(lhs, binding)
        {
            return true;
        }
    }
    if let Some((lhs, rhs)) = line.split_once('=') {
        return binding_is_plain_operand(lhs, binding) && !rhs.trim_start().starts_with('=');
    }
    false
}

/// Whether `text` ends with `binding` as a standalone identifier (not a
/// field path like `self.end`, not a longer identifier).
fn binding_is_plain_operand(text: &str, binding: &str) -> bool {
    let trimmed = text.trim_end();
    let Some(rest) = trimmed.strip_suffix(binding) else {
        return false;
    };
    match rest.chars().next_back() {
        None => true,
        Some(ch) => !(ch == '_' || ch.is_ascii_alphanumeric() || ch == '.'),
    }
}

/// Whether `binding` is a direct identifier operand of a supported
/// comparison, boolean test, or `match` scrutinee on this (masked)
/// line. Field paths (`self.end`) and longer identifiers
/// (`endpoint`) never match.
fn direct_predicate_operand(line: &str, binding: &str) -> Option<PredicateOperandSide> {
    for operator in COMPARISON_OPERATORS {
        let mut search = 0;
        while let Some(offset) = line[search..].find(operator) {
            let at = search + offset;
            let left = bound_left_operand(&line[..at]);
            let right = bound_right_operand(&line[at + operator.len()..]);
            if operand_holds_binding(left, binding) {
                return Some(PredicateOperandSide::Left);
            }
            if operand_holds_binding(right, binding) {
                return Some(PredicateOperandSide::Right);
            }
            search = at + operator.len();
        }
    }
    let condition = line
        .trim_start()
        .strip_prefix("else ")
        .unwrap_or(line.trim_start());
    for prefix in ["if ", "while "] {
        if let Some(rest) = condition.strip_prefix(prefix) {
            let condition_text = rest.split('{').next().unwrap_or(rest);
            // `if let`/`while let` conditions bind a pattern, they do
            // not test our binding (pattern re-binds are handled by
            // `pattern_control_rebinds`).
            if !condition_text.trim_start().starts_with("let ")
                && operand_holds_binding(condition_text, binding)
            {
                return Some(PredicateOperandSide::Boolean);
            }
        }
    }
    if let Some(rest) = line.trim_start().strip_prefix("match ") {
        let scrutinee = rest.split('{').next().unwrap_or(rest);
        if operand_holds_binding(scrutinee, binding) {
            return Some(PredicateOperandSide::MatchScrutinee);
        }
    }
    None
}

/// The operand text immediately left of an operator: bounded by the
/// nearest enclosing `&&`/`||`/`(`/`{` to its left.
fn bound_left_operand(text: &str) -> &str {
    let after_and = text.rsplit("&&").next().unwrap_or(text);
    let after_or = after_and.rsplit("||").next().unwrap_or(after_and);
    after_or.rsplit(['(', '{']).next().unwrap_or(after_or)
}

/// The operand text immediately right of an operator: bounded by the
/// nearest `&&`/`||`/`)`/`{` to its right.
fn bound_right_operand(text: &str) -> &str {
    let before_and = text.split("&&").next().unwrap_or(text);
    let before_or = before_and.split("||").next().unwrap_or(before_and);
    before_or.split([')', '{']).next().unwrap_or(before_or)
}

/// Whether an operand position holds `binding` as a standalone
/// identifier token (bounded against `.`-prefixed fields and longer
/// identifiers on both sides).
fn operand_holds_binding(operand: &str, binding: &str) -> bool {
    let operand = operand.trim();
    if operand == binding {
        return true;
    }
    let mut search = 0;
    while let Some(offset) = operand[search..].find(binding) {
        let at = search + offset;
        let before = operand[..at].chars().next_back();
        let after = operand[at + binding.len()..].chars().next();
        let before_ok =
            !before.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric() || ch == '.');
        let after_ok = !after.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        search = at + binding.len();
    }
    false
}

/// A use line that invokes a macro (`ensure!(…)`, `debug_assert!(…)`):
/// ripr does not expand macros, so the use fails closed.
fn is_macro_invocation_line(line: &str) -> bool {
    line.contains("!(")
}

/// A use line carrying closure pipes (`move |x|`, `|x| …`): the capture
/// and call timing are not modeled, so the use fails closed. `||` in a
/// boolean condition is explicitly not a closure pipe.
fn has_closure_pipes(line: &str) -> bool {
    let without_boolean_ops = line.replace("||", "");
    without_boolean_ops.contains('|')
}

/// The earliest (leftmost) unresolved operation in the initializer, or
/// `None` when the initializer is a bare literal/identifier copy. Any
/// call or arithmetic is unresolved — ripr never pretends to know an
/// operand value it did not evaluate.
fn value_resolution(initializer: &str) -> BindingValueResolution {
    let trimmed = initializer.trim();
    match earliest_operation(trimmed) {
        Some(earliest_operation) => BindingValueResolution::Unresolved { earliest_operation },
        None => BindingValueResolution::ResolvedToText(trimmed.to_string()),
    }
}

fn earliest_operation(trimmed: &str) -> Option<String> {
    const STD_OPERATION_TOKENS: [&str; 5] =
        [".find(", ".rfind(", ".len_utf8(", ".chars(", ".map_or("];
    let mut best: Option<(usize, String)> = None;
    let mut consider = |at: usize, operation: String| {
        if best.as_ref().is_none_or(|(pos, _)| at < *pos) {
            best = Some((at, operation));
        }
    };
    for token in STD_OPERATION_TOKENS {
        if let Some(at) = trimmed.find(token) {
            consider(at, token.to_string());
        }
    }
    if let Some(at) = trimmed.find('(') {
        consider(at, trimmed[..=at].to_string());
    }
    for (at, character) in trimmed.char_indices() {
        match character {
            '+' | '*' | '/' | '%' => consider(at, character.to_string()),
            // A `-` is an operation only between operands; a leading or
            // post-operator `-` is a literal sign, not an operation.
            '-' if at > 0
                && trimmed[..at]
                    .trim_end()
                    .chars()
                    .next_back()
                    .is_some_and(|prev| {
                        prev.is_ascii_alphanumeric() || prev == '_' || prev == ')' || prev == ']'
                    }) =>
            {
                consider(at, character.to_string());
            }
            _ => {}
        }
    }
    best.map(|(_, operation)| operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY_START: usize = 1;

    fn resolve_end(body: &str, changed_line: usize) -> BindingPredicateResolution {
        resolve_changed_binding_uses(
            "end",
            "input.rfind(delim).map_or(0, |idx| idx)",
            body,
            BODY_START,
            changed_line,
        )
    }

    fn blocked_scope(resolution: &BindingPredicateResolution) -> Result<BindingUseScope, String> {
        match resolution {
            BindingPredicateResolution::NoDirectUse { scope } => Ok(scope.clone()),
            BindingPredicateResolution::DirectUses(uses) => {
                Err(format!("expected no direct use, got {uses:?}"))
            }
        }
    }

    fn direct_uses(
        resolution: &BindingPredicateResolution,
    ) -> Result<&[ChangedBindingPredicateUse], String> {
        match resolution {
            BindingPredicateResolution::DirectUses(uses) => Ok(uses),
            BindingPredicateResolution::NoDirectUse { scope } => {
                Err(format!("expected direct uses, blocked by {scope:?}"))
            }
        }
    }

    #[test]
    fn resolves_changed_binding_to_equality_predicate() -> Result<(), String> {
        let body = "pub fn split(input: &str, delim: char) -> &str {\n    let end = input.rfind(delim).map_or(0, |idx| idx);\n    let start = delim.chars().next().map_or(0, |c| c.len_utf8());\n    if end == start {\n        &input[..end]\n    } else {\n        input\n    }\n}\n";
        let resolution = resolve_end(body, 2);
        let uses = direct_uses(&resolution)?;
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].predicate_line, 4);
        assert_eq!(uses[0].predicate_expression, "if end == start {");
        assert_eq!(uses[0].operand_side, PredicateOperandSide::Left);
        assert_eq!(
            uses[0].value_resolution,
            BindingValueResolution::Unresolved {
                earliest_operation: ".rfind(".to_string()
            }
        );
        Ok(())
    }

    #[test]
    fn literal_initializer_resolves_to_text() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let limit = 10;\n    limit > 2\n}\n";
        let resolution = resolve_changed_binding_uses("limit", "10", body, BODY_START, 2);
        let uses = direct_uses(&resolution)?;
        assert_eq!(
            uses[0].value_resolution,
            BindingValueResolution::ResolvedToText("10".to_string())
        );
        Ok(())
    }

    #[test]
    fn earliest_operation_is_leftmost_not_first_listed() {
        // `.rfind(` is listed before `+`, but `+` is the leftmost
        // operation in the initializer, so it is the named edge.
        let resolution = value_resolution("a + b.rfind(c)");
        assert_eq!(
            resolution,
            BindingValueResolution::Unresolved {
                earliest_operation: "+".to_string()
            }
        );
    }

    #[test]
    fn unsupported_calls_and_arithmetic_stay_unresolved() {
        let cases = [
            ("input.len()", "input.len("),
            ("compute(x)", "compute("),
            ("x - 1", "-"),
            ("a * b", "*"),
        ];
        for (initializer, operation) in cases {
            assert_eq!(
                value_resolution(initializer),
                BindingValueResolution::Unresolved {
                    earliest_operation: operation.to_string()
                },
                "initializer `{initializer}` must stay unresolved"
            );
        }
    }

    #[test]
    fn literals_and_identifier_copies_resolve_to_text() {
        for initializer in ["10", "-1", "\"done\"", "copied"] {
            assert_eq!(
                value_resolution(initializer),
                BindingValueResolution::ResolvedToText(initializer.to_string()),
                "initializer `{initializer}` is a bare value copy"
            );
        }
    }

    #[test]
    fn supports_comparison_boolean_and_match_positions() {
        let cases = [
            ("if end == start {", PredicateOperandSide::Left),
            ("if start == end {", PredicateOperandSide::Right),
            ("if end != start {", PredicateOperandSide::Left),
            ("if start >= end {", PredicateOperandSide::Right),
            ("while end > limit {", PredicateOperandSide::Left),
            ("if end {", PredicateOperandSide::Boolean),
            ("else if end {", PredicateOperandSide::Boolean),
            ("match end {", PredicateOperandSide::MatchScrutinee),
            ("if other && end == start {", PredicateOperandSide::Left),
            (
                "if start == end || unrelated {",
                PredicateOperandSide::Right,
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(
                direct_predicate_operand(line, "end"),
                Some(expected),
                "operand position mismatch for `{line}`"
            );
        }
    }

    #[test]
    fn field_paths_and_longer_identifiers_never_relate() {
        assert_eq!(
            direct_predicate_operand("if self.end == start {", "end"),
            None
        );
        assert_eq!(
            direct_predicate_operand("if endpoint == start {", "end"),
            None
        );
        assert_eq!(
            direct_predicate_operand("if end_two == start {", "end"),
            None
        );
        assert_eq!(direct_predicate_operand("if start == rend {", "end"), None);
    }

    #[test]
    fn sibling_function_use_never_relates() -> Result<(), String> {
        // The wiring scopes the resolver to the owner function of the
        // changed line (`find_owner_function`), so a same-named binding
        // in a sibling function is never even scanned. This pins the
        // owner-scoped body's outcome; the end-to-end sibling control
        // lives in the #3294 fixtures.
        let body = "pub fn first(input: &str) -> &str {\n    let end = 1;\n    input\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(blocked_scope(&resolution)?, BindingUseScope::NoPredicateUse);
        Ok(())
    }

    #[test]
    fn inner_scope_shadowing_blocks_later_uses() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let end = 1;\n    {\n        let end = 2;\n        end == 3\n    }\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ShadowedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn destructuring_rebind_blocks_later_uses() -> Result<(), String> {
        let body = "pub fn f(pair: (usize, usize)) -> bool {\n    let end = 1;\n    let (end, other) = pair;\n    end == other\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ShadowedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn reassignment_blocks_later_uses() -> Result<(), String> {
        let body =
            "pub fn f() -> bool {\n    let mut end = 1;\n    end = compute();\n    end == 3\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ReassignedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn compound_reassignment_blocks_later_uses() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let mut end = 1;\n    end += 1;\n    end == 3\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ReassignedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn use_before_reassignment_still_relates() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let mut end = 1;\n    if end == 2 { }\n    end = 5;\n    end == 9\n}\n";
        let resolution = resolve_end(body, 2);
        let uses = direct_uses(&resolution)?;
        assert_eq!(uses.len(), 1, "only the pre-reassignment use relates");
        assert_eq!(uses[0].predicate_line, 3);
        Ok(())
    }

    #[test]
    fn closure_use_fails_closed() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let end = 1;\n    let check = |other: usize| end == other;\n    check(2)\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ClosureCaptureUnsupported
        );
        Ok(())
    }

    #[test]
    fn macro_use_fails_closed() -> Result<(), String> {
        let body = "pub fn f() -> bool {\n    let end = 1;\n    ensure!(end == 2);\n    true\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::MacroExpansionUnsupported
        );
        Ok(())
    }

    #[test]
    fn comment_and_string_mentions_never_relate() -> Result<(), String> {
        let body = "// let end = 1;\nlet end = 1;\n// if end == start { }\nlet label = \"if end == start\";\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(blocked_scope(&resolution)?, BindingUseScope::NoPredicateUse);
        Ok(())
    }

    #[test]
    fn if_let_pattern_rebind_blocks_later_uses() -> Result<(), String> {
        // `if let Some(end) = opt` binds its own `end`: it is a re-bind,
        // never a boolean use of the changed binding.
        let body = "pub fn f(opt: Option<usize>) -> bool {\n    let end = 1;\n    if let Some(end) = opt {\n        end == 3\n    } else {\n        false\n    }\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ShadowedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn for_loop_pattern_rebind_blocks_later_uses() -> Result<(), String> {
        let body = "pub fn f(items: &[usize]) -> bool {\n    let end = 1;\n    for end in items {\n        end == 3\n    }\n    false\n}\n";
        let resolution = resolve_end(body, 2);
        assert_eq!(
            blocked_scope(&resolution)?,
            BindingUseScope::ShadowedBeforeUse
        );
        Ok(())
    }

    #[test]
    fn foreign_if_let_keeps_outer_binding_live() -> Result<(), String> {
        // `if let Some(other)` re-binds `other`, not `end`: the later
        // use of `end` still relates.
        let body = "pub fn f(opt: Option<usize>) -> bool {\n    let end = 1;\n    if let Some(other) = opt {\n        other == 3\n    }\n    end == 4\n}\n";
        let resolution = resolve_end(body, 2);
        let uses = direct_uses(&resolution)?;
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].predicate_line, 6);
        Ok(())
    }

    #[test]
    fn two_predicate_uses_stay_separately_identifiable() -> Result<(), String> {
        let body = "pub fn f() -> usize {\n    let end = 1;\n    if end == 2 { }\n    if end != 3 { }\n    end\n}\n";
        let resolution = resolve_end(body, 2);
        let uses = direct_uses(&resolution)?;
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].predicate_line, 3);
        assert_eq!(uses[0].predicate_expression, "if end == 2 { }");
        assert_eq!(uses[1].predicate_line, 4);
        assert_eq!(uses[1].predicate_expression, "if end != 3 { }");
        Ok(())
    }
}

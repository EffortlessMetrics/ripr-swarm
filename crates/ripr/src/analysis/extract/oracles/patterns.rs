use super::arguments::{
    AssertionComparison, assertion_comparison, assertion_oracle_text,
    boolean_assertion_macro_condition, comparable_expression, custom_assertion_arguments,
    equality_assertion_arguments, is_complete_macro_invocation,
};
use super::pattern_admission::{is_unpinned_pattern, some_matches_pins_a_value};
use super::scan::balanced_group_end;
use crate::analysis::classify::{error_constructor_call_paths, rust_string_literals};
use crate::analysis::extract::mask_comments_and_strings;

/// Structural assertion-text shapes that supplement the parsed
/// [`OracleKind`](crate::domain::OracleKind)
/// when deciding whether an oracle is relevant to a probe family.
///
/// These are deliberately private analysis facts rather than new public oracle
/// kinds: they preserve the conservative fallback behavior for custom assertion
/// helpers without widening the serialized oracle vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OracleTextShape {
    ErrorPath,
    SideEffect,
    MemberAccess,
    AssertionOrExpectation,
}

/// Classifies the small set of assertion-text fallback shapes used by reveal
/// analysis. Keeping text recognition here prevents downstream classifiers from
/// each growing their own string-sniffing authority.
pub(crate) fn has_oracle_text_shape(line: &str, shape: OracleTextShape) -> bool {
    match shape {
        OracleTextShape::ErrorPath => line.contains("Error::") || line.contains("Err"),
        OracleTextShape::SideEffect => {
            line.contains("expect")
                || line.contains("mock")
                || line.contains("saved")
                || line.contains("published")
        }
        OracleTextShape::MemberAccess => contains_member_access(line),
        OracleTextShape::AssertionOrExpectation => {
            line.contains("assert") || line.contains("expect")
        }
    }
}

/// Returns true when the assertion contains a Rust-style member access.
///
/// A bare dot is not enough: decimal literals and range operators also contain
/// dots without observing a constructed field. String-literal contents are
/// excluded for the same reason (#2904).
fn contains_member_access(text: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    let mut prev_was_dot = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if prev_was_dot && (ch == '_' || ch.is_alphabetic()) {
            return true;
        }
        prev_was_dot = ch == '.';
    }
    false
}

pub(super) fn is_snapshot_assertion(line: &str) -> bool {
    let expect_test_comparison = (line.contains("expect![[") || line.contains("expect_file!["))
        && (line.contains(".assert_eq(")
            || line.contains(".assert_debug_eq(")
            || line.contains(".assert_json_eq("));
    let known_snapshot_macros = [
        "assert_snapshot!",
        "assert_yaml_snapshot!",
        "assert_json_snapshot!",
        "assert_debug_snapshot!",
        "assert_display_snapshot!",
        "assert_csv_snapshot!",
        "assert_ron_snapshot!",
        "assert_toml_snapshot!",
        "assert_compact_debug_snapshot!",
        "assert_compact_json_snapshot!",
        "assert_binary_snapshot!",
    ];
    known_snapshot_macros
        .iter()
        .any(|macro_name| contains_macro_invocation(line, macro_name))
        || expect_test_comparison
}

pub(crate) fn contains_macro_invocation(line: &str, macro_name: &str) -> bool {
    line.match_indices(macro_name).any(|(index, _)| {
        let prefix_ok = index == 0
            || !line[..index]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        let suffix_start = index + macro_name.len();
        let suffix_ok = line[suffix_start..]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, '(' | '[' | '{'));
        prefix_ok && suffix_ok
    })
}

pub(super) fn is_exact_error_variant_assertion(line: &str) -> bool {
    (line.contains("assert_matches!") || line.contains("matches!") || line.contains("assert_eq!"))
        && line.contains("Err(")
        && !line.contains("Err(_")
}

/// Returns true when `line` is an assertion on a variable known to hold an
/// unwrap_err result and pins a specific error result.
///
/// This recognizes the two-line pattern:
/// ```text
/// let err = f(-1).unwrap_err();
/// assert_eq!(err, MyError::Negative);          // ← this line
/// assert!(matches!(err, MyError::Negative));   // ← or this line
/// assert_eq!(err, MyError::new("negative"));   // ← or constructor equality
/// ```
/// `bound_error_vars` is the set of variable names bound by `.unwrap_err()`
/// or `.expect_err(...)` earlier in the same test body.
pub(crate) fn is_unwrap_err_bound_error_assertion(
    line: &str,
    bound_error_vars: &std::collections::BTreeSet<String>,
) -> bool {
    if bound_error_vars.is_empty() {
        return false;
    }
    let oracle_text = assertion_oracle_text(line);
    let line = oracle_text.as_deref().unwrap_or(line);
    // The line must be an assertion macro invocation.
    let is_assert = line.contains("assert_eq!")
        || line.contains("assert_matches!")
        || line.contains("matches!")
        || line.contains("assert!");
    if !is_assert {
        return false;
    }
    if is_bound_error_equality_assertion(line, bound_error_vars) {
        return true;
    }
    // Must name at least one enum variant (SomeThing::Variant pattern with uppercase last component).
    if !contains_named_enum_variant(line) {
        return false;
    }
    // Must reference one of the known unwrap_err-bound variable names as a token.
    bound_error_vars
        .iter()
        .any(|var| line_references_variable(line, var))
}

fn is_bound_error_equality_assertion(
    line: &str,
    bound_error_vars: &std::collections::BTreeSet<String>,
) -> bool {
    let Some(args) = equality_assertion_arguments(line) else {
        return false;
    };
    let (Some(left), Some(right)) = (args.first(), args.get(1)) else {
        return false;
    };
    let left = comparable_expression(left);
    let right = comparable_expression(right);
    bound_error_vars.iter().any(|var| {
        let var = comparable_expression(var);
        (left == var && expression_pins_specific_error(&right))
            || (right == var && expression_pins_specific_error(&left))
    })
}

fn expression_pins_specific_error(expression: &str) -> bool {
    contains_named_enum_variant(expression)
        || contains_error_constructor_call(expression)
        || contains_error_payload_literal(expression)
}

fn contains_error_constructor_call(expression: &str) -> bool {
    !error_constructor_call_paths(expression).is_empty()
}

fn contains_error_payload_literal(expression: &str) -> bool {
    rust_string_literals(expression)
        .iter()
        .any(|literal| literal_has_fixed_payload_text(literal))
}

fn literal_has_fixed_payload_text(literal: &str) -> bool {
    let mut fixed = String::new();
    let mut chars = literal.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if matches!(chars.peek(), Some('{')) {
                    let _ = chars.next();
                    fixed.push('{');
                    continue;
                }
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                }
            }
            '}' => {
                if matches!(chars.peek(), Some('}')) {
                    let _ = chars.next();
                    fixed.push('}');
                }
            }
            _ => fixed.push(ch),
        }
    }
    fixed.chars().any(|ch| ch.is_alphanumeric())
}

/// Returns true when the line contains a path-qualified enum variant:
/// at least one token of the form `Foo::Bar` where `Bar` starts with an
/// uppercase letter.
pub(crate) fn contains_named_enum_variant(line: &str) -> bool {
    // Split on non-identifier chars, find tokens containing "::" with an
    // uppercase final component.
    for token in line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':')) {
        if !token.contains("::") {
            continue;
        }
        if let Some(last) = token.rsplit("::").next()
            && last
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            return true;
        }
    }
    false
}

/// Returns true when `line` references `var` as a standalone identifier token
/// (not as a substring of a longer identifier).
fn line_references_variable(line: &str, var: &str) -> bool {
    line.match_indices(var).any(|(idx, _)| {
        let before_ok = idx == 0
            || !line[..idx]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        let after_start = idx + var.len();
        let after_ok = after_start >= line.len()
            || !line[after_start..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        before_ok && after_ok
    })
}

pub(super) fn is_broad_error_assertion(line: &str) -> bool {
    let masked = mask_comments_and_strings(line);
    has_method_segment(&masked, "is_err") || masked.contains("Err(_)")
}

/// RIPR-SPEC-0231 rule 4: `.unwrap(`, `.expect(` and the side checks
/// `is_ok`, `is_some` and `is_none` as whole method names.
pub(super) fn is_smoke_check(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    masked.contains(".unwrap(")
        || masked.contains(".expect(")
        || has_method_segment(&masked, "is_ok")
        || has_method_segment(&masked, "is_some")
        || has_method_segment(&masked, "is_none")
}

/// RIPR-SPEC-0231 rule 4: `r.is_ok()` with `r.is_err()`, or `r.is_some()`
/// with `r.is_none()`, as sibling alternatives of one `||` on the same
/// receiver, accepts every value. `left.is_ok() || right.is_err()` checks two
/// values, and `r.is_ok() && r.is_err() || fallback` joins the calls with
/// `&&`, so neither does.
pub(super) fn tests_both_sides(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    if !masked.contains("||") {
        return false;
    }
    bracket_groups(&masked).into_iter().any(|group| {
        let calls = split_top_level_operator(group, "||")
            .into_iter()
            .filter_map(side_call)
            .collect::<Vec<_>>();
        let has = |receiver: &str, name: &str| {
            calls
                .iter()
                .any(|(other, method)| other == receiver && *method == name)
        };
        calls.iter().any(|(receiver, method)| match *method {
            "is_ok" => has(receiver, "is_err"),
            "is_some" => has(receiver, "is_none"),
            _ => false,
        })
    })
}

/// The whole text and the contents of every bracket group in it.
fn bracket_groups(masked: &str) -> Vec<&str> {
    let bytes = masked.as_bytes();
    let mut groups = vec![masked];
    for (open, byte) in bytes.iter().enumerate() {
        if !matches!(byte, b'(' | b'[' | b'{') {
            continue;
        }
        let mut depth = 0usize;
        for (index, inner) in bytes.iter().enumerate().skip(open) {
            match inner {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        groups.push(&masked[open + 1..index]);
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    groups
}

/// Split on a two-byte operator outside brackets.
fn split_top_level_operator<'a>(text: &'a str, operator: &str) -> Vec<&'a str> {
    let bytes = text.as_bytes();
    let op = operator.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && bytes[index..].starts_with(op) => {
                parts.push(&text[start..index]);
                index += op.len();
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&text[start..]);
    parts
}

/// A disjunct that is exactly `receiver.is_ok()` (or `is_err`, `is_some`,
/// `is_none`), optionally parenthesized: the receiver with whitespace
/// removed, and the method.
fn side_call(disjunct: &str) -> Option<(String, &'static str)> {
    let mut text = disjunct.trim();
    while let Some(inner) = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|inner| balanced(inner))
    {
        text = inner.trim();
    }
    let call = text
        .strip_suffix(')')?
        .trim_end()
        .strip_suffix('(')?
        .trim_end();
    let (receiver, method) = call.rsplit_once('.')?;
    let method = ["is_ok", "is_err", "is_some", "is_none"]
        .into_iter()
        .find(|name| method.trim() == *name)?;
    let receiver = receiver.trim_end();
    if receiver.is_empty() || receiver_start(receiver, receiver.len()) != 0 {
        return None;
    }
    Some((receiver.split_whitespace().collect(), method))
}

fn balanced(text: &str) -> bool {
    let mut depth = 0isize;
    for byte in text.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Start of the receiver expression that ends at byte `end`: identifiers,
/// paths, field and method chains, and balanced brackets.
fn receiver_start(masked: &str, end: usize) -> usize {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut index = end;
    while index > 0 {
        let byte = bytes[index - 1];
        match byte {
            b')' | b']' => depth += 1,
            b'(' | b'[' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            b'.' | b':' | b'_' | b'&' | b'*' | b'?' => {}
            _ if byte.is_ascii_alphanumeric() => {}
            _ => break,
        }
        index -= 1;
    }
    index
}

/// `name` as a whole method name: a call after `.` (`r.is_ok()`), or a path
/// after `::` (`Option::is_some`, passed as a function), and not followed by
/// more identifier characters except the combinator forms `name_and` (and
/// `is_none_or`). `is_okay`, `this_errs` and a `status.is_err` field are not
/// `is_ok` / `is_err` checks.
fn has_method_segment(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(index, _)| {
        let before = text[..index].trim_end();
        let method = before.ends_with('.');
        let path = before.ends_with("::");
        let rest = &text[index + name.len()..];
        let rest = rest
            .strip_prefix("_and")
            .or_else(|| {
                (name == "is_none")
                    .then(|| rest.strip_prefix("_or"))
                    .flatten()
            })
            .unwrap_or(rest);
        let whole = !rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        let called = rest.trim_start().starts_with('(');
        whole && ((method && called) || path)
    })
}

pub(super) fn is_whole_object_equality_assertion(line: &str) -> bool {
    (line.contains("assert_eq!") || line.contains("assert_ne!")) && line.contains('{')
}

/// RIPR-SPEC-0231 rule 1: `assert_ne!`, `debug_assert_ne!` and any other
/// macro spelled `*assert_ne!` (`soft_assert_ne!`, `prop_assert_ne!`) only
/// show the value differs from one alternative, so they never pin an exact
/// value. Any such invocation outside strings and comments counts, which
/// fails closed for a macro written after a path or binding.
pub(super) fn is_inequality_macro_assertion(line: &str) -> bool {
    mask_comments_and_strings(line).contains("assert_ne!")
}

/// An `assert_ne!` operand written as a struct literal (`Config { .. }`),
/// which keeps the `whole_object_equality` kind at weak strength. A closure
/// or block operand also contains `{` but is not a struct literal.
pub(super) fn inequality_has_struct_literal_operand(line: &str) -> bool {
    equality_assertion_arguments(line).is_some_and(|arguments| {
        arguments
            .iter()
            .take(2)
            .any(|operand| is_struct_literal(operand.trim()))
    })
}

fn is_struct_literal(operand: &str) -> bool {
    let operand = operand.strip_prefix('&').unwrap_or(operand).trim_start();
    let Some((path, _)) = operand.split_once('{') else {
        return false;
    };
    let path = path.trim_end();
    let path = path
        .split_once('<')
        .map_or(path, |(head, _)| head)
        .trim_end();
    let last = path.rsplit("::").next().unwrap_or(path);
    operand.ends_with('}')
        && last.starts_with(|ch: char| ch.is_ascii_uppercase())
        && path.split("::").all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

/// RIPR-SPEC-0231 rule 1: `assert!(!matches!(..))` (and the `debug_assert!`
/// and `ensure!` forms) only excludes one pattern, so it pins no value and
/// names no error variant the result must have. The negated pattern must be
/// the whole condition; a compound condition is read by
/// [`negated_pattern_conjunct_rest`] or capped by the caller.
pub(super) fn is_negated_pattern_assertion(line: &str) -> bool {
    boolean_assertion_macro_condition(line)
        .is_some_and(|(_, condition)| is_negated_pattern_condition(&condition))
}

/// One complete `!matches!(..)`, optionally parenthesized, with nothing else.
fn is_negated_pattern_condition(condition: &str) -> bool {
    let Some(negated) = unwrap_parentheses(condition.trim()).strip_prefix('!') else {
        return false;
    };
    let inner = unwrap_parentheses(negated.trim());
    is_complete_macro_invocation(inner, "matches!")
        || is_complete_macro_invocation(inner, "assert_matches!")
}

fn unwrap_parentheses(mut text: &str) -> &str {
    while let Some(inner) = super::arguments::parenthesized_contents(text) {
        text = inner.trim();
    }
    text
}

/// For a boolean assertion whose condition is a top-level `&&` chain with at
/// least one whole negated-pattern conjunct, the same assertion over only the
/// other conjuncts (`None` when the condition is not such a chain, and an
/// empty string when every conjunct is a negated pattern). A negated pattern
/// pins nothing, so the remaining conjuncts carry the assertion's reading.
///
/// `&&` binds tighter than `||`, so a condition with any top-level `||` is not
/// a conjunct chain and returns `None` (the caller then caps it).
pub(super) fn negated_pattern_conjunct_rest(line: &str) -> Option<String> {
    let (macro_name, condition) = boolean_assertion_macro_condition(line)?;
    let condition = unwrap_parentheses(condition.trim());
    if has_top_level_disjunction(condition) {
        return None;
    }
    let conjuncts = split_top_level_conjuncts(condition);
    if conjuncts.len() < 2
        || !conjuncts
            .iter()
            .any(|conjunct| is_negated_pattern_condition(conjunct))
    {
        return None;
    }
    let rest = conjuncts
        .iter()
        .filter(|conjunct| !is_negated_pattern_condition(conjunct))
        .map(|conjunct| conjunct.trim())
        .collect::<Vec<_>>();
    if rest.is_empty() {
        return Some(String::new());
    }
    // Each conjunct ends its own line, so a trailing `//` comment on one
    // cannot swallow the next conjunct or the closing `);`.
    Some(format!("{macro_name}({}\n);", rest.join("\n && ")))
}

/// RIPR-SPEC-0231 rule 3 (#7695): for a boolean assertion whose condition is
/// a top-level `&&` chain with at least one whole `matches!(..)` conjunct,
/// the same assertion over each conjunct alone (`None` otherwise). A
/// conjunction is no stronger than its strongest conjunct, so the caller
/// reads each one and a `matches!` keeps its pattern's reading inside the
/// chain (`assert!(ready && matches!(r, Ok(_)))` reads as `Ok(_)` does).
pub(super) fn pattern_conjunct_assertions(line: &str) -> Option<Vec<String>> {
    let (macro_name, condition) = boolean_assertion_macro_condition(line)?;
    let condition = unwrap_parentheses(condition.trim());
    if has_top_level_disjunction(condition) {
        return None;
    }
    let conjuncts = split_top_level_conjuncts(condition);
    let is_pattern = |conjunct: &str| {
        // Masked, so a leading comment cannot hide the pattern.
        let masked = mask_comments_and_strings(conjunct.trim());
        is_complete_macro_invocation(unwrap_parentheses(masked.trim()), "matches!")
    };
    if conjuncts.len() < 2 || !conjuncts.iter().any(|conjunct| is_pattern(conjunct)) {
        return None;
    }
    // Each conjunct ends its own line, so a trailing `//` comment cannot
    // swallow the closing `);`.
    Some(
        conjuncts
            .iter()
            .map(|conjunct| format!("{macro_name}({}\n);", conjunct.trim()))
            .collect(),
    )
}

/// RIPR-SPEC-0231 rule 1 (#7558): the boolean assertion with every
/// parenthesized `&&` conjunct that a `||` decides removed, so the other
/// conjuncts decide the reading, as they do beside a negated pattern
/// (`ensure!(c == d && (a || b))` reads as `ensure!(c == d)`). `None` when
/// no conjunct is such a group or a top-level `||` decides the whole
/// condition; an empty string when nothing else remains. A group that also
/// holds a pinning conjunct (`(x == 1 || y) && z == 2` in parentheses)
/// drops whole, which only weakens the reading.
pub(super) fn deciding_disjunction_conjunct_rest(line: &str) -> Option<String> {
    let (macro_name, condition) = boolean_assertion_macro_condition(line)?;
    let condition = unwrap_parentheses(condition.trim());
    if has_top_level_disjunction(condition) {
        return None;
    }
    let conjuncts = split_top_level_conjuncts(condition);
    let is_deciding_group = |conjunct: &str| {
        let masked = mask_comments_and_strings(conjunct.trim());
        super::arguments::parenthesized_contents(masked.trim())
            .is_some_and(has_deciding_disjunction)
    };
    if conjuncts.len() < 2 || !conjuncts.iter().any(|conjunct| is_deciding_group(conjunct)) {
        return None;
    }
    let rest = conjuncts
        .iter()
        .filter(|conjunct| !is_deciding_group(conjunct))
        .map(|conjunct| conjunct.trim())
        .collect::<Vec<_>>();
    if rest.is_empty() {
        return Some(String::new());
    }
    // Each conjunct ends its own line, so a trailing `//` comment on one
    // cannot swallow the next conjunct or the closing `);`.
    Some(format!("{macro_name}({}\n);", rest.join("\n && ")))
}

/// Splits on `&&` outside any delimiter, string or comment.
fn split_top_level_conjuncts(condition: &str) -> Vec<&str> {
    let masked = mask_comments_and_strings(condition);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut start = 0;
    let mut parts = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'&' if depth == 0 && bytes.get(index + 1) == Some(&b'&') => {
                parts.push(condition.get(start..index).unwrap_or_default());
                index += 2;
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(condition.get(start..).unwrap_or_default());
    parts
}

/// Whether `condition` has `||` outside any delimiter, string or comment.
pub(super) fn has_top_level_disjunction(condition: &str) -> bool {
    let masked = mask_comments_and_strings(condition);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 && bytes.get(index + 1) == Some(&b'|') => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

/// The reading a `matches!` takes when an assertion compares it with `==` or
/// `!=` (RIPR-SPEC-0231 rule 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PatternPolarity {
    /// The comparison asserts the pattern holds (`== true`, `!= false`).
    Positive,
    /// The comparison excludes the pattern or cannot be resolved to a
    /// positive reading, so it fails closed at the inequality strength.
    NotPositive,
}

/// RIPR-SPEC-0231 rule 1, structurally: when an assertion compares a
/// `matches!` (or `assert_matches!`) with `==` or `!=`, either as the two
/// operands of an equality macro (`assert_eq!`, `assert_ne!` and their
/// `debug_` and `prop_` spellings) or at the top level of a boolean
/// assertion condition (`assert!`, `debug_assert!`, `prop_assert!`,
/// `ensure!`), resolve its polarity against the other operand. Only an
/// un-negated pattern against a boolean literal, optionally negated
/// (`!true`), can be positive: `== true`, `!= false`, `== !false`. Every
/// other comparison is not positive: `== false`, `!= true`, `== !true`, a
/// negated pattern operand (`!matches!(..) == false` is a double negation
/// and is not read through), two pattern operands, a non-literal operand,
/// or a compared pattern nested inside a larger condition
/// (`score == 4 && matches!(..) != true`). `None` means the assertion
/// compares no pattern, so the other rules decide.
pub(super) fn pattern_comparison_polarity(line: &str) -> Option<PatternPolarity> {
    match assertion_comparison(line)? {
        AssertionComparison::Operands {
            not_equal,
            left,
            right,
        } => {
            if pattern_operand_negations(&left).is_some()
                || pattern_operand_negations(&right).is_some()
            {
                return Some(resolve_pattern_comparison(&left, &right, not_equal));
            }
            (contains_compared_pattern(&left) || contains_compared_pattern(&right))
                .then_some(PatternPolarity::NotPositive)
        }
        AssertionComparison::Condition(condition) => {
            let condition = unwrap_parentheses(condition.trim());
            if let Some((operator, width)) = top_level_comparison_operator(condition) {
                let left = condition.get(..operator).unwrap_or_default();
                let right = condition.get(operator + width..).unwrap_or_default();
                if pattern_operand_negations(left).is_some()
                    || pattern_operand_negations(right).is_some()
                {
                    let not_equal = condition.get(operator..operator + width) == Some("!=");
                    return Some(resolve_pattern_comparison(left, right, not_equal));
                }
            }
            contains_compared_pattern(condition).then_some(PatternPolarity::NotPositive)
        }
    }
}

fn resolve_pattern_comparison(left: &str, right: &str, not_equal: bool) -> PatternPolarity {
    let literal = match (
        pattern_operand_negations(left),
        pattern_operand_negations(right),
    ) {
        (Some(0), None) => boolean_literal_value(right),
        (None, Some(0)) => boolean_literal_value(left),
        _ => None,
    };
    match literal {
        Some(value) if value != not_equal => PatternPolarity::Positive,
        _ => PatternPolarity::NotPositive,
    }
}

/// How many negations (`!` prefixes and `.not()` calls) wrap one complete
/// `matches!` or `assert_matches!` operand, or `None` when the operand is not
/// such a pattern.
fn pattern_operand_negations(operand: &str) -> Option<usize> {
    let mut text = unwrap_parentheses(operand.trim());
    let mut negations = 0;
    loop {
        if is_complete_macro_invocation(text, "matches!")
            || is_complete_macro_invocation(text, "assert_matches!")
        {
            return Some(negations);
        }
        let rest = text.strip_prefix('!').or_else(|| strip_not_call(text))?;
        text = unwrap_parentheses(rest.trim());
        negations += 1;
    }
}

/// The receiver of a trailing `.not()` call (`std::ops::Not`).
fn strip_not_call(text: &str) -> Option<&str> {
    text.trim_end()
        .strip_suffix(')')?
        .trim_end()
        .strip_suffix('(')?
        .trim_end()
        .strip_suffix("not")?
        .trim_end()
        .strip_suffix('.')
}

/// The value of a boolean literal operand, through parentheses and any
/// number of `!` negations (`!true` is `false`).
fn boolean_literal_value(operand: &str) -> Option<bool> {
    let mut text = unwrap_parentheses(operand.trim());
    let mut negated = false;
    loop {
        match text {
            "true" => return Some(!negated),
            "false" => return Some(negated),
            _ => {}
        }
        text = unwrap_parentheses(text.strip_prefix('!')?.trim());
        negated = !negated;
    }
}

/// One `matches!` invocation in a masked condition: where its (possibly
/// path-qualified) name starts and where its closing delimiter ends.
fn pattern_invocation_spans(text: &str) -> Vec<(usize, usize)> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    masked
        .match_indices("matches!")
        .filter_map(|(found, name)| {
            let mut start = found;
            while start > 0 && (is_ident(bytes[start - 1]) || bytes[start - 1] == b':') {
                start -= 1;
            }
            let after_name = found + name.len();
            let open = after_name
                + masked
                    .get(after_name..)?
                    .find(|ch: char| !ch.is_whitespace())?;
            let contents = super::arguments::delimited_contents_at(text, open)?;
            Some((start, open + contents.len() + 2))
        })
        .collect()
}

/// Whether a `matches!` in `text` is an operand of `==` or `!=`, looking
/// through negations, `.not()` calls and parentheses around it.
fn contains_compared_pattern(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    pattern_invocation_spans(text)
        .into_iter()
        .any(|(start, end)| {
            let mut before = start;
            while before > 0
                && matches!(
                    bytes[before - 1],
                    b'(' | b'!' | b' ' | b'\t' | b'\n' | b'\r'
                )
            {
                before -= 1;
            }
            let compared_before =
                before >= 2 && matches!(&bytes[before - 2..before], b"==" | b"!=");
            let mut after = masked.get(end..).unwrap_or_default();
            loop {
                let trimmed = after.trim_start_matches(|ch: char| ch.is_whitespace() || ch == ')');
                match strip_leading_not_call(trimmed) {
                    Some(rest) => after = rest,
                    None => {
                        after = trimmed;
                        break;
                    }
                }
            }
            compared_before || after.starts_with("==") || after.starts_with("!=")
        })
}

/// The text after a leading `.not()` call.
fn strip_leading_not_call(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('.')?.trim_start().strip_prefix("not")?;
    rest.trim_start()
        .strip_prefix('(')?
        .trim_start()
        .strip_prefix(')')
}

/// Whether a boolean assertion's condition negates a `matches!` anywhere,
/// with a `!` before it or a `.not()` call after it, for example inside `||`
/// or a nested group. Such a condition cannot be read conjunct by conjunct,
/// so the caller caps it at the inequality strength.
pub(super) fn contains_negated_pattern(line: &str) -> bool {
    let Some((_, condition)) = boolean_assertion_macro_condition(line) else {
        return false;
    };
    let masked = mask_comments_and_strings(&condition);
    let bytes = masked.as_bytes();
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    pattern_invocation_spans(&condition)
        .into_iter()
        .any(|(start, end)| {
            // Walk back over whitespace and `(` to the operator before it.
            let mut index = start;
            while index > 0 && (bytes[index - 1].is_ascii_whitespace() || bytes[index - 1] == b'(')
            {
                index -= 1;
            }
            let prefixed =
                index > 0 && bytes[index - 1] == b'!' && (index < 2 || !is_ident(bytes[index - 2]));
            let after = masked
                .get(end..)
                .unwrap_or_default()
                .trim_start_matches(|ch: char| ch.is_whitespace() || ch == ')');
            prefixed || strip_leading_not_call(after).is_some()
        })
}

/// RIPR-SPEC-0231 rule 1: an assertion macro whose own name says it asserts
/// inequality or a non-match (`assert_not_matches!`, `assert_not_eq!`): the
/// first macro invoked has an `assert` name with an inequality segment.
/// `*assert_ne!` is [`is_inequality_macro_assertion`]'s case.
pub(super) fn is_inequality_named_assertion_macro(line: &str) -> bool {
    first_macro_name(line).is_some_and(|name| {
        name.contains("assert") && name.split('_').any(is_inequality_name_segment)
    })
}

/// The name of the first macro invocation (`name!(`, `name![`, `name!{`)
/// outside strings and comments.
fn first_macro_name(line: &str) -> Option<String> {
    let masked = mask_comments_and_strings(line);
    let bytes = masked.as_bytes();
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    masked.match_indices('!').find_map(|(bang, _)| {
        let opens = masked
            .get(bang + 1..)?
            .trim_start()
            .starts_with(['(', '[', '{']);
        let mut start = bang;
        while start > 0 && is_ident(bytes[start - 1]) {
            start -= 1;
        }
        (opens && start < bang).then(|| masked[start..bang].to_ascii_lowercase())
    })
}

/// RIPR-SPEC-0231 rule 1: a call to an `assert_*` helper function whose name
/// has a `ne`, `not` or `neq` segment asserts inequality, whatever else the
/// name says and whatever macros its arguments use
/// (`assert_not_equal(matches!(r, Err(E::Bad)), true)`).
pub(super) fn is_inequality_named_assertion_helper(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some((callee, _)) = trimmed.split_once('(') else {
        return false;
    };
    if callee.contains('!') {
        return false;
    }
    custom_assertion_helper_name(trimmed).is_some_and(|name| {
        name.starts_with("assert_") && name.split('_').any(is_inequality_name_segment)
    })
}

/// One `_`-separated helper-name segment that spells inequality.
fn is_inequality_name_segment(segment: &str) -> bool {
    matches!(
        segment,
        "ne" | "neq"
            | "not"
            | "noteq"
            | "notequal"
            | "nonequal"
            | "unequal"
            | "inequal"
            | "differ"
            | "differs"
            | "different"
            | "distinct"
    )
}

/// Whether an inequality-named helper reads as an observed call or side
/// effect rather than a value comparison. Only the helper name and its
/// receiver count, never its arguments (`assert_not_equal(expect_value(), 3)`
/// is a comparison). A name with a value-comparison segment (`equal`, `eq`,
/// `ne`, ..) is never exempt, whatever its receiver (`mock.assert_ne(x, 3)`,
/// `assert_ne_state(machine, State::Idle)`). Otherwise a name with a
/// call-count or observation token (`assert_not_called`, `assert_not_sent`)
/// is exempt, and so is an argument-free verification on a mock-named
/// receiver (`mock.assert_not_ready()`).
pub(super) fn is_observer_named_assertion_helper(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(name) = custom_assertion_helper_name(trimmed) else {
        return false;
    };
    let name = name.trim_end_matches('!');
    if name.split('_').any(is_value_comparison_name_segment) {
        return false;
    }
    const OBSERVATION_TOKENS: &[&str] = &[
        "called",
        "calls",
        "invoked",
        "invocation",
        "sent",
        "received",
        "emitted",
        "published",
        "written",
        "logged",
        "recorded",
        "saved",
        "stored",
        "persisted",
        "dispatched",
        "delivered",
        "notified",
        "fired",
        "triggered",
        "requested",
    ];
    if OBSERVATION_TOKENS.iter().any(|token| name.contains(token)) {
        return true;
    }
    has_mock_named_receiver(trimmed)
        && custom_assertion_arguments(trimmed).is_some_and(|arguments| {
            arguments
                .iter()
                .all(|argument| mask_comments_and_strings(argument).trim().is_empty())
        })
}

/// One `_`-separated helper-name segment that names a value comparison.
fn is_value_comparison_name_segment(segment: &str) -> bool {
    (is_inequality_name_segment(segment) && segment != "not")
        || matches!(
            segment,
            "eq" | "equal" | "equals" | "same" | "match" | "matches"
        )
}

/// Whether a `.assert_*` method call's receiver is mock-named
/// (`mock.assert_not_called()`, `mock_bus.assert_*`, `self.mock.assert_*`).
fn has_mock_named_receiver(line: &str) -> bool {
    let masked = mask_comments_and_strings(line);
    let Some(callee) = masked.split('(').next() else {
        return false;
    };
    callee.rsplit_once('.').is_some_and(|(receiver, _)| {
        receiver
            .rsplit(['.', ':'])
            .find(|segment| !segment.trim().is_empty())
            .is_some_and(|segment| segment.to_ascii_lowercase().contains("mock"))
    })
}

/// An `ensure!` condition whose only comparison is `!=`: rule 1 at step 0.
/// A mixed condition falls through, and earns exact credit only from a
/// top-level `==` conjunct with no deciding `||`
/// ([`has_top_level_exact_equality`], #7558).
pub(super) fn is_inequality_only_comparison(condition: &str) -> bool {
    let (equal, not_equal) = comparison_operators(condition);
    not_equal && !equal
}

pub(super) fn is_duplicative_equality_assertion(line: &str) -> bool {
    let Some(args) = equality_assertion_arguments(line) else {
        return false;
    };
    let Some(left) = args.first() else {
        return false;
    };
    let Some(right) = args.get(1) else {
        return false;
    };
    comparable_expression(left) == comparable_expression(right)
}

pub(super) fn is_duplicative_comparison(condition: &str) -> bool {
    let Some((operator, width)) = top_level_comparison_operator(condition) else {
        return false;
    };
    let left = condition[..operator].trim();
    let right = condition[operator + width..].trim();
    !left.is_empty()
        && !right.is_empty()
        && comparable_expression(left) == comparable_expression(right)
}

pub(super) fn is_exact_value_assertion(line: &str) -> bool {
    // `assert_ne!` is owned by RIPR-SPEC-0231 rule 1 and never exact.
    line.contains("assert_eq!")
        || line.contains("assert_matches!")
        || (line.contains("matches!")
            && (some_matches_pins_a_value(line)
                // A pinning `==` conjunct keeps the chain exact (RIPR-SPEC-0231
                // rule 3's decision), including the runtime controls'
                // `matches!(value, _) && value == 2`.
                || boolean_assertion_macro_condition(line)
                    .is_some_and(|(_, condition)| has_top_level_exact_equality(&condition))))
}

/// RIPR-SPEC-0231 rule 1 (#7558): whether an `||` decides the condition:
/// at the top level, or inside a parenthesized `&&` conjunct (read again by
/// the same rule), so the condition passes whenever one arm does and pins
/// nothing. An `||` inside a call argument, closure, index or block belongs
/// to that operand.
pub(super) fn has_deciding_disjunction(condition: &str) -> bool {
    has_top_level_disjunction(condition)
        || split_top_level_conjuncts(condition)
            .into_iter()
            .any(|conjunct| {
                // Masked, so a leading comment cannot hide the group.
                let masked = mask_comments_and_strings(conjunct.trim());
                super::arguments::parenthesized_contents(masked.trim())
                    .is_some_and(has_deciding_disjunction)
            })
}

/// RIPR-SPEC-0231 rule 1 (#7558): whether a boolean condition pins a value.
/// It must have no top-level `||` (a disjunction passes whenever one arm
/// does), and some top-level `&&` conjunct must itself be an `==`
/// comparison at its own top level. An `==` inside a closure, a call
/// argument, a negation or an `||` arm pins nothing; a parenthesized
/// conjunct is read again by the same rule.
pub(super) fn has_top_level_exact_equality(condition: &str) -> bool {
    !has_top_level_disjunction(condition)
        && split_top_level_conjuncts(condition)
            .into_iter()
            .any(|conjunct| {
                // Masked, so a leading comment cannot hide the group.
                let masked = mask_comments_and_strings(conjunct.trim());
                let masked = masked.trim();
                if let Some(inner) = super::arguments::parenthesized_contents(masked) {
                    return has_top_level_exact_equality(inner);
                }
                is_pinning_equality(conjunct, masked)
            })
}

/// Whether one conjunct is a top-level `==` that pins a value. `masked` is
/// the conjunct trimmed and masked. `value == value` compares a value with
/// itself, and `matches!(r, Ok(_)) == true` restates an unpinned pattern, so
/// neither pins anything.
fn is_pinning_equality(conjunct: &str, masked: &str) -> bool {
    top_level_comparison_operator(masked).is_some_and(|(at, _)| {
        masked.get(at..).is_some_and(|rest| rest.starts_with("=="))
            && !compares_unpinned_pattern_with_bool(conjunct, at)
    }) && !is_duplicative_comparison(conjunct.trim())
}

/// RIPR-SPEC-0231 rule 3 (#7715): `matches!(r, Ok(_)) == true` states the
/// pattern and nothing more, so it pins no value when the pattern does not.
/// `at` is the `==` offset in the trimmed, masked conjunct.
fn compares_unpinned_pattern_with_bool(conjunct: &str, at: usize) -> bool {
    let conjunct = conjunct.trim();
    let masked = mask_comments_and_strings(conjunct);
    let offset = masked.len() - masked.trim_start().len();
    let at = offset + at;
    let (Some(left), Some(right)) = (conjunct.get(..at), conjunct.get(at + 2..)) else {
        return false;
    };
    // Redundant parentheses (`(true)`, `(matches!(r, Ok(_)))`) change nothing.
    let is_bool =
        |side: &str| boolean_literal_value(mask_comments_and_strings(side).trim()).is_some();
    let is_pattern = |side: &str| is_unpinned_pattern(unwrap_parentheses(side.trim()));
    (is_bool(left) && is_pattern(right)) || (is_bool(right) && is_pattern(left))
}

/// Which of `==` and `!=` appear outside string literals and comments.
fn comparison_operators(condition: &str) -> (bool, bool) {
    let masked = mask_comments_and_strings(condition);
    let mut equal = false;
    let mut not_equal = false;
    let mut chars = masked.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '=' if chars.peek() == Some(&'=') => {
                equal = true;
                chars.next();
            }
            '!' if chars.peek() == Some(&'=') => {
                not_equal = true;
                chars.next();
            }
            _ => {}
        }
    }
    (equal, not_equal)
}

fn top_level_comparison_operator(condition: &str) -> Option<(usize, usize)> {
    // Comments and literals are not delimiters. Mask them before counting
    // brackets so an unmatched `}` inside a nested comment cannot hide a
    // real `==` (`matches!(r, /* } */ Ok(3)) == true`, #7822). Offsets stay
    // valid on the original because masking preserves byte length.
    let masked = mask_comments_and_strings(condition);
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut chars = masked.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '=' | '!'
                if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && matches!(chars.peek(), Some((_, '='))) =>
            {
                return Some((index, 2));
            }
            _ => {}
        }
    }
    None
}

/// `line` with every `Result::expect_err(<message>)` call reduced to `.()`,
/// so no later sniff sees the call name or anything in its message (a
/// message such as `"mock.verify() state"` names nothing). A zero-argument
/// `expect_err()` is a mock's expectation for a method named `err`
/// (mockall's `expect_<method>`), so it stays. A message left open at the
/// end of the line is dropped with the rest of the line.
pub(super) fn without_expect_err(line: &str) -> String {
    const CALL: &str = ".expect_err(";
    // Masking keeps byte length, so offsets found on it index `line`.
    let masked = mask_comments_and_strings(line);
    let mut out = String::with_capacity(line.len());
    let mut copied = 0;
    let mut search = 0;
    while let Some(found) = masked[search..].find(CALL) {
        let start = search + found;
        let open = start + CALL.len() - 1;
        match balanced_group_end(&masked, open) {
            Some(close) if line[open + 1..close].trim().is_empty() => {}
            Some(close) => {
                out.push_str(&line[copied..start]);
                out.push_str(".()");
                copied = close + 1;
            }
            None => {
                out.push_str(&line[copied..start]);
                out.push_str(".()");
                copied = line.len();
                break;
            }
        }
        search = open + 1;
    }
    out.push_str(&line[copied..]);
    out
}

pub(super) fn is_mock_expectation_line(line: &str) -> bool {
    // `r.expect_err(..)` is a `Result` read, not a mock expectation
    // (RIPR-SPEC-0227 decision 2, #7604), and `expect_` or mock text in its
    // message or a comment names nothing.
    let lower = without_expect_err(&line.to_ascii_lowercase());
    let code = mask_comments_and_strings(&lower);
    let has_expectation_call = code.contains("expect_") && lower.contains('(');
    let has_mock_verification_call = lower.contains("mock")
        && [
            ".assert_",
            ".checkpoint(",
            ".times(",
            ".verify(",
            "assert_expectations(",
        ]
        .iter()
        .any(|token| lower.contains(token));
    has_expectation_call || has_mock_verification_call
}

pub(super) fn is_side_effect_observer_assertion(line: &str) -> bool {
    // `r.expect_err("state")` reads the `Result`, not an effect (#7604).
    let lower = without_expect_err(&line.to_ascii_lowercase());
    let has_observer_token = [
        "event",
        "emitted",
        "published",
        "sent",
        "saved",
        "persist",
        "state",
        "stored",
        "metric",
        "counter",
        "recorded",
    ]
    .iter()
    .any(|token| lower.contains(token));
    has_observer_token && (lower.contains("assert") || lower.contains("expect"))
}

/// RIPR-SPEC-0231 rule 5: step 9 credits an effect observer only when a
/// whole identifier segment (`events_sent`, `sentCount`, `events`) names the
/// effect, in the asserted subject rather than only in a free function call
/// such as `is_present()`. The words never match inside another word
/// (`present`, `statement`, `consent`).
pub(super) fn is_effect_observer_subject_assertion(line: &str) -> bool {
    const OBSERVER_WORDS: [&str; 11] = [
        "event",
        "emitted",
        "published",
        "sent",
        "saved",
        "persist",
        "state",
        "stored",
        "metric",
        "counter",
        "recorded",
    ];
    let lower = without_expect_err(&line.to_ascii_lowercase());
    if !(lower.contains("assert") || lower.contains("expect")) {
        return false;
    }
    // In a known assertion macro only the asserted operands are subjects,
    // and a word inside their comments or strings is not one. Other text
    // keeps the line-level reading.
    let masked = assertion_oracle_text(line).map_or_else(
        || without_expect_err(line),
        |operands| mask_comments_and_strings(&operands),
    );
    identifiers_with_position(&masked).any(|(start, identifier)| {
        let free_call = masked[start + identifier.len()..]
            .trim_start()
            .starts_with('(')
            && !masked[..start].trim_end().ends_with('.');
        !free_call
            && identifier_segments(identifier).any(|segment| {
                OBSERVER_WORDS.contains(&segment.as_str())
                    || segment
                        .strip_suffix('s')
                        .is_some_and(|base| OBSERVER_WORDS.contains(&base))
            })
    })
}

fn identifiers_with_position(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = None;
    let mut out = Vec::new();
    for (index, ch) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        let ident = ch.is_ascii_alphanumeric() || ch == '_';
        match (start, ident) {
            (None, true) => start = Some(index),
            (Some(begin), false) => {
                let word = &text[begin..index];
                if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                    out.push((begin, word));
                }
                start = None;
            }
            _ => {}
        }
    }
    out.into_iter()
}

/// `_`-separated and case-split segments, lowercased: `sentCount` gives
/// `sent` and `count`.
fn identifier_segments(identifier: &str) -> impl Iterator<Item = String> + '_ {
    identifier.split('_').flat_map(|part| {
        let mut segments = Vec::new();
        let mut current = String::new();
        let mut previous_lower = false;
        for ch in part.chars() {
            if ch.is_ascii_uppercase() && previous_lower && !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            previous_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            current.push(ch.to_ascii_lowercase());
        }
        if !current.is_empty() {
            segments.push(current);
        }
        segments
    })
}

pub(super) fn is_custom_assertion_helper(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.contains('!')
        && (trimmed.starts_with("assert_")
            || trimmed.contains("::assert_")
            || trimmed.contains(".assert_"))
        && trimmed.contains('(')
}

/// Whether a custom `assert_*` helper call has the argument count of a value
/// comparison: two or more arguments for a free function, or at least one
/// for a `.assert_*` method whose receiver is the other side.
pub(super) fn is_comparison_shaped_custom_assertion(line: &str) -> bool {
    is_custom_assertion_helper(line)
        && custom_assertion_arguments(line).is_some_and(|arguments| {
            if line.contains(".assert_") {
                !arguments.is_empty()
            } else {
                arguments.len() >= 2
            }
        })
}

pub(super) fn is_clear_exact_custom_assertion_helper(line: &str) -> bool {
    // Inequality wins: `assert_noteq` ends with `eq` but pins nothing.
    if !is_custom_assertion_helper(line) || is_inequality_named_assertion_helper(line) {
        return false;
    }
    let Some(name) = custom_assertion_helper_name(line) else {
        return false;
    };
    let Some(arguments) = custom_assertion_arguments(line) else {
        return false;
    };
    let argument_count_supports_exact = if line.contains(".assert_") {
        !arguments.is_empty()
    } else {
        arguments.len() >= 2
    };
    // RIPR-SPEC-0231 rule 6: only an equality name is strong. Excluding a
    // `ne`, `not` or `neq` segment is rule 1's case (#6670), not this check.
    let segments = name.split('_').collect::<Vec<_>>();
    let last = segments.last().copied();
    let equality_name = matches!(last, Some("eq" | "equal" | "equals"))
        || (segments.len() > 1 && matches!(last, Some("matches")));
    argument_count_supports_exact && equality_name
}

fn custom_assertion_helper_name(line: &str) -> Option<String> {
    let before_args = line.split_once('(')?.0.trim();
    let name = before_args
        .rsplit([':', '.'])
        .find(|part| !part.is_empty())?
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_ascii_lowercase())
    }
}

/// True when `line` is an `assert!`/`debug_assert!` whose whole condition is
/// an exact-equality `.any()` membership check over an iterated collection
/// (`assert!(lines.iter().any(|l| l == "audited 42"))`, #6991,
/// RIPR-SPEC-0231 rule 7). Such an assertion fails for any wrong member
/// value, so it pins an exact value exactly like `assert_eq!`.
///
/// The shape is deliberately narrow: the closure body must be one `==` of
/// the bound element (bare, dereferenced, or field-selected, never a call)
/// against a literal or const path, with no negation, no relational
/// operator, and no `&&`/`||` widening. Anything wider stays weak: a
/// missed promotion keeps a gap open, while a wrong one manufactures a
/// false `exposed`.
pub(super) fn is_exact_membership_any_assertion(line: &str) -> bool {
    exact_membership_any_condition(line).is_some()
}

fn exact_membership_any_condition(line: &str) -> Option<()> {
    let masked = mask_comments_and_strings(line);
    let mut inner = strip_assert_condition(&masked)?;
    // Peel redundant wrapping parens; a leading `!` or a trailing
    // `&&`/`||` operand survives peeling and fails the shape below.
    loop {
        let trimmed = inner.trim();
        if !trimmed.starts_with('(') {
            inner = trimmed;
            break;
        }
        let (_, close) = balanced_paren_span(trimmed, 0)?;
        if close + 1 != trimmed.len() {
            return None;
        }
        inner = trimmed[1..close].trim();
    }
    let any_dot = single_any_call(inner)?;
    let receiver = inner.get(..any_dot)?.trim();
    let (closure_start, closure_end, after_any) = any_closure_span(inner, any_dot)?;
    if !inner.get(after_any..)?.trim().is_empty() {
        return None;
    }
    collection_iteration_head(receiver)?;
    let closure = inner.get(closure_start..closure_end)?;
    let (param, body_masked) = exact_equality_closure(closure)?;
    // Masking preserves byte layout, so the masked body's offset reads the
    // original expected side (literals live only in the original text).
    let body_offset = body_masked
        .as_ptr()
        .addr()
        .checked_sub(masked.as_ptr().addr())?;
    let body_original = line.get(body_offset..body_offset.checked_add(body_masked.len())?)?;
    exact_membership_operands(body_masked, body_original, param)
}

/// The condition of a leading `assert!(`/`debug_assert!(` invocation: the
/// text between its outer parens. Custom helpers and other macros never
/// match, so they keep their existing later-step reading.
fn strip_assert_condition(masked: &str) -> Option<&str> {
    let trimmed = masked.trim();
    let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim();
    let after_name = trimmed
        .strip_prefix("debug_assert!")
        .or_else(|| trimmed.strip_prefix("assert!"))?;
    let after_paren = after_name.trim_start().strip_prefix('(')?;
    after_paren.strip_suffix(')')
}

/// Byte span of the contents of the paren group opening at `open`.
/// All delimiters are ASCII, so every returned index is a char boundary.
fn balanced_paren_span(text: &str, open: usize) -> Option<(usize, usize)> {
    if text.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((open + 1, index));
                }
            }
            _ => {}
        }
    }
    None
}

/// Offset of `.any` when the condition holds exactly one `.any(` call.
/// String and comment contents are already masked, so a literal spelling
/// `.any(` can never count.
fn single_any_call(condition: &str) -> Option<usize> {
    let mut found = None;
    let bytes = condition.as_bytes();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if bytes[index] == b'.'
            && bytes.get(index + 1) == Some(&b'a')
            && bytes.get(index + 2) == Some(&b'n')
            && bytes.get(index + 3) == Some(&b'y')
            && bytes.get(index + 4..).is_some_and(opens_call_paren)
        {
            if found.is_some() {
                return None;
            }
            found = Some(index);
            index += 4;
        } else {
            index += 1;
        }
    }
    found
}

fn opens_call_paren(rest: &[u8]) -> bool {
    rest.iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'(')
}

/// Contents span of the `.any(...)` argument list plus the offset just
/// past its closing paren, from the `.` offset.
fn any_closure_span(condition: &str, any_dot: usize) -> Option<(usize, usize, usize)> {
    let bytes = condition.as_bytes();
    let mut open = any_dot.checked_add(4)?;
    while bytes
        .get(open)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        open = open.checked_add(1)?;
    }
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let (start, end) = balanced_paren_span(condition, open)?;
    Some((start, end, end.checked_add(1)?))
}

/// The collection expression when `receiver` ends in a no-argument
/// `.iter()`/`.iter_mut()`/`.into_iter()` call over a call-free value path.
/// Method chains with arguments, turbofish, and indexing never match: the
/// receiver must read as iterating a held collection, not as computing one.
fn collection_iteration_head(receiver: &str) -> Option<&str> {
    let dot = receiver.rfind('.')?;
    let head = receiver.get(..dot)?.trim();
    let call: String = receiver
        .get(dot + 1..)?
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if !matches!(call.as_str(), "iter()" | "iter_mut()" | "into_iter()") {
        return None;
    }
    if head.is_empty() || head.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    for segment in head.split("::") {
        if segment.is_empty() {
            return None;
        }
        for part in segment.split('.') {
            if part.is_empty() {
                return None;
            }
            // Only bare `()` calls read a collection; `strip_suffix`
            // leaves any argument list behind, which is never an ident.
            let bare = part.strip_suffix("()").unwrap_or(part);
            if bare.is_empty() || !is_ident_or_index(bare) {
                return None;
            }
        }
    }
    Some(head)
}

fn is_ident_or_index(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.bytes().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The bound parameter and masked body of a `|param| body` closure with a
/// single identifier parameter. `move`, patterns, and multi-parameter
/// closures never match. The body keeps its trailing layout (a masked
/// literal is trailing spaces) so its span still addresses the original
/// expected side.
fn exact_equality_closure(closure: &str) -> Option<(&str, &str)> {
    let after_first = closure.trim_start().strip_prefix('|')?;
    let params_end = after_first.find('|')?;
    let params = after_first.get(..params_end)?.trim();
    if params.is_empty() || params == "_" {
        return None;
    }
    // One parameter only; a second `|` or a comma outside any bracket
    // pair means more. Commas nested in a type ascription (`&(u32, u32)`)
    // still read as one parameter; angle-nested commas stay rejected
    // (fail-closed residual: `<` also opens const-generic comparisons).
    if params.bytes().any(|byte| byte == b'|') || has_top_level_comma(params) {
        return None;
    }
    let name = params.split(':').next()?.trim();
    if !is_path_ident(name) {
        return None;
    }
    let body = after_first.get(params_end + 1..)?;
    Some((name, body))
}

/// True when `params` holds a comma at nesting depth zero, outside any
/// `()`, `[]`, or `{}` pair. A nested comma belongs to a type ascription,
/// not to a second parameter.
fn has_top_level_comma(params: &str) -> bool {
    let mut depth = 0u32;
    for byte in params.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// True when the masked body is exactly `element == expected` with no
/// other operator, and the original expected side is a literal or const
/// path. The masked and original bodies share byte layout, so the caller
/// may slice the original at masked offsets.
fn exact_membership_operands(masked_body: &str, original_body: &str, param: &str) -> Option<()> {
    if masked_body.contains("&&")
        || masked_body.contains("||")
        || masked_body.bytes().any(|byte| {
            matches!(
                byte,
                b'!' | b'<'
                    | b'>'
                    | b'&'
                    | b'|'
                    | b'('
                    | b')'
                    | b'['
                    | b']'
                    | b'{'
                    | b'}'
                    | b'?'
                    | b';'
                    | b','
            )
        })
    {
        return None;
    }
    let mut equality = masked_body.match_indices("==");
    let (offset, _) = equality.next()?;
    if equality.next().is_some() {
        return None;
    }
    let element = masked_body.get(..offset)?.trim();
    let expected = original_body.get(offset + 2..)?.trim();
    if element.is_empty() || expected.is_empty() {
        return None;
    }
    if !element_accesses_param(element, param) {
        return None;
    }
    if !is_pinned_expected_value(expected) {
        return None;
    }
    Some(())
}

/// True when `element` is the closure parameter with only dereferences
/// and field or tuple selections: `l`, `*l`, `l.total`. Any call,
/// index, or foreign root never matches.
fn element_accesses_param(element: &str, param: &str) -> bool {
    let compact: String = element.chars().filter(|ch| !ch.is_whitespace()).collect();
    let access = compact.trim_start_matches('*');
    if access.is_empty() {
        return false;
    }
    let mut fields = access.split('.');
    if fields.next() != Some(param) {
        return false;
    }
    fields.all(|field| !field.is_empty() && is_ident_or_index(field))
}

/// True when `expected` is a boolean, numeric, string, or character
/// literal, or a path whose last segment reads as a constant or variant
/// (`Config::LIMIT`, `Color::Red`, `None`). Calls, constructors,
/// lowercase bindings, bare uppercase names, and operators never pin.
fn is_pinned_expected_value(expected: &str) -> bool {
    if matches!(expected, "true" | "false") {
        return true;
    }
    if is_numeric_literal(expected) || is_string_literal(expected) || is_char_literal(expected) {
        return true;
    }
    is_const_path(expected)
}

fn is_numeric_literal(expected: &str) -> bool {
    let digits = expected.strip_prefix(['-', '+']).unwrap_or(expected);
    let Some(first) = digits.bytes().next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    if digits.contains("..") {
        return false;
    }
    let (mantissa, exponent) = split_decimal_exponent(digits);
    if !is_decimal_mantissa(mantissa) {
        return false;
    }
    match exponent {
        None => true,
        Some(exp) => {
            let exp = exp.strip_prefix(['-', '+']).unwrap_or(exp);
            let mut bytes = exp.bytes();
            matches!(bytes.next(), Some(b) if b.is_ascii_digit())
                && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        }
    }
}

/// Splits one optional `e`/`E` decimal exponent (`1e-3`, `1.5E+6`) from
/// the mantissa. Radix-prefixed mantissas (`0x1E`) never split: the radix
/// prefix owns the `e`.
fn split_decimal_exponent(digits: &str) -> (&str, Option<&str>) {
    let bytes = digits.as_bytes();
    if bytes.len() > 2
        && bytes[0] == b'0'
        && matches!(bytes[1], b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
    {
        return (digits, None);
    }
    match digits.find(['e', 'E']) {
        Some(idx) => {
            let (mantissa, rest) = digits.split_at(idx);
            (mantissa, rest.get(1..))
        }
        None => (digits, None),
    }
}

fn is_decimal_mantissa(mantissa: &str) -> bool {
    let mut parts = mantissa.split('.');
    let int = parts.next().unwrap_or_default();
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return false;
    }
    match parts.next() {
        None => parts.next().is_none(),
        Some(frac) => {
            !frac.is_empty()
                && frac.bytes().next().is_some_and(|b| b.is_ascii_digit())
                && frac.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && parts.next().is_none()
        }
    }
}

fn is_string_literal(expected: &str) -> bool {
    let bytes = expected.as_bytes();
    let mut index = 0;
    if bytes.first().is_some_and(|b| matches!(b, b'b' | b'c'))
        && bytes.get(1).is_some_and(|b| matches!(b, b'"' | b'r'))
    {
        index += 1;
    }
    let mut hashes = 0;
    let mut raw = false;
    if bytes.get(index) == Some(&b'r') {
        raw = true;
        index += 1;
        while bytes.get(index) == Some(&b'#') {
            hashes += 1;
            index += 1;
        }
    }
    if bytes.get(index) != Some(&b'"') {
        return false;
    }
    index += 1;
    if !raw {
        while let Some(byte) = bytes.get(index) {
            match byte {
                b'\\' => index += 2,
                b'"' => return bytes.len() == index + 1,
                _ => index += 1,
            }
        }
        return false;
    }
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let mut closing = 0;
            while bytes.get(index + 1 + closing) == Some(&b'#') {
                closing += 1;
            }
            if closing == hashes {
                return bytes.len() == index + 1 + hashes;
            }
        }
        index += 1;
    }
    false
}

fn is_char_literal(expected: &str) -> bool {
    let body = expected
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''));
    let Some(body) = body else {
        return false;
    };
    if body.is_empty() || body.contains('\n') {
        return false;
    }
    if let Some(escape) = body.strip_prefix('\\') {
        if let Some(code) = escape
            .strip_prefix("u{")
            .and_then(|rest| rest.strip_suffix('}'))
        {
            return !code.is_empty()
                && code.len() <= 6
                && code.bytes().all(|b| b.is_ascii_hexdigit());
        }
        if let Some(hex) = escape.strip_prefix('x') {
            return hex.len() == 2 && hex.bytes().all(|b| b.is_ascii_hexdigit());
        }
        return matches!(escape, "n" | "r" | "t" | "\\" | "0" | "'" | "\"");
    }
    body.chars().count() == 1
}

/// True when `expected` names a constant or variant that cannot be a
/// local binding: a `::` path whose last segment starts uppercase
/// (`Config::LIMIT`, `Color::Red`), or bare `None` (`let None = ..`
/// never compiles, so no local can shadow it). A bare uppercase name
/// (`EXPECTED`) may be a local assigned from dynamic data, so it never
/// pins.
fn is_const_path(expected: &str) -> bool {
    if expected == "None" {
        return true;
    }
    if expected.is_empty() || !expected.contains("::") {
        return false;
    }
    let mut last = "";
    for segment in expected.split("::") {
        if segment.is_empty() || !is_path_ident(segment) {
            return false;
        }
        last = segment;
    }
    last.bytes()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
}

fn is_path_ident(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod spec_0106_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn vars(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // Control 1 (POSITIVE): unwrap_err binding + named variant → upgrade fires.
    #[test]
    fn is_unwrap_err_bound_error_assertion_upgrades_named_variant() {
        let bound = vars(&["err"]);
        assert!(
            is_unwrap_err_bound_error_assertion("assert_eq!(err, CalcError::Negative);", &bound),
            "exact variant assertion on bound var must be recognized"
        );
        assert!(
            is_unwrap_err_bound_error_assertion(
                "assert!(matches!(err, CalcError::Negative));",
                &bound
            ),
            "matches! variant assertion on bound var must be recognized"
        );
    }

    #[test]
    fn is_unwrap_err_bound_error_assertion_upgrades_constructor_payload_equality()
    -> Result<(), String> {
        let bound = vars(&["err"]);
        if !is_unwrap_err_bound_error_assertion(
            r#"assert_eq!(err, CargoAllowError::new(format!("duplicate allow id `{}`", id)));"#,
            &bound,
        ) {
            return Err(
                "exact constructor-payload equality on bound error must be recognized".to_string(),
            );
        }
        if is_unwrap_err_bound_error_assertion("assert_eq!(err, expected_error);", &bound) {
            return Err(
                "opaque expected variables must not be promoted to exact error variants"
                    .to_string(),
            );
        }
        if is_unwrap_err_bound_error_assertion(
            r#"assert_ne!(err, CargoAllowError::new(format!("duplicate allow id `{}`", id)));"#,
            &bound,
        ) {
            return Err("negative constructor-payload assertions must not be promoted".to_string());
        }
        if is_unwrap_err_bound_error_assertion("assert_ne!(err, CalcError::Negative);", &bound) {
            return Err("negative enum-variant assertions must not be promoted".to_string());
        }
        Ok(())
    }

    // Control 3 (GENERIC): generic assertion without variant token → no upgrade.
    #[test]
    fn is_unwrap_err_bound_error_assertion_rejects_placeholder_only_string_payload()
    -> Result<(), String> {
        let bound = vars(&["err"]);
        let cases = [
            (
                "placeholder-only",
                r#"assert_eq!(err, format!("{}", id));"#,
                false,
            ),
            (
                "escaped-placeholder-only",
                r#"assert_eq!(err, format!("{{}} {}", id));"#,
                false,
            ),
            (
                "fixed-text",
                r#"assert_eq!(err, format!("duplicate allow id `{}`", id));"#,
                true,
            ),
            (
                "escaped-fixed-text",
                r#"assert_eq!(err, format!("{{duplicate}} {}", id));"#,
                true,
            ),
        ];
        for (label, assertion, expected) in cases {
            let actual = is_unwrap_err_bound_error_assertion(assertion, &bound);
            if actual != expected {
                return Err(format!("{label}: expected {expected}, got {actual}"));
            }
        }
        Ok(())
    }

    #[test]
    fn generic_assertion_on_bound_var_not_upgraded() {
        let bound = vars(&["err"]);
        assert!(
            !is_unwrap_err_bound_error_assertion(
                "assert!(err.to_string().contains(\"error\"));",
                &bound
            ),
            "generic string-contains assertion must not be treated as ExactErrorVariant"
        );
        assert!(
            !is_unwrap_err_bound_error_assertion("assert!(result.is_err());", &bound),
            "is_err assertion must not be treated as ExactErrorVariant"
        );
    }

    // Empty bound_vars guard — must return false without accessing bound_vars.
    #[test]
    fn empty_bound_vars_returns_false() {
        let empty = vars(&[]);
        assert!(
            !is_unwrap_err_bound_error_assertion("assert_eq!(err, CalcError::Negative);", &empty),
            "empty bound_vars must short-circuit to false"
        );
    }

    // Variable must be referenced as a token (not a substring).
    #[test]
    fn variable_must_be_token_not_substring() {
        let bound = vars(&["err"]);
        // "cerr" contains "err" as a substring — must not match.
        assert!(
            !is_unwrap_err_bound_error_assertion("assert_eq!(cerr, CalcError::Negative);", &bound),
            "substring match of variable must not fire"
        );
    }

    #[test]
    fn contains_named_enum_variant_recognizes_qualified_variant() {
        assert!(
            contains_named_enum_variant("assert_eq!(err, CalcError::Negative);"),
            "CalcError::Negative must be recognized"
        );
        assert!(
            !contains_named_enum_variant("assert!(err.to_string().contains(\"error\"));"),
            "no qualified variant — must return false"
        );
    }

    #[test]
    fn exact_equality_ignores_string_contents_and_escapes() -> Result<(), String> {
        for (condition, expected) in [
            ("state == TerminalState::Pass", true),
            ("state != TerminalState::Pending", false),
            (r#"label.contains("==")"#, false),
            (r#"label == "a || b""#, true),
            (r#"label.contains("escaped \"!=\" text")"#, false),
            ("ready = true", false),
            // #7558: only a top-level `==` outside every `||` arm pins a value.
            ("a != b || c == d", false),
            ("a == b || c == d", false),
            ("xs.iter().position(|x| *x == 1) != None", false),
            ("f(|x| x == 1)", false),
            ("!(a == b)", false),
            ("a != b && c == d", true),
            ("(a == b) && ready", true),
            ("(a == b || c) && ready", false),
            ("total(&cart) == 42 // a || b", true),
            ("/* pinned */ (a == b) && ready", true),
            // #7822: comment brackets are not delimiters of the `==`.
            ("matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == true", true),
            ("matches!(r, // , ( [ ) ] }\n Ok(3)) == true", true),
            // An unpinned pattern compared with a bool still pins nothing.
            (
                "matches!(r, /* , ( [ /* , ) ] */ } */ Ok(_)) == true",
                false,
            ),
        ] {
            let actual = has_top_level_exact_equality(condition);
            if actual != expected {
                return Err(format!(
                    "comparison classification mismatch for {condition}: {actual}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn bool_compared_matches_polarity_ignores_comment_brackets() -> Result<(), String> {
        // #7822: an unmatched `}` in a nested comment must not turn a
        // positive `== true` comparison into NotPositive.
        for (line, expected) in [
            (
                "ensure!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == true);",
                Some(PatternPolarity::Positive),
            ),
            (
                "ensure!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(_)) == true);",
                Some(PatternPolarity::Positive),
            ),
            (
                "ensure!(matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)) == false);",
                Some(PatternPolarity::NotPositive),
            ),
            (
                "assert!(true == matches!(r, /* , ( [ /* , ) ] */ } */ Ok(3)));",
                Some(PatternPolarity::Positive),
            ),
        ] {
            let actual = pattern_comparison_polarity(line);
            if actual != expected {
                return Err(format!(
                    "polarity mismatch for {line}: {actual:?}, expected {expected:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn deciding_disjunctions_are_found_past_comments_and_groups() -> Result<(), String> {
        for (condition, expected) in [
            ("a || b", true),
            ("ready && (a || b)", true),
            ("ready && /* fallback */ (a || b)", true),
            ("((a == b || c))", true),
            ("ready && f(a || b)", false),
            ("xs.iter().any(|x| x == 1 || x == 2)", false),
            ("a == b // || c", false),
            (r#"a == "x || y""#, false),
        ] {
            let actual = has_deciding_disjunction(condition);
            if actual != expected {
                return Err(format!(
                    "deciding disjunction mismatch for {condition}: {actual}"
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod oracle_text_shape_tests {
    use super::*;

    #[test]
    fn preserves_reveal_family_fallbacks() {
        let cases = [
            (
                OracleTextShape::ErrorPath,
                "assert_eq!(kind, Error::Denied);",
                true,
            ),
            (
                OracleTextShape::ErrorPath,
                "assert_eq!(kind, Success::Ready);",
                false,
            ),
            (
                OracleTextShape::SideEffect,
                "assert!(event.published);",
                true,
            ),
            (OracleTextShape::SideEffect, "assert!(result.ready);", false),
            (
                OracleTextShape::MemberAccess,
                "assert_eq!(item.id, 3);",
                true,
            ),
            (
                OracleTextShape::MemberAccess,
                "assert!(3.14_f64 > 0.0_f64);",
                false,
            ),
            (
                OracleTextShape::MemberAccess,
                "assert_eq!(msg, \"error.timeout\");",
                false,
            ),
            (
                OracleTextShape::MemberAccess,
                r#"assert_eq!(msg, "error.\"timeout");"#,
                false,
            ),
            (
                OracleTextShape::AssertionOrExpectation,
                "expect_send_called();",
                true,
            ),
            (
                OracleTextShape::AssertionOrExpectation,
                "record_send_call();",
                false,
            ),
        ];

        for (shape, text, expected) in cases {
            assert_eq!(
                has_oracle_text_shape(text, shape),
                expected,
                "text-shape classification mismatch for {shape:?} and {text}"
            );
        }
    }
}

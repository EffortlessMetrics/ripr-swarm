use super::arguments::{
    assertion_oracle_text, comparable_expression, custom_assertion_arguments,
    equality_assertion_arguments,
};
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
    line.contains("is_err") || line.contains("Err(_)")
}

pub(super) fn is_whole_object_equality_assertion(line: &str) -> bool {
    (line.contains("assert_eq!") || line.contains("assert_ne!")) && line.contains('{')
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
    line.contains("assert_eq!")
        || line.contains("assert_ne!")
        || line.contains("assert_matches!")
        || line.contains("matches!")
}

pub(super) fn contains_exact_comparison(condition: &str) -> bool {
    let mut chars = condition.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
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
        match ch {
            '"' => in_string = true,
            '=' | '!' if matches!(chars.peek(), Some('=')) => return true,
            _ => {}
        }
    }
    false
}

fn top_level_comparison_operator(condition: &str) -> Option<(usize, usize)> {
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = condition.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
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
        match ch {
            '"' => in_string = true,
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

pub(super) fn is_mock_expectation_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let has_expectation_call = lower.contains("expect_") && lower.contains('(');
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
    let lower = line.to_ascii_lowercase();
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

pub(super) fn is_custom_assertion_helper(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.contains('!')
        && (trimmed.starts_with("assert_")
            || trimmed.contains("::assert_")
            || trimmed.contains(".assert_"))
        && trimmed.contains('(')
}

pub(super) fn is_clear_exact_custom_assertion_helper(line: &str) -> bool {
    if !is_custom_assertion_helper(line) {
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
    argument_count_supports_exact
        && (name.contains("_eq")
            || name.contains("_equal")
            || name.contains("_matches")
            || name.ends_with("eq")
            || name.ends_with("equal")
            || name.ends_with("matches"))
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
    fn exact_comparison_ignores_string_contents_and_escapes() -> Result<(), String> {
        for (condition, expected) in [
            ("state == TerminalState::Pass", true),
            ("state != TerminalState::Pending", true),
            (r#"label.contains("==")"#, false),
            (r#"label.contains("escaped \"!=\" text")"#, false),
            ("ready = true", false),
        ] {
            let actual = contains_exact_comparison(condition);
            if actual != expected {
                return Err(format!(
                    "comparison classification mismatch for {condition}: {actual}"
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

//! Bounded literal-driven `match` transfer (#3296, the match-arm arm
//! of the #3215 helper-evidence slice).
//!
//! A match helper has the exact shape the corpus pins:
//!
//! ```text
//! match <exact scrutinee> {
//!     "literal" => "value",
//!     _ => "value",
//! }
//! ```
//!
//! as the whole helper body (a tail expression). When the scrutinee
//! resolves exactly (#3295 facts) to a string or a char and every
//! arm's pattern is `_` or one or more `|`-joined literals of that same
//! type (an escape decodes through the shared #3295 literal parser) and
//! every arm's value is a plain string literal or one nested direct
//! call to a unique helper, the evaluator returns the first arm whose
//! literal equals the scrutinee — source order is authoritative,
//! exactly like the Rust match. A
//! nested call resolves through the shared bounded context (the
//! #3296 recursive row): distinct `(helper, bound inputs)` states
//! unroll within the explicit hop bound, while a repeated state (a
//! true cycle) or the bound itself refuses. An arm may carry a `if
//! <expr>` guard: exactly resolvable guards (one comparison over the
//! shared #3295 operands) are evaluated over the exact inputs and the
//! arm is selected only when the comparison holds. Anything else — an
//! unevaluable guard (chained, method-call or otherwise non-comparison
//! shape), a computed value, a bare-identifier pattern, a range or path
//! pattern, a pattern whose type differs from the scrutinee's, any
//! statement after the closing brace — refuses the whole shape and
//! returns `None`; the caller's
//! existing limitation wording stays the disclosure.

use super::helper_transfer::HelperEval;
use super::value_transfer::{EvalOutcome, ExactInputs, TypedValue, evaluate_initializer};
use crate::analysis::language::mask_rust_comments_and_strings;

/// The parsed match body: the exact scrutinee expression and the
/// literal arms in source order.
struct MatchShape {
    scrutinee: String,
    arms: Vec<MatchArm>,
}

/// An arm's value: a plain string literal, or one nested direct call
/// to a unique helper (`label_of("a")`) resolved through the shared
/// bounded context (#3296 recursive row).
enum MatchValue {
    Literal(String),
    Call(String),
}

struct MatchArm {
    /// `None` is the `_` wildcard arm; `Some(literals)` is one literal
    /// pattern or the `|`-joined alternatives, each an exact string or
    /// char. A wildcard still respects source order.
    pattern: Option<Vec<TypedValue>>,
    /// A `if <expr>` guard: the arm is selected only when the pattern
    /// matches AND the guard evaluates to `true` over the exact inputs.
    /// `None` is an unguarded arm.
    guard: Option<String>,
    value: MatchValue,
}

/// Evaluate a match helper's return over its exact inputs, when the
/// helper body is one literal `match` tail expression.
pub(crate) fn match_return_value(
    helper: &crate::analysis::facts::FunctionSummary,
    inputs: &ExactInputs,
    eval: &HelperEval<'_>,
) -> Option<TypedValue> {
    let shape = MatchShape::parse(&helper.body)?;
    let subject = match evaluate_initializer(&shape.scrutinee, inputs) {
        EvalOutcome::Exact {
            value: value @ (TypedValue::Str(_) | TypedValue::Char(_)),
            ..
        } => value,
        _ => return None,
    };
    for arm in &shape.arms {
        let selected = match &arm.pattern {
            None => true,
            Some(literals) => {
                // A literal of the other type would not compile against
                // this scrutinee; refuse rather than guess a mismatch.
                if literals.iter().any(|literal| {
                    std::mem::discriminant(literal) != std::mem::discriminant(&subject)
                }) {
                    return None;
                }
                literals.contains(&subject)
            }
        };
        if !selected {
            continue;
        }
        // A guard must evaluate exactly: an unresolvable guard could
        // select a different arm, so the whole match refuses rather
        // than guess (the all-or-nothing Required shape).
        if let Some(guard) = &arm.guard {
            match evaluate_guard(guard, inputs) {
                Some(true) => {}
                Some(false) => continue,
                None => return None,
            }
        }
        return match &arm.value {
            MatchValue::Literal(value) => Some(TypedValue::Str(value.clone())),
            MatchValue::Call(call) => super::helper_transfer::nested_call_value(call, inputs, eval),
        };
    }
    // No arm matched and no wildcard was reached.
    None
}

impl MatchShape {
    /// Parse the match shape from raw function-body text. Returns
    /// `None` for any other body — including let-chains, statements
    /// after the match, or a match that is not the tail expression —
    /// so the other helper evaluators keep their own authority.
    fn parse(body: &str) -> Option<MatchShape> {
        let masked = mask_rust_comments_and_strings(body);
        let raw_lines: Vec<&str> = body.lines().collect();
        let masked_lines: Vec<&str> = masked.lines().collect();
        if raw_lines.len() != masked_lines.len() {
            return None;
        }
        let mut scrutinee: Option<String> = None;
        let mut arms: Vec<MatchArm> = Vec::new();
        let mut saw_match_close = false;
        for (raw_line, masked_line) in raw_lines.iter().zip(masked_lines.iter()) {
            let raw = raw_line.trim();
            let trimmed = masked_line.trim();
            if trimmed.starts_with('#')
                || trimmed.starts_with("pub ")
                || trimmed.starts_with("pub(")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("async ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("unsafe ")
                || trimmed.is_empty()
            {
                continue;
            }
            if saw_match_close {
                // Only the function's closing brace may follow the
                // match; the match is the tail expression.
                if trimmed != "}" {
                    return None;
                }
                continue;
            }
            if scrutinee.is_none() {
                // `match <scrutinee> {` — the structure check uses the
                // masked line, the scrutinee text the raw line
                // (masking erases string literals, and a literal or
                // family scrutinee is supported through #3295).
                if !trimmed.starts_with("match ") || !trimmed.ends_with('{') {
                    return None;
                }
                let rest = raw.strip_prefix("match ")?.strip_suffix('{')?;
                scrutinee = Some(rest.trim().to_string());
                continue;
            }
            if trimmed == "}" {
                saw_match_close = true;
                continue;
            }
            if let Some(arm) = parse_arm(raw) {
                arms.push(arm);
                continue;
            }
            // Any other line inside the match is an unsupported edge.
            return None;
        }
        if !saw_match_close || arms.is_empty() {
            return None;
        }
        Some(MatchShape {
            scrutinee: scrutinee?,
            arms,
        })
    }
}

/// `"literal" => "value",` — the pattern is `_` or `|`-joined string or
/// char literals, optionally with an `if <expr>` guard; the value is a
/// plain string literal or one nested direct call (`label_of("a")`) to
/// a unique helper with literal or bound-parameter arguments. Guards,
/// bindings, ranges and paths refuse the arm (and with it the whole
/// helper) unless the guard evaluates exactly over the bound inputs: an
/// unresolved pattern could match anything, and any other value shape
/// is not an exact return the shared authorities can evaluate.
fn parse_arm(line: &str) -> Option<MatchArm> {
    let trimmed = line.trim().trim_end_matches(',').trim();
    let (pattern, value) = trimmed.split_once(" => ")?;
    let (pattern, guard) = split_guard(pattern)?;
    let pattern = if pattern == "_" {
        None
    } else {
        Some(literal_alternatives(pattern)?)
    };
    let value = if let Some(literal) = literal_string(value) {
        MatchValue::Literal(literal)
    } else {
        direct_call(value)?;
        MatchValue::Call(value.to_string())
    };
    Some(MatchArm {
        pattern,
        guard,
        value,
    })
}

/// Split an arm's pattern section into the pattern and an optional
/// `if <expr>` guard, honoring quotes: ` if ` inside a string or char
/// literal is content, not a guard introducer. An unterminated literal
/// refuses the arm.
fn split_guard(pattern: &str) -> Option<(&str, Option<String>)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in pattern.char_indices() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                // An ` if ` at this space, outside any literal,
                // introduces the guard.
                ' ' if pattern[index..].starts_with(" if ") => {
                    let guard = pattern[index + 4..].trim();
                    if guard.is_empty() {
                        return None;
                    }
                    return Some((pattern[..index].trim(), Some(guard.to_string())));
                }
                ' ' => {}
                _ => {}
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    Some((pattern.trim(), None))
}

/// A guard comparison operator.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardCmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Evaluate a match guard over the exact bound inputs. Fail-closed: a
/// single comparison whose operands each evaluate exactly — through
/// the shared #3295 evaluator, or `str::len()` (bytes, as Rust defines
/// it) on an exact string — against a same-typed operand. Anything
/// else (boolean combinators, method calls beyond `len()`, ordering
/// comparisons on strings or chars, mixed types) refuses the whole
/// match rather than guessing.
fn evaluate_guard(guard: &str, inputs: &ExactInputs) -> Option<bool> {
    let guard = strip_outer_parens(guard);
    let (operator, lhs, rhs) = split_comparison(guard)?;
    let lhs = guard_operand(lhs, inputs)?;
    let rhs = guard_operand(rhs, inputs)?;
    compare_guard_operands(&lhs, operator, &rhs)
}

/// Strip balanced parentheses that wrap the whole expression, repeatedly
/// (Rust permits `if (kind.len() > 2)` and doubly wrapped forms). Quotes
/// are honored so a literal `(` stays content. Input whose outer parens
/// do not balance around the whole expression, or whose quotes never
/// close, is returned unchanged so the evaluators refuse it fail-closed.
fn strip_outer_parens(text: &str) -> &str {
    let mut text = text.trim();
    loop {
        if !text.starts_with('(') || !text.ends_with(')') {
            return text;
        }
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        let mut wraps_whole = false;
        for (index, character) in text.char_indices() {
            match quote {
                Some(open) => {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == open {
                        quote = None;
                    }
                }
                None => match character {
                    '"' | '\'' => quote = Some(character),
                    '(' => depth += 1,
                    ')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            wraps_whole = index == text.len() - 1;
                            break;
                        }
                    }
                    _ => {}
                },
            }
        }
        if !wraps_whole || quote.is_some() {
            return text;
        }
        text = text[1..text.len() - 1].trim();
    }
}

/// Split `<lhs> <op> <rhs>` outside quotes, two-character operators
/// first. Anything but exactly one comparison refuses.
fn split_comparison(guard: &str) -> Option<(GuardCmp, &str, &str)> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let characters: Vec<(usize, char)> = guard.char_indices().collect();
    let mut position = 0;
    while position < characters.len() {
        let (index, character) = characters[position];
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                '=' if guard[index..].starts_with("==") => {
                    return Some((GuardCmp::Eq, &guard[..index], &guard[index + 2..]));
                }
                '!' if guard[index..].starts_with("!=") => {
                    return Some((GuardCmp::Ne, &guard[..index], &guard[index + 2..]));
                }
                '<' => {
                    if guard[index..].starts_with("<=") {
                        return Some((GuardCmp::Le, &guard[..index], &guard[index + 2..]));
                    }
                    return Some((GuardCmp::Lt, &guard[..index], &guard[index + 1..]));
                }
                '>' => {
                    if guard[index..].starts_with(">=") {
                        return Some((GuardCmp::Ge, &guard[..index], &guard[index + 2..]));
                    }
                    return Some((GuardCmp::Gt, &guard[..index], &guard[index + 1..]));
                }
                _ => {}
            },
        }
        position += 1;
    }
    None
}

/// Evaluate one guard operand: the shared #3295 evaluator over strings,
/// chars, booleans and plain decimals, plus `str::len()` (Rust's byte
/// length) on an exact string. Every other shape refuses.
fn guard_operand(text: &str, inputs: &ExactInputs) -> Option<TypedValue> {
    let text = strip_outer_parens(text);
    if let Some(receiver) = text.strip_suffix(".len()") {
        return match evaluate_initializer(receiver.trim(), inputs) {
            EvalOutcome::Exact {
                value: TypedValue::Str(string),
                ..
            } => Some(TypedValue::Index(string.len())),
            _ => None,
        };
    }
    match evaluate_initializer(text, inputs) {
        EvalOutcome::Exact {
            value:
                value @ (TypedValue::Str(_)
                | TypedValue::Char(_)
                | TypedValue::Bool(_)
                | TypedValue::Index(_)),
            ..
        } => Some(value),
        _ => None,
    }
}

/// Compare two exactly-evaluated guard operands. Ordering comparisons
/// are exact only for indices; equality is exact for every scalar the
/// operands evaluate to. Mixed or wider types refuse.
fn compare_guard_operands(lhs: &TypedValue, operator: GuardCmp, rhs: &TypedValue) -> Option<bool> {
    use TypedValue::{Bool, Char, Index, Str};
    let equality_only = |equal: bool| -> Option<bool> {
        match operator {
            GuardCmp::Eq => Some(equal),
            GuardCmp::Ne => Some(!equal),
            _ => None,
        }
    };
    match (lhs, rhs) {
        (Index(a), Index(b)) => Some(match operator {
            GuardCmp::Eq => a == b,
            GuardCmp::Ne => a != b,
            GuardCmp::Lt => a < b,
            GuardCmp::Le => a <= b,
            GuardCmp::Gt => a > b,
            GuardCmp::Ge => a >= b,
        }),
        (Str(a), Str(b)) => equality_only(a == b),
        (Char(a), Char(b)) => equality_only(a == b),
        (Bool(a), Bool(b)) => equality_only(a == b),
        _ => None,
    }
}

/// A single direct-call shape: `name(args)` with an identifier callee
/// and a balanced argument list. Full resolution (uniqueness,
/// splittability, binding) happens in the shared nested-call
/// authority, which refuses anything unsupported.
fn direct_call(text: &str) -> Option<()> {
    let trimmed = text.trim();
    let name = trimmed.split('(').next()?;
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        || !trimmed.contains('(')
        || !trimmed.ends_with(')')
    {
        return None;
    }
    Some(())
}

/// One literal pattern or `|`-joined alternatives, each evaluated
/// through the shared #3295 literal parser so an escape decodes exactly
/// as it does in the caller's argument. A guard, a binding, a range or
/// a path does not evaluate to a string or char and refuses the arm.
fn literal_alternatives(pattern: &str) -> Option<Vec<TypedValue>> {
    let mut literals = Vec::new();
    for alternative in split_alternatives(pattern)? {
        // The shared parser only strips the outer quotes, so `"a"..="z"`
        // must be rejected here as a range, not read as one string.
        if !is_single_literal(alternative) {
            return None;
        }
        match evaluate_initializer(alternative, &ExactInputs::new()) {
            EvalOutcome::Exact {
                value: value @ (TypedValue::Str(_) | TypedValue::Char(_)),
                ..
            } => literals.push(value),
            _ => return None,
        }
    }
    Some(literals)
}

/// True when `text` is exactly one quoted literal: the first unescaped
/// closing quote is the last character.
pub(super) fn is_single_literal(text: &str) -> bool {
    let mut characters = text.char_indices();
    let Some((_, open)) = characters.next() else {
        return false;
    };
    if open != '"' && open != '\'' {
        return false;
    }
    let mut escaped = false;
    for (index, character) in characters {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == open {
            return index + character.len_utf8() == text.len();
        }
    }
    false
}

/// Split a pattern on `|` outside string and char literals. An
/// unterminated literal refuses the pattern.
fn split_alternatives(pattern: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in pattern.char_indices() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                '|' => {
                    parts.push(pattern[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    parts.push(pattern[start..].trim());
    Some(parts)
}

/// A plain string literal: non-empty, no quote, backslash, apostrophe,
/// or newline inside — the same conservative literal rule the scanner
/// states use.
fn literal_string(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    if !inner.is_empty() && !inner.contains(['"', '\\', '\'', '\n']) {
        Some(inner.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::facts::{FunctionSummary, RustIndex};
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    fn match_fn(name: &str, body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 10,
            body: body.into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn inputs(pairs: &[(&str, &str)]) -> ExactInputs {
        let mut inputs = ExactInputs::new();
        for (parameter, literal) in pairs {
            inputs.insert(parameter.to_string(), literal.to_string());
        }
        inputs
    }

    fn root_eval(index: &RustIndex) -> HelperEval<'_> {
        HelperEval::root(index, true)
    }

    fn returned(helper: &FunctionSummary, kind: &str) -> Result<String, String> {
        let index = RustIndex::default();
        let eval = root_eval(&index);
        let bound = inputs(&[("kind", kind)]);
        match match_return_value(helper, &bound, &eval) {
            Some(value) => match value {
                TypedValue::Str(text) => Ok(text),
                other => Err(format!("expected a string value, got {}", other.render())),
            },
            None => Err("match must resolve over an exact input".into()),
        }
    }

    const BODY: &str = "pub fn label(kind: &str) -> &'static str {
    match kind {
        \"word\" => \"alpha\",
        \"text\" => \"beta\",
        _ => \"other\",
    }
}";

    #[test]
    fn match_resolves_each_supported_arm() -> Result<(), String> {
        let helper = match_fn("label", BODY);
        assert_eq!(returned(&helper, "\"word\"")?, "alpha");
        assert_eq!(returned(&helper, "\"text\"")?, "beta");
        // A value no literal arm names reaches the wildcard in order.
        assert_eq!(returned(&helper, "\"other\"")?, "other");
        Ok(())
    }

    #[test]
    fn wildcard_and_duplicate_arms_respect_source_order() -> Result<(), String> {
        // A wildcard placed first wins everything, exactly like the
        // Rust match; a duplicate literal keeps the first arm.
        let reordered = BODY.replace(
            "        \"word\" => \"alpha\",\n        \"text\" => \"beta\",\n        _ => \"other\",",
            "        _ => \"first\",\n        \"word\" => \"alpha\",\n        \"word\" => \"second\",",
        );
        assert_ne!(reordered, BODY, "the reordered-arm substitution must apply");
        let helper = match_fn("label", &reordered);
        assert_eq!(returned(&helper, "\"word\"")?, "first");
        assert_eq!(returned(&helper, "\"text\"")?, "first");
        Ok(())
    }

    #[test]
    fn no_matching_arm_without_wildcard_stops() -> Result<(), String> {
        let no_wildcard = BODY.replace("        _ => \"other\",\n", "");
        assert_ne!(
            no_wildcard, BODY,
            "the wildcard-removal substitution must apply"
        );
        let helper = match_fn("label", &no_wildcard);
        let index = RustIndex::default();
        let eval = root_eval(&index);
        let bound = inputs(&[("kind", "\"other\"")]);
        assert!(
            match_return_value(&helper, &bound, &eval).is_none(),
            "a scrutinee no literal names and no wildcard catches must stop"
        );
        Ok(())
    }

    #[test]
    fn guard_binding_range_and_computed_shapes_fail_closed() -> Result<(), String> {
        let cases: [(&str, &str); 9] = [
            (
                "guard with an unresolvable operand",
                "        \"word\" if kind.len() > other => \"alpha\",",
            ),
            (
                "guard with an unsupported method call",
                "        \"word\" if kind.starts_with(\"w\") => \"alpha\",",
            ),
            ("bare binding pattern", "        word => \"alpha\","),
            (
                "binding alternative",
                "        \"text\" | word => \"alpha\",",
            ),
            ("range pattern", "        \"a\"..=\"z\" => \"alpha\","),
            (
                "computed argument in a call value",
                "        \"word\" => pick(kind.trim()),",
            ),
            (
                "char literal against a string scrutinee",
                "        'w' => \"alpha\",",
            ),
            (
                "mixed-type alternatives",
                "        'w' | \"word\" => \"alpha\",",
            ),
            ("char range pattern", "        'a'..='z' => \"alpha\","),
        ];
        let index = RustIndex::default();
        let eval = root_eval(&index);
        for (name, arm) in cases {
            let body = BODY.replace("        \"word\" => \"alpha\",", arm);
            assert_ne!(body, BODY, "the {name} substitution must apply");
            let helper = match_fn("label", &body);
            assert!(
                match_return_value(&helper, &inputs(&[("kind", "\"word\"")]), &eval).is_none(),
                "a {name} arm must refuse the match"
            );
        }
        Ok(())
    }

    #[test]
    fn guarded_arms_evaluate_over_exact_inputs() -> Result<(), String> {
        // RIPR-SPEC-0164 (spec-b-0164-guard-arm): a guard the shared
        // authorities can evaluate selects the arm exactly, like the
        // Rust match; a guard that evaluates false falls through in
        // source order.
        let guarded = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" if kind.len() > 2 => \"alpha\",",
        );
        assert_ne!(guarded, BODY, "the guarded-arm substitution must apply");
        let helper = match_fn("label", &guarded);
        assert_eq!(returned(&helper, "\"word\"")?, "alpha");

        // The same guard shape against a scrutinee that fails it falls
        // through to the later literal arm instead.
        let len_guarded = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" if kind.len() > 5 => \"alpha\",\n        \"word\" => \"second\",",
        );
        let helper_len = match_fn("label", &len_guarded);
        assert_eq!(returned(&helper_len, "\"word\"")?, "second");

        // A guard applies to a whole alternative.
        let alt_guarded = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" | \"text\" if kind.len() > 2 => \"alpha\",",
        );
        let helper_alt = match_fn("label", &alt_guarded);
        assert_eq!(returned(&helper_alt, "\"word\"")?, "alpha");
        assert_eq!(returned(&helper_alt, "\"text\"")?, "alpha");

        // A wildcard guard decides between two wildcard arms: with no
        // literal arms left, the guarded wildcard takes the scrutinee
        // that passes its guard and the plain wildcard takes the rest.
        let wildcard_guarded = BODY.replace(
            "        \"word\" => \"alpha\",\n        \"text\" => \"beta\",\n        _ => \"other\",",
            "        _ if kind.len() > 2 => \"other\",\n        _ => \"short\",",
        );
        assert_ne!(
            wildcard_guarded, BODY,
            "the wildcard-guard substitution must apply"
        );
        let helper_wild = match_fn("label", &wildcard_guarded);
        assert_eq!(returned(&helper_wild, "\"word\"")?, "other");
        assert_eq!(returned(&helper_wild, "\"hi\"")?, "short");

        // String equality is exact too.
        let eq_guarded = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" if kind == \"word\" => \"alpha\",",
        );
        let helper_eq = match_fn("label", &eq_guarded);
        assert_eq!(returned(&helper_eq, "\"word\"")?, "alpha");
        Ok(())
    }

    #[test]
    fn parenthesized_guards_evaluate_like_bare_ones() -> Result<(), String> {
        // Rust permits parentheses around the whole comparison, doubled
        // too; stripping balanced outer parens keeps the guard exactly
        // resolvable instead of downgrading the match to unknown.
        for guard in [
            "\"word\" if (kind.len() > 2) => \"alpha\",",
            "\"word\" if ((kind.len() > 2)) => \"alpha\",",
            "\"word\" if (kind.len()) > 2 => \"alpha\",",
        ] {
            let body = BODY.replace(
                "        \"word\" => \"alpha\",",
                &format!("        {guard}"),
            );
            assert_ne!(body, BODY, "the {guard} substitution must apply");
            let helper = match_fn("label", &body);
            assert_eq!(
                returned(&helper, "\"word\"")?,
                "alpha",
                "{guard} must select the arm"
            );
        }

        // An unbalanced paren is not rescued: the guard stays unevaluable
        // and the whole match refuses fail-closed.
        let unbalanced = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" if (kind.len() > 2 => \"alpha\",",
        );
        let index = RustIndex::default();
        let eval = root_eval(&index);
        let helper = match_fn("label", &unbalanced);
        assert!(
            match_return_value(&helper, &inputs(&[("kind", "\"word\"")]), &eval).is_none(),
            "an unbalanced guard paren must refuse the match"
        );

        // A literal paren is content inside quotes, never structure.
        let quoted = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"w(ord\" if kind == \"w(ord\" => \"alpha\",",
        );
        let helper_quoted = match_fn("label", &quoted);
        assert_eq!(returned(&helper_quoted, "\"w(ord\"")?, "alpha");
        Ok(())
    }

    #[test]
    fn non_literal_scrutinee_and_non_tail_bodies_fail_closed() -> Result<(), String> {
        let index = RustIndex::default();
        let eval = root_eval(&index);
        // String-literal arms against a char scrutinee do not compile;
        // refuse rather than guess a match.
        let char_scrutinee = BODY.replace("match kind", "match c");
        let helper = match_fn("label", &char_scrutinee);
        let bound = inputs(&[("c", "'a'")]);
        assert!(
            match_return_value(&helper, &bound, &eval).is_none(),
            "a scrutinee of the other literal type must refuse the match"
        );
        // An integer scrutinee is outside the string/char lattice.
        let integer = BODY.replace("match kind", "match n");
        let helper_int = match_fn("label", &integer);
        assert!(
            match_return_value(&helper_int, &inputs(&[("n", "7")]), &eval).is_none(),
            "an integer scrutinee must refuse the match"
        );
        // A statement after the closing brace is not the pinned shape.
        let with_tail = BODY.replace("    }\n}", "    }\n    kind\n}");
        assert_ne!(
            with_tail, BODY,
            "the tail-statement substitution must apply"
        );
        let helper2 = match_fn("label", &with_tail);
        assert!(
            match_return_value(&helper2, &inputs(&[("kind", "\"word\"")]), &eval).is_none(),
            "a body with a statement after the match must refuse"
        );
        // An unresolvable scrutinee never guesses.
        let unknown = BODY.replace("match kind", "match unknown_var");
        let helper3 = match_fn("label", &unknown);
        assert!(
            match_return_value(&helper3, &inputs(&[("kind", "\"word\"")]), &eval).is_none(),
            "an unbound scrutinee must refuse the match"
        );
        Ok(())
    }

    #[test]
    fn alternative_patterns_select_each_listed_literal() -> Result<(), String> {
        let body = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" | \"term\" => \"alpha\",",
        );
        assert_ne!(body, BODY, "the alternative substitution must apply");
        let helper = match_fn("label", &body);
        assert_eq!(returned(&helper, "\"word\"")?, "alpha");
        assert_eq!(returned(&helper, "\"term\"")?, "alpha");
        assert_eq!(returned(&helper, "\"text\"")?, "beta");
        // A scrutinee neither alternative names falls to the wildcard.
        assert_eq!(returned(&helper, "\"wor\"")?, "other");
        Ok(())
    }

    #[test]
    fn escaped_patterns_decode_like_the_callers_literal() -> Result<(), String> {
        let body = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"a\\tb\" | \"c|d\" => \"alpha\",",
        );
        assert_ne!(body, BODY, "the escaped-pattern substitution must apply");
        let helper = match_fn("label", &body);
        // The tab decodes on both sides; the `|` inside a literal is
        // part of the string, not an alternative separator.
        assert_eq!(returned(&helper, "\"a\\tb\"")?, "alpha");
        assert_eq!(returned(&helper, "\"c|d\"")?, "alpha");
        // The two-character text `\t` is not the decoded tab.
        assert_eq!(returned(&helper, "\"a\\\\tb\"")?, "other");
        assert_eq!(returned(&helper, "\"c\"")?, "other");
        Ok(())
    }

    #[test]
    fn char_scrutinee_selects_char_arms_in_source_order() -> Result<(), String> {
        let body = "pub fn tag(c: char) -> &'static str {
    match c {
        'a' | 'b' => \"low\",
        'c' => \"third\",
        _ => \"other\",
    }
}";
        let helper = match_fn("tag", body);
        let index = RustIndex::default();
        let eval = root_eval(&index);
        for (input, expected) in [
            ("'a'", "low"),
            ("'b'", "low"),
            ("'c'", "third"),
            ("'z'", "other"),
        ] {
            match match_return_value(&helper, &inputs(&[("c", input)]), &eval) {
                Some(TypedValue::Str(text)) => assert_eq!(text, expected, "input {input}"),
                other => return Err(format!("expected {expected} for {input}, got {other:?}")),
            }
        }
        Ok(())
    }

    #[test]
    fn nested_call_values_resolve_within_the_hop_bound() -> Result<(), String> {
        // One self-recursive helper: label_of("word") re-enters with
        // "text" — two distinct (helper, inputs) states on one path,
        // inside the hop bound. The real entry (helper_return_value)
        // owns the depth accounting, so the test goes through it.
        let body = BODY.replace(
            "        \"word\" => \"alpha\",",
            "        \"word\" => label_of(\"text\"),",
        );
        assert_ne!(body, BODY, "the nested-call substitution must apply");
        let helper = match_fn("label_of", &body);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![helper.clone()],
            ..Default::default()
        });
        let eval = root_eval(&index);
        let bound = inputs(&[("kind", "\"word\"")]);
        match super::super::helper_transfer::helper_return_value(&helper, &bound, &eval) {
            Some(TypedValue::Str(text)) => assert_eq!(text, "beta"),
            other => {
                return Err(format!(
                    "expected the nested call to resolve, got {other:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn repeated_states_and_beyond_bound_chains_refuse() -> Result<(), String> {
        // `_ => label_of(kind)` re-enters the same (helper, inputs)
        // state: a true cycle refuses.
        let cyclic = BODY.replace("        _ => \"other\",", "        _ => label_of(kind),");
        assert_ne!(cyclic, BODY, "the cycle substitution must apply");
        let helper = match_fn("label_of", &cyclic);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![helper.clone()],
            ..Default::default()
        });
        let eval = root_eval(&index);
        let bound = inputs(&[("kind", "\"zz\"")]);
        assert!(
            super::super::helper_transfer::helper_return_value(&helper, &bound, &eval).is_none(),
            "a repeated (helper, inputs) state must refuse"
        );

        // A self-chain: c -> b -> a is three evaluations (inside the
        // bound); d -> c -> b -> a is four (beyond it).
        let chain = "pub fn label_of(kind: &str) -> &'static str {
    match kind {
        \"a\" => \"alpha\",
        \"b\" => label_of(\"a\"),
        \"c\" => label_of(\"b\"),
        \"d\" => label_of(\"c\"),
    }
}";
        let chained = match_fn("label_of", chain);
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![chained.clone()],
            ..Default::default()
        });
        let eval = root_eval(&index);
        let inside = inputs(&[("kind", "\"c\"")]);
        match super::super::helper_transfer::helper_return_value(&chained, &inside, &eval) {
            Some(TypedValue::Str(text)) => assert_eq!(text, "alpha"),
            other => {
                return Err(format!(
                    "expected the in-bound chain to resolve, got {other:?}"
                ));
            }
        }
        let beyond = inputs(&[("kind", "\"d\"")]);
        assert!(
            super::super::helper_transfer::helper_return_value(&chained, &beyond, &eval).is_none(),
            "a chain beyond the hop bound must refuse"
        );
        Ok(())
    }

    #[test]
    fn literal_tail_callee_values_resolve_and_ambiguous_tails_refuse() -> Result<(), String> {
        let label = match_fn(
            "label_of",
            "pub fn label_of(kind: &str) -> &'static str {
    match kind {
        \"word\" => pick(kind),
        _ => \"other\",
    }
}",
        );
        let index_with = |pick_body: &str| {
            RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
                functions: vec![label.clone(), match_fn("pick", pick_body)],
                ..Default::default()
            })
        };
        let resolve = |index: &RustIndex| {
            let eval = root_eval(index);
            super::super::helper_transfer::helper_return_value(
                &label,
                &inputs(&[("kind", "\"word\"")]),
                &eval,
            )
        };
        let single = index_with("fn pick(_kind: &str) -> &'static str {\n    \"alpha\"\n}");
        match resolve(&single) {
            Some(TypedValue::Str(text)) => assert_eq!(text, "alpha"),
            other => {
                return Err(format!(
                    "expected the literal callee to resolve, got {other:?}"
                ));
            }
        }
        // Two literal lines are not one tail value.
        let two =
            index_with("fn pick(_kind: &str) -> &'static str {\n    \"alpha\"\n    \"beta\"\n}");
        assert!(resolve(&two).is_none(), "two literal lines must refuse");
        // A quote-bounded expression is not a whole literal.
        let concat = index_with("fn pick(_kind: &str) -> String {\n    \"a\" + \"b\"\n}");
        assert!(
            resolve(&concat).is_none(),
            "a quoted expression must refuse, got {:?}",
            resolve(&concat)
        );
        // A computed tail is not a whole literal.
        let computed = index_with("fn pick(kind: &str) -> String {\n    kind.to_uppercase()\n}");
        assert!(resolve(&computed).is_none(), "a computed tail must refuse");
        Ok(())
    }

    #[test]
    fn a_discarded_literal_statement_does_not_displace_a_binding_tail() {
        let label = match_fn(
            "label_of",
            "pub fn label_of(kind: &str) -> &'static str {\n    match kind {\n        \"word\" => pick(kind),\n        _ => \"other\",\n    }\n}",
        );
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![
                label.clone(),
                match_fn(
                    "pick",
                    "fn pick(_k: &str) -> &'static str {\n    \"unused\";\n    let value = \"alpha\";\n    value\n}",
                ),
            ],
            ..Default::default()
        });
        let eval = root_eval(&index);
        let value = super::super::helper_transfer::helper_return_value(
            &label,
            &inputs(&[("kind", "\"word\"")]),
            &eval,
        );
        assert!(value.is_some(), "the let-bound tail must still resolve");
    }

    #[test]
    fn a_literal_ending_in_r_or_a_raw_identifier_is_still_a_literal_tail() -> Result<(), String> {
        let label = match_fn(
            "label_of",
            "pub fn label_of(kind: &str) -> &'static str {\n    match kind {\n        \"word\" => pick(kind),\n        _ => \"other\",\n    }\n}",
        );
        // `r` ending a literal or an identifier is not a raw-string opener.
        for (body, expected) in [
            (
                "fn pick(_k: &str) -> &'static str {\n    \"other\"\n}",
                "other",
            ),
            (
                "fn pick(_k: &str) -> &'static str {\n    \"user\"\n}",
                "user",
            ),
            (
                "fn pick(_k: &str) -> &'static str {\n    \"color\"\n}",
                "color",
            ),
            ("fn pick(_k: &str) -> &'static str {\n    \"#\"\n}", "#"),
            (
                "fn pick(_k: &str) -> &'static str {\n    // a raw ident r#type\n    \"alpha\"\n}",
                "alpha",
            ),
        ] {
            let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
                functions: vec![label.clone(), match_fn("pick", body)],
                ..Default::default()
            });
            let eval = root_eval(&index);
            match super::super::helper_transfer::helper_return_value(
                &label,
                &inputs(&[("kind", "\"word\"")]),
                &eval,
            ) {
                Some(TypedValue::Str(text)) => assert_eq!(text, expected, "{body}"),
                other => return Err(format!("{body}: expected Str({expected:?}), got {other:?}")),
            }
        }
        Ok(())
    }

    #[test]
    fn raw_string_interiors_async_bodies_and_early_returns_are_not_literal_tails() {
        let label = match_fn(
            "label_of",
            "pub fn label_of(kind: &str) -> &'static str {\n    match kind {\n        \"word\" => pick(kind),\n        _ => \"other\",\n    }\n}",
        );
        let bodies = [
            "fn pick(_k: &str) -> &'static str {\n    r#\"\n\"x\"\n\"#\n}",
            "fn pick(_k: &str) -> &'static [u8] {\n    br#\"\n\"x\"\n\"#\n}",
            "fn pick(_k: &str) -> &'static str {\n    r##\"\n\"x\"\n\"##\n}",
            "async fn pick(_k: &str) -> &'static str {\n    \"x\"\n}",
            "fn pick(k: &str) -> &'static str {\n    \"x\";\n    return k;\n}",
            // A quoted line inside a block comment is comment text, not a tail.
            "fn pick(_k: &str) -> &'static str {\n    /*\n    \"x\"\n    */\n}",
            "fn pick(_k: &str) -> &'static str {\n    /*\n    \"alpha\"\n    */\n    const { \"beta\" }\n}",
        ];
        for body in bodies {
            let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
                functions: vec![label.clone(), match_fn("pick", body)],
                ..Default::default()
            });
            let eval = root_eval(&index);
            assert!(
                super::super::helper_transfer::helper_return_value(
                    &label,
                    &inputs(&[("kind", "\"word\"")]),
                    &eval,
                )
                .is_none(),
                "{body}"
            );
        }
    }
}

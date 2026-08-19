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
//! resolves exactly (#3295 facts) and every arm's pattern is a plain
//! string literal or `_` and every arm's value is a plain string
//! literal, the evaluator returns the first arm whose literal equals
//! the scrutinee — source order is authoritative, exactly like the
//! Rust match. Anything else — a guard, an alternative pattern, a
//! computed value, a bare-identifier pattern, a non-string scrutinee,
//! any statement after the closing brace — refuses the whole shape
//! and returns `None`; the caller's existing limitation wording stays
//! the disclosure.

use super::value_transfer::{EvalOutcome, ExactInputs, TypedValue, evaluate_initializer};
use crate::analysis::language::mask_rust_comments_and_strings;

/// The parsed match body: the exact scrutinee expression and the
/// literal arms in source order.
struct MatchShape {
    scrutinee: String,
    arms: Vec<MatchArm>,
}

struct MatchArm {
    /// `None` is the `_` wildcard arm; `Some(literal)` is a plain
    /// string-literal pattern. A wildcard still respects source order.
    pattern: Option<String>,
    value: String,
}

/// Evaluate a match helper's return over its exact inputs, when the
/// helper body is one literal `match` tail expression.
pub(crate) fn match_return_value(
    helper: &crate::analysis::facts::FunctionSummary,
    inputs: &ExactInputs,
) -> Option<TypedValue> {
    let shape = MatchShape::parse(&helper.body)?;
    let subject = match evaluate_initializer(&shape.scrutinee, inputs) {
        EvalOutcome::Exact {
            value: TypedValue::Str(text),
            ..
        } => text,
        _ => return None,
    };
    for arm in &shape.arms {
        match &arm.pattern {
            Some(literal) if literal != &subject => continue,
            _ => return Some(TypedValue::Str(arm.value.clone())),
        }
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

/// `"literal" => "value",` — the pattern is a plain string literal or
/// `_`, the value a plain string literal. Guards, alternatives,
/// bindings, paths, and escapes all refuse the arm (and with it the
/// whole helper): an unresolved pattern could match anything, and a
/// non-literal value is not an exact return.
fn parse_arm(line: &str) -> Option<MatchArm> {
    let trimmed = line.trim().trim_end_matches(',').trim();
    let (pattern, value) = trimmed.split_once(" => ")?;
    let pattern = if pattern == "_" {
        None
    } else {
        Some(literal_string(pattern)?)
    };
    let value = literal_string(value)?;
    Some(MatchArm { pattern, value })
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
    use crate::analysis::facts::FunctionSummary;
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    fn match_fn(name: &str, body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 10,
            body: body.to_string(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            is_test: false,
            attrs: Vec::new(),
        }
    }

    fn inputs(pairs: &[(&str, &str)]) -> ExactInputs {
        let mut inputs = ExactInputs::new();
        for (parameter, literal) in pairs {
            inputs.insert(parameter.to_string(), literal.to_string());
        }
        inputs
    }

    fn returned(helper: &FunctionSummary, kind: &str) -> Result<String, String> {
        let bound = inputs(&[("kind", kind)]);
        match match_return_value(helper, &bound) {
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
        let bound = inputs(&[("kind", "\"other\"")]);
        assert!(
            match_return_value(&helper, &bound).is_none(),
            "a scrutinee no literal names and no wildcard catches must stop"
        );
        Ok(())
    }

    #[test]
    fn guard_alternative_binding_and_computed_shapes_fail_closed() -> Result<(), String> {
        let cases: [(&str, &str); 5] = [
            (
                "guard arm",
                "        \"word\" if kind.len() > 2 => \"alpha\",",
            ),
            (
                "alternative pattern",
                "        \"word\" | \"text\" => \"alpha\",",
            ),
            ("bare binding pattern", "        word => \"alpha\","),
            ("computed arm value", "        \"word\" => pick(kind),"),
            (
                "escaped literal pattern",
                "        \"a\\\"b\" => \"alpha\",",
            ),
        ];
        for (name, arm) in cases {
            let body = BODY.replace("        \"word\" => \"alpha\",", arm);
            assert_ne!(body, BODY, "the {name} substitution must apply");
            let helper = match_fn("label", &body);
            assert!(
                match_return_value(&helper, &inputs(&[("kind", "\"word\"")])).is_none(),
                "a {name} arm must refuse the match"
            );
        }
        Ok(())
    }

    #[test]
    fn non_string_scrutinee_and_non_tail_bodies_fail_closed() -> Result<(), String> {
        // A char scrutinee is a named limitation, not a guessed match.
        let char_scrutinee = BODY.replace("match kind", "match c");
        let helper = match_fn("label", &char_scrutinee);
        let bound = inputs(&[("c", "'a'")]);
        assert!(
            match_return_value(&helper, &bound).is_none(),
            "a non-string scrutinee must refuse the match"
        );
        // A statement after the closing brace is not the pinned shape.
        let with_tail = BODY.replace("    }\n}", "    }\n    kind\n}");
        assert_ne!(
            with_tail, BODY,
            "the tail-statement substitution must apply"
        );
        let helper2 = match_fn("label", &with_tail);
        assert!(
            match_return_value(&helper2, &inputs(&[("kind", "\"word\"")])).is_none(),
            "a body with a statement after the match must refuse"
        );
        // An unresolvable scrutinee never guesses.
        let unknown = BODY.replace("match kind", "match unknown_var");
        let helper3 = match_fn("label", &unknown);
        assert!(
            match_return_value(&helper3, &inputs(&[("kind", "\"word\"")])).is_none(),
            "an unbound scrutinee must refuse the match"
        );
        Ok(())
    }
}

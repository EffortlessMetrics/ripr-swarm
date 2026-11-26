//! #16296: an equality assertion that compares a test-local boolean with a
//! boolean literal asserts the local's defining expression.
//!
//! `let ok = value > 0; assert_eq!(ok, true);` is the same check as
//! `assert_eq!(value > 0, true)`, which RIPR-SPEC-0231 rule 1 reads as a
//! relational check. Read alone, the assertion line has the shape of an exact
//! pin (`assert_eq!(name, literal)`), so a relational, inequality or pattern
//! check stored in a bool would otherwise count as an exact pin.
//!
//! The reading only lowers. It applies when the function binds the name
//! exactly once, with a plain immutable `let` that has an initializer. The
//! assertion then takes the weaker of its own reading and the reading of the
//! same assertion with the initializer written in place. Any other binding
//! shape keeps the line's own reading.

use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasName};

use super::arguments::{AssertionComparison, assertion_comparison};
use super::classify::{OracleClassification, classify_assertion_with};
use super::pattern_admission::LocalDecls;
use super::patterns::{boolean_literal_value, unwrap_parentheses};
use crate::domain::OracleStrength;

/// `classification` for `assertion`, lowered to the reading of the local's
/// defining expression when `assertion` compares a local boolean that
/// `function` binds once with a boolean literal.
pub(crate) fn with_bool_local_inlined(
    classification: OracleClassification,
    assertion: &str,
    function: &SyntaxNode,
    decls: &LocalDecls,
) -> OracleClassification {
    if !matches!(
        classification.strength,
        OracleStrength::Strong | OracleStrength::Medium
    ) {
        return classification;
    }
    let Some(inlined) = inlined_assertion(assertion, |name| single_let_initializer(function, name))
    else {
        return classification;
    };
    let reading = classify_assertion_with(&inlined, decls);
    if lowered_strength(&reading.strength) > lowered_strength(&classification.strength) {
        reading
    } else {
        classification
    }
}

/// The equality assertion with the compared local replaced by its
/// initializer, when one operand is a bare local `initializer` resolves and
/// the other is a boolean literal.
fn inlined_assertion(
    assertion: &str,
    initializer: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let AssertionComparison::Operands {
        not_equal,
        left,
        right,
    } = assertion_comparison(assertion)?
    else {
        return None;
    };
    let local = |operand: &str| {
        let operand = unwrap_parentheses(operand.trim());
        is_local_name(operand).then(|| operand.to_string())
    };
    let (left, right) = if boolean_literal_value(&right).is_some() {
        (initializer(&local(&left)?)?, right)
    } else if boolean_literal_value(&left).is_some() {
        (left, initializer(&local(&right)?)?)
    } else {
        return None;
    };
    let name = if not_equal {
        "assert_ne!"
    } else {
        "assert_eq!"
    };
    Some(format!("{name}({left}, {right});"))
}

/// A lower-case identifier that is not a keyword or boolean literal.
fn is_local_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_lowercase())
        && text
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !matches!(text, "_" | "true" | "false" | "self")
}

/// The initializer of the one binding of `name` in `function`, when that
/// binding is a plain immutable `let name = ..;` (an optional type is
/// allowed). Two bindings, a pattern, closure or match binding of the name,
/// `mut`, `ref` or a missing initializer give `None`.
fn single_let_initializer(function: &SyntaxNode, name: &str) -> Option<String> {
    let mut bindings = function
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| pattern.name().is_some_and(|found| found.text() == name));
    let binding = bindings.next()?;
    if bindings.next().is_some()
        || binding.mut_token().is_some()
        || binding.ref_token().is_some()
        || binding.pat().is_some()
    {
        return None;
    }
    let statement = binding.syntax().parent().and_then(ast::LetStmt::cast)?;
    if statement.let_else().is_some() {
        return None;
    }
    let initializer = statement.initializer()?.syntax().text().to_string();
    let initializer = initializer.trim();
    (!initializer.is_empty()).then(|| initializer.to_string())
}

/// Higher is weaker. Only a real comparison reading lowers: an unknown or
/// absent reading of the written-in-place assertion changes nothing.
fn lowered_strength(strength: &OracleStrength) -> u8 {
    match strength {
        OracleStrength::Strong | OracleStrength::Unknown | OracleStrength::None => 0,
        OracleStrength::Medium => 1,
        OracleStrength::Weak => 2,
        OracleStrength::Smoke => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OracleKind;

    fn read(body: &str, assertion: &str) -> Result<OracleClassification, String> {
        let source = format!("fn t() {{\n{body}\n{assertion}\n}}\n");
        let parse = ra_ap_syntax::SourceFile::parse(&source, ra_ap_syntax::Edition::Edition2021);
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)
            .ok_or("no function")?;
        let decls = LocalDecls::from_source(&source);
        Ok(with_bool_local_inlined(
            classify_assertion_with(assertion, &decls),
            assertion,
            function.syntax(),
            &decls,
        ))
    }

    #[test]
    fn a_compared_bool_local_reads_as_its_initializer() -> Result<(), String> {
        for (body, assertion) in [
            ("let ok = value(100) > 0;", "assert_eq!(ok, true);"),
            ("let ok = value(100) != 95;", "assert_eq!(true, ok);"),
            ("let ok: bool = value(100) >= 1;", "assert_eq!((ok), true);"),
            (
                "let ok = matches!(value(100), 90 | 100);",
                "assert_eq!(ok, true);",
            ),
            (
                "let ok = matches!(value(100), _);",
                "debug_assert_eq!(ok, true);",
            ),
        ] {
            let reading = read(body, assertion)?;
            if reading.strength == OracleStrength::Strong {
                return Err(format!("{body} {assertion}: still strong: {reading:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn a_pinning_initializer_or_an_unsettled_binding_keeps_the_line_reading() -> Result<(), String>
    {
        let exact = OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
        };
        for (body, assertion) in [
            // The initializer pins one value.
            ("let ok = value(100) == 90;", "assert_eq!(ok, true);"),
            (
                "let ok = matches!(value(100), 90);",
                "assert_eq!(ok, true);",
            ),
            // A non-boolean local is an ordinary exact pin.
            ("let v = value(100);", "assert_eq!(v, 90);"),
            // Two bindings, a mutable binding or a pattern binding are not
            // settled, so the line keeps its own reading.
            (
                "let ok = value(100) > 0; let ok = value(100) == 90;",
                "assert_eq!(ok, true);",
            ),
            ("let mut ok = value(100) > 0;", "assert_eq!(ok, true);"),
            (
                "let (ok, _) = (value(100) > 0, 1);",
                "assert_eq!(ok, true);",
            ),
            ("let ok;", "assert_eq!(ok, true);"),
        ] {
            let reading = read(body, assertion)?;
            if reading != exact {
                return Err(format!(
                    "{body} {assertion}: expected exact, got {reading:?}"
                ));
            }
        }
        Ok(())
    }
}

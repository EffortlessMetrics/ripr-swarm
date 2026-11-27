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

use std::cell::OnceCell;
use std::collections::BTreeMap;

use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasName};

use super::arguments::{AssertionComparison, assertion_comparison};
use super::classify::{OracleClassification, classify_assertion_with};
use super::pattern_admission::LocalDecls;
use super::patterns::{boolean_literal_value, transparent_operand};
use crate::domain::OracleStrength;

/// One test function's local bindings, read once on the first assertion that
/// needs them, so a test with many compared locals walks its tree once.
pub(crate) struct BoolLocals<'a> {
    function: &'a SyntaxNode,
    initializers: OnceCell<BTreeMap<String, Option<String>>>,
}

impl<'a> BoolLocals<'a> {
    pub(crate) fn new(function: &'a SyntaxNode) -> Self {
        Self {
            function,
            initializers: OnceCell::new(),
        }
    }

    /// The initializer of the one binding of `name`, when that binding is a
    /// plain immutable `let name = ..;` (an optional type is allowed). Two
    /// bindings, a pattern, closure or match binding of the name, `mut`,
    /// `ref`, `@`, `let .. else` or a missing initializer give `None`.
    fn initializer(&self, name: &str) -> Option<String> {
        self.initializers
            .get_or_init(|| let_initializers(self.function))
            .get(name)
            .cloned()
            .flatten()
    }
}

/// `classification` for `assertion`, lowered to the reading of the local's
/// defining expression when `assertion` compares a local boolean that the
/// function binds once with a boolean literal.
pub(crate) fn with_bool_local_inlined(
    classification: OracleClassification,
    assertion: &str,
    locals: &BoolLocals<'_>,
    decls: &LocalDecls,
) -> OracleClassification {
    if !matches!(
        classification.strength,
        OracleStrength::Strong | OracleStrength::Medium
    ) {
        return classification;
    }
    let Some(inlined) = inlined_assertion(assertion, |name| locals.initializer(name)) else {
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
/// the other is a boolean literal. Comments, grouping parentheses and a block
/// around either operand are transparent (`assert_eq!(ok, /* want */ true)`).
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
    let left = transparent_operand(&left);
    let right = transparent_operand(&right);
    let local = |operand: &str| local_key(operand).map(str::to_string);
    let (left, right) = if boolean_literal_value(right).is_some() {
        (initializer(&local(left)?)?, right.to_string())
    } else if boolean_literal_value(left).is_some() {
        (left.to_string(), initializer(&local(right)?)?)
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

/// The binding key of a bare identifier operand, with a raw `r#` prefix
/// removed so `r#ok` and `ok` name the same local. The text only has to be
/// one word: no ASCII punctuation (other than `_`), space or leading digit, so
/// a path, call or field access gives `None`. Non-ASCII characters are
/// accepted as they are (Unicode letters and combining marks such as
/// `x\u{301}`), because whether the word is a local at all is decided by the
/// function's binding table, which holds only names the parser bound.
fn local_key(text: &str) -> Option<&str> {
    let name = text.strip_prefix("r#").unwrap_or(text);
    let word = |ch: char| ch == '_' || ch.is_ascii_alphanumeric() || !ch.is_ascii();
    (name
        .chars()
        .next()
        .is_some_and(|first| !first.is_ascii_digit())
        && name.chars().all(word)
        && !matches!(name, "_" | "true" | "false" | "self"))
    .then_some(name)
}

/// Every name an identifier pattern binds in `function`, with its
/// initializer when it is bound exactly once by a plain immutable `let`.
fn let_initializers(function: &SyntaxNode) -> BTreeMap<String, Option<String>> {
    let mut bindings = BTreeMap::new();
    for pattern in function.descendants().filter_map(ast::IdentPat::cast) {
        let Some(name) = pattern
            .name()
            .and_then(|name| local_key(name.text()).map(str::to_string))
        else {
            continue;
        };
        let initializer = plain_let_initializer(&pattern);
        bindings
            .entry(name)
            .and_modify(|seen: &mut Option<String>| *seen = None)
            .or_insert(initializer);
    }
    bindings
}

fn plain_let_initializer(pattern: &ast::IdentPat) -> Option<String> {
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let statement = pattern.syntax().parent().and_then(ast::LetStmt::cast)?;
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
            &BoolLocals::new(function.syntax()),
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
            // Comments around either operand are transparent.
            (
                "let ok = value(100) > 0;",
                "assert_eq!(ok, /* expected */ true);",
            ),
            (
                "let ok = value(100) > 0;",
                "assert_eq!(/* got */ ok, true);",
            ),
            // Any identifier spelling resolves through the binding table, and
            // a raw identifier names the same local as its plain spelling.
            (
                "let résultat = value(100) > 0;",
                "assert_eq!(résultat, true);",
            ),
            (
                "let Ok_state = value(100) > 0;",
                "assert_eq!(Ok_state, true);",
            ),
            ("let r#ok = value(100) > 0;", "assert_eq!(ok, true);"),
            ("let ok = value(100) > 0;", "assert_eq!(r#ok, true);"),
        ] {
            let reading = read(body, assertion)?;
            if reading.strength == OracleStrength::Strong {
                return Err(format!("{body} {assertion}: still strong: {reading:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn a_local_named_with_a_combining_mark_reads_as_its_relational_initializer()
    -> Result<(), String> {
        // `x\u{301}` is a Rust identifier whose mark is not alphanumeric.
        let reading = read(
            "let x\u{301} = value(100) > 0;",
            "assert_eq!(x\u{301}, true);",
        )?;
        let relational = OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        };
        if reading != relational {
            return Err(format!("expected a weak relational check, got {reading:?}"));
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
            // `r#ok` rebinds `ok`, so the name is bound twice.
            (
                "let ok = value(100) > 0; let r#ok = value(100) == 90;",
                "assert_eq!(ok, true);",
            ),
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

//! Bounded discrimination for a changed `_` arm of a string-literal match
//! (#6616, RIPR-SPEC-0002 `assert_matches!`-style exact checks).
//!
//! A wildcard arm has no pattern token, so the lexical confirmation in
//! `reveal` can never tie an assertion to it: `assert_matches!(parse_unit(
//! "q"), Err(UnitError::Unknown))` reads `observation_unverified` even
//! though `"q"` misses every sibling literal and so selects the changed arm.
//!
//! This producer refines only that missing discriminator, from parsed source,
//! when every gate below holds; anything else leaves the evidence unchanged:
//!
//! - the owner is a unique, non-async free function with one immutable
//!   `&str` parameter, no statements, and a tail `match <parameter>`;
//! - every arm is unguarded and unattributed, the changed arm is the last
//!   one and its pattern is `_`, and every other pattern is a plain string
//!   literal or an alternation of them;
//! - the related test calls the owner directly, is a plain `#[test]` with no
//!   parameters, and holds the assertion as a top-level statement that no
//!   earlier `return` can skip;
//! - the assertion is a bare `assert_eq!` (either operand) or
//!   `assert_matches!` (value operand) whose owner call has exactly one plain
//!   string argument that equals no sibling pattern, the indexed oracle on
//!   that line is an exact value or exact error-variant oracle, and an
//!   `assert_matches!` pattern names only constructors, paths and literals
//!   (no wildcard, rest, or binding);
//! - no workspace `macro_rules!` redefines `assert_eq`, and every workspace
//!   `macro_rules! assert_matches` panics on a mismatch.
//!
//! A sibling-literal input, a broad pattern, a deferred or skipped assertion,
//! or a non-literal input keeps the existing `observation_unverified` gap.

use crate::analysis::classify::ProbeContext;
use crate::analysis::facts::TestSummary;
use crate::analysis::rust_index::find_file_facts;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{
    Confidence, OracleKind, Probe, ProbeFamily, RelationReason, StageEvidence, StageState,
};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};

pub(super) fn discrimination(
    context: &ProbeContext<'_>,
    observe: &StageEvidence,
    current: &StageEvidence,
) -> Option<StageEvidence> {
    if context.probe.family != ProbeFamily::MatchArm
        || observe.state != StageState::Yes
        || current.state != StageState::Weak
        || !current.summary.contains("observation_unverified")
    {
        return None;
    }
    let owner = context.owner_fn?;
    if context.probe.owner.as_ref() != Some(&owner.id)
        || context
            .index
            .functions()
            .iter()
            .filter(|function| function.name == owner.name)
            .count()
            != 1
    {
        return None;
    }
    let facts = find_file_facts(context.index, &owner.file)?;
    if facts.used_lexical_fallback {
        return None;
    }
    let siblings = wildcard_siblings(&facts.source, &owner.name, context.probe)?;
    if !assertion_macros_are_trusted(context) {
        return None;
    }
    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::DirectOwnerCall {
            continue;
        }
        let Some(test_facts) = find_file_facts(context.index, &test.file) else {
            continue;
        };
        if test_facts.used_lexical_fallback
            || context.test_file_imports_foreign_callee_name(
                &test.file,
                &test_facts.source,
                &owner.name,
            )
        {
            continue;
        }
        if selects_wildcard(&test_facts.source, test, &owner.name, &siblings) {
            return Some(StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                "Exact string input misses every sibling literal pattern and selects this wildcard arm; an exact oracle in the same assertion observes the owner's result",
            ));
        }
    }
    None
}

/// The sibling literal patterns when the probe is the trailing `_` arm of
/// the owner's whole-body `match <parameter>` over string literals.
fn wildcard_siblings(source: &str, owner: &str, probe: &Probe) -> Option<Vec<String>> {
    let root = parse_clean_source_file(source)?.tree();
    let mut candidates = root
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|name| name.text() == owner))
        .filter(|function| {
            line_of(source, function.syntax()).is_some_and(|start| start <= probe.location.line)
                && end_line_of(source, function.syntax())
                    .is_some_and(|end| probe.location.line <= end)
        });
    let function = candidates.next()?;
    if candidates.next().is_some()
        || function.async_token().is_some()
        || !matches!(
            function.syntax().parent().map(|parent| parent.kind()),
            Some(SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST)
        )
    {
        return None;
    }
    let parameters = function.param_list()?;
    if parameters.self_param().is_some() {
        return None;
    }
    let parameters = parameters.params().collect::<Vec<_>>();
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    let ty = parameter.ty()?.syntax().text().to_string();
    if ty.split_whitespace().collect::<String>() != "&str" {
        return None;
    }
    let binding = ast::IdentPat::cast(parameter.pat()?.syntax().clone())?;
    let name = binding.name()?.text().to_string();
    if binding.syntax().text().to_string().trim() != name {
        return None;
    }
    let statements = function.body()?.stmt_list()?;
    if statements.statements().next().is_some() {
        return None;
    }
    let expression = ast::MatchExpr::cast(statements.tail_expr()?.syntax().clone())?;
    if bare_name(&expression.expr()?)? != name {
        return None;
    }
    let arms = expression.match_arm_list()?.arms().collect::<Vec<_>>();
    let (last, earlier) = arms.split_last()?;
    if !matches!(last.pat()?, ast::Pat::WildcardPat(_))
        || line_of(source, last.syntax())? != probe.location.line
        || !arm_source_matches(last, &probe.expression)?
    {
        return None;
    }
    let mut siblings = Vec::new();
    for arm in arms.iter() {
        if arm.guard().is_some() || arm.attrs().next().is_some() {
            return None;
        }
    }
    for arm in earlier {
        match arm.pat()? {
            ast::Pat::LiteralPat(literal) => siblings.push(plain_string(literal.syntax())?),
            ast::Pat::OrPat(alternatives) => {
                for alternative in alternatives.pats() {
                    let ast::Pat::LiteralPat(literal) = alternative else {
                        return None;
                    };
                    siblings.push(plain_string(literal.syntax())?);
                }
            }
            _ => return None,
        }
    }
    Some(siblings)
}

/// The changed arm's claimed text is the pattern through `=>` or the whole
/// current arm; any other spelling is a different arm.
fn arm_source_matches(arm: &ast::MatchArm, claimed: &str) -> Option<bool> {
    let full = arm.syntax().text().to_string();
    let start = u32::from(arm.syntax().text_range().start());
    let end = u32::from(arm.fat_arrow_token()?.text_range().end());
    let pattern = full.get(..end.checked_sub(start)? as usize)?;
    let trimmed = |text: &str| text.trim().trim_end_matches(',').trim_end().to_string();
    Some(trimmed(&full) == trimmed(claimed) || pattern.trim() == claimed.trim())
}

fn selects_wildcard(source: &str, test: &TestSummary, owner: &str, siblings: &[String]) -> bool {
    selects_wildcard_inner(source, test, owner, siblings).unwrap_or(false)
}

fn selects_wildcard_inner(
    source: &str,
    test: &TestSummary,
    owner: &str,
    siblings: &[String],
) -> Option<bool> {
    let root = parse_clean_source_file(source)?.tree();
    let mut candidates = root
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|name| name.text() == test.name))
        .filter(|function| {
            line_of(source, function.syntax()).is_some_and(|start| start <= test.start_line)
                && end_line_of(source, function.syntax()).is_some_and(|end| test.start_line <= end)
        });
    let function = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    let attributes = function.attrs().collect::<Vec<_>>();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    if attribute.syntax().text() != "#[test]"
        || function.async_token().is_some()
        || function.param_list()?.params().next().is_some()
    {
        return None;
    }
    for statement in function.body()?.stmt_list()?.statements() {
        if statement
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::RETURN_EXPR)
        {
            // Nothing after an early `return` is statically certain to run.
            return Some(false);
        }
        let ast::Stmt::ExprStmt(statement) = statement else {
            continue;
        };
        let Some(call) = statement
            .expr()
            .and_then(|expression| ast::MacroExpr::cast(expression.syntax().clone()))
            .and_then(|expression| expression.macro_call())
        else {
            continue;
        };
        let Some(line) = line_of(source, call.syntax()) else {
            continue;
        };
        let exact_oracle = test.assertions.iter().any(|assertion| {
            assertion.line == line
                && matches!(
                    assertion.kind,
                    OracleKind::ExactValue | OracleKind::ExactErrorVariant
                )
        });
        if exact_oracle && assertion_selects(&call, owner, siblings).unwrap_or(false) {
            return Some(true);
        }
    }
    Some(false)
}

fn assertion_selects(call: &ast::MacroCall, owner: &str, siblings: &[String]) -> Option<bool> {
    let path = call.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let name = path.segment()?.name_ref()?.text().to_string();
    let operands = macro_operands(call)?;
    let inputs = match (name.as_str(), operands.as_slice()) {
        ("assert_eq", [left, right, ..]) => vec![left, right],
        ("assert_matches", [value, pattern, ..]) => {
            if !exact_pattern(pattern.syntax()) {
                return None;
            }
            vec![value]
        }
        _ => return None,
    };
    Some(inputs.into_iter().any(|operand| {
        owner_string_input(operand, owner).is_some_and(|input| !siblings.contains(&input))
    }))
}

fn macro_operands(call: &ast::MacroCall) -> Option<Vec<ast::Expr>> {
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.strip_prefix('(')?.strip_suffix(')')?;
    let source = format!("fn __operands() {{ ({inner}) }}");
    let root = parse_clean_source_file(&source)?.tree();
    let function = root.syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let tuple = ast::TupleExpr::cast(body.tail_expr()?.syntax().clone())?;
    Some(tuple.fields().collect())
}

/// A pattern operand parsed as an expression pins one exact value only when
/// it is built from constructor calls, capitalized paths, tuples and
/// literals. `_`, `..`, lowercase bindings, guards and alternations refuse.
fn exact_pattern(node: &SyntaxNode) -> bool {
    node.descendants().all(|node| match node.kind() {
        SyntaxKind::CALL_EXPR
        | SyntaxKind::ARG_LIST
        | SyntaxKind::TUPLE_EXPR
        | SyntaxKind::PAREN_EXPR
        | SyntaxKind::LITERAL
        | SyntaxKind::PATH
        | SyntaxKind::PATH_SEGMENT
        | SyntaxKind::NAME_REF => true,
        SyntaxKind::PREFIX_EXPR => node.text().to_string().trim_start().starts_with('-'),
        SyntaxKind::PATH_EXPR => ast::PathExpr::cast(node.clone())
            .and_then(|path| path.path())
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text().starts_with(|ch: char| ch.is_ascii_uppercase())),
        _ => false,
    })
}

/// `owner("literal")`: a bare call of the owner with one plain string.
fn owner_string_input(expression: &ast::Expr, owner: &str) -> Option<String> {
    let call = ast::CallExpr::cast(expression.syntax().clone())?;
    if bare_name(&call.expr()?)? != owner {
        return None;
    }
    let arguments = call.arg_list()?.args().collect::<Vec<_>>();
    let [argument] = arguments.as_slice() else {
        return None;
    };
    let literal = ast::Literal::cast(argument.syntax().clone())?;
    plain_string(literal.syntax())
}

fn bare_name(expression: &ast::Expr) -> Option<String> {
    let path = ast::PathExpr::cast(expression.syntax().clone())?.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    // A turbofish or any other segment syntax is not the bare name.
    let segment = path.segment()?;
    let name = segment.name_ref()?.text().to_string();
    (segment.syntax().text().to_string().trim() == name).then_some(name)
}

/// Unescaped cooked string literals only; other spellings stay unverified.
fn plain_string(node: &SyntaxNode) -> Option<String> {
    let text = node.text().to_string();
    let value = text.trim().strip_prefix('"')?.strip_suffix('"')?;
    (!value.contains(['\\', '"', '\n', '\r'])).then(|| value.to_string())
}

fn line_of(source: &str, node: &SyntaxNode) -> Option<usize> {
    let start = u32::from(node.text_range().start()) as usize;
    Some(source.get(..start)?.bytes().filter(|b| *b == b'\n').count() + 1)
}

fn end_line_of(source: &str, node: &SyntaxNode) -> Option<usize> {
    let end = u32::from(node.text_range().end()) as usize;
    Some(source.get(..end)?.bytes().filter(|b| *b == b'\n').count() + 1)
}

/// A workspace `assert_eq` redefinition, or an `assert_matches` definition
/// that never panics, could accept a wrong value; refuse both.
fn assertion_macros_are_trusted(context: &ProbeContext<'_>) -> bool {
    context.index.files().iter().all(|(_, facts)| {
        if !facts.source.contains("macro_rules!") {
            return true;
        }
        let Some(parse) = parse_clean_source_file(&facts.source) else {
            return false;
        };
        parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::MacroRules::cast)
            .all(
                |rules| match rules.name().map(|name| name.text().to_string()) {
                    Some(name) if name == "assert_eq" => false,
                    Some(name) if name == "assert_matches" => rules
                        .token_tree()
                        .is_some_and(|body| body.syntax().text().to_string().contains("panic!")),
                    Some(_) => true,
                    None => false,
                },
            )
    })
}

#[cfg(test)]
mod tests {
    use crate::app::{CheckInput, OutputFormat, check_workspace};
    use crate::domain::{ExposureClass, ProbeFamily};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    const SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum UnitError {
    Blank,
    Unknown,
}

pub fn parse_unit(raw: &str) -> Result<u32, UnitError> {
    match raw {
        "" => Err(UnitError::Blank),
        "k" => Ok(1_000),
        "m" => Ok(1_000_000),
        _ => Err(UnitError::Blank),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_matches {
        ($value:expr, $pattern:pat) => {
            __BODY__
        };
    }

    #[test]
    fn unit_checks() {
        __ASSERTIONS__
    }
}
"#;

    const PANICKING: &str = "match $value {\n                $pattern => {}\n                other => panic!(\"{other:?} does not match {}\", stringify!($pattern)),\n            }";

    const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -9,7 +9,7 @@ pub fn parse_unit(raw: &str) -> Result<u32, UnitError> {
         "" => Err(UnitError::Blank),
         "k" => Ok(1_000),
         "m" => Ok(1_000_000),
-        _ => Err(UnitError::Unknown),
+        _ => Err(UnitError::Blank),
     }
 }
"#;

    /// The class of the changed `_ =>` match-arm finding.
    fn wildcard_arm_class(body: &str, assertions: &str) -> Result<ExposureClass, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root: PathBuf = std::env::temp_dir().join(format!(
            "ripr-wildcard-arm-{}-{sequence}",
            std::process::id()
        ));
        let result = (|| {
            std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"wildcard-arm\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(
                root.join("src/lib.rs"),
                SOURCE
                    .replace("__BODY__", body)
                    .replace("__ASSERTIONS__", assertions),
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(root.join("diff.patch"), DIFF).map_err(|error| error.to_string())?;
            let output = check_workspace(CheckInput {
                root: root.clone(),
                base: None,
                diff_file: Some(root.join("diff.patch")),
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let mut arms = output.findings.iter().filter(|finding| {
                finding.probe.family == ProbeFamily::MatchArm && finding.probe.location.line == 12
            });
            let finding = arms
                .next()
                .ok_or_else(|| "missing the changed wildcard match-arm finding".to_string())?;
            if arms.next().is_some() {
                return Err("duplicate wildcard match-arm findings".to_string());
            }
            Ok(finding.class.clone())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn exact_input_past_every_sibling_literal_observes_the_wildcard_arm() -> Result<(), String> {
        for assertion in [
            "assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            "assert_eq!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            "assert_eq!(Err(UnitError::Blank), parse_unit(\"zz\"));",
        ] {
            assert_eq!(
                wildcard_arm_class(PANICKING, assertion)?,
                ExposureClass::Exposed,
                "{assertion}"
            );
        }
        Ok(())
    }

    #[test]
    fn sibling_broad_skipped_or_untrusted_assertions_stay_unverified() -> Result<(), String> {
        let mut credited = Vec::new();
        for (body, assertion) in [
            // The input selects a sibling arm, not the changed wildcard arm.
            (PANICKING, "assert_matches!(parse_unit(\"k\"), Ok(1_000));"),
            (
                PANICKING,
                "assert_eq!(parse_unit(\"\"), Err(UnitError::Blank));",
            ),
            // The pattern accepts any error, so the arm's value is not pinned.
            (PANICKING, "assert_matches!(parse_unit(\"q\"), Err(_));"),
            // The assertion never runs.
            (
                PANICKING,
                "return;\n        assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            ),
            (
                PANICKING,
                "let _later = || assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            ),
            // A local assert_matches! that never panics checks nothing.
            (
                "let _ = $value;",
                "assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
            ),
        ] {
            let class = wildcard_arm_class(body, assertion)?;
            if class != ExposureClass::WeaklyExposed {
                credited.push(format!("{assertion} [{body}] -> {}", class.as_str()));
            }
        }
        assert!(credited.is_empty(), "must stay unverified: {credited:#?}");
        Ok(())
    }
}

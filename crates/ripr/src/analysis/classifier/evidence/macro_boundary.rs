//! Owner return values that reach an exact pin only through a macro
//! argument (#6614, RIPR-SPEC-0117 boundary).
//!
//! A test pins `assert_eq!(banner(), "[ripr]")` while the changed owner
//! `title()` only appears as an argument of the macro that forms
//! `banner()`'s whole return value (`format!("[{}]", title())`,
//! `twice!(base_rate)`). ripr does not expand macros, so it can neither
//! confirm that the owner's value survives into the pinned value nor
//! claim that it does not. Reporting `weakly_exposed` there names an
//! actionable gap the analyzer has not established.
//!
//! This module only withdraws the propagation claim (`Opaque`) for that
//! exact shape, so the finding reads `propagation_unknown`. It never adds
//! reach, observation, or discrimination, and every gate fails closed to
//! the unchanged evidence:
//!
//! - the probe is a `return_value` probe whose discriminator is still
//!   `observation_unverified` and whose propagation is not already `Yes`;
//! - the owner name and the pinned callee name are each unique among the
//!   indexed functions, and the callee is a production function other than
//!   the owner;
//! - the related test's assertion is an admitted plain `assert_eq!`
//!   (exact value or whole object) that does not name the owner, with one
//!   operand that is a complete bare call of the callee;
//! - the callee body has no statements and its tail expression is one macro
//!   invocation whose argument tokens name the owner as a whole word.
//!
//! A call of the wrapper that projects its value (`banner().len()`), an
//! assertion that is not an exact equality, a macro that does not name the
//! owner, or a macro that is not the wrapper's whole return value keeps the
//! existing evidence.

use crate::analysis::classify::{ProbeContext, contains_as_whole_word};
use crate::analysis::facts::{FunctionSourceRole, OracleFact, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Confidence, OracleKind, ProbeFamily, StageEvidence, StageState};
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, ast};

pub(super) fn propagation(
    context: &ProbeContext<'_>,
    propagate: &StageEvidence,
    discriminate: &StageEvidence,
    assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
) -> Option<StageEvidence> {
    if context.probe.family != ProbeFamily::ReturnValue
        || propagate.state == StageState::Yes
        || discriminate.state != StageState::Weak
        || !discriminate.summary.contains("observation_unverified")
    {
        return None;
    }
    let owner = context.owner_fn?;
    if unique_function(context, &owner.name).is_none() {
        return None;
    }
    for (test, _) in &context.related_tests {
        for assertion in &test.assertions {
            if !matches!(
                assertion.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            ) || contains_as_whole_word(&assertion.text, &owner.name)
                || !assertion_admitted(test, assertion)
            {
                continue;
            }
            for callee in pinned_bare_calls(&assertion.text) {
                if callee == owner.name {
                    continue;
                }
                let Some(wrapper) = unique_function(context, &callee) else {
                    continue;
                };
                if wrapper.source_role != FunctionSourceRole::Production {
                    continue;
                }
                if let Some(macro_name) = tail_macro_naming(&wrapper.body, &callee, &owner.name) {
                    return Some(StageEvidence::new(
                        StageState::Opaque,
                        Confidence::Low,
                        format!(
                            "The changed return value reaches the pinned `{callee}()` only as an argument of `{macro_name}!`; macro expansion is not modeled, so propagation to {}'s exact assertion is unresolved (macro_argument_propagation_unresolved)",
                            test.name
                        ),
                    ));
                }
            }
        }
    }
    None
}

fn unique_function<'a>(
    context: &ProbeContext<'a>,
    name: &str,
) -> Option<&'a crate::analysis::facts::FunctionSummary> {
    let mut functions = context
        .index
        .functions()
        .iter()
        .filter(|function| function.name == name);
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

/// Bare callee names of the operands of a plain `assert_eq!` that are a
/// complete call (`banner()`, `rate(3)`); a projected call (`banner().len()`)
/// or a path call (`m::banner()`) names nothing here.
fn pinned_bare_calls(text: &str) -> Vec<String> {
    let Some(operands) = assert_eq_operands(text) else {
        return Vec::new();
    };
    operands
        .iter()
        .take(2)
        .filter_map(|operand| {
            let call = ast::CallExpr::cast(operand.syntax().clone())?;
            let path = ast::PathExpr::cast(call.expr()?.syntax().clone())?.path()?;
            if path.qualifier().is_some() {
                return None;
            }
            Some(path.segment()?.name_ref()?.text().to_string())
        })
        .collect()
}

fn assert_eq_operands(text: &str) -> Option<Vec<ast::Expr>> {
    let source = format!("fn __assertion() {{ {} }}", text.trim());
    let root = parse_clean_source_file(&source)?.tree();
    let function = root.syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    let expression = match (
        body.statements().collect::<Vec<_>>().as_slice(),
        body.tail_expr(),
    ) {
        ([ast::Stmt::ExprStmt(statement)], None) => statement.expr()?,
        ([], Some(tail)) => tail,
        _ => return None,
    };
    let call = ast::MacroExpr::cast(expression.syntax().clone())?.macro_call()?;
    if call.path()?.syntax().text() != "assert_eq" {
        return None;
    }
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

/// The macro name when `body` is a function whose only content is a tail
/// macro invocation whose arguments name `owner` as a whole word.
fn tail_macro_naming(body: &str, name: &str, owner: &str) -> Option<String> {
    let root = parse_clean_source_file(body)?.tree();
    let mut functions = root
        .syntax()
        .children()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|n| n.text() == name));
    let function = functions.next()?;
    if functions.next().is_some() {
        return None;
    }
    let statements = function.body()?.stmt_list()?;
    if statements.statements().next().is_some() {
        return None;
    }
    let call = ast::MacroExpr::cast(statements.tail_expr()?.syntax().clone())?.macro_call()?;
    let arguments = call.token_tree()?.syntax().text().to_string();
    if !contains_as_whole_word(&arguments, owner) {
        return None;
    }
    let path = call.path()?;
    Some(path.segment()?.name_ref()?.text().to_string())
}

#[cfg(test)]
mod tests {
    use super::{pinned_bare_calls, tail_macro_naming};
    use crate::app::{CheckInput, OutputFormat, check_workspace};
    use crate::domain::{ExposureClass, ProbeFamily};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn pinned_bare_calls_reads_only_complete_bare_calls() {
        assert_eq!(
            pinned_bare_calls("assert_eq!(banner(), \"[x]\");"),
            ["banner"]
        );
        assert_eq!(pinned_bare_calls("assert_eq!(14, rate(2));"), ["rate"]);
        assert!(pinned_bare_calls("assert_eq!(banner().len(), 3);").is_empty());
        assert!(pinned_bare_calls("assert_eq!(m::banner(), \"[x]\");").is_empty());
        assert!(pinned_bare_calls("assert_ne!(banner(), \"[x]\");").is_empty());
        assert!(pinned_bare_calls("assert!(banner() == \"[x]\");").is_empty());
    }

    #[test]
    fn tail_macro_naming_requires_the_whole_return_to_be_the_macro() {
        let format = "pub fn banner() -> String {\n    format!(\"[{}]\", title())\n}";
        assert_eq!(
            tail_macro_naming(format, "banner", "title").as_deref(),
            Some("format")
        );
        let bare = "pub fn doubled() -> u32 {\n    twice!(base_rate)\n}";
        assert_eq!(
            tail_macro_naming(bare, "doubled", "base_rate").as_deref(),
            Some("twice")
        );
        let unrelated = "pub fn banner() -> String {\n    format!(\"[{}]\", other())\n}";
        assert_eq!(tail_macro_naming(unrelated, "banner", "title"), None);
        let statement =
            "pub fn banner() -> String {\n    println!(\"{}\", title());\n    String::new()\n}";
        assert_eq!(tail_macro_naming(statement, "banner", "title"), None);
        let plain = "pub fn banner() -> String {\n    title().to_string()\n}";
        assert_eq!(tail_macro_naming(plain, "banner", "title"), None);
    }

    const SOURCE: &str = r#"macro_rules! twice {
    ($f:ident) => {
        $f() * 2
    };
}

pub fn base_rate() -> u32 {
    3 + 4
}

pub fn doubled_rate() -> u32 {
    twice!(base_rate)
}

fn title() -> &'static str {
    concat!("ri", "pr")
}

pub fn banner() -> String {
    format!("[{}]", title())
}

pub fn other() -> &'static str {
    "x"
}

pub fn plain_banner() -> String {
    format!("[{}]", other())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks() {
        __ASSERTIONS__
    }
}
"#;

    const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,7 +5,7 @@
 }

 pub fn base_rate() -> u32 {
-    7
+    3 + 4
 }

 pub fn doubled_rate() -> u32 {
@@ -13,7 +13,7 @@
 }

 fn title() -> &'static str {
-    "ripr"
+    concat!("ri", "pr")
 }

 pub fn banner() -> String {
"#;

    /// The return_value classification of each changed owner, keyed by line.
    fn classify(assertions: &str) -> Result<Vec<(usize, ExposureClass)>, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root: PathBuf = std::env::temp_dir().join(format!(
            "ripr-macro-boundary-{}-{sequence}",
            std::process::id()
        ));
        let result = (|| {
            std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"macro-boundary\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(
                root.join("src/lib.rs"),
                SOURCE.replace("__ASSERTIONS__", assertions),
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
            let mut classes = output
                .findings
                .iter()
                .filter(|finding| finding.probe.family == ProbeFamily::ReturnValue)
                .map(|finding| (finding.probe.location.line, finding.class.clone()))
                .collect::<Vec<_>>();
            classes.sort_by_key(|(line, _)| *line);
            Ok(classes)
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn exact_pin_through_a_macro_argument_is_propagation_unknown() -> Result<(), String> {
        let classes =
            classify("assert_eq!(doubled_rate(), 14);\n        assert_eq!(banner(), \"[ripr]\");")?;
        assert_eq!(
            classes,
            [
                (8, ExposureClass::PropagationUnknown),
                (16, ExposureClass::PropagationUnknown),
            ]
        );
        Ok(())
    }

    #[test]
    fn unrelated_or_projected_wrapper_assertions_stay_actionable() -> Result<(), String> {
        // The wrapper is called but its pinned value does not carry the
        // owner (`plain_banner` formats `other()`), or the assertion projects
        // the wrapper's value instead of pinning it.
        let classes = classify(
            "assert_eq!(plain_banner(), \"[x]\");\n        assert!(doubled_rate() > 0);\n        assert_eq!(banner().len(), 6);",
        )?;
        assert_eq!(
            classes,
            [
                (8, ExposureClass::WeaklyExposed),
                (16, ExposureClass::WeaklyExposed),
            ]
        );
        Ok(())
    }
}

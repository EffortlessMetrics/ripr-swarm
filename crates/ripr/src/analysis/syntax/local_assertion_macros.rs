//! Same-file `macro_rules!` assertion wrappers called from test bodies
//! (#9122).
//!
//! A test crate often hides its assertion behind a local macro:
//!
//! ```text
//! macro_rules! assert_next_eq {
//!     ($io:ident, $expect:expr) => {{ assert_eq!(next($io), $expect); }};
//! }
//!
//! #[test]
//! fn reads_frame() { assert_next_eq!(io, b"abc"); }
//! ```
//!
//! The parser sees the call as an opaque token tree, so the test read as
//! having no assertion. This module is not a macro evaluator. It reports,
//! per test, the statement-level calls of a local macro together with the
//! macro's transcriber text, and refuses everything it cannot attribute:
//!
//! - the definition is the only macro of that name anywhere in the file,
//!   sits at item position behind no `cfg`, precedes the call and encloses
//!   it, and no `use` imports the name;
//! - the call is a plain statement of the test function's own body: not in
//!   a closure, `async` or nested block, branch, loop, nested `fn` or
//!   another macro's input, and it carries no attribute.
//!
//! What the transcriber proves is decided by the facts layer
//! (`facts::test_helpers`), which reads the transcriber text and never grades
//! it above what the text itself shows.

use ra_ap_syntax::ast::{self, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode};
use std::collections::{BTreeMap, BTreeSet};

use super::local_test_macros::{
    in_definition_scope, inner_elements, is_item_position, local_macro_names,
};
use super::nesting::parse_clean_source_file;
use super::owner_pin::supported_item_context;
use super::ra::LineIndex;

/// One statement-level call of a same-file macro from a test function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalMacroCall {
    pub macro_name: String,
    /// Name of the enclosing function.
    pub function: String,
    /// 1-based lines of the call statement in the file.
    pub start_line: usize,
    pub end_line: usize,
    /// Source text of the call.
    pub text: String,
    /// Every arm of the macro.
    pub arms: Vec<MacroArm>,
}

/// One `matcher => transcriber` arm, delimiters included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MacroArm {
    pub matcher: String,
    pub transcriber: String,
}

/// Every attributable statement-level call of a same-file macro.
pub(crate) fn local_macro_calls(source: &str) -> Vec<LocalMacroCall> {
    if !source.contains("macro_rules") {
        return Vec::new();
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let root = parse.tree();
    let (definitions, imported) = definitions(root.syntax());
    if definitions.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(source);
    let mut calls = Vec::new();
    for call in root.syntax().descendants().filter_map(ast::MacroCall::cast) {
        let Some(name) = call.path().map(|path| path.syntax().text().to_string()) else {
            continue;
        };
        let Some((definition, arms)) = definitions.get(&name) else {
            continue;
        };
        if imported.contains(&name)
            || !in_definition_scope(&call, definition)
            || call.attrs().next().is_some()
        {
            continue;
        }
        let Some(function) = call.syntax().ancestors().find_map(ast::Fn::cast) else {
            continue;
        };
        let Some(function_name) = function.name() else {
            continue;
        };
        if !is_plain_statement_of(call.syntax(), &function) {
            continue;
        }
        let range = call.syntax().text_range();
        calls.push(LocalMacroCall {
            macro_name: name,
            function: function_name.text().to_string(),
            start_line: lines.line(range.start()),
            end_line: lines.line_for_range_end(range.end()),
            text: call.syntax().text().to_string(),
            arms: arms.clone(),
        });
    }
    calls
}

/// A usable definition and the transcriber text of each of its arms.
type Definitions = BTreeMap<String, (ast::MacroRules, Vec<MacroArm>)>;

/// Unique, scoped, un-gated definitions and the names a `use` imports.
fn definitions(root: &SyntaxNode) -> (Definitions, BTreeSet<String>) {
    let (counts, imported) = local_macro_names(root);
    let mut found = BTreeMap::new();
    for rule in root.descendants().filter_map(ast::MacroRules::cast) {
        let Some(name) = rule.name().map(|name| name.text().to_string()) else {
            continue;
        };
        if counts.get(&name) != Some(&1)
            || name.starts_with("r#")
            || !is_item_position(rule.syntax())
            || !supported_item_context(rule.syntax())
            || rule.attrs().any(|attr| {
                // `#[macro_export]` and `#[macro_use]` do not change the
                // textual scope read here; any other attribute may gate it.
                !attr.path().is_some_and(|path| {
                    path.syntax().text() == "macro_export" || path.syntax().text() == "macro_use"
                })
            })
        {
            continue;
        }
        if let Some(arms) = transcribers(&rule) {
            found.insert(name, (rule, arms));
        }
    }
    (found, imported)
}

/// The transcriber text of every arm; `None` for any unexpected layout.
fn transcribers(definition: &ast::MacroRules) -> Option<Vec<MacroArm>> {
    let body = definition.token_tree()?;
    let elements = inner_elements(body.syntax());
    let mut arms = Vec::new();
    let mut index = 0;
    while index < elements.len() {
        let matcher = elements.get(index)?;
        let eq = elements.get(index + 1)?;
        let gt = elements.get(index + 2)?;
        let transcriber = elements.get(index + 3)?;
        if matcher.as_node().is_none()
            || eq.kind() != SyntaxKind::EQ
            || gt.kind() != SyntaxKind::R_ANGLE
        {
            return None;
        }
        arms.push(MacroArm {
            matcher: matcher.as_node()?.text().to_string(),
            transcriber: transcriber.as_node()?.text().to_string(),
        });
        index += 4;
        if elements
            .get(index)
            .is_some_and(|separator| separator.kind() == SyntaxKind::SEMICOLON)
        {
            index += 1;
        }
    }
    (!arms.is_empty()).then_some(arms)
}

/// Whether `call` is a plain statement of `function`'s own body block, with
/// no closure, branch, loop, nested block, attribute or nested item between.
fn is_plain_statement_of(call: &SyntaxNode, function: &ast::Fn) -> bool {
    let Some(body) = function.body() else {
        return false;
    };
    if call
        .children()
        .any(|child| child.kind() == SyntaxKind::ATTR)
    {
        return false;
    }
    for node in call.ancestors().skip(1) {
        if &node == body.syntax() {
            return true;
        }
        let plain = matches!(
            node.kind(),
            SyntaxKind::STMT_LIST | SyntaxKind::MACRO_EXPR | SyntaxKind::MACRO_STMTS
        ) || ast::ExprStmt::can_cast(node.kind());
        // An attribute on the statement or its expression (`#[cfg(..)]`) may
        // disable the call.
        if !plain
            || node
                .children()
                .any(|child| child.kind() == SyntaxKind::ATTR)
        {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_call_of_a_unique_macro_is_reported_with_its_arms() -> Result<(), String> {
        let calls = local_macro_calls(
            r#"
macro_rules! check {
    ($v:expr) => {{ assert_eq!($v, 1); }};
}

#[test]
fn a() {
    check!(compute());
}
"#,
        );
        let [call] = calls.as_slice() else {
            return Err(format!("expected one call, got {calls:?}"));
        };
        assert_eq!(call.macro_name, "check");
        assert_eq!(call.function, "a");
        assert_eq!(call.start_line, 8);
        assert_eq!(call.arms.len(), 1);
        assert!(call.arms[0].transcriber.contains("assert_eq!"));
        assert_eq!(call.arms[0].matcher, "($v:expr)");
        Ok(())
    }

    #[test]
    fn refuses_calls_that_may_not_run_or_resolve_elsewhere() {
        for body in [
            "let f = || check!(1); f();",
            "if flag { check!(1); }",
            "for _ in 0..1 { check!(1); }",
            "async { check!(1); };",
            "{ check!(1); }",
            "#[cfg(unix)] check!(1);",
            "fn inner() { check!(1); }",
            "macro_rules! check { ($v:expr) => {}; }",
        ] {
            let source = format!(
                "macro_rules! check {{ ($v:expr) => {{ assert_eq!($v, 1); }}; }}\n#[test]\nfn a() {{ {body} }}\n"
            );
            let calls = local_macro_calls(&source);
            assert!(
                calls.iter().all(|call| call.function != "a"),
                "{body} must not attribute a call: {calls:?}"
            );
        }
    }

    #[test]
    fn refuses_duplicate_imported_gated_and_unscoped_definitions() {
        for source in [
            // Duplicate name.
            "macro_rules! check { ($v:expr) => { assert_eq!($v, 1); }; }\nmod m { macro_rules! check { ($v:expr) => {}; } }\n#[test]\nfn a() { check!(1); }\n",
            // Imported name.
            "use other::check;\nmacro_rules! check { ($v:expr) => { assert_eq!($v, 1); }; }\n#[test]\nfn a() { check!(1); }\n",
            // Behind a cfg.
            "#[cfg(unix)]\nmacro_rules! check { ($v:expr) => { assert_eq!($v, 1); }; }\n#[test]\nfn a() { check!(1); }\n",
            // Defined after the call.
            "#[test]\nfn a() { check!(1); }\nmacro_rules! check { ($v:expr) => { assert_eq!($v, 1); }; }\n",
            // Defined in a module that does not enclose the call.
            "mod m { macro_rules! check { ($v:expr) => { assert_eq!($v, 1); }; } }\n#[test]\nfn a() { check!(1); }\n",
        ] {
            assert!(
                local_macro_calls(source).is_empty(),
                "must refuse: {source}"
            );
        }
    }
}

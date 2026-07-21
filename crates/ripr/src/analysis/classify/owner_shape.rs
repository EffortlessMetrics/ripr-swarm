//! RIPR-SPEC-0133: detect assertion-shaped owners (oracles).
//!
//! The R-I-P-R guidance presumes the changed owner is code *under* test. When
//! the changed owner is itself an assertion helper, advice like "add a
//! co-located test that observes the changed owner" asks for a test of the
//! oracle. This module detects that shape conservatively so
//! `decision::recommended_next_step` can reframe the guidance; it never
//! changes the exposure class.
//!
//! The rule is explainable in one sentence (see
//! `crate::domain::ASSERTION_SHAPED_OWNER_REASON`): the owner's body contains
//! at least one assert-family macro (`assert!`, `assert_eq!`, `assert_ne!`,
//! `debug_assert!`, `debug_assert_eq!`, `debug_assert_ne!`) or `.expect(`
//! call, every non-declaration, non-control statement in the body is one of
//! those, and no non-test function in the index calls it.
//!
//! Fail-closed on purpose: any unrecognized statement, and any non-test caller
//! (even a bare-name token coincidence), disqualifies the owner. Under-emit is
//! preferred over over-emit.

use super::super::rust_index::{FunctionSummary, RustIndex};

/// Assert-family macros in scope for RIPR-SPEC-0133. `assert_matches!` and
/// snapshot macros are deliberately excluded: a body using them leaves an
/// unrecognized statement, which keeps the detection fail-closed.
const ASSERT_FAMILY_MACROS: [&str; 6] = [
    "assert!(",
    "assert_eq!(",
    "assert_ne!(",
    "debug_assert!(",
    "debug_assert_eq!(",
    "debug_assert_ne!(",
];

pub(in crate::analysis) fn is_assertion_shaped_owner(
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    body_is_assertion_dominated(&owner.body) && !has_non_test_caller(owner, index)
}

/// A non-test caller is any non-test function in the index (other than the
/// owner itself) whose call facts mention the owner's bare name. Bare-name
/// matching can collide with a different same-named function, but a collision
/// only *blocks* the oracle reframe — the fail-closed direction.
fn has_non_test_caller(owner: &FunctionSummary, index: &RustIndex) -> bool {
    index.functions.iter().any(|function| {
        !function.is_test
            && function.id != owner.id
            && function.calls.iter().any(|call| call.name == owner.name)
    })
}

/// Dominance rule: at least one assert-family statement, and zero statements
/// that are neither declarations (`let`), control flow, structure, nor
/// assert-family calls.
///
/// Statements are accumulated line-by-line until a line ends with `;` or `{`
/// (or closes a block). There is no paren-depth tracking: a multi-line
/// statement simply keeps accumulating, and the joined text is classified as a
/// whole, so char literals like `'('` cannot skew the scan. A statement the
/// scanner cannot recognize counts as `Other` and blocks the detection —
/// fail-closed.
fn body_is_assertion_dominated(body: &str) -> bool {
    let mut assert_statements = 0usize;
    let mut other_statements = 0usize;
    let mut statement = String::new();

    for raw_line in body.lines() {
        let line = strip_comments_and_strings(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !statement.is_empty() {
            statement.push(' ');
        }
        statement.push_str(trimmed);

        let completes =
            trimmed.ends_with(';') || trimmed.ends_with('{') || trimmed.starts_with('}');
        if completes {
            record_statement(&statement, &mut assert_statements, &mut other_statements);
            statement.clear();
        }
    }
    if !statement.is_empty() {
        record_statement(&statement, &mut assert_statements, &mut other_statements);
    }

    assert_statements >= 1 && other_statements == 0
}

fn record_statement(statement: &str, assert_statements: &mut usize, other_statements: &mut usize) {
    match classify_statement(statement) {
        StatementKind::Assert => *assert_statements += 1,
        StatementKind::Other => *other_statements += 1,
        StatementKind::Ignorable => {}
    }
}

enum StatementKind {
    Assert,
    Other,
    Ignorable,
}

fn classify_statement(statement: &str) -> StatementKind {
    if contains_assert_family_call(statement) {
        return StatementKind::Assert;
    }
    let mut rest = statement;
    loop {
        let stripped = rest
            .strip_prefix("pub(crate) ")
            .or_else(|| rest.strip_prefix("pub "))
            .or_else(|| rest.strip_prefix("async "))
            .or_else(|| rest.strip_prefix("unsafe "))
            .or_else(|| rest.strip_prefix("const "));
        match stripped {
            Some(next) => rest = next,
            None => break,
        }
    }
    // The enclosing `fn` signature, declarations, and control-flow headers do
    // not count for or against dominance.
    let ignorable = rest.starts_with("fn ")
        || rest.starts_with("let ")
        || rest.starts_with('}')
        || rest.starts_with('#')
        || ["for ", "if ", "else", "match ", "while ", "loop"]
            .iter()
            .any(|prefix| rest.starts_with(prefix));
    if ignorable {
        StatementKind::Ignorable
    } else {
        StatementKind::Other
    }
}

fn contains_assert_family_call(text: &str) -> bool {
    ASSERT_FAMILY_MACROS
        .iter()
        .any(|needle| contains_word_call(text, needle))
        || text.contains(".expect(")
}

/// Word-boundary match so `my_assert!(` does not count as `assert!(`. The
/// `debug_assert*` needles match at their own start; the `_` before `assert!(`
/// inside them correctly fails the boundary for the shorter needle.
fn contains_word_call(text: &str, needle: &str) -> bool {
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(needle) {
        let index = start + pos;
        let boundary = index == 0
            || !text[..index]
                .chars()
                .last()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if boundary {
            return true;
        }
        start = index + 1;
    }
    false
}

/// Blank out `//` comments and string-literal contents so an assertion
/// mentioned in prose (a comment, or a failure message naming another macro)
/// does not count as a statement.
fn strip_comments_and_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
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
            out.push(' ');
            continue;
        }
        if ch == '/' && chars.peek().is_some_and(|next| *next == '/') {
            break;
        }
        if ch == '"' {
            in_string = true;
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::CallFact;
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    fn function(
        file: &str,
        name: &str,
        body: &str,
        is_test: bool,
        calls: Vec<&str>,
    ) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("{file}::{name}")),
            name: name.to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 1,
            body: body.to_string(),
            calls: calls
                .into_iter()
                .map(|call| CallFact {
                    line: 1,
                    name: call.to_string(),
                    text: format!("{call}()"),
                })
                .collect(),
            returns: Vec::new(),
            literals: Vec::new(),
            is_test,
            attrs: Vec::new(),
        }
    }

    const HAWK_SHAPED_BODY: &str = "fn assert_workspace_source_paths_are_stable(fragments: &[Fragment], expected_root: &Path) {\n    for fragment in fragments {\n        assert_eq!(\n            fragment.crate_root.as_deref(),\n            Some(expected_root),\n            \"crate reported an unexpected source root\",\n        );\n        for span in &fragment.spans {\n            assert!(\n                !span.file.contains('\\\\'),\n                \"span path keeps a native separator\",\n            );\n        }\n    }\n}\n";

    #[test]
    fn hawk_shaped_helper_is_assertion_shaped() -> Result<(), String> {
        let owner = function(
            "tests/fragments.rs",
            "assert_workspace_source_paths_are_stable",
            HAWK_SHAPED_BODY,
            false,
            Vec::new(),
        );
        let caller_test = function(
            "tests/fragments.rs",
            "workspace_paths_stay_stable",
            "fn workspace_paths_stay_stable() { assert_workspace_source_paths_are_stable(); }",
            true,
            vec!["assert_workspace_source_paths_are_stable"],
        );
        let index = RustIndex {
            functions: vec![owner.clone(), caller_test],
            ..RustIndex::default()
        };

        assert!(
            is_assertion_shaped_owner(&owner, &index),
            "test-called assertion helper must be assertion-shaped"
        );
        Ok(())
    }

    #[test]
    fn production_caller_blocks_assertion_shaped_detection() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "check_invariants",
            "fn check_invariants(value: i32) {\n    assert!(value >= 0);\n    assert_eq!(value % 2, 0);\n}\n",
            false,
            Vec::new(),
        );
        let production_caller = function(
            "src/lib.rs",
            "validate",
            "fn validate(value: i32) {\n    check_invariants(value);\n}\n",
            false,
            vec!["check_invariants"],
        );
        let index = RustIndex {
            functions: vec![owner.clone(), production_caller],
            ..RustIndex::default()
        };

        assert!(
            !is_assertion_shaped_owner(&owner, &index),
            "a non-test caller must keep the standard guidance"
        );
        Ok(())
    }

    #[test]
    fn non_assert_statement_blocks_dominance() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "load_config",
            "fn load_config(path: &Path) -> Config {\n    let text = fs::read_to_string(path).expect(\"config readable\");\n    parse_config(&text)\n}\n",
            false,
            Vec::new(),
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(
            !is_assertion_shaped_owner(&owner, &index),
            "a tail `parse_config(&text)` expression is not an assertion"
        );
        Ok(())
    }

    #[test]
    fn no_assert_family_statement_blocks_dominance() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "normalize",
            "fn normalize(value: i32) -> i32 {\n    let clamped = value.max(0);\n    clamped\n}\n",
            false,
            Vec::new(),
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(
            !is_assertion_shaped_owner(&owner, &index),
            "zero assert-family statements is never assertion-shaped"
        );
        Ok(())
    }

    #[test]
    fn comment_and_string_mentions_do_not_count_as_assertions() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "compute",
            "fn compute(value: i32) -> i32 {\n    // assert_eq!(value, 0);\n    let label = \"assert!(ready)\";\n    let _ = label;\n    value + 1\n}\n",
            false,
            Vec::new(),
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(
            !is_assertion_shaped_owner(&owner, &index),
            "comment/string mentions of assert macros must not count"
        );
        Ok(())
    }

    #[test]
    fn word_boundary_keeps_my_assert_out_of_the_family() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "helper",
            "fn helper(value: i32) {\n    my_assert!(value > 0);\n}\n",
            false,
            Vec::new(),
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(
            !is_assertion_shaped_owner(&owner, &index),
            "my_assert! is not the assert! macro"
        );
        Ok(())
    }

    #[test]
    fn test_annotated_caller_does_not_count_as_a_non_test_caller() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "assert_invariants",
            "fn assert_invariants(value: i32) {\n    assert!(value >= 0);\n    assert_eq!(value % 2, 0);\n}\n",
            false,
            Vec::new(),
        );
        let test_caller = function(
            "src/lib.rs",
            "invariants_hold",
            "fn invariants_hold() {\n    assert_invariants(2);\n}\n",
            true,
            vec!["assert_invariants"],
        );
        let index = RustIndex {
            functions: vec![owner.clone(), test_caller],
            ..RustIndex::default()
        };

        assert!(
            is_assertion_shaped_owner(&owner, &index),
            "a #[test] caller is not a production caller"
        );
        Ok(())
    }

    #[test]
    fn expect_in_let_binding_counts_as_assert_statement() -> Result<(), String> {
        let owner = function(
            "src/lib.rs",
            "assert_fragments_parse",
            "fn assert_fragments_parse(dir: &Path) {\n    for entry in fs::read_dir(dir).expect(\"fragments dir readable\") {\n        let path = entry.expect(\"entry readable\").path();\n        let text = fs::read_to_string(&path).expect(\"fragment readable\");\n        assert!(text.starts_with('{'));\n    }\n}\n",
            false,
            Vec::new(),
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(
            is_assertion_shaped_owner(&owner, &index),
            "expect-based setup plus asserts stays assertion-shaped"
        );
        Ok(())
    }
}

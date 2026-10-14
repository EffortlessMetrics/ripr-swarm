//! Bounded SPEC-0225 expected literal bindings; no general value-flow resolution.

use super::{OracleFact, TestSummary, is_plain_identifier, whole_word_offsets};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};

/// Resolve only an immutable, once-used expected binding immediately before
/// this exact top-level assertion in the test's own block. The caller still
/// applies every whole-value literal/equality/owner gate to its initializer.
pub(super) fn expected_literal_binding(
    test: &TestSummary,
    assertion: &OracleFact,
    operand: &str,
) -> Option<String> {
    let name = operand.trim();
    if !is_plain_identifier(name) {
        return None;
    }
    let masked = mask_comments_and_strings(&test.body);
    let uses = whole_word_offsets(&masked, name);
    let [binding_use, _] = uses.as_slice() else {
        return None;
    };
    let parse = parse_clean_source_file(&test.body)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let block = function.body()?.stmt_list()?;
    let statements: Vec<_> = block.statements().collect();
    for pair in statements.windows(2) {
        let statement = pair[1].syntax();
        let text = statement.text().to_string();
        let start = usize::from(statement.text_range().start());
        if test.start_line + test.body.get(..start)?.matches('\n').count() != assertion.line
            || text.trim().trim_end_matches(';').trim()
                != assertion.text.trim().trim_end_matches(';').trim()
        {
            continue;
        }
        let ast::Stmt::LetStmt(binding) = &pair[0] else {
            return None;
        };
        if binding.attrs().next().is_some() || binding.let_else().is_some() {
            return None;
        }
        let ast::Pat::IdentPat(pattern) = binding.pat()? else {
            return None;
        };
        let declared = pattern.name()?;
        if pattern.mut_token().is_some()
            || pattern.ref_token().is_some()
            || pattern.pat().is_some()
            || declared.text() != name
            || usize::from(declared.syntax().text_range().start()) != *binding_use
        {
            return None;
        }
        return Some(binding.initializer()?.syntax().text().to_string());
    }
    None
}

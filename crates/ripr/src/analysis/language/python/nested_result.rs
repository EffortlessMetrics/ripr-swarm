//! A nested function whose result the enclosing function throws away
//! (RIPR-SPEC-0238 rule 31, #12182).
//!
//! `def outer(): def inner(): return 3; inner(); return 2` reaches `inner`
//! through `outer`, but `assert outer() == 2` reads nothing `inner` returned.
//! A changed `return` in `inner` therefore passes either way. Reach is a call
//! path; credit needs a result path. The owner fact is true only when every
//! call the enclosing body makes is an expression statement of its own, so a
//! call whose value may be returned, bound, passed or asserted keeps today's
//! reading.

use super::related_tests::{PythonRelatedCandidate, contains_call_name};
use super::table_rows::UnvariedTableOperand;
use super::{PythonOwner, PythonTest};
use crate::domain::OwnerKind;

/// When the changed owner is a nested function whose result the enclosing
/// body discards, and no related test calls the nested name itself, name the
/// gap. A related test that calls the name directly may be exercising a
/// different binding, so it keeps today's reading.
pub(super) fn python_nested_result_gap(
    owner: &PythonOwner,
    line_text: &str,
    old_line_text: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Option<UnvariedTableOperand> {
    // Discarding a call's result does not discard its side effects: a
    // `return values.append(2)` changes what the enclosing function returns
    // through `values`. Only a plain `return` expression with no call, walrus,
    // `await` or `yield` is known to affect nothing but its own result.
    if !is_effect_free_return(line_text) || !old_line_text.is_none_or(is_effect_free_return) {
        return None;
    }
    if !owner.nested_result_discarded
        || owner.owner_kind != Some(OwnerKind::Function)
        || owner.qualified_name == owner.name
        || related_candidates.is_empty()
        || related_candidates
            .iter()
            .any(|candidate| test_calls_name(candidate.test, &owner.name))
    {
        return None;
    }
    Some(UnvariedTableOperand {
        reason: format!(
            "the enclosing function calls `{}` only as a statement of its own, so its result reaches nothing the test asserts",
            owner.name
        ),
        discriminator: format!(
            "an assertion on a value computed from the result of `{}`",
            owner.qualified_name
        ),
        action: format!(
            "Assert on a value that depends on the result of `{}`.",
            owner.qualified_name
        ),
    })
}

/// A `return <expr>` line whose expression reads values without calling,
/// binding, suspending or yielding anything. Any `(` is treated as a possible
/// call, so `return f"@{m}"` and `return m + 1` qualify and `return g(m)` does
/// not.
pub(super) fn is_effect_free_return(line: &str) -> bool {
    let line = line.trim();
    let Some(expr) = line.strip_prefix("return") else {
        return false;
    };
    expr.starts_with([' ', '\t'])
        && !expr.contains(['(', ':'])
        && !expr
            .split(|ch: char| !(ch == '_' || ch.is_alphanumeric()))
            .any(|word| matches!(word, "await" | "yield" | "lambda"))
}

fn test_calls_name(test: &PythonTest, name: &str) -> bool {
    contains_call_name(&test.body_text, name)
}

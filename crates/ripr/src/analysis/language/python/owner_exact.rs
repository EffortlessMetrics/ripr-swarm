//! Per-finding exact reading of `f(g(x)) == g(x)` (#9814, RIPR-SPEC-0238
//! rule 25).
//!
//! Extraction demotes the assertion because `g(x)` runs on both sides: a
//! changed `g` passes through an identity-like `f`. The same assertion does
//! pin the outer callee `f`, so the demotion cannot be decided before the
//! finding's owner is known. Extraction records the outer callee; this module
//! re-reads the assertion as an exact value check for a finding whose owner is
//! that bare function, and every other owner keeps the demotion.

use super::{OracleKind, OracleStrength, PythonOwner, PythonTest};

/// `test` with each assertion whose independent outer callee is `owner`
/// read as an exact value check, or `None` when no assertion qualifies. The
/// owner must be the plain module-level function the assertion names.
pub(super) fn owner_exact_test(test: &PythonTest, owner: &PythonOwner) -> Option<PythonTest> {
    if owner.name.is_empty() || owner.qualified_name != owner.name {
        return None;
    }
    let qualifies = |callee: &Option<String>| callee.as_deref() == Some(owner.name.as_str());
    if !test
        .assertions
        .iter()
        .any(|assertion| qualifies(&assertion.owner_exact_callee))
    {
        return None;
    }
    let mut resolved = test.clone();
    for assertion in &mut resolved.assertions {
        if qualifies(&assertion.owner_exact_callee) {
            assertion.oracle_kind = OracleKind::ExactValue;
            assertion.oracle_strength = OracleStrength::Strong;
            assertion.owner_exact_callee = None;
        }
    }
    Some(resolved)
}

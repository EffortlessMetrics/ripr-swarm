//! Kani harnesses for the edit-cage path-surface decision (RIPR-SPEC-0371 lane).
//!
//! Paths are drawn from a closed corpus that contains the boundary cases
//! (`a`, `a/b`, `ab`, `a/`, `c`), not from arbitrary strings: the property is
//! the rule-matching and precedence logic, not string handling, and CBMC does
//! not terminate on unconstrained `String`. The claim is therefore bounded to
//! that corpus. Run: `cargo kani -p ripr --harness <name>`.

use super::*;

const CORPUS: [&str; 5] = ["a", "a/b", "ab", "a/", "c"];

fn any_path() -> &'static str {
    let index: usize = kani::any();
    kani::assume(index < CORPUS.len());
    CORPUS[index]
}

fn any_scope() -> CagePathScope {
    if kani::any() {
        CagePathScope::Exact
    } else {
        CagePathScope::Subtree
    }
}

fn any_rule() -> CagePathRule {
    CagePathRule {
        path: any_path().to_string(),
        scope: any_scope(),
    }
}

fn policy_with(
    selected: CagePathRule,
    allowed: Vec<CagePathRule>,
    forbidden: Vec<CagePathRule>,
    operational: Vec<CagePathRule>,
) -> EditCagePolicy {
    EditCagePolicy {
        selected_target: selected,
        allowed_edit_surface: allowed,
        forbidden_paths: forbidden,
        expected_operational_writes: operational,
        ignored_build_output: None,
        untracked_build_lockfile: None,
        inline_test_module_target: false,
    }
}

/// A subtree rule matches its own path and strict `/`-separated descendants,
/// never a sibling that merely shares a string prefix (`a` vs `ab`); an exact
/// rule matches only the identical path.
#[kani::proof]
#[kani::unwind(8)]
fn rule_match_respects_path_boundaries() {
    let rule = any_rule();
    let candidate = any_path();
    let (p, c) = (rule.path.as_bytes(), candidate.as_bytes());
    let descendant = c.len() > p.len() && &c[..p.len()] == p && c[p.len()] == b'/';
    let expected = match rule.scope {
        CagePathScope::Exact => c == p,
        CagePathScope::Subtree => c == p || descendant,
    };
    kani::assert(
        rule.matches(candidate) == expected,
        "rule matching follows exact/subtree boundary semantics",
    );
    kani::cover!(rule.scope == CagePathScope::Subtree && rule.path == "a" && candidate == "a/b");
    kani::cover!(rule.scope == CagePathScope::Subtree && rule.path == "a" && candidate == "ab");
    kani::cover!(rule.scope == CagePathScope::Exact && rule.path == "a" && candidate == "a/b");
}

fn count(violations: &[EditCageViolation], kind: EditCageViolationKind) -> usize {
    let mut total = 0;
    for violation in violations {
        if violation.kind == kind {
            total += 1;
        }
    }
    total
}

/// Precedence of the path-surface decision for one changed path: a forbidden
/// match reports exactly one `ForbiddenPath` and never reaches the allow
/// check; otherwise the path is `OutsideAllowedSurface` exactly when neither
/// the allowed surface nor the expected operational writes cover it. A rule
/// in the allowed surface can never override a forbidden rule.
#[kani::proof]
#[kani::unwind(8)]
fn forbidden_wins_and_unlisted_paths_are_outside() {
    let forbidden = any_rule();
    let allowed = any_rule();
    let operational = any_rule();
    // The changed path ranges over the closed corpus with a concrete value per
    // iteration: a symbolic choice of `&str` makes the string copy inside the
    // production function symbolic-length, which CBMC reports as spurious
    // assertion failures (observed, not a defect in the edit cage).
    for path in CORPUS {
        precedence_body(
            path,
            forbidden.clone(),
            allowed.clone(),
            operational.clone(),
        );
    }
}

fn precedence_body(
    path: &'static str,
    forbidden: CagePathRule,
    allowed: CagePathRule,
    operational: CagePathRule,
) {
    let policy = policy_with(
        CagePathRule {
            path: "zz".to_string(),
            scope: CagePathScope::Exact,
        },
        vec![allowed.clone()],
        vec![forbidden.clone()],
        vec![operational.clone()],
    );
    let mut selected_changed = false;
    let mut violations = Vec::new();
    evaluate_normalized_path(
        &policy,
        path.to_string(),
        false,
        true,
        &mut selected_changed,
        &mut violations,
    );
    let forbidden_hit = forbidden.matches(path);
    let covered = allowed.matches(path) || operational.matches(path);
    let forbidden_count = count(&violations, EditCageViolationKind::ForbiddenPath);
    let outside_count = count(&violations, EditCageViolationKind::OutsideAllowedSurface);
    if forbidden_hit {
        kani::assert(
            forbidden_count == 1 && outside_count == 0,
            "a forbidden match reports ForbiddenPath once and never OutsideAllowedSurface",
        );
    } else {
        kani::assert(forbidden_count == 0, "no forbidden match, no ForbiddenPath");
        kani::assert(
            outside_count == usize::from(!covered),
            "OutsideAllowedSurface exactly when no allow rule covers the path",
        );
    }
    kani::assert(
        violations.len() == forbidden_count + outside_count,
        "no other violation kind is produced for a non-target path",
    );
    kani::assert(!selected_changed, "the selected target did not move");
    kani::cover!(forbidden_hit && allowed.matches(path));
    kani::cover!(!forbidden_hit && !covered);
    kani::cover!(!forbidden_hit && covered);
}

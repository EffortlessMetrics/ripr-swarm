//! #14951: the typed boundary-input limit the activate stage names, and the
//! uncalled-owner reading `ripr check` borrows, agree on the same seam.

use super::*;
use crate::domain::StaticLimitKind;

const LIMIT: StaticLimitKind = StaticLimitKind::ActivationBoundaryInputUnresolved;

/// `owner` is never called; one same-file test relates to it by file only.
fn index_with_uncalled(owner: &str) -> Result<FixtureIndex, String> {
    let source = format!(
        "pub fn discount_percent(amount: u32) -> u32 {{
    if amount >= 100 {{ 15 }} else {{ 0 }}
}}

{owner}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn discount_boundary() {{
        assert_eq!(discount_percent(100), 15);
        assert_eq!(discount_percent(99), 0);
    }}
}}
"
    );
    index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])
}

/// Every predicate-boundary seam of `uncalled` with the full producer's
/// reading and the shared uncalled-owner decision.
fn readings(owner: &str) -> Result<Vec<(String, bool, bool, StageEvidence)>, String> {
    let index = index_with_uncalled(owner)?;
    let mut rows = Vec::new();
    for seam in inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index) {
        if !seam.owner().ends_with("::uncalled") || seam.kind() != SeamKind::PredicateBoundary {
            continue;
        }
        let owner_fn = index
            .functions()
            .iter()
            .find(|function| function.name == "uncalled")
            .ok_or("uncalled owner not indexed")?;
        let evidence = evidence_for_seam(&seam, &index);
        // Fixture construction: the seam has a related test that does not
        // call the owner, the premise the shared decision assumes.
        assert!(!evidence.related_tests.is_empty(), "{evidence:?}");
        assert_eq!(evidence.reach.state, StageState::No, "{evidence:?}");
        rows.push((
            seam.expression().to_string(),
            evidence.names_static_limit(LIMIT),
            boundary_input_unresolved_for_uncalled_owner(&seam, &index, owner_fn),
            evidence.activate.clone(),
        ));
    }
    if rows.is_empty() {
        return Err(format!("no predicate seam in {owner}"));
    }
    Ok(rows)
}

#[test]
fn a_computed_operand_names_the_typed_limit_in_the_activate_stage() -> Result<(), String> {
    for (expression, named, shared, activate) in readings(
        "pub fn uncalled(cents: u32) -> u32 {
    if cents % 100 > 49 { cents / 100 + 1 } else { cents / 100 }
}",
    )? {
        assert_eq!(expression, "cents % 100 > 49");
        assert!(named, "{activate:?}");
        assert!(shared, "{activate:?}");
        assert_eq!(activate.state, StageState::Unknown);
        assert!(
            activate
                .summary
                .starts_with("activation_boundary_input_unresolved: Boundary activation operands are local or computed"),
            "{activate:?}"
        );
    }
    Ok(())
}

#[test]
fn boundary_input_limit_agrees_with_the_seam_for_uncalled_owners() -> Result<(), String> {
    // (owner source, whether the activate stage names the limit)
    let cases = [
        (
            "pub fn uncalled(cents: u32) -> u32 {
    if cents % 100 > 49 { 1 } else { 0 }
}",
            true,
        ),
        (
            "pub fn uncalled(cents: u32) -> u32 {
    let rem = cents % 100;
    if rem > 49 { 1 } else { 0 }
}",
            true,
        ),
        // A parameter operand compared with a literal is an ordinary gap.
        (
            "pub fn uncalled(cents: u32) -> u32 {
    if cents > 49 { 1 } else { 0 }
}",
            false,
        ),
        // A parameter compared with a visible same-file constant too.
        (
            "const LIMIT: u32 = 49;
pub fn uncalled(cents: u32) -> u32 {
    if cents > LIMIT { 1 } else { 0 }
}",
            false,
        ),
        // A closure-derived and an iterator-derived operand.
        (
            "pub fn uncalled(values: &[u32]) -> bool {
    let top = || values.len() as u32;
    if top() > 3 { true } else { false }
}",
            true,
        ),
        (
            "pub fn uncalled(values: &[u32]) -> u32 {
    let mut hits = 0;
    for value in values.iter() {
        if *value > 3 { hits += 1; }
    }
    hits
}",
            true,
        ),
        // A field of `self` compared with a literal.
        (
            "pub struct Meter { inner: u32 }
impl Meter {
    pub fn uncalled(&self) -> bool {
        if self.inner > 10 { true } else { false }
    }
}",
            true,
        ),
    ];
    for (owner, expected) in cases {
        for (expression, named, shared, activate) in readings(owner)? {
            assert_eq!(named, expected, "{expression}: {activate:?}");
            assert_eq!(shared, named, "{expression}: {activate:?}");
        }
    }
    Ok(())
}

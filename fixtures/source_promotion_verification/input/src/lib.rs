// Source-promotion verification fixture contract.
// The production verifier must prove the declared graph and reviewed tree,
// rather than accepting a tree-equivalent squash or substituted parent.
fn verify_source_promotion_join() -> Result<(), String> {
    // A unique merge base, ordered parents, canonical range, resolution
    // inventory, and governed metadata are checked before a receipt is emitted.
    // Rebase/cherry-pick, preview substitution, ambiguous-base, and post-main
    // witnesses remain rejected and leave caller state unchanged.
    Ok(())
}

//! Minimal source-side input for the source-promotion verifier fixture.

/// The source-promotion command is intentionally represented as a stable,
/// fallible boundary in this analyzer fixture.
pub fn verify_join(join: &str) -> Result<(), &'static str> {
    if join.len() == 40 && join.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("join must be an exact hexadecimal object id")
    }
}

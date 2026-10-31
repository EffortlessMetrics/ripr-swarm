//! Guards for the in-suite fixture variant builds (#11197).
//!
//! The fixture machinery owns the one `cargo build -p ripr` each xtask
//! process owes the corpus (#3054). Variant builds must never rewrite the
//! shared `target/debug/ripr` the running suite executes: a mid-suite swap
//! lets cross-process artifact tests observe two different analyzer
//! identities in one run.

use super::*;

#[test]
fn perl_variant_build_never_rewrites_shared_debug_binary() -> Result<(), String> {
    // The fixture machinery resolves its relative target paths against the
    // process cwd, and the unit-test binary runs at the xtask package dir
    // while the spawned `cargo build -p ripr` writes the workspace target at
    // the repository root; the fixture-cache controls run their children at
    // the repository root for the same reason.
    with_repo_cwd(|| {
        use crate::reports::{ripr_fixture_binary, ripr_perl_fixture_binary};
        // Materialize the shared default binary exactly as the fixture corpus
        // does, then stamp it before the variant build runs.
        ripr_fixture_binary()?;
        let shared = crate::ripr_debug_binary();
        let before = binary_stamp(&shared)?;
        let perl = ripr_perl_fixture_binary()?;
        assert!(
            std::path::Path::new(&perl).is_file(),
            "perl variant binary must exist at its copy-to path: {perl}"
        );
        let after = binary_stamp(&shared)?;
        assert_eq!(
            before,
            after,
            "in-suite variant build rewrote {} mid-suite; cross-process \
             artifact tests would observe two analyzer identities (#11197)",
            shared.display()
        );
        Ok(())
    })
}

fn binary_stamp(path: &std::path::Path) -> Result<(SystemTime, u64), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("stat {}: {error}", path.display()))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("mtime {}: {error}", path.display()))?;
    Ok((modified, metadata.len()))
}

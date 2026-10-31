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
    // The contract is structural: the variant is built under a target
    // directory that is not the shared one, and is published under its own
    // name, so no step of the variant build names the shared binary. This used
    // to stamp `target/debug/ripr` around a real variant build; that check
    // observed every unrelated in-suite Cargo run too, so it failed on main
    // whenever one overlapped (Test Analytics), and it held the suite for
    // minutes on Cargo's build lock.
    with_repo_cwd(|| {
        use crate::reports::{fixture_variant_target_dir, perl_variant_build_plan};
        let shared = crate::ripr_debug_binary();
        let shared_target = std::path::absolute(
            shared
                .parent()
                .and_then(std::path::Path::parent)
                .ok_or("shared binary has no target root")?,
        )
        .map_err(|error| format!("resolve shared target root: {error}"))?;
        let plan = perl_variant_build_plan(shared.clone(), fixture_variant_target_dir()?);
        assert_eq!(
            plan.cargo_args,
            ["build", "-p", "ripr", "--features", "lang-perl"],
            "the argv stays a plain feature build so native-lock observers match it"
        );
        assert_ne!(
            plan.target_dir, shared_target,
            "the variant must not build into the shared target (#11197)"
        );
        assert!(
            plan.target_dir.starts_with(&shared_target),
            "the variant overlay stays under the shared root so CI caches and \
             `cargo clean` cover it: {} vs {}",
            plan.target_dir.display(),
            shared_target.display()
        );
        let shared_abs = std::path::absolute(&shared)
            .map_err(|error| format!("resolve shared binary: {error}"))?;
        assert!(
            !plan.built.starts_with(shared_target.join("debug")),
            "Cargo must write the variant outside the shared debug dir: {}",
            plan.built.display()
        );
        assert_ne!(std::path::absolute(&plan.published).ok(), Some(shared_abs));
        assert_ne!(
            plan.published.file_name(),
            shared.file_name(),
            "the published copy carries its own name, never the shared one (#11197)"
        );
        assert_eq!(
            plan.published.parent(),
            shared.parent(),
            "the published copy sits beside the shared binary"
        );
        Ok(())
    })
}

//! Cross-file native context must not be inferred from child-file attributes.

use super::*;
use crate::analysis::facts::{
    build_index_from_paths_with_kani_context, reuse_cached_index_with_kani_context,
};
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD: &str = "#[kani::proof]\nfn proof_boundary() { assert!(crate::fits(0, 1)); }\n";
const OWNER: &str = "pub fn fits(n: u32, max: u32) -> bool { n < max }\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        std::fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let path = temporary.join(format!(
            "ripr-kani-parent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let fixture = Self(path);
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname = \"kani-parent-control\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|error| error.to_string())?;
        fixture.write("src/lib.rs", &format!("mod proofs;\n{OWNER}"))?;
        fixture.write("src/proofs.rs", CHILD)?;
        Ok(fixture)
    }

    fn write(&self, path: &str, source: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), source).map_err(|error| error.to_string())
    }

    fn paths() -> Vec<PathBuf> {
        vec!["src/lib.rs".into(), "src/proofs.rs".into()]
    }

    fn index(&self, source: &str) -> Result<RustIndex, String> {
        build_index_from_paths_with_kani_context(
            &self.0,
            &Self::paths(),
            &[],
            Some(&context(source)),
        )
        .map(|cached| cached.index)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn context(source: &str) -> KaniAnalysisContext {
    KaniAnalysisContext {
        profile: KaniAnalysisContext::PROFILE.into(),
        source: source.into(),
        tests: false,
    }
}

fn inventory(index: &RustIndex) -> Result<&KaniSourceInventory, String> {
    index
        .kani_source_inventory
        .as_ref()
        .ok_or_else(|| "Kani source inventory missing".into())
}

fn only_proof(index: &RustIndex) -> Result<&KaniHarnessSource, String> {
    let inventory = inventory(index)?;
    assert_eq!(inventory.harnesses.len(), 1);
    assert_eq!(inventory.native_selection, "unknown");
    assert_eq!(inventory.native_execution, "not_observed");
    assert_eq!(inventory.functional_protection, "not_established");
    inventory.harnesses.first().ok_or_else(|| "proof missing".into())
}

fn assert_parent_unknown(index: &RustIndex) -> Result<(), String> {
    let proof = only_proof(index)?;
    assert_eq!(proof.file, PathBuf::from("src/proofs.rs"));
    assert_eq!(proof.function_name, "proof_boundary");
    assert_eq!(proof.applicability, KaniApplicability::Unknown);
    assert!(
        proof
            .limitations
            .iter()
            .any(|code| code == "native_parent_context_unmodeled")
    );
    assert!(index.tests().is_empty());
    Ok(())
}

#[test]
fn out_of_line_kani_context_never_claims_file_local_enablement() -> Result<(), String> {
    let fixture = Fixture::new()?;
    // Even a true/unconditional external parent needs contextual analysis,
    // which the declared file-local profile does not implement.
    for gate in [
        "#[cfg(kani)]",
        "#[cfg(not(kani))]",
        "#[cfg(feature = \"unresolved\")]",
        "",
    ] {
        fixture.write("src/lib.rs", &format!("{gate}\nmod proofs;\n{OWNER}"))?;
        let index = fixture.index("src/proofs.rs")?;
        assert_parent_unknown(&index)?;
        assert_eq!(
            inventory(&index)?.indexed_source_digest,
            Some(source_digest(CHILD.as_bytes()))
        );
    }
    Ok(())
}

#[test]
fn inline_kani_context_keeps_its_declared_applicability() -> Result<(), String> {
    let fixture = Fixture::new()?;
    for (gate, expected) in [
        ("#[cfg(kani)]", KaniApplicability::Enabled),
        ("#[cfg(not(kani))]", KaniApplicability::Disabled),
        ("#[cfg(feature = \"unresolved\")]", KaniApplicability::Unknown),
    ] {
        fixture.write(
            "src/lib.rs",
            &format!("{OWNER}\n{gate}\nmod local {{\n{CHILD}}}\n"),
        )?;
        let index = fixture.index("src/lib.rs")?;
        let proof = only_proof(&index)?;
        assert_eq!(proof.applicability, expected);
        assert!(
            !proof
                .limitations
                .iter()
                .any(|code| code == "native_parent_context_unmodeled")
        );
    }
    Ok(())
}

#[test]
fn absent_ancestry_is_unknown_but_local_false_cfg_still_disables() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.write("src/lib.rs", &format!("{OWNER}\n{CHILD}"))?;
    let mut index = fixture.index("src/lib.rs")?;
    assert_eq!(only_proof(&index)?.applicability, KaniApplicability::Enabled);
    index.execution_module_parents = None;
    apply_context(&mut index, Some(&context("src/lib.rs")))?;
    let proof = only_proof(&index)?;
    assert_eq!(proof.applicability, KaniApplicability::Unknown);
    assert!(
        proof
            .limitations
            .iter()
            .any(|code| code == "native_parent_context_unmodeled")
    );

    fixture.write("src/lib.rs", &format!("mod proofs;\n{OWNER}"))?;
    fixture.write("src/proofs.rs", &format!("#[cfg(not(kani))]\n{CHILD}"))?;
    let index = fixture.index("src/proofs.rs")?;
    assert_eq!(only_proof(&index)?.applicability, KaniApplicability::Disabled);
    Ok(())
}

#[test]
fn parent_only_edit_rebuilds_without_changing_the_child_digest() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let context = context("src/proofs.rs");
    let cached = build_index_from_paths_with_kani_context(
        &fixture.0,
        &Fixture::paths(),
        &[],
        Some(&context),
    )?;
    assert_parent_unknown(&cached.index)?;
    let old_input = cached.files_content_hash.clone();
    let old_digest = inventory(&cached.index)?.indexed_source_digest.clone();
    let changed_parent = format!("#[cfg(not(kani))]\nmod proofs;\n{OWNER}");
    fixture.write("src/lib.rs", &changed_parent)?;
    let loaded = [
        (PathBuf::from("src/lib.rs"), changed_parent.into_bytes()),
        (PathBuf::from("src/proofs.rs"), CHILD.as_bytes().to_vec()),
    ];
    let cached =
        reuse_cached_index_with_kani_context(cached, &fixture.0, &loaded, &[], Some(&context))?;
    assert_parent_unknown(&cached.index)?;
    assert_ne!(cached.files_content_hash, old_input);
    assert_eq!(inventory(&cached.index)?.indexed_source_digest, old_digest);
    let fresh = fixture.index("src/proofs.rs")?;
    assert_eq!(
        serde_json::to_value(inventory(&cached.index)?).map_err(|error| error.to_string())?,
        serde_json::to_value(inventory(&fresh)?).map_err(|error| error.to_string())?
    );
    Ok(())
}

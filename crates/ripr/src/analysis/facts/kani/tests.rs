use super::*;
use crate::analysis::facts::{
    build_analysis_index_with_kani_context, build_index_from_paths_with_kani_context,
    build_index_with_kani_context, reuse_cached_index_with_kani_context,
};
use crate::analysis::{diff, probes, seam_inventory};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str =
    include_str!("../../../../tests/data/source-role-corpus/cases/kani_source_context/src/lib.rs");
const MANIFEST: &str =
    include_str!("../../../../tests/data/source-role-corpus/cases/kani_source_context/Cargo.toml");

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        std::fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let path = temporary.join(format!(
            "ripr-kani-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let fixture = Self(path);
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("Cargo.toml"), MANIFEST)
            .map_err(|error| error.to_string())?;
        fixture.write(source)?;
        Ok(fixture)
    }

    fn write(&self, source: &str) -> Result<(), String> {
        std::fs::write(self.0.join("src/lib.rs"), source).map_err(|error| error.to_string())
    }

    fn index(&self, context: Option<&KaniAnalysisContext>) -> Result<RustIndex, String> {
        build_index_with_kani_context(&self.0, &[PathBuf::from("src/lib.rs")], &[], context)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn context(tests: bool) -> KaniAnalysisContext {
    KaniAnalysisContext {
        profile: KaniAnalysisContext::PROFILE.into(),
        source: "src/lib.rs".into(),
        tests,
    }
}

fn inventory(index: &RustIndex) -> Result<&KaniSourceInventory, String> {
    index
        .kani_source_inventory
        .as_ref()
        .ok_or_else(|| "native source inventory missing".into())
}

#[test]
fn ordinary_kani_source_has_separate_identity_and_no_test_or_seam_membership() -> Result<(), String>
{
    let fixture = Fixture::new(SOURCE)?;
    let index = fixture.index(Some(&context(false)))?;
    let inventory = inventory(&index)?;
    assert_eq!(inventory.harnesses.len(), 6);
    assert_eq!(inventory.native_selection, "unknown");
    assert_eq!(inventory.native_execution, "not_observed");
    assert_eq!(inventory.functional_protection, "not_established");
    assert_eq!(
        inventory.indexed_source_digest,
        Some(source_digest(SOURCE.as_bytes()))
    );
    assert_eq!(
        index
            .tests()
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        ["ordinary"]
    );
    assert_eq!(
        index
            .functions()
            .iter()
            .find(|function| function.name == "boundary")
            .ok_or("production owner missing")?
            .source_role,
        FunctionSourceRole::Production
    );
    assert_eq!(
        index
            .functions()
            .iter()
            .find(|function| function.name == "local")
            .ok_or("nested function missing")?
            .source_role,
        FunctionSourceRole::KaniProofLocal
    );
    for proof in &inventory.harnesses {
        let fact = index
            .functions()
            .iter()
            .find(|function| function.id == proof.symbol)
            .ok_or("source identity missing")?;
        assert_eq!(fact.source_role, FunctionSourceRole::KaniProofHarness);
        assert!(!fact.source_role.registers_executable_test());
        assert_eq!(
            (fact.start_line, fact.end_line),
            (proof.start_line, proof.end_line)
        );
    }
    let same: Vec<_> = inventory
        .harnesses
        .iter()
        .filter(|harness| harness.function_name == "same")
        .collect();
    assert_eq!(same.len(), 2);
    assert_eq!(same[0].symbol.0, "src/lib.rs::proofs::left::same");
    assert_eq!(same[1].symbol.0, "src/lib.rs::proofs::right::same");
    let seams = seam_inventory::inventory_seams_from_index(&["src/lib.rs".into()], &index);
    assert!(!seams.is_empty());
    assert!(
        seams
            .iter()
            .all(|seam| seam.owner() == "src/lib.rs::boundary")
    );
    let plain = fixture.index(None)?;
    assert!(plain.kani_source_inventory.is_none());
    assert_eq!(plain.tests().len(), 1);
    assert_eq!(
        plain
            .functions()
            .iter()
            .find(|function| function.name == "exact")
            .ok_or("unconfigured proof missing")?
            .source_role,
        FunctionSourceRole::ProofHarness,
        "removing the declared context must retain the unconditional proof-source floor"
    );
    Ok(())
}

#[test]
fn native_cfg_is_declared_and_unknown_atoms_never_become_host_truth() -> Result<(), String> {
    let fixture = Fixture::new(SOURCE)?;
    for tests in [false, true] {
        let index = fixture.index(Some(&context(tests)))?;
        let actual: Vec<_> = inventory(&index)?
            .harnesses
            .iter()
            .map(|harness| (harness.function_name.as_str(), harness.applicability))
            .collect();
        assert_eq!(
            actual,
            [
                ("exact", KaniApplicability::Enabled),
                ("disabled", KaniApplicability::Disabled),
                (
                    "native_test_mode",
                    if tests {
                        KaniApplicability::Enabled
                    } else {
                        KaniApplicability::Disabled
                    }
                ),
                ("feature_unknown", KaniApplicability::Unknown),
                ("same", KaniApplicability::Enabled),
                ("same", KaniApplicability::Enabled),
            ]
        );
        assert_eq!(
            index.tests().len(),
            1,
            "native --tests declaration cannot change Cargo test membership"
        );
    }
    for (predicate, expected) in [
        ("kani", Some(true)),
        ("kani_host", Some(false)),
        ("kani_ra", None),
        ("target_os = \"linux\"", None),
        ("custom", None),
        ("feature = \"test\"", None),
        ("any(kani, custom)", Some(true)),
        ("all(not(kani), custom)", Some(false)),
        ("all(kani, custom)", None),
    ] {
        let attribute = format!("#[cfg({predicate})]");
        assert_eq!(
            cfg_predicates::attribute_kani_availability(&attribute, false),
            expected
        );
    }
    assert_eq!(
        cfg_predicates::attribute_test_build_availability("#[cfg(kani)]"),
        None
    );
    assert_eq!(
        cfg_predicates::attribute_test_build_availability("#[cfg(test)]"),
        Some(true)
    );
    for (predicate, expected) in [
        ("not(kani)", KaniApplicability::Disabled),
        ("not(kani_host)", KaniApplicability::Enabled),
        ("all(kani, not(kani_host))", KaniApplicability::Enabled),
        ("any(not(kani), custom)", KaniApplicability::Unknown),
        ("all(not(kani), custom)", KaniApplicability::Disabled),
        ("feature = \"x\"", KaniApplicability::Unknown),
    ] {
        let source = format!(
            "mod proof_scope {{ #![cfg({predicate})]\n#[kani::proof]\nfn proof() {{}}\n}}\n"
        );
        let fixture = Fixture::new(&source)?;
        let index = fixture.index(Some(&context(false)))?;
        assert_eq!(
            inventory(&index)?.harnesses[0].applicability,
            expected,
            "inner module cfg was lost"
        );
    }
    Ok(())
}

#[test]
fn canonical_proof_cannot_donate_assertions_or_keep_conflicting_test_registration()
-> Result<(), String> {
    let source = "pub fn boundary(value: u8) -> bool { value < 10 }\n#[cfg(test)]\nmod proofs {\n#[kani::proof]\nfn helper() { assert_eq!(super::boundary(1), true); }\n#[test]\nfn caller() { helper(); }\n#[test]\n#[kani::proof]\nfn conflict() { assert!(true); }\n}\n";
    let fixture = Fixture::new(source)?;
    let native = fixture.index(Some(&context(true)))?;
    assert_eq!(native.tests().len(), 1);
    let caller = &native.tests()[0];
    assert_eq!(caller.name, "caller");
    assert!(
        caller.assertions.is_empty(),
        "proof helper assertions entered ordinary test evidence"
    );
    assert!(
        inventory(&native)?
            .harnesses
            .iter()
            .find(|harness| harness.function_name == "conflict")
            .ok_or("conflicting proof missing")?
            .limitations
            .iter()
            .any(|code| code == "ordinary_test_role_withheld")
    );
    let plain = fixture.index(None)?;
    assert_eq!(
        plain.tests().len(),
        2,
        "unconfigured ordinary membership changed"
    );
    Ok(())
}

#[test]
fn source_provider_and_opaque_context_counterexamples_withhold_admission() -> Result<(), String> {
    for (prefix, suffix, code) in [
        ("mod kani {}\n", "", "provider_binding_unknown"),
        ("use other as kani;\n", "", "provider_binding_unknown"),
        ("use other::*;\n", "", "provider_binding_unknown"),
        ("emit_items!();\n", "", "provider_binding_unknown"),
        ("#[inject]\nstruct Other;\n", "", "provider_binding_unknown"),
        (
            "#[derive(Unknown)]\nstruct Other;\n",
            "",
            "provider_binding_unknown",
        ),
        (
            "#[cfg_attr(kani, inject)]\nstruct Other;\n",
            "",
            "provider_binding_unknown",
        ),
        (
            "#[unknown_transform]\nmod m {\n",
            "}\n",
            "opaque_attribute_context",
        ),
        (
            "#[cfg_attr(kani, warn(dead_code))]\n",
            "",
            "conditional_attribute_context_unsupported",
        ),
        (
            "#![cfg_attr(kani, warn(dead_code))]\n",
            "",
            "conditional_attribute_context_unsupported",
        ),
        (
            "const _: () = {\n",
            "};\n",
            "ordinary_free_unit_proof_required",
        ),
        (
            "const _: () = { mod m {\n",
            "} };\n",
            "ordinary_free_unit_proof_required",
        ),
    ] {
        let source = format!("{prefix}#[kani::proof]\nfn proof() {{ assert!(true); }}\n{suffix}");
        let fixture = Fixture::new(&source)?;
        let index = fixture.index(Some(&context(false)))?;
        let inventory = inventory(&index)?;
        assert!(
            inventory.harnesses.is_empty(),
            "lookalike gained native authority: {source}"
        );
        assert!(
            inventory.limitations.iter().any(|limit| limit.code == code),
            "missing refusal {code}: {inventory:?}"
        );
        assert!(index.tests().is_empty());
    }
    let fixture = Fixture::new("#[kani::proof]\nfn proof() { mod kani {} assert!(true); }\n")?;
    assert_eq!(
        inventory(&fixture.index(Some(&context(false)))?)?
            .harnesses
            .len(),
        1,
        "body-local binding does not bind the outer proof attribute"
    );
    let fixture = Fixture::new("#[kani::proof]\nfn proof() { #![cfg(not(kani))] }\n")?;
    let index = fixture.index(Some(&context(false)))?;
    assert!(inventory(&index)?.harnesses.is_empty());
    assert!(
        inventory(&index)?
            .limitations
            .iter()
            .any(|limit| limit.code == "function_inner_attributes_unsupported")
    );
    Ok(())
}

#[test]
fn unsupported_native_forms_stay_limitations_and_modifiers_give_no_credit() -> Result<(), String> {
    for (source, code) in [
        (
            "#[cfg_attr(kani, kani::proof)]\nfn conditional() {}\n",
            "noncanonical_or_conditional_proof_unsupported",
        ),
        (
            "#[kani::proof_for_contract(owner)]\nfn contract() {}\n",
            "noncanonical_or_conditional_proof_unsupported",
        ),
        (
            "#[kani::proof]\nfn generic<T>() {}\n",
            "ordinary_free_unit_proof_required",
        ),
        ("mod external;\n", "out_of_line_module_not_expanded"),
        (
            "mod outer { mod external; }\n",
            "out_of_line_module_not_expanded",
        ),
    ] {
        let fixture = Fixture::new(source)?;
        let index = fixture.index(Some(&context(false)))?;
        assert!(inventory(&index)?.harnesses.is_empty());
        assert!(
            inventory(&index)?
                .limitations
                .iter()
                .any(|limit| limit.code == code)
        );
    }
    let source = "#[kani::proof]\n#[kani::should_panic]\nfn expected_panic() { panic!(); }\n#[kani::proof]\n#[kani::stub(owner, replacement)]\nfn stubbed() {}\n";
    let fixture = Fixture::new(source)?;
    let index = fixture.index(Some(&context(false)))?;
    let inventory = inventory(&index)?;
    assert_eq!(
        inventory
            .harnesses
            .iter()
            .map(|harness| harness.function_name.as_str())
            .collect::<Vec<_>>(),
        ["expected_panic", "stubbed"]
    );
    assert!(
        inventory
            .harnesses
            .iter()
            .all(|harness| harness.limitations == ["native_modifier_semantics_unmodeled"])
    );
    assert!(index.tests().is_empty());
    assert_eq!(inventory.functional_protection, "not_established");
    Ok(())
}

#[test]
fn native_context_cache_rebuilds_on_remove_flip_revert_and_provider_edits() -> Result<(), String> {
    let fixture = Fixture::new(SOURCE)?;
    let mut loaded = vec![(PathBuf::from("src/lib.rs"), SOURCE.as_bytes().to_vec())];
    let mut cached = build_analysis_index_with_kani_context(&fixture.0, &loaded, &[], None)?;
    let false_context = context(false);
    let true_context = context(true);
    let mut absent_context = context(false);
    absent_context.source = "src/absent.rs".into();
    for selected in [
        Some(&false_context),
        Some(&true_context),
        None,
        Some(&absent_context),
        Some(&false_context),
    ] {
        cached = reuse_cached_index_with_kani_context(cached, &fixture.0, &loaded, &[], selected)?;
        let cold = fixture.index(selected)?;
        assert_eq!(
            cached.index.kani_source_inventory,
            cold.kani_source_inventory
        );
        assert_eq!(
            serde_json::to_value(&cached.index).map_err(|error| error.to_string())?,
            serde_json::to_value(&cold).map_err(|error| error.to_string())?
        );
        assert_eq!(
            cached
                .index
                .functions()
                .iter()
                .find(|function| function.name == "exact")
                .ok_or("proof missing after context transition")?
                .source_role,
            if selected.is_some_and(|selected| selected.source == "src/lib.rs") {
                FunctionSourceRole::KaniProofHarness
            } else {
                FunctionSourceRole::ProofHarness
            },
            "context removal or an unindexed configured source must restore the ordinary proof floor"
        );
    }
    let streamed = build_index_from_paths_with_kani_context(
        &fixture.0,
        &["src/lib.rs".into()],
        &[],
        Some(&false_context),
    )?;
    assert_eq!(
        cached.index.kani_source_inventory,
        streamed.index.kani_source_inventory
    );
    let original_digest = inventory(&cached.index)?.indexed_source_digest.clone();
    let shadowed = format!("mod kani {{}}\n{SOURCE}");
    fixture.write(&shadowed)?;
    loaded[0].1 = shadowed.into_bytes();
    cached = reuse_cached_index_with_kani_context(
        cached,
        &fixture.0,
        &loaded,
        &[],
        Some(&false_context),
    )?;
    assert!(inventory(&cached.index)?.harnesses.is_empty());
    assert_ne!(
        inventory(&cached.index)?.indexed_source_digest,
        original_digest
    );
    fixture.write(SOURCE)?;
    loaded[0].1 = SOURCE.as_bytes().to_vec();
    cached = reuse_cached_index_with_kani_context(
        cached,
        &fixture.0,
        &loaded,
        &[],
        Some(&false_context),
    )?;
    assert_eq!(
        inventory(&cached.index)?.indexed_source_digest,
        original_digest
    );
    assert_eq!(inventory(&cached.index)?.harnesses.len(), 6);
    Ok(())
}

#[test]
fn proof_only_edits_and_removed_proofs_never_seed_production_probes() -> Result<(), String> {
    for (source, patch, empty) in [
        (
            "#[kani::proof]\nfn proof() {\n    assert!(2 < 3);\n}\n",
            "@@ -3 +3 @@\n-    assert!(1 < 3);\n+    assert!(2 < 3);\n",
            true,
        ),
        (
            "pub fn boundary(x: u8) -> bool { x < 10 }\n",
            "@@ -1,5 +1 @@\n-#[kani::proof]\n-fn proof() {\n-    assert!(1 < 3);\n-}\n pub fn boundary(x: u8) -> bool { x < 10 }\n",
            true,
        ),
        (
            "#[kani::proof]\nfn proof() {}\npub fn boundary(x: u8) -> bool { x < 10 }\n",
            "@@ -1,3 +1,3 @@\n #[kani::proof]\n fn proof() {}\n-pub fn boundary(x: u8) -> bool { x < 9 }\n+pub fn boundary(x: u8) -> bool { x < 10 }\n",
            false,
        ),
    ] {
        let fixture = Fixture::new(source)?;
        let index = fixture.index(Some(&context(false)))?;
        let changes = diff::parse_unified_diff(&format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n{patch}"
        ));
        let change = changes.first().ok_or("diff fixture missing")?;
        let probes = probes::probes_for_file_with_relations(&fixture.0, change, &index, &[]);
        assert_eq!(
            probes.is_empty(),
            empty,
            "wrong source role on {patch}: {probes:?}"
        );
    }
    Ok(())
}

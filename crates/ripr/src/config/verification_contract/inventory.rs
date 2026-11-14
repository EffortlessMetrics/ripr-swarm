//! Checked inventory of verification declaration sources and field owners (#6428).

use super::model::VerificationContractSourceClass;

/// One row in the governed source inventory: class, semantic owner, and owned
/// declaration domains for VP29A.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerificationSourceInventoryRow {
    pub class: VerificationContractSourceClass,
    pub owner: &'static str,
    pub domains: &'static [&'static str],
    /// Whether VP29A compilation consumes this source today.
    pub compiled_in_vp29a: bool,
}

/// Every admitted verification declaration source and its field owner (#6428).
pub(crate) const VERIFICATION_SOURCE_INVENTORY: &[VerificationSourceInventoryRow] = &[
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::RiprBuiltinPolicy,
        owner: "product built-in verification policy",
        domains: &["safety_floors", "claim_floors"],
        compiled_in_vp29a: true,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::TrackedRiprConfiguration,
        owner: "config loader (`crates/ripr/src/config.rs`)",
        domains: &[
            "analysis.test_harnesses",
            "languages",
            "oracles",
            "severity",
        ],
        compiled_in_vp29a: true,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::CargoManifestOrMetadata,
        owner: "Cargo manifest/metadata producers",
        domains: &["packages", "targets", "features"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::NextestOrRunnerConfiguration,
        owner: "nextest/runner configuration producers",
        domains: &["groups", "profiles", "retries", "filters"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::RegisteredHarnessManifest,
        owner: "#3532 harness registry (`analysis.test_harnesses`)",
        domains: &["custom_harness", "registered_attribute"],
        compiled_in_vp29a: true,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::RepositoryProofOrProductGateRegistry,
        owner: "#5198 repository proof registry",
        domains: &["proof_propositions", "roles", "fallbacks"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::SpecializedProofRegistry,
        owner: "#5221 specialized proof registry",
        domains: &["families", "phases", "denominators"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::ExecutionContextRegistry,
        owner: "#5222 execution-context registry",
        domains: &["context_requirements"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::ProviderCapabilityRegistry,
        owner: "provider/cargo-proof capability registries (#3300)",
        domains: &["provider_ids", "capabilities"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::CommandCatalog,
        owner: "#1617 typed command registry",
        domains: &["command_specs"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::ValidatedGeneratedManifest,
        owner: "registered generated-manifest producers",
        domains: &["generated_artifacts"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::LocalOperatorRestriction,
        owner: "local operator execution policy (non-semantic)",
        domains: &["execution_denials"],
        compiled_in_vp29a: false,
    },
    VerificationSourceInventoryRow {
        class: VerificationContractSourceClass::RuntimeObservationReference,
        owner: "runtime observation references (non-authoritative)",
        domains: &["observations"],
        compiled_in_vp29a: false,
    },
];

const _: () =
    assert!(VERIFICATION_SOURCE_INVENTORY.len() == VerificationContractSourceClass::ALL.len());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_lists_every_admitted_source_class_once() {
        let mut seen = std::collections::BTreeSet::new();
        for row in VERIFICATION_SOURCE_INVENTORY {
            assert!(
                seen.insert(row.class),
                "duplicate inventory row for {:?}",
                row.class
            );
            assert!(!row.owner.is_empty());
            assert!(!row.domains.is_empty());
        }
        assert_eq!(
            seen.len(),
            VerificationContractSourceClass::ALL.len(),
            "inventory must cover every closed source class"
        );
    }
}

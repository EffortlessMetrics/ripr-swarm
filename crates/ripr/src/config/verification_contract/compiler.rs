//! Pure repository verification contract compiler (#6428).

use super::inventory::VERIFICATION_SOURCE_INVENTORY;
use super::model::{
    REPOSITORY_VERIFICATION_CONTRACT_COMPILER_GENERATION,
    REPOSITORY_VERIFICATION_CONTRACT_SCHEMA_VERSION, RepositoryVerificationAuthorityMapV1,
    RepositoryVerificationContractDecisionV1, RepositoryVerificationContractSourceV1,
    RepositoryVerificationContractV1, RepositoryVerificationDeclarationRefV1,
    VerificationContractConflictV1, VerificationContractDecisionState,
    VerificationContractSourceClass,
};
use crate::config::{RiprConfig, TestHarnessRegistration};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

const SHA256_PREFIX: &str = "sha256:";

/// Inputs for passive contract compilation. No Cargo or network I/O.
pub(crate) struct VerificationContractCompileInput<'a> {
    pub repository_root: &'a Path,
    /// Workspace-relative path to tracked configuration when present.
    pub ripr_toml_path: Option<&'a Path>,
    /// Digest of tracked configuration bytes when loaded.
    pub ripr_toml_content_digest: Option<String>,
    pub config: &'a RiprConfig,
}

/// Compile admitted VP29A sources into one deterministic contract.
pub(crate) fn compile_repository_verification_contract(
    input: VerificationContractCompileInput<'_>,
) -> RepositoryVerificationContractDecisionV1 {
    let mut sources = Vec::new();
    let mut declarations = Vec::new();
    let mut conflicts = Vec::new();
    let mut missing_registrations = Vec::new();
    let mut authority_map = RepositoryVerificationAuthorityMapV1::default();

    for row in VERIFICATION_SOURCE_INVENTORY {
        for domain in row.domains {
            authority_map
                .by_domain
                .entry((*domain).to_string())
                .or_insert_with(|| row.owner.to_string());
        }
        if !row.compiled_in_vp29a {
            missing_registrations.push(format!(
                "{}: source class `{}` not yet consumed by VP29A compiler",
                row.owner,
                row.class.as_str()
            ));
        }
    }

    sources.push(builtin_policy_source());
    if let Some(tracked) = tracked_configuration_source(&input) {
        sources.push(tracked);
    } else {
        missing_registrations.push(
            "tracked ripr.toml configuration: no content digest supplied for contract generation"
                .to_string(),
        );
    }

    let harness_source_id = VerificationContractSourceClass::RegisteredHarnessManifest.as_str();
    sources.push(RepositoryVerificationContractSourceV1 {
        source_id: harness_source_id.to_string(),
        class: VerificationContractSourceClass::RegisteredHarnessManifest,
        schema_generation: "3532-v1".to_string(),
        location: "ripr.toml [analysis.test_harnesses]".to_string(),
        trust_class: "tracked_repository_configuration".to_string(),
        owned_domains: vec![
            "custom_harness".to_string(),
            "registered_attribute".to_string(),
        ],
        content_digest: harness_registry_digest(input.config),
        limitations: Vec::new(),
    });

    let mut seen_declaration_ids = BTreeMap::<String, String>::new();
    for (index, registration) in input.config.analysis().test_harnesses().iter().enumerate() {
        let declaration_id = format!("harness:{}", registration.registration_id);
        let exact_location = format!("analysis.test_harnesses[{index}]");
        let semantic_fields_digest = harness_declaration_digest(registration);
        if let Some(first_location) = seen_declaration_ids.get(&declaration_id) {
            conflicts.push(VerificationContractConflictV1 {
                code: "duplicate_declaration_id".to_string(),
                message: format!(
                    "declaration `{declaration_id}` appears more than once with the same semantic ID"
                ),
                source_ids: vec![harness_source_id.to_string()],
                repair_locations: vec![first_location.clone(), exact_location.clone()],
            });
        } else {
            seen_declaration_ids.insert(declaration_id.clone(), exact_location.clone());
        }
        declarations.push(RepositoryVerificationDeclarationRefV1 {
            declaration_id,
            kind: registration.kind.as_str().to_string(),
            owner_registry: "#3532 harness registry".to_string(),
            source_id: harness_source_id.to_string(),
            exact_location,
            semantic_fields_digest,
        });
    }

    let repository_root = normalize_repo_relative(input.repository_root);
    let config_identity_digest = input
        .ripr_toml_content_digest
        .clone()
        .unwrap_or_else(|| "missing:tracked_configuration".to_string());

    let contract_without_digest = RepositoryVerificationContractV1 {
        schema_version: REPOSITORY_VERIFICATION_CONTRACT_SCHEMA_VERSION.to_string(),
        compiler_generation: REPOSITORY_VERIFICATION_CONTRACT_COMPILER_GENERATION.to_string(),
        repository_root: repository_root.clone(),
        config_identity_digest: config_identity_digest.clone(),
        sources: sources.clone(),
        declarations: declarations.clone(),
        authority_map: authority_map.clone(),
        conflicts: conflicts.clone(),
        missing_registrations: missing_registrations.clone(),
        semantic_digest: String::new(),
    };
    let semantic_digest = semantic_contract_digest(&contract_without_digest);
    let contract = RepositoryVerificationContractV1 {
        semantic_digest: semantic_digest.clone(),
        ..contract_without_digest
    };

    let state = if !conflicts.is_empty() {
        VerificationContractDecisionState::Conflicting
    } else if missing_registrations.is_empty() {
        VerificationContractDecisionState::Complete
    } else {
        VerificationContractDecisionState::Partial
    };

    RepositoryVerificationContractDecisionV1 {
        state,
        contract,
        rejected_stronger_states: Vec::new(),
    }
}

fn builtin_policy_source() -> RepositoryVerificationContractSourceV1 {
    RepositoryVerificationContractSourceV1 {
        source_id: VerificationContractSourceClass::RiprBuiltinPolicy
            .as_str()
            .to_string(),
        class: VerificationContractSourceClass::RiprBuiltinPolicy,
        schema_generation: REPOSITORY_VERIFICATION_CONTRACT_COMPILER_GENERATION.to_string(),
        location: "ripr built-in verification policy".to_string(),
        trust_class: "product_builtin".to_string(),
        owned_domains: vec!["safety_floors".to_string(), "claim_floors".to_string()],
        content_digest: digest_text(REPOSITORY_VERIFICATION_CONTRACT_COMPILER_GENERATION),
        limitations: vec![
            "VP29A: built-in floors are referenced, not expanded, in this compiler slice"
                .to_string(),
        ],
    }
}

fn tracked_configuration_source(
    input: &VerificationContractCompileInput,
) -> Option<RepositoryVerificationContractSourceV1> {
    let digest = input.ripr_toml_content_digest.as_ref()?;
    let location = input
        .ripr_toml_path
        .as_ref()
        .map(|path| normalize_repo_relative(path))
        .unwrap_or_else(|| "ripr.toml".to_string());
    Some(RepositoryVerificationContractSourceV1 {
        source_id: VerificationContractSourceClass::TrackedRiprConfiguration
            .as_str()
            .to_string(),
        class: VerificationContractSourceClass::TrackedRiprConfiguration,
        schema_generation: "ripr-config-v1".to_string(),
        location,
        trust_class: "tracked_repository_configuration".to_string(),
        owned_domains: vec![
            "analysis.test_harnesses".to_string(),
            "languages".to_string(),
            "oracles".to_string(),
            "severity".to_string(),
        ],
        content_digest: digest.clone(),
        limitations: Vec::new(),
    })
}

fn harness_registry_digest(config: &RiprConfig) -> String {
    let mut lines = Vec::new();
    for registration in config.analysis().test_harnesses() {
        lines.push(harness_identity_line(registration));
    }
    digest_text(&lines.join("\n"))
}

fn harness_declaration_digest(registration: &TestHarnessRegistration) -> String {
    digest_text(&harness_identity_line(registration))
}

fn harness_identity_line(registration: &TestHarnessRegistration) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        registration.registration_id,
        registration.target.to_string_lossy().replace('\\', "/"),
        registration.kind.as_str(),
        registration.adapter.as_str(),
        registration.marker
    )
}

fn semantic_contract_digest(contract: &RepositoryVerificationContractV1) -> String {
    let mut payload = String::new();
    payload.push_str(&contract.schema_version);
    payload.push('\n');
    payload.push_str(&contract.compiler_generation);
    payload.push('\n');
    payload.push_str(&contract.repository_root);
    payload.push('\n');
    payload.push_str(&contract.config_identity_digest);
    payload.push('\n');
    for source in &contract.sources {
        payload.push_str(&source.source_id);
        payload.push('|');
        payload.push_str(&source.content_digest);
        payload.push('\n');
    }
    for declaration in &contract.declarations {
        payload.push_str(&declaration.declaration_id);
        payload.push('|');
        payload.push_str(&declaration.semantic_fields_digest);
        payload.push('\n');
    }
    for conflict in &contract.conflicts {
        payload.push_str(&conflict.code);
        payload.push('|');
        payload.push_str(&conflict.message);
        payload.push('\n');
    }
    digest_text(&payload)
}

fn digest_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{SHA256_PREFIX}{:x}", hasher.finalize())
}

fn normalize_repo_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TestHarnessAdapter, TestHarnessKind, TestHarnessRegistration};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn sample_registration(id: &str) -> TestHarnessRegistration {
        TestHarnessRegistration {
            registration_id: id.to_string(),
            target: PathBuf::from("tests/mimic.rs"),
            kind: TestHarnessKind::CustomHarnessTarget,
            adapter: TestHarnessAdapter::LibtestMimicV1,
            marker: "libtest_mimic".to_string(),
        }
    }

    fn sample_input(config: &RiprConfig) -> VerificationContractCompileInput<'_> {
        VerificationContractCompileInput {
            repository_root: Path::new("."),
            ripr_toml_path: Some(Path::new("ripr.toml")),
            ripr_toml_content_digest: Some(digest_text("[[analysis.test_harnesses]]\n")),
            config,
        }
    }

    #[test]
    fn compiles_harness_registration_with_identity_preserving_digest() {
        let mut config = RiprConfig::default();
        config
            .analysis
            .test_harnesses
            .push(sample_registration("mimic-suite"));
        let first = compile_repository_verification_contract(sample_input(&config));
        let second = compile_repository_verification_contract(sample_input(&config));
        assert_eq!(
            first.contract.semantic_digest,
            second.contract.semantic_digest
        );
        assert_eq!(first.contract.declarations.len(), 1);
        assert_eq!(
            first.contract.declarations[0].declaration_id,
            "harness:mimic-suite"
        );
        assert_eq!(
            first.contract.declarations[0].semantic_fields_digest,
            harness_declaration_digest(&sample_registration("mimic-suite"))
        );
        assert_eq!(
            first.contract.declarations[0].exact_location,
            "analysis.test_harnesses[0]"
        );
        assert_eq!(first.state, VerificationContractDecisionState::Partial);
        assert!(
            first
                .contract
                .missing_registrations
                .iter()
                .any(|row| row.contains("Cargo manifest")),
            "expected VP29A partial decision to name not-yet-consumed sources"
        );
    }

    #[test]
    fn duplicate_registration_ids_surface_as_conflicts() {
        let mut config = RiprConfig::default();
        config
            .analysis
            .test_harnesses
            .push(sample_registration("dup"));
        config
            .analysis
            .test_harnesses
            .push(sample_registration("dup"));
        let decision = compile_repository_verification_contract(sample_input(&config));
        assert_eq!(
            decision.state,
            VerificationContractDecisionState::Conflicting
        );
        assert!(
            decision
                .contract
                .conflicts
                .iter()
                .any(|conflict| conflict.code == "duplicate_declaration_id"),
            "duplicate semantic IDs must fail closed with both locations"
        );
    }

    #[test]
    fn inventory_covers_builtin_and_tracked_sources_in_contract() {
        let default_config = RiprConfig::default();
        let decision = compile_repository_verification_contract(sample_input(&default_config));
        let classes = decision
            .contract
            .sources
            .iter()
            .map(|source| source.class)
            .collect::<BTreeSet<_>>();
        assert!(classes.contains(&VerificationContractSourceClass::RiprBuiltinPolicy));
        assert!(classes.contains(&VerificationContractSourceClass::TrackedRiprConfiguration));
        assert!(classes.contains(&VerificationContractSourceClass::RegisteredHarnessManifest));
    }
}

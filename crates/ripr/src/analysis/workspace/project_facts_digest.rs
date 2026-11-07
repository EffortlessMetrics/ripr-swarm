//! Canonical semantic digest for shared project facts (#6423). JSON encoding
//! stays in analysis so domain DTOs remain free of `serde_json`.

use std::collections::BTreeSet;

use crate::config;
use crate::domain::{
    FactCurrentnessV1, PROJECT_FACTS_SCHEMA_VERSION, ProjectFactsV1, SourceUnitFactV1,
    TestEnvironmentFactV1,
};

/// Sort, dedupe, and seal `ProjectFactsV1` with a portable semantic digest.
pub(crate) fn seal_project_facts(mut facts: ProjectFactsV1) -> Result<ProjectFactsV1, String> {
    facts.schema_version = PROJECT_FACTS_SCHEMA_VERSION.to_string();
    facts.package_ids.sort();
    facts.package_ids.dedup();
    facts
        .source_units
        .sort_by(|left, right| left.path.cmp(&right.path));
    facts
        .test_environments
        .sort_by(|left, right| left.environment_id.cmp(&right.environment_id));
    facts.semantic_digest = project_facts_semantic_digest(&facts)?;
    Ok(facts)
}

pub(crate) fn project_facts_semantic_digest(facts: &ProjectFactsV1) -> Result<String, String> {
    #[derive(serde::Serialize)]
    struct DigestInput<'a> {
        schema_version: &'a str,
        language: &'a str,
        repository_identity: &'a str,
        package_ids: &'a [String],
        source_units: &'a [SourceUnitFactV1],
        test_environments: &'a [TestEnvironmentFactV1],
        config_input_keys: &'a BTreeSet<String>,
        currentness: &'a FactCurrentnessV1,
        limitations: &'a [String],
    }
    let input = DigestInput {
        schema_version: &facts.schema_version,
        language: &facts.language,
        repository_identity: &facts.repository_identity,
        package_ids: &facts.package_ids,
        source_units: &facts.source_units,
        test_environments: &facts.test_environments,
        config_input_keys: &facts.config_input_keys,
        currentness: &facts.currentness,
        limitations: &facts.limitations,
    };
    let json = serde_json::to_vec(&input).map_err(|error| {
        format!(
            "project facts digest serialization failed: {error}; report this as a ripr bug with the workspace command that produced it"
        )
    })?;
    Ok(config::bytes_fingerprint(&json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CanonicalSourceRole;

    fn sample_unit(path: &str, role: CanonicalSourceRole) -> SourceUnitFactV1 {
        SourceUnitFactV1 {
            path: path.to_string(),
            package_id: "pkg".to_string(),
            role,
            native_role: None,
            basis: "layout".to_string(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
        }
    }

    #[test]
    fn semantic_digest_ignores_host_specific_repository_root_cwd_when_identity_matches()
    -> Result<(), String> {
        let mut left = ProjectFactsV1 {
            schema_version: PROJECT_FACTS_SCHEMA_VERSION.to_string(),
            language: "rust".to_string(),
            repository_identity: "layout:single-package".to_string(),
            repository_root_cwd: ".".to_string(),
            package_ids: vec!["pkg".to_string()],
            source_units: vec![sample_unit("src/lib.rs", CanonicalSourceRole::Production)],
            test_environments: Vec::new(),
            config_input_keys: BTreeSet::new(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
            semantic_digest: String::new(),
        };
        left = seal_project_facts(left)?;
        let mut right = left.clone();
        right.repository_root_cwd = "nested/checkout/root".to_string();
        right.semantic_digest.clear();
        right = seal_project_facts(right)?;
        assert_eq!(left.semantic_digest, right.semantic_digest);
        Ok(())
    }

    #[test]
    fn semantic_digest_changes_when_source_role_changes() -> Result<(), String> {
        let sealed = seal_project_facts(ProjectFactsV1 {
            schema_version: PROJECT_FACTS_SCHEMA_VERSION.to_string(),
            language: "rust".to_string(),
            repository_identity: "layout:single-package".to_string(),
            repository_root_cwd: ".".to_string(),
            package_ids: vec!["pkg".to_string()],
            source_units: vec![sample_unit("src/lib.rs", CanonicalSourceRole::Production)],
            test_environments: Vec::new(),
            config_input_keys: BTreeSet::new(),
            currentness: FactCurrentnessV1 {
                input_keys: BTreeSet::from(["workspace_manifests".to_string()]),
            },
            limitations: Vec::new(),
            semantic_digest: String::new(),
        })?;
        let mut wrong = sealed.clone();
        wrong.source_units[0].role = CanonicalSourceRole::Test;
        wrong.semantic_digest.clear();
        wrong = seal_project_facts(wrong)?;
        assert_ne!(sealed.semantic_digest, wrong.semantic_digest);
        Ok(())
    }
}

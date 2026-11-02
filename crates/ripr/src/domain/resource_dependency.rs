//! Non-code resource dependency edges for verification planning (#5171).

use crate::domain::SymbolId;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Closed resource-kind vocabulary for producer-owned edges.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKindV1 {
    CompileTimeEmbedded,
    RuntimeLiteralRead,
    FixtureOrTestdata,
    GoldenOrSnapshot,
    SchemaOrGrammar,
    TemplateOrGeneratedInput,
    DeclaredHarnessResource,
    ConfigurationInput,
    OpaqueOrDynamicResource,
}

impl ResourceKindV1 {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CompileTimeEmbedded => "compile_time_embedded",
            Self::RuntimeLiteralRead => "runtime_literal_read",
            Self::FixtureOrTestdata => "fixture_or_testdata",
            Self::GoldenOrSnapshot => "golden_or_snapshot",
            Self::SchemaOrGrammar => "schema_or_grammar",
            Self::TemplateOrGeneratedInput => "template_or_generated_input",
            Self::DeclaredHarnessResource => "declared_harness_resource",
            Self::ConfigurationInput => "configuration_input",
            Self::OpaqueOrDynamicResource => "opaque_or_dynamic_resource",
        }
    }
}

/// How a consumer names or resolves a repository resource.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccessKindV1 {
    ExactLiteralRepositoryPath,
    ExactMacroOrHarnessBinding,
    ExactDeclaredManifestBinding,
    ConservativeDirectoryOrPatternBinding,
    DynamicOrUnresolved,
}

impl ResourceAccessKindV1 {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ExactLiteralRepositoryPath => "exact_literal_repository_path",
            Self::ExactMacroOrHarnessBinding => "exact_macro_or_harness_binding",
            Self::ExactDeclaredManifestBinding => "exact_declared_manifest_binding",
            Self::ConservativeDirectoryOrPatternBinding => {
                "conservative_directory_or_pattern_binding"
            }
            Self::DynamicOrUnresolved => "dynamic_or_unresolved",
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePresenceV1 {
    Present,
    Absent,
    Unknown,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCompletenessClassV1 {
    Exact,
    Conservative,
    Opaque,
}

/// One producer-owned resource dependency edge.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResourceDependencyEdgeV1 {
    pub edge_id: String,
    pub semantic_digest: String,
    pub resource_kind: ResourceKindV1,
    pub resource_path: PathBuf,
    pub content_identity: String,
    pub consumer_id: SymbolId,
    pub consumer_file: PathBuf,
    pub consumer_line: usize,
    pub access_kind: ResourceAccessKindV1,
    pub resolution_basis: String,
    pub base_presence: ResourcePresenceV1,
    pub head_presence: ResourcePresenceV1,
    pub completeness_class: ResourceCompletenessClassV1,
    pub first_unresolved_edge: Option<String>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
}

impl ResourceDependencyEdgeV1 {
    pub(crate) fn mint_semantic_digest(
        resource_kind: ResourceKindV1,
        resource_path: &Path,
        consumer_id: &SymbolId,
        access_kind: ResourceAccessKindV1,
        completeness_class: ResourceCompletenessClassV1,
    ) -> String {
        let mut canonical = String::new();
        canonical.push_str(resource_kind.as_str());
        canonical.push('\0');
        canonical.push_str(&resource_path.to_string_lossy().replace('\\', "/"));
        canonical.push('\0');
        canonical.push_str(&consumer_id.0);
        canonical.push('\0');
        canonical.push_str(access_kind.as_str());
        canonical.push('\0');
        canonical.push_str(match completeness_class {
            ResourceCompletenessClassV1::Exact => "exact",
            ResourceCompletenessClassV1::Conservative => "conservative",
            ResourceCompletenessClassV1::Opaque => "opaque",
        });
        format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()))
    }

    pub(crate) fn mint_edge_id(semantic_digest: &str) -> String {
        format!("resource-edge:{semantic_digest}")
    }
}

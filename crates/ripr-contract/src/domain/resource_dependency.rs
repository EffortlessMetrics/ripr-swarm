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
    /// Embedded at compile time, such as with `include_str!` or
    /// `include_bytes!`.
    CompileTimeEmbedded,
    /// Read at run time from a literal path.
    RuntimeLiteralRead,
    /// A test fixture or test data file.
    FixtureOrTestdata,
    /// A golden or snapshot file.
    GoldenOrSnapshot,
    /// A schema or grammar file.
    SchemaOrGrammar,
    /// A template or an input to generated code.
    TemplateOrGeneratedInput,
    /// A resource a test harness declares.
    DeclaredHarnessResource,
    /// A configuration file.
    ConfigurationInput,
    /// A resource whose path could not be resolved statically.
    OpaqueOrDynamicResource,
}

impl ResourceKindV1 {
    /// The stable snake_case label used in output.
    pub fn as_str(self) -> &'static str {
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
    /// A literal repository path.
    ExactLiteralRepositoryPath,
    /// An exact binding through a macro or test harness.
    ExactMacroOrHarnessBinding,
    /// An exact binding declared in a manifest.
    ExactDeclaredManifestBinding,
    /// A directory or pattern that may cover more files than are used.
    ConservativeDirectoryOrPatternBinding,
    /// A dynamic or unresolved reference.
    DynamicOrUnresolved,
}

impl ResourceAccessKindV1 {
    /// The stable snake_case label used in output.
    pub fn as_str(self) -> &'static str {
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

/// Whether a resource exists in one snapshot.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePresenceV1 {
    /// The resource exists.
    Present,
    /// The resource does not exist.
    Absent,
    /// Presence could not be determined.
    Unknown,
}

/// How completely an edge names the resources a consumer depends on.
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCompletenessClassV1 {
    /// The edge names the exact resource.
    Exact,
    /// The edge may cover more resources than are used.
    Conservative,
    /// The resource could not be resolved.
    Opaque,
}

/// One producer-owned resource dependency edge.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResourceDependencyEdgeV1 {
    /// Stable id for the edge, minted from [`Self::semantic_digest`].
    pub edge_id: String,
    /// Digest of the edge's kind, path, consumer, access kind and completeness.
    /// See [`Self::mint_semantic_digest`].
    pub semantic_digest: String,
    /// What kind of resource this is.
    pub resource_kind: ResourceKindV1,
    /// Repository-relative path of the resource.
    pub resource_path: PathBuf,
    /// Content digest of the resource, or a marker when it is missing or
    /// unresolved.
    pub content_identity: String,
    /// The symbol that depends on the resource.
    pub consumer_id: SymbolId,
    /// The file that contains the reference.
    pub consumer_file: PathBuf,
    /// The line of the reference in [`Self::consumer_file`].
    pub consumer_line: usize,
    /// How the consumer names the resource.
    pub access_kind: ResourceAccessKindV1,
    /// Short code naming the rule that resolved the edge, such as
    /// `include_str_literal`.
    pub resolution_basis: String,
    /// Whether the resource exists in the base snapshot.
    pub base_presence: ResourcePresenceV1,
    /// Whether the resource exists in the head snapshot.
    pub head_presence: ResourcePresenceV1,
    /// How completely the edge names the resource.
    pub completeness_class: ResourceCompletenessClassV1,
    /// For an unresolved edge, the reason it could not be resolved.
    pub first_unresolved_edge: Option<String>,
    /// Limitations that apply to this edge.
    pub limitations: Vec<String>,
    /// What the edge does not claim, such as `may_depend_only` or
    /// `no_test_adequacy`.
    pub non_claims: Vec<String>,
}

impl ResourceDependencyEdgeV1 {
    /// Digest the edge's identity as a `sha256:` string. Path separators are
    /// normalized to `/`; content and presence are not part of the digest.
    pub fn mint_semantic_digest(
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

    /// The edge id for a semantic digest.
    pub fn mint_edge_id(semantic_digest: &str) -> String {
        format!("resource-edge:{semantic_digest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(
        kind: ResourceKindV1,
        path: &str,
        consumer: &str,
        access: ResourceAccessKindV1,
        completeness: ResourceCompletenessClassV1,
    ) -> String {
        ResourceDependencyEdgeV1::mint_semantic_digest(
            kind,
            Path::new(path),
            &SymbolId(consumer.to_string()),
            access,
            completeness,
        )
    }

    fn sha256(canonical: &str) -> String {
        format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()))
    }

    #[test]
    fn resource_and_access_tokens_are_the_snake_case_variant_names() {
        assert_eq!(
            [
                ResourceKindV1::CompileTimeEmbedded,
                ResourceKindV1::RuntimeLiteralRead,
                ResourceKindV1::FixtureOrTestdata,
                ResourceKindV1::GoldenOrSnapshot,
                ResourceKindV1::SchemaOrGrammar,
                ResourceKindV1::TemplateOrGeneratedInput,
                ResourceKindV1::DeclaredHarnessResource,
                ResourceKindV1::ConfigurationInput,
                ResourceKindV1::OpaqueOrDynamicResource,
            ]
            .map(ResourceKindV1::as_str),
            [
                "compile_time_embedded",
                "runtime_literal_read",
                "fixture_or_testdata",
                "golden_or_snapshot",
                "schema_or_grammar",
                "template_or_generated_input",
                "declared_harness_resource",
                "configuration_input",
                "opaque_or_dynamic_resource",
            ]
        );
        assert_eq!(
            [
                ResourceAccessKindV1::ExactLiteralRepositoryPath,
                ResourceAccessKindV1::ExactMacroOrHarnessBinding,
                ResourceAccessKindV1::ExactDeclaredManifestBinding,
                ResourceAccessKindV1::ConservativeDirectoryOrPatternBinding,
                ResourceAccessKindV1::DynamicOrUnresolved,
            ]
            .map(ResourceAccessKindV1::as_str),
            [
                "exact_literal_repository_path",
                "exact_macro_or_harness_binding",
                "exact_declared_manifest_binding",
                "conservative_directory_or_pattern_binding",
                "dynamic_or_unresolved",
            ]
        );
    }

    #[test]
    fn the_semantic_digest_hashes_nul_separated_fields_with_forward_slashes() {
        let minted = digest(
            ResourceKindV1::FixtureOrTestdata,
            "tests\\data/input.json",
            "crate::load",
            ResourceAccessKindV1::ExactLiteralRepositoryPath,
            ResourceCompletenessClassV1::Conservative,
        );
        assert_eq!(
            minted,
            sha256(
                "fixture_or_testdata\0tests/data/input.json\0crate::load\0exact_literal_repository_path\0conservative"
            )
        );
        assert_eq!(
            minted,
            digest(
                ResourceKindV1::FixtureOrTestdata,
                "tests/data/input.json",
                "crate::load",
                ResourceAccessKindV1::ExactLiteralRepositoryPath,
                ResourceCompletenessClassV1::Conservative,
            )
        );
        for (completeness, token) in [
            (ResourceCompletenessClassV1::Exact, "exact"),
            (ResourceCompletenessClassV1::Opaque, "opaque"),
        ] {
            assert_eq!(
                digest(
                    ResourceKindV1::GoldenOrSnapshot,
                    "a",
                    "b",
                    ResourceAccessKindV1::DynamicOrUnresolved,
                    completeness,
                ),
                sha256(&format!(
                    "golden_or_snapshot\0a\0b\0dynamic_or_unresolved\0{token}"
                ))
            );
        }
    }

    #[test]
    fn the_edge_id_prefixes_the_semantic_digest() {
        assert_eq!(
            ResourceDependencyEdgeV1::mint_edge_id("sha256:abc"),
            "resource-edge:sha256:abc"
        );
    }
}

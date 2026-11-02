//! Producer-owned non-code resource dependency edges (#5171).

use super::RustIndex;
use super::includes::repository_relative_target;
use crate::analysis::stable_path_text;
use crate::analysis::syntax::ra::{
    RustEmbedResourceDirective, RustEmbedResourceMacro, rust_embed_resource_directives,
};
use crate::domain::{
    ResourceAccessKindV1, ResourceCompletenessClassV1, ResourceDependencyEdgeV1, ResourceKindV1,
    ResourcePresenceV1, SymbolId,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_RESOURCE_EDGES: usize = 4096;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourceDependencyFacts {
    pub(crate) edges: Vec<ResourceDependencyEdgeV1>,
    pub(crate) limitations: Vec<ResourceDependencyLimitation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceDependencyLimitation {
    pub(crate) consumer_file: PathBuf,
    pub(crate) line: usize,
    pub(crate) expression: String,
    pub(crate) reason_code: String,
}

/// Collect resource dependency edges from an indexed Rust workspace snapshot.
pub(crate) fn collect_resource_dependency_edges(
    root: &Path,
    index: &RustIndex,
) -> ResourceDependencyFacts {
    let mut facts = ResourceDependencyFacts::default();
    let canonical_root = std::fs::canonicalize(root).ok();
    let mut edge_count = 0usize;

    for (parent, file_facts) in &index.files {
        if file_facts.used_lexical_fallback {
            facts.limitations.push(limitation(
                parent,
                0,
                "resource_dependency",
                "rust_resource_lexical_fallback",
            ));
            continue;
        }
        let directives =
            match rust_embed_resource_directives(parent, &file_facts.source, MAX_RESOURCE_EDGES) {
                Ok(directives) => directives,
                Err(reason_code) => {
                    facts.limitations.push(limitation(
                        parent,
                        0,
                        "include_str!/include_bytes!",
                        &reason_code,
                    ));
                    continue;
                }
            };
        for directive in directives {
            edge_count += 1;
            if edge_count > MAX_RESOURCE_EDGES {
                facts.limitations.push(limitation(
                    parent,
                    directive.line,
                    &directive.expression,
                    "rust_resource_edge_limit_exceeded",
                ));
                facts.edges.clear();
                return facts;
            }
            let resource_kind = match directive.macro_kind {
                RustEmbedResourceMacro::IncludeStr | RustEmbedResourceMacro::IncludeBytes => {
                    ResourceKindV1::CompileTimeEmbedded
                }
            };
            let Some(ref literal) = directive.literal_path else {
                push_opaque_edge(
                    index,
                    &mut facts,
                    parent,
                    &directive,
                    resource_kind,
                    "dynamic_or_non_literal_embed_argument",
                );
                continue;
            };
            let Some(candidate) = repository_relative_target(parent, literal) else {
                facts.limitations.push(limitation(
                    parent,
                    directive.line,
                    &directive.expression,
                    "rust_resource_path_outside_repository",
                ));
                push_opaque_edge(
                    index,
                    &mut facts,
                    parent,
                    &directive,
                    ResourceKindV1::OpaqueOrDynamicResource,
                    "path_outside_repository",
                );
                continue;
            };
            let full = root.join(&candidate);
            let canonical_target = match std::fs::canonicalize(&full) {
                Ok(path) => path,
                Err(_) => {
                    facts.limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_resource_target_missing",
                    ));
                    continue;
                }
            };
            if canonical_root
                .as_ref()
                .is_none_or(|root| !canonical_target.starts_with(root))
            {
                facts.limitations.push(limitation(
                    parent,
                    directive.line,
                    &directive.expression,
                    "rust_resource_symlink_escape",
                ));
                push_opaque_edge(
                    index,
                    &mut facts,
                    parent,
                    &directive,
                    ResourceKindV1::OpaqueOrDynamicResource,
                    "path_escapes_repository",
                );
                continue;
            }
            let content_identity = content_identity_for_path(index, &candidate);
            let consumer_id = consumer_for_line(index, parent, directive.line);
            let semantic_digest = ResourceDependencyEdgeV1::mint_semantic_digest(
                resource_kind,
                &candidate,
                &consumer_id,
                ResourceAccessKindV1::ExactLiteralRepositoryPath,
                ResourceCompletenessClassV1::Exact,
            );
            facts.edges.push(ResourceDependencyEdgeV1 {
                edge_id: ResourceDependencyEdgeV1::mint_edge_id(&semantic_digest),
                semantic_digest,
                resource_kind,
                resource_path: candidate,
                content_identity,
                consumer_id,
                consumer_file: parent.clone(),
                consumer_line: directive.line,
                access_kind: ResourceAccessKindV1::ExactLiteralRepositoryPath,
                resolution_basis: match directive.macro_kind {
                    RustEmbedResourceMacro::IncludeStr => "include_str_literal".to_string(),
                    RustEmbedResourceMacro::IncludeBytes => "include_bytes_literal".to_string(),
                },
                base_presence: ResourcePresenceV1::Present,
                head_presence: ResourcePresenceV1::Present,
                completeness_class: ResourceCompletenessClassV1::Exact,
                first_unresolved_edge: None,
                limitations: Vec::new(),
                non_claims: vec![
                    "may_depend_only".to_string(),
                    "no_test_adequacy".to_string(),
                ],
            });
        }
    }

    facts.edges.sort_by(|left, right| {
        left.resource_path
            .cmp(&right.resource_path)
            .then(left.consumer_id.0.cmp(&right.consumer_id.0))
            .then(left.consumer_line.cmp(&right.consumer_line))
    });
    facts
}

/// Attach producer-owned resource dependency facts to a finished parse tree.
pub(crate) fn attach_resource_dependencies(root: &Path, index: &mut RustIndex) {
    index.resource_dependencies = collect_resource_dependency_edges(root, index);
}

fn content_identity_for_path(index: &RustIndex, path: &Path) -> String {
    index
        .files
        .get(path)
        .map(|facts| format!("source:{}", source_digest(facts.source.as_ref())))
        .unwrap_or_else(|| "source:missing".to_string())
}

fn source_digest(source: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(source.as_bytes()))
}

fn consumer_for_line(index: &RustIndex, file: &Path, line: usize) -> SymbolId {
    for id in index.live_function_ids() {
        let function = &index.function_facts[id];
        if function.file.as_path() != file {
            continue;
        }
        if line >= function.start_line && line <= function.end_line {
            return function.id.clone();
        }
    }
    SymbolId(format!("{}::file_scope", stable_path_text(file)))
}

fn push_opaque_edge(
    index: &RustIndex,
    facts: &mut ResourceDependencyFacts,
    parent: &Path,
    directive: &RustEmbedResourceDirective,
    resource_kind: ResourceKindV1,
    reason: &str,
) {
    let placeholder = PathBuf::from(format!(
        "opaque/{}:{}",
        stable_path_text(parent),
        directive.line
    ));
    let consumer_id = consumer_for_line(index, parent, directive.line);
    let semantic_digest = ResourceDependencyEdgeV1::mint_semantic_digest(
        resource_kind,
        &placeholder,
        &consumer_id,
        ResourceAccessKindV1::DynamicOrUnresolved,
        ResourceCompletenessClassV1::Opaque,
    );
    facts.edges.push(ResourceDependencyEdgeV1 {
        edge_id: ResourceDependencyEdgeV1::mint_edge_id(&semantic_digest),
        semantic_digest,
        resource_kind,
        resource_path: placeholder,
        content_identity: "content:unresolved".to_string(),
        consumer_id,
        consumer_file: parent.to_path_buf(),
        consumer_line: directive.line,
        access_kind: ResourceAccessKindV1::DynamicOrUnresolved,
        resolution_basis: reason.to_string(),
        base_presence: ResourcePresenceV1::Unknown,
        head_presence: ResourcePresenceV1::Unknown,
        completeness_class: ResourceCompletenessClassV1::Opaque,
        first_unresolved_edge: Some(reason.to_string()),
        limitations: vec![reason.to_string()],
        non_claims: vec!["may_depend_only".to_string(), "widen_fallback".to_string()],
    });
}

fn limitation(
    consumer_file: &Path,
    line: usize,
    expression: &str,
    reason_code: &str,
) -> ResourceDependencyLimitation {
    ResourceDependencyLimitation {
        consumer_file: consumer_file.to_path_buf(),
        line,
        expression: expression.to_string(),
        reason_code: reason_code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::RustIndex;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use crate::analysis::verification_plan::resources::{
        ResourceChangeKindV1, project_resource_plan,
    };
    use crate::domain::RelationReason;
    use std::collections::BTreeSet;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn manual_index(root: &Path, sources: &[(&str, &str)]) -> Result<RustIndex, String> {
        let adapter = RaRustSyntaxAdapter;
        let mut index = RustIndex::default();
        for (relative, source) in sources {
            let path = PathBuf::from(relative);
            let facts = if relative.ends_with(".rs") {
                adapter.summarize_file(&path, source)?
            } else {
                use crate::analysis::facts::FileFacts;
                use std::sync::Arc;
                FileFacts {
                    path: path.clone(),
                    source: Arc::from(*source),
                    ..FileFacts::default()
                }
            };
            index.insert_file(path, facts, true);
        }
        index.finalize()?;
        let _ = root;
        Ok(index)
    }

    #[test]
    fn include_str_literal_produces_exact_resource_edge() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("fixtures"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("fixtures/data.json"), "{}\n")?;
        fs::write(
            root.join("src/lib.rs"),
            "const DATA: &str = include_str!(\"../fixtures/data.json\");\n",
        )?;
        let sources = [
            ("fixtures/data.json", "{}\n"),
            (
                "src/lib.rs",
                "const DATA: &str = include_str!(\"../fixtures/data.json\");\n",
            ),
        ];
        let index = manual_index(&root, &sources)?;
        let facts = collect_resource_dependency_edges(&root, &index);
        assert_eq!(facts.edges.len(), 1, "{:?}", facts);
        assert_eq!(
            facts.edges[0].resource_path,
            PathBuf::from("fixtures/data.json")
        );
        assert_eq!(
            facts.edges[0].access_kind,
            ResourceAccessKindV1::ExactLiteralRepositoryPath
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn comment_and_string_mentions_do_not_create_edges() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-comments-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src"))?;
        let source = r#"
// include_str!("ghost.json");
const NOTE: &str = "include_str!(\"ghost.json\")";
fn production() {}
"#;
        fs::write(root.join("src/lib.rs"), source)?;
        let index = manual_index(&root, &[("src/lib.rs", source)])?;
        let facts = collect_resource_dependency_edges(&root, &index);
        assert!(facts.edges.is_empty(), "{:?}", facts.edges);
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn dynamic_embed_widens_plan_without_exact_consumer() {
        let mut edge = ResourceDependencyEdgeV1 {
            edge_id: "id".to_string(),
            semantic_digest: "digest".to_string(),
            resource_kind: ResourceKindV1::OpaqueOrDynamicResource,
            resource_path: PathBuf::from("generated/out.txt"),
            content_identity: "content:unresolved".to_string(),
            consumer_id: SymbolId("src/lib.rs::file_scope".to_string()),
            consumer_file: PathBuf::from("src/lib.rs"),
            consumer_line: 1,
            access_kind: ResourceAccessKindV1::DynamicOrUnresolved,
            resolution_basis: "dynamic".to_string(),
            base_presence: ResourcePresenceV1::Unknown,
            head_presence: ResourcePresenceV1::Unknown,
            completeness_class: ResourceCompletenessClassV1::Opaque,
            first_unresolved_edge: Some("dynamic".to_string()),
            limitations: vec!["dynamic".to_string()],
            non_claims: vec!["widen_fallback".to_string()],
        };
        let digest = ResourceDependencyEdgeV1::mint_semantic_digest(
            edge.resource_kind,
            &edge.resource_path,
            &edge.consumer_id,
            edge.access_kind,
            edge.completeness_class,
        );
        edge.semantic_digest = digest.clone();
        edge.edge_id = ResourceDependencyEdgeV1::mint_edge_id(&digest);
        let mut changed = BTreeSet::new();
        changed.insert(PathBuf::from("generated/out.txt"));
        let projection = project_resource_plan(
            &[edge],
            &changed,
            &BTreeSet::new(),
            &BTreeSet::new(),
            |_| Some("pkg-a".to_string()),
        );
        assert!(projection.impacted_consumers.is_empty());
        assert!(
            projection
                .limitations
                .iter()
                .any(|item| item.contains("resource_plan_empty_with_opaque_boundary"))
        );
    }

    #[test]
    fn fixture_owner_affinity_is_not_resource_authority() {
        assert_ne!(
            RelationReason::FixtureOwnerAffinity.as_str(),
            ResourceAccessKindV1::ExactLiteralRepositoryPath.as_str()
        );
        assert!(!RelationReason::FixtureOwnerAffinity.establishes_reach(true));
    }

    #[test]
    fn resource_only_change_projects_to_validation_obligation() {
        let resource_path = PathBuf::from("fixtures/data.json");
        let consumer_id = SymbolId("tests/it.rs::reads_fixture".to_string());
        let digest = ResourceDependencyEdgeV1::mint_semantic_digest(
            ResourceKindV1::CompileTimeEmbedded,
            &resource_path,
            &consumer_id,
            ResourceAccessKindV1::ExactLiteralRepositoryPath,
            ResourceCompletenessClassV1::Exact,
        );
        let edge = ResourceDependencyEdgeV1 {
            edge_id: ResourceDependencyEdgeV1::mint_edge_id(&digest),
            semantic_digest: digest,
            resource_kind: ResourceKindV1::CompileTimeEmbedded,
            resource_path: resource_path.clone(),
            content_identity: "source:abc".to_string(),
            consumer_id,
            consumer_file: PathBuf::from("tests/it.rs"),
            consumer_line: 2,
            access_kind: ResourceAccessKindV1::ExactLiteralRepositoryPath,
            resolution_basis: "include_str_literal".to_string(),
            base_presence: ResourcePresenceV1::Present,
            head_presence: ResourcePresenceV1::Present,
            completeness_class: ResourceCompletenessClassV1::Exact,
            first_unresolved_edge: None,
            limitations: Vec::new(),
            non_claims: Vec::new(),
        };
        let mut changed = BTreeSet::new();
        changed.insert(resource_path);
        let projection = project_resource_plan(
            &[edge],
            &changed,
            &BTreeSet::new(),
            &BTreeSet::new(),
            |_| Some("pkg-a".to_string()),
        );
        assert_eq!(projection.validation_obligations.len(), 1);
        assert_eq!(
            projection.validation_obligations[0].change_kind,
            ResourceChangeKindV1::Modified
        );
    }
}

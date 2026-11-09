//! Producer-owned non-code resource dependency edges (#5171).

use super::RustIndex;
use super::includes::repository_relative_target;
use crate::analysis::stable_path_text;
use crate::analysis::syntax::ra::{
    EmbedFileScan, EmbedMacroShadows, RustEmbedResourceDirective, RustEmbedResourceMacro,
    rust_embed_file_scan,
};
use crate::analysis::workspace::package_root;
use crate::domain::{
    ResourceAccessKindV1, ResourceCompletenessClassV1, ResourceDependencyEdgeV1, ResourceKindV1,
    ResourcePresenceV1, SymbolId,
};
use sha2::{Digest, Sha256};
use std::cell::OnceCell;
use std::collections::BTreeMap;
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
    let (scans, package_shadows) = scan_embed_files(index);
    let spans = ConsumerSpans::new(index);

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
        let source = file_facts.source.as_ref();
        if !source.contains("include_str!") && !source.contains("include_bytes!") {
            continue;
        }
        let scope_shadows = package_shadows
            .get(&shadow_scope(parent))
            .copied()
            .unwrap_or_default();
        let Some(scan) = scans.get(parent) else {
            continue;
        };
        let directives = match scan.directives(scope_shadows) {
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
            if directive.shadowed {
                facts.limitations.push(limitation(
                    parent,
                    directive.line,
                    &directive.expression,
                    "rust_resource_macro_shadowed",
                ));
                continue;
            }
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
                    &spans,
                    &mut facts,
                    parent,
                    &directive,
                    ResourceKindV1::OpaqueOrDynamicResource,
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
                    &spans,
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
                    &spans,
                    &mut facts,
                    parent,
                    &directive,
                    ResourceKindV1::OpaqueOrDynamicResource,
                    "path_escapes_repository",
                );
                continue;
            }
            let content_identity = content_identity_for_path(root, index, &candidate);
            let consumer_id = spans.consumer_for_line(parent, directive.line);
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

/// The package a file's macros are visible in. Files outside a conventional
/// package layout form a scope of their own, so only same-file shadows apply.
fn shadow_scope(file: &Path) -> String {
    package_root(file).unwrap_or_else(|| format!("file:{}", stable_path_text(file)))
}

/// Parse each file that mentions an embed macro once, and union the shadows
/// they define or import per package. Module-scoped, `#[macro_use]` and
/// `use`-imported shadows all reduce to this: it fails closed, so a shadow in
/// any file of the package flags every unqualified call in that package.
/// Files the parser could not read still count, by textual mention. The scans
/// are returned so the directive pass reuses the same parse.
fn scan_embed_files(
    index: &RustIndex,
) -> (
    BTreeMap<&PathBuf, EmbedFileScan>,
    BTreeMap<String, EmbedMacroShadows>,
) {
    let mut scans = BTreeMap::new();
    let mut scopes: BTreeMap<String, EmbedMacroShadows> = BTreeMap::new();
    for (path, file_facts) in &index.files {
        let source = file_facts.source.as_ref();
        if !source.contains("include_str") && !source.contains("include_bytes") {
            continue;
        }
        let scan = rust_embed_file_scan(source, MAX_RESOURCE_EDGES);
        scopes
            .entry(shadow_scope(path))
            .or_default()
            .merge(scan.shadows());
        scans.insert(path, scan);
    }
    (scans, scopes)
}

/// Attach producer-owned resource dependency facts to a finished parse tree.
pub(crate) fn attach_resource_dependencies(root: &Path, index: &mut RustIndex) {
    index.resource_dependencies = collect_resource_dependency_edges(root, index);
}

fn content_identity_for_path(root: &Path, index: &RustIndex, path: &Path) -> String {
    if let Some(facts) = index.files.get(path) {
        return format!("source:{}", source_digest(facts.source.as_ref()));
    }
    let disk_path = root.join(path);
    match std::fs::read(&disk_path) {
        Ok(bytes) => format!("bytes:{}", source_digest_bytes(&bytes)),
        Err(_) => "source:missing".to_string(),
    }
}

fn source_digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn source_digest(source: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(source.as_bytes()))
}

/// Function spans grouped by file, built on first use so the live function
/// list is walked once per pass instead of once per embed directive.
type FileSpans<'a> = BTreeMap<&'a Path, Vec<(usize, usize, &'a SymbolId)>>;

struct ConsumerSpans<'a> {
    index: &'a RustIndex,
    by_file: OnceCell<FileSpans<'a>>,
}

impl<'a> ConsumerSpans<'a> {
    fn new(index: &'a RustIndex) -> Self {
        Self {
            index,
            by_file: OnceCell::new(),
        }
    }

    /// Innermost function containing `line`; among equal starts the later
    /// live function wins. Falls back to a file-scope symbol.
    fn consumer_for_line(&self, file: &Path, line: usize) -> SymbolId {
        let by_file = self.by_file.get_or_init(|| {
            let mut grouped = FileSpans::new();
            for id in self.index.live_function_ids() {
                let function = &self.index.function_facts[id];
                grouped.entry(function.file.as_path()).or_default().push((
                    function.start_line,
                    function.end_line,
                    &function.id,
                ));
            }
            grouped
        });
        let mut best: Option<(usize, &SymbolId)> = None;
        for &(start, end, id) in by_file.get(file).map(Vec::as_slice).unwrap_or_default() {
            if line >= start && line <= end && best.is_none_or(|(s, _)| start >= s) {
                best = Some((start, id));
            }
        }
        best.map(|(_, id)| id.clone())
            .unwrap_or_else(|| SymbolId(format!("{}::file_scope", stable_path_text(file))))
    }
}

fn push_opaque_edge(
    spans: &ConsumerSpans<'_>,
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
    let consumer_id = spans.consumer_for_line(parent, directive.line);
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
            // A file the parser rejects is indexed as source text only, the way a
            // lexical fallback would hold it, so the collector sees it.
            let facts = if relative.ends_with(".rs") && !relative.contains("broken") {
                adapter.summarize_file(&path, source)?
            } else {
                use crate::analysis::facts::FileFacts;
                use std::sync::Arc;
                FileFacts {
                    path: path.clone(),
                    source: Arc::from(*source),
                    // Production indexes a parse failure through the lexical
                    // fallback; hold the fixture to the same state.
                    used_lexical_fallback: relative.ends_with(".rs"),
                    ..FileFacts::default()
                }
            };
            index.insert_file(path, facts, true);
        }
        index.finalize()?;
        let _ = root;
        Ok(index)
    }

    fn shadow_fixture(
        sources: &[(&str, &str)],
    ) -> Result<(ResourceDependencyFacts, PathBuf), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-shadow-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("crates/a/fixtures"))?;
        fs::create_dir_all(root.join("crates/b/fixtures"))?;
        fs::write(root.join("crates/a/fixtures/data.json"), "{}\n")?;
        fs::write(root.join("crates/b/fixtures/data.json"), "{}\n")?;
        let index = manual_index(&root, sources)?;
        Ok((collect_resource_dependency_edges(&root, &index), root))
    }

    const EMBED: &str = "const D: &str = include_str!(\"../fixtures/data.json\");\n";

    #[test]
    fn cross_file_macro_rules_shadow_produces_no_exact_edge()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-macro-shadow-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("fixtures"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("fixtures/data.json"), "{}\n")?;
        let macros = "macro_rules! include_str {\n    ($path:literal) => { \"fixed\" };\n}\n";
        fs::write(root.join("src/macros.rs"), macros)?;
        let lib = "const DATA: &str = include_str!(\"../fixtures/data.json\");\n";
        fs::write(root.join("src/lib.rs"), lib)?;
        let sources = [("src/macros.rs", macros), ("src/lib.rs", lib)];
        let index = manual_index(&root, &sources)?;
        let facts = collect_resource_dependency_edges(&root, &index);
        assert!(
            facts.edges.is_empty(),
            "unqualified include_str! shadowed crate-wide must not yield an exact edge: {:?}",
            facts.edges
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cross_file_macro_shadow_in_the_package_withholds_the_exact_edge()
    -> Result<(), Box<dyn std::error::Error>> {
        // The consumer and the shadowing macro live in different files of
        // one package; the old same-file check missed this.
        let (facts, root) = shadow_fixture(&[
            ("crates/a/src/lib.rs", EMBED),
            (
                "crates/a/src/macros.rs",
                "macro_rules! include_str { ($p:literal) => { \"fixed\" }; }\n",
            ),
        ])?;
        assert!(facts.edges.is_empty(), "{:?}", facts.edges);
        assert!(
            facts
                .limitations
                .iter()
                .any(|l| l.reason_code == "rust_resource_macro_shadowed"
                    && l.consumer_file == Path::new("crates/a/src/lib.rs")),
            "{:?}",
            facts.limitations
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn unparseable_sibling_that_mentions_the_macro_withholds_the_exact_edge()
    -> Result<(), Box<dyn std::error::Error>> {
        // A sibling file the parser cannot read might define the shadowing
        // macro; the package fails closed on it, and a package that does not
        // mention the name keeps its edge.
        let (facts, root) = shadow_fixture(&[
            ("crates/a/src/lib.rs", EMBED),
            ("crates/a/src/broken.rs", "fn ( ??? include_str ???"),
            ("crates/b/src/lib.rs", EMBED),
            ("crates/b/src/broken.rs", "fn ( ??? nothing here ???"),
        ])?;
        let consumers: Vec<&Path> = facts
            .edges
            .iter()
            .map(|edge| edge.consumer_file.as_path())
            .collect();
        assert_eq!(
            consumers,
            vec![Path::new("crates/b/src/lib.rs")],
            "{facts:?}"
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn use_imported_shadow_withholds_the_exact_edge() -> Result<(), Box<dyn std::error::Error>> {
        let (facts, root) = shadow_fixture(&[
            (
                "crates/a/src/lib.rs",
                "use crate::macros::include_str;\nconst D: &str = include_str!(\"../fixtures/data.json\");\n",
            ),
            ("crates/a/src/macros.rs", "pub fn unrelated() {}\n"),
        ])?;
        assert!(facts.edges.is_empty(), "{:?}", facts.edges);
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn shadow_in_another_package_and_other_macro_name_leave_the_exact_edge()
    -> Result<(), Box<dyn std::error::Error>> {
        // Controls: a shadow of include_str in crate b does not touch crate
        // a, and a shadow of include_bytes does not touch an include_str call.
        let (facts, root) = shadow_fixture(&[
            ("crates/a/src/lib.rs", EMBED),
            (
                "crates/a/src/bytes.rs",
                "macro_rules! include_bytes { () => {}; }\n",
            ),
            (
                "crates/b/src/macros.rs",
                "macro_rules! include_str { () => {}; }\n",
            ),
        ])?;
        assert_eq!(facts.edges.len(), 1, "{:?}", facts);
        assert_eq!(
            facts.edges[0].consumer_file,
            PathBuf::from("crates/a/src/lib.rs")
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
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
        assert_eq!(projection.widened_packages.len(), 1);
    }

    #[test]
    fn several_embeds_in_one_file_each_pick_their_own_consumer()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-multi-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("fixtures"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("fixtures/a.txt"), "a\n")?;
        fs::write(root.join("fixtures/b.txt"), "b\n")?;
        fs::write(root.join("fixtures/c.txt"), "c\n")?;
        let source = r#"
const TOP: &str = include_str!("../fixtures/c.txt");
fn first() -> &'static str {
    include_str!("../fixtures/a.txt")
}
fn second() -> &'static str {
    include_str!("../fixtures/b.txt")
}
"#;
        fs::write(root.join("src/lib.rs"), source)?;
        let index = manual_index(&root, &[("src/lib.rs", source)])?;
        let facts = collect_resource_dependency_edges(&root, &index);
        let consumer = |name: &str| {
            facts
                .edges
                .iter()
                .find(|edge| edge.resource_path.ends_with(name))
                .map(|edge| edge.consumer_id.0.clone())
        };
        assert_eq!(facts.edges.len(), 3);
        assert!(consumer("a.txt").is_some_and(|id| id.contains("first")));
        assert!(consumer("b.txt").is_some_and(|id| id.contains("second")));
        assert!(consumer("c.txt").is_some_and(|id| id.ends_with("::file_scope")));
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn nested_function_embed_selects_innermost_consumer() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-nested-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("fixtures"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("fixtures/x.txt"), "x\n")?;
        let source = r#"
fn outer() {
    fn inner() {
        let _ = include_str!("../fixtures/x.txt");
    }
    let _ = inner();
}
"#;
        fs::write(root.join("src/lib.rs"), source)?;
        let index = manual_index(&root, &[("src/lib.rs", source)])?;
        let facts = collect_resource_dependency_edges(&root, &index);
        assert_eq!(facts.edges.len(), 1);
        assert!(
            facts.edges[0].consumer_id.0.contains("inner"),
            "{:?}",
            facts.edges[0].consumer_id
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn content_identity_reads_fixture_bytes_from_disk() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-bytes-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("fixtures"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("fixtures/data.json"), "{}\n")?;
        let lib = "const DATA: &str = include_str!(\"../fixtures/data.json\");\n";
        fs::write(root.join("src/lib.rs"), lib)?;
        let index = manual_index(&root, &[("src/lib.rs", lib)])?;
        let first = collect_resource_dependency_edges(&root, &index);
        assert_eq!(first.edges.len(), 1);
        assert!(
            first.edges[0].content_identity.starts_with("bytes:sha256:"),
            "{}",
            first.edges[0].content_identity
        );
        fs::write(root.join("fixtures/data.json"), "{\"ok\":true}\n")?;
        let second = collect_resource_dependency_edges(&root, &index);
        assert_ne!(
            first.edges[0].content_identity, second.edges[0].content_identity,
            "fixture edit should change disk-backed content identity"
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn dynamic_include_bytes_producer_widens_package_on_resource_change()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-resource-deps-dynamic-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src"))?;
        let source = r#"fn read() { let _ = include_bytes!(concat!(env!("OUT_DIR"), "/x.bin")); }"#;
        fs::write(root.join("src/lib.rs"), source)?;
        let index = manual_index(&root, &[("src/lib.rs", source)])?;
        let facts = collect_resource_dependency_edges(&root, &index);
        assert_eq!(facts.edges.len(), 1);
        assert_eq!(
            facts.edges[0].resource_kind,
            ResourceKindV1::OpaqueOrDynamicResource
        );
        let mut changed = BTreeSet::new();
        changed.insert(PathBuf::from("generated/out.bin"));
        let projection = project_resource_plan(
            &facts.edges,
            &changed,
            &BTreeSet::new(),
            &BTreeSet::new(),
            |_| Some("pkg-a".to_string()),
        );
        assert_eq!(projection.widened_packages.len(), 1);
        fs::remove_dir_all(&root)?;
        Ok(())
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

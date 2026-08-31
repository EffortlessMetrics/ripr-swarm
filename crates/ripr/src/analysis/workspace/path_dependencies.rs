//! Forward/reverse path-dependency adjacency (#2969, slice B of #2665).
//!
//! Built deterministically in memory from the path-dependency edges captured
//! by slice A (#2968/#3037, see
//! [`crate::analysis::seam_cache::PathDependencyEdge`]). No network, no
//! filesystem walking of dependencies, no Cargo invocation: node identities
//! are the captured repo-relative manifest paths and the lexically resolved
//! dependency paths, so the graph answers "which manifests does this manifest
//! reach through path dependencies" (forward) and "which manifests reach this
//! manifest" (reverse) without implying that any registry or external
//! dependency metadata was resolved. External dependency provenance stays
//! `unavailable`.
//!
//! Honesty contract:
//!
//! - all iteration is `BTreeMap`/`BTreeSet`-ordered, so no neighbor list or
//!   walk result depends on hash-map iteration order;
//! - edges without a resolved identity (`UnsupportedAbsolutePath`,
//!   `UnresolvedWorkspaceInheritance`, `InvalidDeclaration`) cannot
//!   participate in the adjacency; that gap is disclosed through
//!   `connected_edge_count` and the status detail, never silently dropped;
//! - a cycle terminates the walk and is disclosed as a cycle marker (the
//!   back-edge target), never recursed into;
//! - a non-empty capture-limitation list means the captured edge inventory is
//!   partial, so the status degrades to `limited` and says so;
//! - when no path dependency was declared at all the status is `complete`
//!   with an empty node set and the detail states that truthfully.
//!
//! Scope boundary: this slice builds and exposes the graph only. Scope
//! expansion and dep-driven impact analysis consume it later (#2970).

use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::seam_cache::WorkspaceGraphProvenance;

/// Build status of the path-dependency adjacency, disclosed alongside every
/// rendered neighbor list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathDependencyGraphStatus {
    /// Every scanned manifest was parsed and no capture limitation exists, so
    /// the adjacency covers the full captured edge inventory. An empty edge
    /// inventory is also `complete`: there were no path dependencies to walk.
    Complete,
    /// At least one capture limitation was reported, so the captured edge
    /// inventory is partial and the adjacency may be missing edges.
    Limited,
    /// No manifest was found in the scan root, so no adjacency was built.
    Unavailable,
}

impl PathDependencyGraphStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Limited => "limited",
            Self::Unavailable => "unavailable",
        }
    }
}

/// One reachability walk result: the transitively reachable manifests plus
/// the cycle markers cut to keep the walk finite.
///
/// Reserved-until-consumed: the walk surface is the #2970 consumer contract
/// (scope expansion walks the graph); slice B builds and test-proves it, so
/// it is dead in non-test builds until that slice lands.
#[allow(
    dead_code,
    reason = "walk result type is the #2970 consumer contract; exercised by tests in slice B"
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathDependencyWalk {
    /// Manifest identities reached from the start node, sorted, excluding the
    /// start itself (a start that is re-entered through a cycle is disclosed
    /// in `cycle_manifests` instead of being listed as reached).
    reachable: Vec<String>,
    /// Targets of back-edges to manifests already on the current walk path,
    /// sorted. Non-empty means the walked subgraph is cyclic; the walk
    /// terminated by cutting these edges instead of recursing into them.
    cycle_manifests: Vec<String>,
}

impl PathDependencyWalk {
    /// Manifests reached from the start node, sorted, start excluded.
    #[allow(
        dead_code,
        reason = "walk result type is the #2970 consumer contract; exercised by tests in slice B"
    )]
    pub(crate) fn reachable(&self) -> &[String] {
        &self.reachable
    }

    /// Cycle markers: back-edge targets already on the walk path, sorted.
    #[allow(
        dead_code,
        reason = "walk result type is the #2970 consumer contract; exercised by tests in slice B"
    )]
    pub(crate) fn cycle_manifests(&self) -> &[String] {
        &self.cycle_manifests
    }
}

/// Deterministic forward/reverse adjacency over captured path-dependency
/// edges. Nodes are repo-relative, `/`-separated manifest paths: the
/// declaring manifest, and the target identity formed from the lexically
/// resolved dependency path (a resolved directory `foo` names the manifest
/// `foo/Cargo.toml`; a resolved path that already names a `Cargo.toml` file
/// is kept as-is; the scan root names `Cargo.toml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathDependencyAdjacency {
    status: PathDependencyGraphStatus,
    detail: Option<String>,
    edge_count: usize,
    connected_edge_count: usize,
    nodes: BTreeSet<String>,
    forward: BTreeMap<String, BTreeSet<String>>,
    reverse: BTreeMap<String, BTreeSet<String>>,
}

impl PathDependencyAdjacency {
    /// Build the adjacency from the captured path-dependency edges in
    /// `provenance`. Pure: reads no filesystem and no network.
    pub(crate) fn build(provenance: &WorkspaceGraphProvenance) -> Self {
        // `unavailable` is reserved for an absent manifest scan. When the
        // capture still recorded limitations (a partial inventory), the
        // adjacency must disclose them as `limited` instead of taking this
        // shortcut and silently dropping the capture (#3613 review).
        if provenance.package_graph_status == "unavailable"
            && provenance.path_dependency_limitations.is_empty()
        {
            return Self {
                status: PathDependencyGraphStatus::Unavailable,
                detail: Some(
                    "no local Cargo.toml manifest was found; the path-dependency adjacency \
                     was not built"
                        .to_string(),
                ),
                edge_count: 0,
                connected_edge_count: 0,
                nodes: BTreeSet::new(),
                forward: BTreeMap::new(),
                reverse: BTreeMap::new(),
            };
        }

        let edges = &provenance.path_dependency_edges;
        let mut forward: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        let mut connected_edge_count = 0usize;
        for edge in edges {
            // Only `Resolved` edges name a manifest that exists inside the
            // scan root. Outside-root, missing-target, absolute, and
            // invalid declarations stay disconnected: inventing a node for
            // them would fabricate a phantom workspace manifest in a
            // `complete` graph (#3613 review); they remain disclosed in
            // `edge_count` and the detail/limitations. The declaring
            // manifest joins the node set only through a connected edge so
            // a manifest whose every edge is disconnected does not appear
            // as an isolated participant.
            if edge.resolution != crate::analysis::seam_cache::PathDependencyResolution::Resolved {
                continue;
            }
            let Some(resolved) = edge.resolved_path.as_deref() else {
                continue;
            };
            nodes.insert(edge.from_manifest.clone());
            let target = manifest_identity_from_resolved_path(resolved);
            nodes.insert(target.clone());
            forward
                .entry(edge.from_manifest.clone())
                .or_default()
                .insert(target.clone());
            reverse
                .entry(target)
                .or_default()
                .insert(edge.from_manifest.clone());
            connected_edge_count += 1;
        }

        let edge_count = edges.len();
        let status = if provenance.path_dependency_limitations.is_empty() {
            PathDependencyGraphStatus::Complete
        } else {
            PathDependencyGraphStatus::Limited
        };
        let detail = compose_detail(status, edge_count, connected_edge_count, provenance);
        Self {
            status,
            detail,
            edge_count,
            connected_edge_count,
            nodes,
            forward,
            reverse,
        }
    }

    /// Disclosed build status for this adjacency.
    pub(crate) fn status(&self) -> PathDependencyGraphStatus {
        self.status
    }

    /// Human-readable status detail, present whenever there is a named
    /// boundary to disclose (unavailable scan, empty inventory, partial
    /// inventory, or edges that cannot participate).
    pub(crate) fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Number of captured edges the adjacency was built from.
    pub(crate) fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Number of captured edges that carry a resolved identity and therefore
    /// participate in the adjacency. `edge_count - connected_edge_count` is
    /// the disclosed non-participating remainder.
    pub(crate) fn connected_edge_count(&self) -> usize {
        self.connected_edge_count
    }

    /// Every manifest that participates in at least one captured edge (either
    /// side), sorted. A crate declared by no path dependency — isolated from
    /// the path-dep graph — is not a node.
    pub(crate) fn nodes(&self) -> &BTreeSet<String> {
        &self.nodes
    }

    /// Whether `manifest` participates in the graph at all. Unknown manifests
    /// and crates with no path-dependency edges both return `false`.
    ///
    /// Reserved-until-consumed by the #2970 scope-expansion slice.
    #[allow(
        dead_code,
        reason = "reserved for the #2970 scope-expansion consumer; exercised by tests in slice B"
    )]
    pub(crate) fn contains_node(&self, manifest: &str) -> bool {
        self.nodes.contains(manifest)
    }

    /// Direct forward neighbors of `manifest` (the manifests it declares path
    /// dependencies on), sorted. `None` when the manifest has no outgoing
    /// connected edges, including when it is not a graph node.
    pub(crate) fn forward_neighbors(&self, manifest: &str) -> Option<&BTreeSet<String>> {
        self.forward.get(manifest)
    }

    /// Direct reverse neighbors of `manifest` (the manifests that declare a
    /// path dependency on it), sorted. `None` when nothing depends on the
    /// manifest through a connected path edge.
    pub(crate) fn reverse_neighbors(&self, manifest: &str) -> Option<&BTreeSet<String>> {
        self.reverse.get(manifest)
    }

    /// Transitive forward reach: every manifest `manifest` reaches through
    /// path dependencies, with cycles cut and disclosed. `None` when the
    /// manifest is not a graph node.
    ///
    /// Reserved-until-consumed by the #2970 scope-expansion slice.
    #[allow(
        dead_code,
        reason = "reserved for the #2970 scope-expansion consumer; exercised by tests in slice B"
    )]
    pub(crate) fn forward_walk(&self, manifest: &str) -> Option<PathDependencyWalk> {
        self.walk(&self.forward, manifest)
    }

    /// Transitive reverse reach: every manifest that reaches `manifest`
    /// through path dependencies, with cycles cut and disclosed. `None` when
    /// the manifest is not a graph node.
    ///
    /// Reserved-until-consumed by the #2970 scope-expansion slice.
    #[allow(
        dead_code,
        reason = "reserved for the #2970 scope-expansion consumer; exercised by tests in slice B"
    )]
    pub(crate) fn reverse_walk(&self, manifest: &str) -> Option<PathDependencyWalk> {
        self.walk(&self.reverse, manifest)
    }

    /// Iterative depth-first walk over one adjacency direction. `visited`
    /// expands each node once so shared subgraphs are walked once; a child
    /// already on the current path is a cycle back-edge: disclosed, never
    /// recursed. Together the two guards bound the walk to O(nodes + edges)
    /// steps on any input, cyclic or not.
    #[allow(
        dead_code,
        reason = "reserved for the #2970 scope-expansion consumer; exercised by tests in slice B"
    )]
    fn walk(
        &self,
        adjacency: &BTreeMap<String, BTreeSet<String>>,
        start: &str,
    ) -> Option<PathDependencyWalk> {
        if !self.nodes.contains(start) {
            return None;
        }
        let empty: BTreeSet<String> = BTreeSet::new();
        let mut on_path: BTreeSet<String> = BTreeSet::new();
        on_path.insert(start.to_string());
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut reachable: BTreeSet<String> = BTreeSet::new();
        let mut cycles: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<(String, std::vec::IntoIter<String>)> = Vec::new();
        let children = adjacency.get(start).unwrap_or(&empty);
        stack.push((
            start.to_string(),
            children.iter().cloned().collect::<Vec<_>>().into_iter(),
        ));
        loop {
            let advance = stack
                .last_mut()
                .and_then(|(_node, children)| children.next());
            match advance {
                Some(child) => {
                    if on_path.contains(&child) {
                        cycles.insert(child);
                        continue;
                    }
                    if visited.contains(&child) {
                        continue;
                    }
                    visited.insert(child.clone());
                    reachable.insert(child.clone());
                    let grandchildren = adjacency
                        .get(&child)
                        .unwrap_or(&empty)
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter();
                    on_path.insert(child.clone());
                    stack.push((child, grandchildren));
                }
                None => match stack.pop() {
                    Some((node, _)) => {
                        on_path.remove(&node);
                    }
                    None => break,
                },
            }
        }
        Some(PathDependencyWalk {
            reachable: reachable.into_iter().collect(),
            cycle_manifests: cycles.into_iter().collect(),
        })
    }
}

/// Node identity of one resolved dependency path. Cargo path dependencies
/// name either a directory (its manifest is `<dir>/Cargo.toml`) or a manifest
/// file directly; the resolved capture identity already uses `/` separators.
fn manifest_identity_from_resolved_path(resolved: &str) -> String {
    if resolved == "Cargo.toml" || resolved.ends_with("/Cargo.toml") {
        resolved.to_string()
    } else if resolved.is_empty() {
        "Cargo.toml".to_string()
    } else {
        format!("{resolved}/Cargo.toml")
    }
}

/// Status detail: name the honest boundary whenever one exists — a partial
/// edge inventory, an empty inventory, or edges that cannot participate
/// because they carry no resolved identity.
fn compose_detail(
    status: PathDependencyGraphStatus,
    edge_count: usize,
    connected_edge_count: usize,
    provenance: &WorkspaceGraphProvenance,
) -> Option<String> {
    let mut detail = match status {
        PathDependencyGraphStatus::Limited => Some(format!(
            "path-dependency edge capture reported {} limitation(s); the adjacency covers \
             a partial edge inventory",
            provenance.path_dependency_limitations.len()
        )),
        PathDependencyGraphStatus::Complete if edge_count == 0 => Some(
            "no path dependencies were declared in the scanned manifests; the adjacency \
             has no nodes"
                .to_string(),
        ),
        PathDependencyGraphStatus::Complete | PathDependencyGraphStatus::Unavailable => None,
    };
    let disconnected = edge_count - connected_edge_count;
    if disconnected > 0 {
        let boundary = format!(
            "{disconnected} of {edge_count} edges do not resolve to an existing \
             in-workspace manifest and do not participate in the adjacency"
        );
        detail = Some(match detail {
            Some(existing) => format!("{existing}; {boundary}"),
            None => boundary,
        });
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::seam_cache::{
        PathDependencyEdge, PathDependencyLimitation, PathDependencyLimitationKind,
        PathDependencyResolution, PathDependencySection, PathDependencySource,
        workspace_graph_provenance,
    };
    use std::path::{Path, PathBuf};

    fn unique_dir(label: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "ripr-path-dep-adjacency-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_manifest(root: &Path, relative: &str, text: &str) -> Result<(), String> {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {}: {err}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|err| format!("write {}: {err}", path.display()))
    }

    fn adjacency_for(root: &Path) -> PathDependencyAdjacency {
        PathDependencyAdjacency::build(&workspace_graph_provenance(root))
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    /// The task matrix: a -> b -> c through path dependencies plus an
    /// unrelated crate d. Fails under a plausible wrong implementation such
    /// as swapped forward/reverse maps (forward(a) would be empty and
    /// reverse(c) would lose a) or an order-dependent build.
    #[test]
    fn adjacency_walks_forward_and_reverse_over_a_path_dep_chain() -> Result<(), String> {
        let root = unique_dir("chain");
        let _ = std::fs::remove_dir_all(&root);
        for (name, deps) in [
            ("a", "b = { path = \"../b\" }"),
            ("b", "c = { path = \"../c\" }"),
            ("c", ""),
            ("d", ""),
        ] {
            let mut manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
            if !deps.is_empty() {
                manifest.push_str(&format!("\n[dependencies]\n{deps}\n"));
            }
            write_manifest(&root, &format!("{name}/Cargo.toml"), &manifest)?;
        }

        let adjacency = adjacency_for(&root);
        assert_eq!(
            adjacency.status(),
            PathDependencyGraphStatus::Complete,
            "a fully parsed chain has no capture limitation"
        );
        assert_eq!(adjacency.edge_count(), 2);
        assert_eq!(adjacency.connected_edge_count(), 2);
        assert_eq!(adjacency.nodes().len(), 3, "d is isolated: {adjacency:?}");
        assert!(!adjacency.contains_node("d/Cargo.toml"));

        // Direct neighbors: forward is the declared direction, reverse the
        // depended-on direction. Swapping the two maps fails here first.
        assert_eq!(
            adjacency
                .forward_neighbors("a/Cargo.toml")
                .map(|set| set.len()),
            Some(1)
        );
        assert!(
            adjacency
                .forward_neighbors("a/Cargo.toml")
                .is_some_and(|set| set.contains("b/Cargo.toml"))
        );
        assert!(
            adjacency
                .forward_neighbors("b/Cargo.toml")
                .is_some_and(|set| set.contains("c/Cargo.toml"))
        );
        assert_eq!(adjacency.forward_neighbors("c/Cargo.toml"), None);
        assert!(
            adjacency
                .reverse_neighbors("c/Cargo.toml")
                .is_some_and(|set| set.contains("b/Cargo.toml"))
        );
        assert!(
            adjacency
                .reverse_neighbors("b/Cargo.toml")
                .is_some_and(|set| set.contains("a/Cargo.toml"))
        );
        assert_eq!(adjacency.reverse_neighbors("a/Cargo.toml"), None);

        // Transitive walks.
        let forward_a = adjacency
            .forward_walk("a/Cargo.toml")
            .ok_or_else(|| "a must be a graph node".to_string())?;
        assert_eq!(
            forward_a.reachable(),
            strings(&["b/Cargo.toml", "c/Cargo.toml"])
        );
        assert!(forward_a.cycle_manifests().is_empty());
        let forward_b = adjacency
            .forward_walk("b/Cargo.toml")
            .ok_or_else(|| "b must be a graph node".to_string())?;
        assert_eq!(forward_b.reachable(), strings(&["c/Cargo.toml"]));
        let reverse_c = adjacency
            .reverse_walk("c/Cargo.toml")
            .ok_or_else(|| "c must be a graph node".to_string())?;
        assert_eq!(
            reverse_c.reachable(),
            strings(&["a/Cargo.toml", "b/Cargo.toml"])
        );
        let reverse_a = adjacency
            .reverse_walk("a/Cargo.toml")
            .ok_or_else(|| "a must be a graph node".to_string())?;
        assert!(reverse_a.reachable().is_empty(), "nothing reaches a");

        // Isolated crate d: not a node, so both walks name that explicitly
        // instead of returning an empty walk.
        assert_eq!(adjacency.forward_walk("d/Cargo.toml"), None);
        assert_eq!(adjacency.reverse_walk("d/Cargo.toml"), None);

        // Deterministic rebuild: same input, byte-equal graph.
        assert_eq!(adjacency, adjacency_for(&root));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Cycles terminate and are disclosed as cycle markers: a walk over a
    /// two-manifest cycle and over a crate feeding the cycle both finish.
    #[test]
    fn adjacency_cycle_walk_terminates_and_discloses_the_cycle_marker() -> Result<(), String> {
        let root = unique_dir("cycle");
        let _ = std::fs::remove_dir_all(&root);
        write_manifest(
            &root,
            "x/Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ny = { path = \"../y\" }\n",
        )?;
        write_manifest(
            &root,
            "y/Cargo.toml",
            "[package]\nname = \"y\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nx = { path = \"../x\" }\n",
        )?;
        write_manifest(
            &root,
            "z/Cargo.toml",
            "[package]\nname = \"z\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ny = { path = \"../y\" }\n",
        )?;

        let adjacency = adjacency_for(&root);
        let walk_x = adjacency
            .forward_walk("x/Cargo.toml")
            .ok_or_else(|| "x must be a graph node".to_string())?;
        assert_eq!(walk_x.reachable(), strings(&["y/Cargo.toml"]));
        assert_eq!(
            walk_x.cycle_manifests(),
            strings(&["x/Cargo.toml"]),
            "re-entering the start through the cycle is the disclosed marker"
        );

        let walk_z = adjacency
            .forward_walk("z/Cargo.toml")
            .ok_or_else(|| "z must be a graph node".to_string())?;
        assert_eq!(
            walk_z.reachable(),
            strings(&["x/Cargo.toml", "y/Cargo.toml"])
        );
        assert_eq!(
            walk_z.cycle_manifests(),
            strings(&["y/Cargo.toml"]),
            "the back-edge into the already-visited cycle member is cut and named"
        );

        let reverse_x = adjacency
            .reverse_walk("x/Cargo.toml")
            .ok_or_else(|| "x must be a graph node".to_string())?;
        assert_eq!(
            reverse_x.reachable(),
            strings(&["y/Cargo.toml", "z/Cargo.toml"])
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// A manifest declaring a path dependency on itself yields one deduped
    /// self-neighbor and a disclosed self-cycle, not a non-terminating walk.
    #[test]
    fn adjacency_self_loop_deduplicates_and_discloses() -> Result<(), String> {
        let root = unique_dir("self-loop");
        let _ = std::fs::remove_dir_all(&root);
        write_manifest(
            &root,
            "s/Cargo.toml",
            "[package]\nname = \"s\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ns = { path = \".\" }\n",
        )?;

        let adjacency = adjacency_for(&root);
        let neighbors = adjacency
            .forward_neighbors("s/Cargo.toml")
            .ok_or_else(|| "s must have forward neighbors".to_string())?;
        assert_eq!(
            neighbors.iter().collect::<Vec<_>>(),
            vec!["s/Cargo.toml"],
            "the self-loop is a single deduplicated neighbor"
        );
        let walk = adjacency
            .forward_walk("s/Cargo.toml")
            .ok_or_else(|| "s must be a graph node".to_string())?;
        assert!(walk.reachable().is_empty(), "the self-loop adds no reach");
        assert_eq!(walk.cycle_manifests(), strings(&["s/Cargo.toml"]));

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Status honesty: no manifests means the graph was not built; manifests
    /// without any path dependency mean a complete-but-empty graph that says
    /// so; an unparsed manifest means the inventory is partial and the status
    /// degrades to `limited`.
    #[test]
    fn adjacency_status_names_each_honesty_case() -> Result<(), String> {
        let missing_root = unique_dir("status-missing");
        let _ = std::fs::remove_dir_all(&missing_root);
        let missing = adjacency_for(&missing_root);
        assert_eq!(missing.status(), PathDependencyGraphStatus::Unavailable);
        assert_eq!(
            missing.detail(),
            Some(
                "no local Cargo.toml manifest was found; the path-dependency adjacency was not built"
            )
        );
        assert_eq!(missing.nodes().len(), 0);

        let empty_root = unique_dir("status-empty");
        let _ = std::fs::remove_dir_all(&empty_root);
        write_manifest(
            &empty_root,
            "Cargo.toml",
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\n",
        )?;
        let empty = adjacency_for(&empty_root);
        assert_eq!(empty.status(), PathDependencyGraphStatus::Complete);
        assert_eq!(empty.edge_count(), 0);
        assert_eq!(empty.nodes().len(), 0);
        assert_eq!(
            empty.detail(),
            Some(
                "no path dependencies were declared in the scanned manifests; the adjacency has no nodes"
            ),
            "an empty graph must say there were no path dependencies, not imply success silently"
        );

        let limited_root = unique_dir("status-limited");
        let _ = std::fs::remove_dir_all(&limited_root);
        write_manifest(&limited_root, "Cargo.toml", "[package\nname = \"broken\"\n")?;
        write_manifest(
            &limited_root,
            "ok/Cargo.toml",
            "[package]\nname = \"ok\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ninner = { path = \"inner\" }\n",
        )?;
        write_manifest(
            &limited_root,
            "ok/inner/Cargo.toml",
            "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
        )?;
        let limited = adjacency_for(&limited_root);
        assert_eq!(limited.status(), PathDependencyGraphStatus::Limited);
        assert!(
            limited
                .detail()
                .is_some_and(|detail| detail.contains("partial edge inventory"))
        );
        assert!(limited.contains_node("ok/Cargo.toml"));
        assert!(limited.contains_node("ok/inner/Cargo.toml"));

        let _ = std::fs::remove_dir_all(&empty_root);
        let _ = std::fs::remove_dir_all(&limited_root);
        Ok(())
    }

    /// The same target declared in two sections of one manifest collapses to
    /// one neighbor: the adjacency is node-level, the counts stay edge-level.
    #[test]
    fn adjacency_deduplicates_multiple_declarations_of_the_same_target() -> Result<(), String> {
        let root = unique_dir("dedup");
        let _ = std::fs::remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\", \"tool\"]\n",
        )?;
        write_manifest(
            &root,
            "tool/Cargo.toml",
            "[package]\nname = \"tool\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\ntool = { path = \"../tool\" }\n\n\
             [dev-dependencies]\ntool = { path = \"../tool\" }\n",
        )?;

        let adjacency = adjacency_for(&root);
        assert_eq!(adjacency.edge_count(), 2, "both section edges are captured");
        assert_eq!(adjacency.connected_edge_count(), 2);
        let neighbors = adjacency
            .forward_neighbors("app/Cargo.toml")
            .ok_or_else(|| "app must have forward neighbors".to_string())?;
        assert_eq!(
            neighbors.iter().collect::<Vec<_>>(),
            vec!["tool/Cargo.toml"],
            "two declarations of the same target are one adjacency neighbor"
        );

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    /// Edges without a resolved identity cannot participate; the remainder is
    /// disclosed through the counts and the detail instead of being dropped
    /// silently. Synthetic provenance keeps this matrix platform-free.
    #[test]
    fn adjacency_excludes_edges_without_resolved_identity_and_discloses_the_remainder()
    -> Result<(), String> {
        let edges = vec![
            edge_with_resolution(PathDependencyResolution::Resolved, Some("lib/dep")),
            edge_with_resolution(PathDependencyResolution::UnsupportedAbsolutePath, None),
            edge_with_resolution(
                PathDependencyResolution::UnresolvedWorkspaceInheritance,
                None,
            ),
            edge_with_resolution(PathDependencyResolution::InvalidDeclaration, None),
        ];
        let provenance = WorkspaceGraphProvenance {
            package_graph_status: "complete".to_string(),
            path_dependency_edges: edges,
            path_dependency_limitations: Vec::new(),
            ..WorkspaceGraphProvenance::default()
        };
        let adjacency = PathDependencyAdjacency::build(&provenance);
        assert_eq!(adjacency.status(), PathDependencyGraphStatus::Complete);
        assert_eq!(adjacency.edge_count(), 4);
        assert_eq!(adjacency.connected_edge_count(), 1);
        assert_eq!(
            adjacency.detail(),
            Some(
                "3 of 4 edges do not resolve to an existing in-workspace manifest and do \
                 not participate in the adjacency"
            )
        );
        let neighbors = adjacency
            .forward_neighbors("app/Cargo.toml")
            .ok_or_else(|| "the connected edge must participate in the adjacency".to_string())?;
        assert_eq!(
            neighbors.iter().collect::<Vec<_>>(),
            vec!["lib/dep/Cargo.toml"]
        );
        Ok(())
    }

    /// A partial inventory with disconnected edges composes both disclosures
    /// into one detail line.
    #[test]
    fn adjacency_limited_status_composes_both_disclosures() {
        let provenance = WorkspaceGraphProvenance {
            package_graph_status: "complete".to_string(),
            path_dependency_edges: vec![edge_with_resolution(
                PathDependencyResolution::UnsupportedAbsolutePath,
                None,
            )],
            path_dependency_limitations: vec![PathDependencyLimitation {
                manifest: "crates/app/Cargo.toml".to_string(),
                kind: PathDependencyLimitationKind::UnfollowedWorkspaceRedirect,
                detail: "redirect not followed".to_string(),
            }],
            ..WorkspaceGraphProvenance::default()
        };
        let adjacency = PathDependencyAdjacency::build(&provenance);
        assert_eq!(adjacency.status(), PathDependencyGraphStatus::Limited);
        let detail = adjacency.detail().unwrap_or_default();
        assert!(
            detail.contains("1 limitation(s)"),
            "the partial-inventory boundary must be named: {detail}"
        );
        assert!(
            detail.contains("1 of 1 edges do not resolve to an existing in-workspace manifest"),
            "the non-participating edge count must be named: {detail}"
        );
    }

    /// Outside-root and missing-target resolutions carry a lexical identity
    /// but do not name an existing in-workspace manifest: they must stay
    /// disconnected so receipts cannot report phantom or external manifests
    /// as adjacency nodes (#3613 review).
    #[test]
    fn adjacency_excludes_outside_root_and_missing_target_edges() -> Result<(), String> {
        let mut stray =
            edge_with_resolution(PathDependencyResolution::TargetMissing, Some("ghost"));
        stray.from_manifest = "stray/Cargo.toml".to_string();
        let edges = vec![
            edge_with_resolution(PathDependencyResolution::Resolved, Some("lib/dep")),
            edge_with_resolution(
                PathDependencyResolution::ResolvedOutsideWorkspace,
                Some("../outside"),
            ),
            stray,
        ];
        let provenance = WorkspaceGraphProvenance {
            package_graph_status: "complete".to_string(),
            path_dependency_edges: edges,
            path_dependency_limitations: Vec::new(),
            ..WorkspaceGraphProvenance::default()
        };
        let adjacency = PathDependencyAdjacency::build(&provenance);
        assert_eq!(adjacency.edge_count(), 3);
        assert_eq!(adjacency.connected_edge_count(), 1);
        assert!(
            !adjacency.contains_node("../outside/Cargo.toml"),
            "an outside-root resolution must not become a node"
        );
        assert!(
            !adjacency.contains_node("ghost/Cargo.toml"),
            "a missing-target resolution must not become a node"
        );
        assert!(
            !adjacency.contains_node("stray/Cargo.toml"),
            "a manifest whose every edge is disconnected must not appear as an \
             isolated participant (#3613 review)"
        );
        assert!(
            adjacency
                .forward_neighbors("app/Cargo.toml")
                .is_some_and(
                    |neighbors| neighbors.iter().collect::<Vec<_>>() == vec!["lib/dep/Cargo.toml"]
                ),
            "only the resolved in-workspace edge participates"
        );
        Ok(())
    }

    /// `unavailable` is reserved for an absent manifest scan: provenance that
    /// captured a partial inventory plus limitations must stay `limited` with
    /// its edges, not collapse into a bare `unavailable` that discards the
    /// capture (#3613 review).
    #[test]
    fn unavailable_provenance_with_limitations_stays_limited_and_keeps_edges() {
        let provenance = WorkspaceGraphProvenance {
            package_graph_status: "unavailable".to_string(),
            path_dependency_edges: vec![edge_with_resolution(
                PathDependencyResolution::Resolved,
                Some("lib/dep"),
            )],
            path_dependency_limitations: vec![PathDependencyLimitation {
                manifest: "crates/app/Cargo.toml".to_string(),
                kind: PathDependencyLimitationKind::UnfollowedWorkspaceRedirect,
                detail: "redirect not followed".to_string(),
            }],
            ..WorkspaceGraphProvenance::default()
        };
        let adjacency = PathDependencyAdjacency::build(&provenance);
        assert_eq!(adjacency.status(), PathDependencyGraphStatus::Limited);
        assert_eq!(adjacency.connected_edge_count(), 1);
        assert!(
            adjacency
                .detail()
                .is_some_and(|detail| detail.contains("1 limitation(s)")),
            "the capture limitations must be disclosed: {:?}",
            adjacency.detail()
        );
    }

    fn edge_with_resolution(
        resolution: PathDependencyResolution,
        resolved_path: Option<&str>,
    ) -> PathDependencyEdge {
        PathDependencyEdge {
            from_manifest: "app/Cargo.toml".to_string(),
            section: PathDependencySection::Dependencies,
            target: None,
            dependency_name: "dep".to_string(),
            declared_path: Some("../dep".to_string()),
            resolved_path: resolved_path.map(str::to_string),
            resolution,
            source: PathDependencySource::Package,
        }
    }

    /// Identity normalization: directories gain `/Cargo.toml`, manifest-file
    /// shapes are kept, and the scan root resolves to `Cargo.toml`.
    #[test]
    fn manifest_identity_from_resolved_path_normalizes_each_shape() {
        assert_eq!(manifest_identity_from_resolved_path("b"), "b/Cargo.toml");
        assert_eq!(
            manifest_identity_from_resolved_path("crates/shared"),
            "crates/shared/Cargo.toml"
        );
        assert_eq!(
            manifest_identity_from_resolved_path("shared/Cargo.toml"),
            "shared/Cargo.toml",
            "a declared path already naming a manifest is kept as-is"
        );
        assert_eq!(
            manifest_identity_from_resolved_path("Cargo.toml"),
            "Cargo.toml"
        );
        assert_eq!(
            manifest_identity_from_resolved_path(""),
            "Cargo.toml",
            "an empty resolution is the scan root manifest"
        );
    }

    /// A start node whose only role is being depended on (a reverse-side
    /// leaf) is a node: its forward walk exists and is empty, which is
    /// distinct from `None` (not a node at all).
    #[test]
    fn adjacency_reverse_leaf_has_an_empty_forward_walk() -> Result<(), String> {
        let root = unique_dir("reverse-leaf");
        let _ = std::fs::remove_dir_all(&root);
        write_manifest(
            &root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\", \"lib\"]\n",
        )?;
        write_manifest(
            &root,
            "lib/Cargo.toml",
            "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n",
        )?;
        write_manifest(
            &root,
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nlib = { path = \"../lib\" }\n",
        )?;

        let adjacency = adjacency_for(&root);
        let walk = adjacency
            .forward_walk("lib/Cargo.toml")
            .ok_or_else(|| "lib must be a graph node".to_string())?;
        assert!(walk.reachable().is_empty());
        assert!(walk.cycle_manifests().is_empty());
        assert_eq!(adjacency.reverse_neighbors("app/Cargo.toml"), None);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}

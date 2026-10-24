//! Execution ancestry is distinct from the representative source-role chain.
use super::role_composition::MAX_CONTEXT_CHAIN_DEPTH;
use super::{RustIndex, SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct ExecutionContext {
    pub(crate) roots: BTreeSet<PathBuf>,
    pub(crate) edges: Vec<SourceRoleProvenanceEdge>,
}

impl RustIndex {
    /// Every approved module ancestry, or unknown. Serialized file facts do
    /// not carry this authority; the composition pass must rebuild it first.
    /// DAG heights bound depth without enumerating alternative paths.
    pub(crate) fn execution_context(&self, file: &Path) -> Option<ExecutionContext> {
        let parents = self.execution_module_parents.as_ref()?;
        let mut heights = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        self.execution_context_height(file, parents, &mut heights, &mut visiting)?;
        let mut edges = Vec::new();
        let mut roots = BTreeSet::new();
        for path in heights.keys() {
            match parents.get(path).filter(|edges| !edges.is_empty()) {
                Some(incoming) => edges.extend(incoming.iter().cloned()),
                None => {
                    roots.insert(path.clone());
                }
            }
        }
        Some(ExecutionContext { roots, edges })
    }

    fn execution_context_height(
        &self,
        file: &Path,
        parents: &BTreeMap<PathBuf, Vec<SourceRoleProvenanceEdge>>,
        heights: &mut BTreeMap<PathBuf, usize>,
        visiting: &mut BTreeSet<PathBuf>,
    ) -> Option<usize> {
        if let Some(height) = heights.get(file) {
            return Some(*height);
        }
        // Bound the active traversal too: heights alone are only known on
        // return, after a cycle or an excessively long chain was entered.
        if visiting.len() > MAX_CONTEXT_CHAIN_DEPTH || !visiting.insert(file.to_path_buf()) {
            return None;
        }
        let facts = self.files().get(file)?;
        if facts.used_lexical_fallback
            || facts.role_provenance.earliest_unresolved_reason.is_some()
            || self.include_targets.contains(file)
            || self.include_parents.contains_key(file)
            || facts
                .role_provenance
                .edges
                .iter()
                .any(|edge| edge.kind != SourceRoleProvenanceEdgeKind::Module)
        {
            return None;
        }
        let mut height = 0;
        if let Some(incoming) = parents.get(file) {
            for edge in incoming {
                height = height.max(
                    1 + self.execution_context_height(&edge.parent, parents, heights, visiting)?,
                );
            }
        } else if !facts.role_provenance.edges.is_empty() {
            // A representative chain without its approved parent set is
            // incomplete authority, never a singleton context.
            return None;
        }
        if height > MAX_CONTEXT_CHAIN_DEPTH {
            return None;
        }
        visiting.remove(file);
        heights.insert(file.to_path_buf(), height);
        Some(height)
    }
}

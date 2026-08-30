mod build;
pub(crate) mod cfg_predicates;
mod includes;
mod model;
mod parameterized_tests;
mod role_composition;
mod test_styles;

use std::path::{Path, PathBuf};

pub fn build_index(root: &Path, files: &[PathBuf]) -> Result<model::RustIndex, String> {
    let mut index = build::build_index(root, files)?;
    parameterized_tests::promote_explicit_test_case_functions(&mut index);
    test_styles::normalize_index_test_styles(&mut index);
    // Composition runs strictly after the normalizer: the normalizer
    // recomputes every role from same-file text and would stomp composed
    // roles (#3533). Composed grants only ever upgrade `Production` to the
    // evidence-only `CfgTestModule`, never the reverse. The workspace root
    // anchors crate-root identity for default module resolution.
    role_composition::compose_index_source_roles(&mut index, root);
    Ok(index)
}

pub(crate) fn build_index_from_loaded_files_with_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<build::CachedRustIndex, String> {
    let mut cached = build::build_index_from_loaded_files_with_cache(root, files)?;
    parameterized_tests::promote_explicit_test_case_functions(&mut cached.index);
    test_styles::normalize_index_test_styles(&mut cached.index);
    role_composition::compose_index_source_roles(&mut cached.index, root);
    Ok(cached)
}

// Keep compilation-unit rebasing available at the facts facade for index consumers.
pub(crate) use includes::compilation_unit_path_from_parents;
pub use model::{
    CallFact, FileFacts, FunctionFact, FunctionSourceRole, FunctionSummary, LiteralFact,
    ModuleDeclarationFact, ModulePathTarget, OracleFact, ProbeShapeFact, ReturnFact,
    RustIncludeLimitation, RustIndex, SourceRoleProvenance, SourceRoleProvenanceEdge,
    SourceRoleProvenanceEdgeKind, TestFact, TestSummary,
};
#[cfg(test)]
pub(crate) use model::{WorkspaceFileAuthority, WorkspaceRootAuthority};

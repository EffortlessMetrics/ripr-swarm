mod build;
pub(crate) mod cfg_predicates;
mod harness_registry;
mod includes;
mod model;
mod parameterized_tests;
mod role_composition;
mod test_styles;

use std::path::{Path, PathBuf};

use crate::config::TestHarnessRegistration;

pub fn build_index(root: &Path, files: &[PathBuf]) -> Result<model::RustIndex, String> {
    build_index_with_test_harnesses(root, files, &[])
}

/// Index construction with the repository-governed harness registry
/// (#3532) applied: exact registrations derive typed subject and
/// limitation facts after the ordinary role authorities run. Without
/// registrations the result is identical to [`build_index`].
pub fn build_index_with_test_harnesses(
    root: &Path,
    files: &[PathBuf],
    registrations: &[TestHarnessRegistration],
) -> Result<model::RustIndex, String> {
    build_index_with_test_harnesses_and_production_like_targets(
        root,
        files,
        registrations,
        &std::collections::BTreeSet::new(),
    )
}

pub fn build_index_with_test_harnesses_and_production_like_targets(
    root: &Path,
    files: &[PathBuf],
    registrations: &[TestHarnessRegistration],
    production_like_targets: &std::collections::BTreeSet<PathBuf>,
) -> Result<model::RustIndex, String> {
    let mut index = build::build_index(root, files)?;
    parameterized_tests::promote_explicit_test_case_functions(&mut index);
    test_styles::normalize_index_test_styles(&mut index);
    // Composition runs strictly after the normalizer: the normalizer
    // recomputes every role from same-file text and would stomp composed
    // roles (#3533). Composed grants only ever upgrade `Production` to the
    // evidence-only `CfgTestModule`, never the reverse. The workspace root
    // anchors crate-root identity for default module resolution.
    role_composition::compose_index_source_roles(&mut index, root);
    harness_registry::apply_registrations(&mut index, registrations, production_like_targets);
    Ok(index)
}

#[cfg(test)]
pub(crate) fn build_index_from_loaded_files_with_cache_and_test_harnesses(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    build_index_from_loaded_files_with_cache_and_test_harnesses_and_production_like_targets(
        root,
        files,
        registrations,
        &std::collections::BTreeSet::new(),
    )
}

pub(crate) fn build_index_from_loaded_files_with_cache_and_test_harnesses_and_production_like_targets(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
    production_like_targets: &std::collections::BTreeSet<PathBuf>,
) -> Result<build::CachedRustIndex, String> {
    let mut cached = build::build_index_from_loaded_files_with_cache(root, files)?;
    parameterized_tests::promote_explicit_test_case_functions(&mut cached.index);
    test_styles::normalize_index_test_styles(&mut cached.index);
    role_composition::compose_index_source_roles(&mut cached.index, root);
    harness_registry::apply_registrations(
        &mut cached.index,
        registrations,
        production_like_targets,
    );
    Ok(cached)
}

/// Test-only compatibility facade for the cache path without harness
/// registrations. Keep the same post-parse normalization and contextual
/// role composition as the production index builder; callers that need the
/// registry use the explicit sibling above.
#[cfg(test)]
pub(crate) fn build_index_from_loaded_files_with_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<build::CachedRustIndex, String> {
    build_index_from_loaded_files_with_cache_and_test_harnesses(root, files, &[])
}

// Keep compilation-unit rebasing available at the facts facade for index consumers.
pub(crate) use includes::compilation_unit_path_from_parents;
pub use model::{
    CallFact, FileFacts, FunctionFact, FunctionSourceRole, FunctionSummary, HarnessLimitationFact,
    HarnessSelectorCapability, HarnessSubjectClaim, HarnessSubjectFact, LiteralFact,
    ModuleDeclarationFact, ModulePathTarget, OracleFact, ProbeShapeFact, ResolvedIncludeParent,
    ReturnFact, RustIncludeLimitation, RustIndex, SourceRoleProvenance, SourceRoleProvenanceEdge,
    SourceRoleProvenanceEdgeKind, TestFact, TestSummary,
};
#[cfg(test)]
pub(crate) use model::{WorkspaceFileAuthority, WorkspaceRootAuthority};

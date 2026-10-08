mod build;
pub(crate) use build::CachedRustIndex;
use build::MissAttribution;
#[cfg(test)]
pub(crate) use build::streamed_source_bytes;
pub(crate) use build::{RUST_SOURCE_NOT_UTF8_REASON, rust_source_text};
pub(crate) mod cfg_predicates;
pub(crate) mod drop_in;
mod harness_registry;
mod includes;
mod index;
pub(crate) mod member_crates;
mod model;
mod parameterized_tests;
mod role_composition;
mod test_helpers;
mod test_styles;
pub(crate) use test_styles::BUILT_IN_TEST_ATTRIBUTE_PATHS;

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
    let mut index = index_phase("index_parse", || build::build_index(root, files))?;
    index_phase("index_parameterized_tests", || {
        parameterized_tests::promote_explicit_test_case_functions(&mut index);
        index.refresh_memberships()
    })?;
    index_phase("index_test_styles", || {
        test_styles::normalize_index_test_styles(&mut index)?;
        index.refresh_memberships()
    })?;
    // Composition runs strictly after the normalizer: the normalizer
    // recomputes every role from same-file text and would stomp composed
    // roles (#3533). Composed grants only ever upgrade `Production` to the
    // evidence-only `CfgTestModule`, never the reverse. The workspace root
    // anchors crate-root identity for default module resolution.
    index_phase("index_role_composition", || {
        role_composition::compose_index_source_roles(&mut index, root);
        Ok(())
    })?;
    // Explicit harness registrations are the most specific authority, so
    // they run after composition and a composed generic grant can never
    // overwrite a registered subject's role (#3532). The workspace root
    // anchors the Cargo target metadata validation (#3608).
    index_phase("index_harness_registry", || {
        harness_registry::apply_registrations(&mut index, root, registrations);
        Ok(())
    })?;
    // Helper crediting reads final roles, so it runs after every role
    // authority.
    index_phase("index_test_helper_credit", || {
        test_helpers::credit_same_file_assertion_helpers(&mut index);
        Ok(())
    })?;
    index.finalize()?;
    Ok(index)
}

/// Parsed facts only, through the same file-fact cache: no role
/// composition, harness registration or helper crediting. For scans that
/// read call and body facts by name and keep every function and test
/// whatever its role (the #5320 reach closure).
pub(crate) fn parse_loaded_files_with_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<RustIndex, String> {
    let mut cached = index_phase("index_cached_parse", || {
        build::build_index_from_loaded_files_with_cache(root, files, MissAttribution::Skipped)
    })?;
    cached.index.finalize()?;
    Ok(cached.index)
}

/// Legacy retain-everything oracle for the streaming path (#4996): all
/// production inventory builds stream from disk, so only tests use this.
#[cfg(test)]
pub(crate) fn build_index_from_loaded_files_with_cache_and_test_harnesses(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    build_cached_index_with_test_harnesses(root, files, registrations, MissAttribution::Named)
}

/// [`build_index_from_loaded_files_with_cache_and_test_harnesses`] for
/// callers that never report which cache entries a miss replaced (diff
/// analysis): it skips the whole-cache read that names them.
pub(crate) fn build_analysis_index_from_loaded_files(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    build_cached_index_with_test_harnesses(root, files, registrations, MissAttribution::Skipped)
}

fn build_cached_index_with_test_harnesses(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
    attribution: MissAttribution,
) -> Result<build::CachedRustIndex, String> {
    let cached = index_phase("index_cached_parse", || {
        build::build_index_from_loaded_files_with_cache(root, files, attribution)
    })?;
    post_process_cached_index(cached, root, registrations)
}

/// Every post-parse phase, shared by the loaded and streaming builders so
/// the two can never drift (#4996).
fn post_process_cached_index(
    mut cached: build::CachedRustIndex,
    root: &Path,
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    index_phase("index_parameterized_tests", || {
        parameterized_tests::promote_explicit_test_case_functions(&mut cached.index);
        cached.index.refresh_memberships()
    })?;
    index_phase("index_test_styles", || {
        test_styles::normalize_index_test_styles(&mut cached.index)?;
        cached.index.refresh_memberships()
    })?;
    index_phase("index_role_composition", || {
        role_composition::compose_index_source_roles(&mut cached.index, root);
        Ok(())
    })?;
    // Explicit harness registrations are the most specific authority, so
    // they run after composition and a composed generic grant can never
    // overwrite a registered subject's role (#3532). The workspace root
    // anchors the Cargo target metadata validation (#3608); the verdict
    // is recomputed after cache retrieval, so a manifest edit re-validates
    // immediately.
    index_phase("index_harness_registry", || {
        harness_registry::apply_registrations(&mut cached.index, root, registrations);
        Ok(())
    })?;
    // Helper crediting reads final roles, so it runs after every role
    // authority.
    index_phase("index_test_helper_credit", || {
        test_helpers::credit_same_file_assertion_helpers(&mut cached.index);
        Ok(())
    })?;
    cached.index.finalize()?;
    Ok(cached)
}

/// Streaming twin of
/// [`build_index_from_loaded_files_with_cache_and_test_harnesses`]
/// (issue #4996): identical post-processing over an index built from
/// on-demand reads, so production inventory never retains the whole raw
/// source corpus.
pub(crate) fn build_index_from_paths_with_cache_and_test_harnesses(
    root: &Path,
    paths: &[PathBuf],
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    let cached = index_phase("index_cached_parse", || {
        build::build_index_from_paths_with_cache(root, paths, MissAttribution::Named)
    })?;
    post_process_cached_index(cached, root, registrations)
}

// The Cargo-validated file-wide harness evidence grant (#3608) is shared
// by every role surface (diff seeding, seam inventory, LSP scope) so a
// misdeclared registration degrades identically everywhere.
pub(crate) use harness_registry::validated_file_wide_harness_targets;
pub(crate) use test_styles::attributes_define_test;

// Keep compilation-unit rebasing available at the facts facade for index consumers.
pub(crate) use includes::compilation_unit_path_from_parents;
pub use index::{FactSlice, FileData, FileFactsView};
#[cfg(test)]
pub(crate) use model::OwnedRustIndex;
pub use model::{
    CallFact, FileFacts, FunctionContainer, FunctionFact, FunctionImplContext, FunctionItemFact,
    FunctionSourceRole, FunctionSummary, HarnessLimitationFact, HarnessSelectorCapability,
    HarnessSubjectClaim, HarnessSubjectFact, LetBindingFact, LiteralFact, ModuleDeclarationFact,
    ModulePathTarget, OracleFact, ProbeShapeFact, ProbeShapeKind, ResolvedIncludeParent,
    ReturnFact, RustIncludeLimitation, RustIndex, SourceRoleProvenance, SourceRoleProvenanceEdge,
    SourceRoleProvenanceEdgeKind, SourceText, TestFact, TestSummary, UnresolvedPropertyMacroFact,
};
// Hot evidence loops hash each indexed file once and validate by digest.
pub(crate) use model::WorkspaceFileAuthority;
#[cfg(test)]
pub(crate) use model::WorkspaceRootAuthority;
pub(crate) use model::source_digest;

/// Phase tracing extends the existing opt-in latency stream; normal output is
/// unchanged. A cancelled intermediate index is never returned as complete.
fn index_phase<T>(name: &str, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    crate::analysis::cancellation::checkpoint()?;
    let started = std::time::Instant::now();
    let trace = std::env::var_os("RIPR_REPO_EXPOSURE_LATENCY_TRACE").is_some();
    if trace {
        eprintln!("ripr_repo_exposure_latency phase={name} status=start duration_ms=0");
    }
    let result = work();
    if trace {
        let status = if result.is_ok() { "finished" } else { "failed" };
        eprintln!(
            "ripr_repo_exposure_latency phase={name} status={status} duration_ms={}",
            started.elapsed().as_millis()
        );
    }
    let value = result?;
    crate::analysis::cancellation::checkpoint()?;
    Ok(value)
}

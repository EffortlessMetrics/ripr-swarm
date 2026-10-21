mod build;
/// Test-only count of the index-build phases (`index_parse` /
/// `index_cached_parse`) started on the observing thread, mirroring the
/// phase names the latency trace emits (#8547 build-count proof). Thread
/// local, so parallel tests never pollute each other's counts; the parse
/// phases themselves run on the caller's thread even when their inner
/// batch work uses rayon.
#[cfg(test)]
pub(crate) mod index_build_probe {
    use std::cell::Cell;

    thread_local! {
        static BUILDS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record() {
        BUILDS.with(|builds| builds.set(builds.get() + 1));
    }

    /// Run `work` with a zeroed counter and return its result plus the
    /// number of index-build phases it started.
    pub(crate) fn observe<T>(work: impl FnOnce() -> T) -> (T, usize) {
        BUILDS.with(|builds| builds.set(0));
        let result = work();
        (result, BUILDS.with(Cell::get))
    }
}
mod doctests;
pub(crate) use build::CachedRustIndex;
use build::MissAttribution;
#[cfg(test)]
pub(crate) use build::streamed_source_bytes;
pub(crate) use build::{RUST_SOURCE_NOT_UTF8_REASON, rust_source_text};
pub(crate) use doctests::SYNTHETIC_NAME as DOCTEST_FUNCTION_NAME;
pub(crate) mod cfg_predicates;
pub(crate) mod drop_in;
mod harness_registry;
mod includes;
mod index;
mod macro_generated_tests;

/// The index builder's macro-generated test registration, for fixtures that
/// assemble an index by hand.
#[cfg(test)]
pub(crate) fn register_macro_generated_tests_for_test(index: &mut model::RustIndex) {
    macro_generated_tests::register_macro_generated_tests(index);
}
pub(crate) mod member_crates;
mod model;
mod parameterized_tests;
mod role_composition;
mod test_helpers;
mod test_styles;
pub(crate) use test_styles::BUILT_IN_TEST_ATTRIBUTE_PATHS;

use std::path::{Path, PathBuf};

use crate::config::TestHarnessRegistration;

/// Normalize local roles in reconstructed source with the same authorities as
/// ordinary indexing. External composition is intentionally supplied separately
/// by the caller's indexed context, rather than guessed from an isolated file.
pub(crate) fn normalize_reconstructed_source_roles(
    index: &mut RustIndex,
    root: &Path,
    registrations: &[TestHarnessRegistration],
) -> Result<(), String> {
    parameterized_tests::promote_explicit_test_case_functions(index);
    index.refresh_memberships()?;
    test_styles::normalize_index_test_styles(index)?;
    index.refresh_memberships()?;
    harness_registry::apply_registrations(index, root, registrations);
    index.finalize()
}

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
    // #5334: same-file macro_rules! test generators, expanded in one
    // bounded shape. After the style normalizer, which rebuilds
    // tests from parsed functions and has none for a generated test.
    index_phase("index_macro_generated_tests", || {
        macro_generated_tests::register_macro_generated_tests(&mut index);
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
    // #6648: doc examples rustdoc runs. After composition, whose module
    // edges decide which files belong to the library.
    index_phase("index_doctests", || {
        doctests::register_doctests(&mut index);
        index.refresh_memberships()
    })?;
    // Helper crediting reads final roles, so it runs after every role
    // authority.
    index_phase("index_test_helper_credit", || {
        test_helpers::credit_same_file_assertion_helpers(&mut index);
        test_helpers::credit_same_file_assertion_macros(&mut index);
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

/// Reuse an already-built cached index as the index for `files` (#8547):
/// files whose indexed source still matches the loaded bytes are kept
/// as-is, new or changed files are parsed through the file-fact cache and
/// inserted, files no longer in the list are removed, and the shared
/// post-parse phases re-run once over the merged index. When nothing
/// changed, the index is returned untouched, so one check invocation
/// builds its workspace index once and reuses it instead of re-decoding
/// the fact cache and re-running post-processing per consumer.
pub(crate) fn reuse_cached_index_with_loaded_files(
    mut cached: build::CachedRustIndex,
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
    registrations: &[TestHarnessRegistration],
) -> Result<build::CachedRustIndex, String> {
    let kept: std::collections::BTreeSet<&Path> =
        files.iter().map(|(path, _)| path.as_path()).collect();
    let mut extras: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (path, bytes) in files {
        let current = cached.index.files().get(path).is_some_and(|facts| {
            // The stored source is the decoded text (BOM stripped, invalid
            // bytes replaced), so compare against the same decoding.
            let source: &str = facts.data().source.as_ref();
            build::rust_source_text(bytes).text == source
        });
        if !current {
            extras.push((path.clone(), bytes.clone()));
        }
    }
    let dropped: Vec<PathBuf> = cached
        .index
        .files()
        .iter()
        .map(|(path, _)| path.clone())
        .filter(|path| !kept.contains(path.as_path()))
        .collect();
    if extras.is_empty() && dropped.is_empty() {
        return Ok(cached);
    }
    // Post-processing is only incrementally exact when the file set grows by
    // leaf additions. A dropped file can retract module provenance and helper
    // credits already applied to retained files, and an added or changed file
    // with an `include!` directive can re-parent already rebased function
    // identities; none of those can be undone in place, so fail closed to a
    // fresh build over exactly `files` (verdicts equal a non-reusing build).
    // A changed file (an extra replacing an indexed path) retracts too:
    // `remove_file` leaves include parents/targets/limitations and module
    // provenance derived from the old bytes in place.
    let retracts = !dropped.is_empty()
        || extras
            .iter()
            .any(|(path, _)| cached.index.files().get(path).is_some());
    let rebases_includes = extras
        .iter()
        .any(|(_, bytes)| bytes.windows(8).any(|w| w == b"include!"));
    if retracts || rebases_includes {
        drop(cached);
        return build_cached_index_with_test_harnesses(
            root,
            files,
            registrations,
            MissAttribution::Skipped,
        );
    }
    for path in &dropped {
        cached.index.remove_file(path);
    }
    if !extras.is_empty() {
        let stats = index_phase("index_cached_parse", || {
            build::extend_index_from_loaded_files_with_cache(&mut cached.index, root, &extras)
        })?;
        cached.file_fact_cache.hits += stats.hits;
        cached.file_fact_cache.misses += stats.misses;
        cached.file_fact_cache.corrupt_ignored += stats.corrupt_ignored;
        cached.file_fact_cache.stores += stats.stores;
        cached.file_fact_cache.store_errors += stats.store_errors;
        cached
            .file_fact_cache
            .invalidated_files
            .extend(stats.invalidated_files);
        cached
            .file_fact_cache
            .store_failures
            .extend(stats.store_failures);
        cached.file_fact_cache.store_failures_dropped += stats.store_failures_dropped;
        // An added file that makes module composition unknowable (lexical
        // fallback emits no `mod` declarations; an unknown `#[path]` target
        // may compile any file) flips workspace-wide `outside_module_tree`
        // provenance that retained files already carry and that composition
        // only writes when non-empty, so the stale claim cannot be undone in
        // place. Fail closed to a fresh build.
        let unknowable = extras.iter().any(|(path, _)| {
            cached.index.files().get(path).is_some_and(|facts| {
                facts.used_lexical_fallback
                    || facts.module_declarations.iter().any(|declaration| {
                        declaration.path_target == model::ModulePathTarget::Unknown
                    })
            })
        });
        if unknowable {
            drop(cached);
            return build_cached_index_with_test_harnesses(
                root,
                files,
                registrations,
                MissAttribution::Skipped,
            );
        }
    }
    let mut hash = crate::analysis::seam_cache::FilesContentHashBuilder::new();
    for (path, bytes) in files {
        hash.push(path, bytes);
    }
    cached.files_content_hash = hash.finish();
    post_process_cached_index(cached, root, registrations)
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
    // #5334: same-file macro_rules! test generators, expanded in one
    // bounded shape. After the style normalizer, which rebuilds
    // tests from parsed functions and has none for a generated test.
    index_phase("index_macro_generated_tests", || {
        macro_generated_tests::register_macro_generated_tests(&mut cached.index);
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
    // #6648: doc examples rustdoc runs. After composition, whose module
    // edges decide which files belong to the library.
    index_phase("index_doctests", || {
        doctests::register_doctests(&mut cached.index);
        cached.index.refresh_memberships()
    })?;
    // Helper crediting reads final roles, so it runs after every role
    // authority.
    index_phase("index_test_helper_credit", || {
        test_helpers::credit_same_file_assertion_helpers(&mut cached.index);
        test_helpers::credit_same_file_assertion_macros(&mut cached.index);
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
    CallFact, CallText, FileFacts, FunctionContainer, FunctionFact, FunctionImplContext,
    FunctionItemFact, FunctionSourceRole, FunctionSummary, HarnessLimitationFact,
    HarnessSelectorCapability, HarnessSubjectClaim, HarnessSubjectFact, LetBindingFact,
    LiteralFact, ModuleDeclarationFact, ModulePathTarget, OracleFact, ProbeShapeFact,
    ProbeShapeKind, ResolvedIncludeParent, ReturnFact, RustIncludeLimitation, RustIndex,
    SourceRoleProvenance, SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind, SourceText,
    TestFact, TestSummary, UnresolvedPropertyMacroFact,
};
// Hot evidence loops hash each indexed file once and validate by digest.
pub(crate) use model::WorkspaceFileAuthority;
#[cfg(test)]
pub(crate) use model::WorkspaceRootAuthority;
pub(crate) use model::source_digest;

/// Phase tracing extends the existing opt-in latency stream; normal output is
/// unchanged. A cancelled intermediate index is never returned as complete.
fn index_phase<T>(name: &str, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    #[cfg(test)]
    if matches!(name, "index_parse" | "index_cached_parse") {
        index_build_probe::record();
    }
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
